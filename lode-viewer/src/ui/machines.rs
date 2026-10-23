//! Machines: the machines the viewer follows itself, each with its link's
//! state, and the form that adds one.

use super::{Frame, MARGIN};
use crate::app::{Action, App, FormField, KeyChoice};
use crate::draw::{Rect, mono, sans};
use crate::remote::Kind;
use crate::theme::{self, Theme};

/// Draw the page into `area`, scrolled by the page's scroll.
pub(super) fn paint(app: &App, c: &mut Frame<'_, '_>, t: &Theme, area: Rect) {
    let x = area.x + MARGIN;
    let w = (area.w - 2.0 * MARGIN).min(980.0);
    let top = area.y + 28.0 - app.machines_scroll;
    let mut y = top;
    let _ = c.text(sans(18.0, 600), "Machines", (x, y, 30.0), t.ink, None);
    y += 48.0;
    let root = app.machines_root.as_ref().map_or_else(
        || "~/.local/share/ferrix/lode-receive".to_owned(),
        |root| root.display().to_string(),
    );
    y += c.wrapped(
        sans(12.5, 400),
        &format!("Each machine's log is kept in a store of its own under {root}, so it stays when the link drops."),
        (x, y, 18.0),
        t.ink3,
        w,
    );
    y += 14.0;
    y = followed(app, c, t, Rect::new(x, y, w, 0.0));
    y += 28.0;
    let bottom = form(app, c, t, Rect::new(x, y, w, 0.0));
    app.machines_height.set(bottom - top + 28.0 + 40.0);
}

/// The machines followed, a row each.
fn followed(app: &App, c: &mut Frame<'_, '_>, t: &Theme, place: Rect) -> f32 {
    let _ = c.text(
        sans(13.0, 600),
        "Followed",
        (place.x, place.y, 20.0),
        t.ink3,
        None,
    );
    let mut y = place.y + 28.0;
    let rows = app.remotes.len().max(1);
    let card = Rect::new(place.x, y, place.w, 64.0 * rows as f32);
    c.boxed(card, 10.0, t.bg, t.line);
    if app.remotes.is_empty() {
        let _ = c.text(
            sans(13.0, 400),
            "None yet: add one below.",
            (card.x + 18.0, card.y, 64.0),
            t.ink4,
            None,
        );
        return card.bottom();
    }
    for (index, remote) in app.remotes.iter().enumerate() {
        let row = Rect::new(card.x, y, card.w, 64.0);
        // A press on the row shows its settings below; Open and Remove,
        // drawn after, take their own presses.
        let select = Action::SelectRemote(index);
        if app.selected_remote == Some(index) {
            c.fill(
                Rect::new(row.x + 1.0, row.y + 1.0, row.w - 2.0, row.h - 2.0),
                9.0,
                t.blue_bg,
            );
        } else if app.hover.as_ref() == Some(&select) {
            c.fill(
                Rect::new(row.x + 1.0, row.y + 1.0, row.w - 2.0, row.h - 2.0),
                9.0,
                t.hover,
            );
        }
        c.hit(row, select);
        if index > 0 {
            c.fill(
                Rect::new(row.x + 1.0, row.y, row.w - 2.0, 1.0),
                0.0,
                t.line2,
            );
        }
        let state = app.remote_line(index);
        let dot = if state.error.is_some() {
            theme::RED
        } else if state.up {
            theme::GREEN
        } else {
            theme::AMBER
        };
        c.dot(row.x + 22.0, row.y + 22.0, 4.5, dot);
        let _ = c.text(
            sans(13.5, 600),
            &remote.name,
            (row.x + 36.0, row.y + 10.0, 24.0),
            t.ink,
            Some(160.0),
        );
        let at = format!(
            "{} {}@{}:{}",
            remote.kind.word(),
            if remote.user.is_empty() {
                ""
            } else {
                &remote.user
            },
            remote.host,
            remote.port
        );
        let _ = c.text(
            mono(12.0, 400),
            &at,
            (row.x + 210.0, row.y + 10.0, 24.0),
            t.ink3,
            Some(250.0),
        );
        let _ = c.text(
            sans(12.0, 400),
            &state.text,
            (row.x + 36.0, row.y + 34.0, 20.0),
            t.ink4,
            Some(430.0),
        );
        if let Some(error) = &state.error {
            let _ = c.text(
                sans(12.0, 400),
                error,
                (row.x + 470.0, row.y + 34.0, 20.0),
                t.red_ink,
                Some(row.w - 640.0),
            );
        }
        let open = Rect::new(row.right() - 164.0, row.y + 16.0, 70.0, 30.0);
        c.boxed(open, 6.0, t.bg, t.line3);
        c.text_center(sans(12.5, 500), "Open", open, t.ink);
        c.hit(open, Action::OpenRemote(index));
        let remove = Rect::new(row.right() - 86.0, row.y + 16.0, 70.0, 30.0);
        c.boxed(remove, 6.0, t.bg, t.line3);
        c.text_center(sans(12.5, 500), "Remove", remove, t.red_ink);
        c.hit(remove, Action::RemoveRemote(index));
        y += 64.0;
    }
    card.bottom()
}

/// What the TLS set-up will do on the machine, for the person to confirm
/// before anything is done.
fn confirm(
    c: &mut Frame<'_, '_>,
    t: &Theme,
    place: Rect,
    remote: &crate::remote::Remote,
    plan: &crate::tls::Plan,
) {
    let commands = crate::tls::remote_commands(plan);
    let inner = place.w - 64.0;
    // Every command is shown whole, wrapped, never cut.
    let wrapped: Vec<Vec<String>> = commands
        .iter()
        .map(|command| c.wrap(mono(11.5, 400), command, inner))
        .collect();
    let prints = format!(
        "receiver {} · CA {} · server {}",
        plan.listen, plan.ca_fingerprint, plan.server_fingerprint
    );
    let print_lines = c.wrap(mono(11.0, 400), &prints, inner);
    let lines: usize = wrapped.iter().map(Vec::len).sum();
    let h =
        150.0 + 18.0 * lines as f32 + 4.0 * commands.len() as f32 + 18.0 * print_lines.len() as f32;
    let panel = Rect::new(place.x + 12.0, place.y + 8.0, place.w - 24.0, h);
    c.fill(
        Rect::new(panel.x + 2.0, panel.y + 3.0, panel.w, panel.h),
        10.0,
        t.fill,
    );
    c.boxed(panel, 10.0, t.bg, t.ink);
    let x = panel.x + 20.0;
    let mut y = panel.y + 16.0;
    let title = format!(
        "These changes will be made on {}@{}:{} as it is shown, nothing more:",
        remote.user, remote.host, remote.port
    );
    y += c.wrapped(sans(13.5, 600), &title, (x, y, 22.0), t.ink, inner) + 8.0;
    for command in &wrapped {
        for line in command {
            let _ = c.text(mono(11.5, 400), line, (x, y, 18.0), t.ink2, None);
            y += 18.0;
        }
        y += 4.0;
    }
    y += 6.0;
    for line in &print_lines {
        let _ = c.text(mono(11.0, 400), line, (x, y, 18.0), t.ink4, None);
        y += 18.0;
    }
    y += 10.0;
    y += c.wrapped(
        sans(12.0, 400),
        "Then lode-receive is checked here, and started as your own service if it is not running.",
        (x, y, 18.0),
        t.ink3,
        inner,
    ) + 10.0;
    let apply = Rect::new(x, y, 190.0, 32.0);
    c.fill(apply, 6.0, t.ink);
    c.text_center(sans(13.0, 500), "Apply on the machine", apply, t.inv);
    c.hit(apply, Action::TlsConfirm);
    let cancel = Rect::new(apply.right() + 10.0, y, 90.0, 32.0);
    c.boxed(cancel, 6.0, t.bg, t.line3);
    c.text_center(sans(13.0, 500), "Cancel", cancel, t.ink);
    c.hit(cancel, Action::TlsCancel);
}

/// A radio choice's ring at (`cx`, `cy`): filled when chosen, an open
/// ring when not, and pale when it cannot be chosen. `inside` is what is
/// behind it.
fn radio(
    c: &mut Frame<'_, '_>,
    t: &Theme,
    (cx, cy): (f32, f32),
    on: bool,
    enabled: bool,
    inside: theme::Color,
) {
    if on {
        c.dot(cx, cy, 7.0, t.blue);
        c.dot(cx, cy, 5.5, inside);
        c.dot(cx, cy, 3.5, t.blue);
    } else if enabled {
        c.dot(cx, cy, 7.0, t.ink4);
        c.dot(cx, cy, 5.6, inside);
    } else {
        c.dot(cx, cy, 7.0, t.line3);
        c.dot(cx, cy, 5.6, t.bg3);
    }
}

/// One choice of a radio group: its box, ring, label and note. Answers
/// nothing; presses go to `action` when it is enabled.
#[allow(clippy::too_many_arguments)]
fn choice(
    app: &App,
    c: &mut Frame<'_, '_>,
    t: &Theme,
    place: Rect,
    (label, note): (&str, &str),
    on: bool,
    enabled: bool,
    action: Action,
) {
    let hover = enabled && !on && app.hover.as_ref() == Some(&action);
    let inside = if on {
        t.bg
    } else if hover {
        t.hover
    } else if enabled {
        t.bg
    } else {
        t.bg3
    };
    if on {
        c.fill(place, 7.0, t.bg);
        c.boxed(place, 7.0, t.blue_bg, t.blue);
    } else if hover {
        c.boxed(place, 7.0, t.hover, t.ink4);
    } else if enabled {
        c.boxed(place, 7.0, t.bg, t.line3);
    } else {
        c.fill(place, 7.0, t.bg3);
        c.dashed_outline(place, 7.0, t.line3);
    }
    let inside = if on { t.blue_bg.over(t.bg) } else { inside };
    let one_line = note.is_empty();
    let cy = if one_line {
        place.y + place.h / 2.0
    } else {
        place.y + 15.0
    };
    radio(c, t, (place.x + 16.0, cy), on, enabled, inside);
    let ink = if enabled { t.ink } else { t.ink5 };
    if one_line {
        let _ = c.text(
            sans(13.0, 500),
            label,
            (place.x + 30.0, place.y, place.h),
            ink,
            Some(place.w - 38.0),
        );
    } else {
        let _ = c.text(
            sans(13.0, 500),
            label,
            (place.x + 30.0, place.y + 5.0, 20.0),
            ink,
            Some(place.w - 38.0),
        );
        let note_ink = if enabled { t.ink4 } else { t.amber_ink };
        let _ = c.wrapped(
            sans(11.0, 400),
            note,
            (place.x + 30.0, place.y + 24.0, 15.0),
            note_ink,
            place.w - 40.0,
        );
    }
    // A choice that cannot be made still answers a press: with why.
    c.hit(place, action);
}

/// A one-line input of the form: its box, its value or placeholder, the
/// caret, and what is wrong with it beside it. Answers its height.
fn input(
    app: &App,
    c: &mut Frame<'_, '_>,
    t: &Theme,
    field: FormField,
    place: Rect,
    placeholder: &str,
    note_w: f32,
) {
    let focused = app.form.focus == Some(field);
    c.boxed(place, 6.0, t.bg, if focused { t.blue } else { t.line3 });
    let value = app.form.value(field);
    let text_x = place.x + 10.0;
    let w = if value.is_empty() {
        let _ = c.text(
            sans(12.5, 400),
            placeholder,
            (text_x, place.y, place.h),
            t.ink5,
            Some(place.w - 20.0),
        );
        0.0
    } else {
        if focused && app.form.all {
            let width = c.width(mono(12.5, 400), value).min(place.w - 20.0);
            c.fill(
                Rect::new(text_x - 2.0, place.y + 6.0, width + 4.0, place.h - 12.0),
                3.0,
                t.blue_bg,
            );
        }
        c.text(
            mono(12.5, 400),
            value,
            (text_x, place.y, place.h),
            t.ink,
            Some(place.w - 20.0),
        )
    };
    if focused {
        c.fill(
            // Before an empty field's placeholder, after a value.
            Rect::new(
                if w > 0.0 {
                    text_x + 1.0 + w
                } else {
                    text_x - 3.0
                },
                place.y + 7.0,
                1.5,
                place.h - 14.0,
            ),
            0.0,
            t.ink,
        );
    }
    c.hit(place, Action::FormFocus(field));
    // What is wrong, as it is typed: an empty field says so only once Add
    // was pressed.
    let show = !value.trim().is_empty() || app.form.error.is_some();
    if show && let Some(why) = app.form.problem(field) {
        let _ = c.wrapped(
            sans(12.0, 400),
            why,
            (place.right() + 12.0, place.y + 7.0, 17.0),
            t.red_ink,
            note_w,
        );
    }
}

/// A small button; answers its right edge.
fn button(
    app: &App,
    c: &mut Frame<'_, '_>,
    t: &Theme,
    place: Rect,
    label: &str,
    action: Action,
) -> f32 {
    let hover = app.hover.as_ref() == Some(&action);
    c.boxed(
        place,
        6.0,
        if hover { t.hover } else { t.bg },
        if hover { t.ink4 } else { t.line3 },
    );
    c.text_center(sans(12.5, 500), label, place, t.ink);
    c.hit(place, action);
    place.right()
}

/// A pasted key as the area shows it: its BEGIN and END lines, and its
/// body as dots, so a key is never on the screen to be read off it.
#[must_use]
pub fn masked_key(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| {
            if line.starts_with("-----") || line.trim().is_empty() {
                line.to_owned()
            } else {
                "•".repeat(line.chars().count().min(64))
            }
        })
        .collect()
}

/// What is kept of the machine the form shows beyond its fields: its
/// pairing, its store and what arrives. Answers the height drawn.
fn details(app: &App, c: &mut Frame<'_, '_>, t: &Theme, place: Rect, label_w: f32) -> f32 {
    let Some(index) = app.selected_remote else {
        return 0.0;
    };
    let Some(remote) = app.remotes.get(index) else {
        return 0.0;
    };
    let line = app.remote_line(index);
    let mut rows: Vec<(&str, String)> = Vec::new();
    if remote.kind == Kind::Tls {
        let pairing = match remote.pairing {
            crate::remote::Pairing::Paired => "paired",
            crate::remote::Pairing::Applying => "being set up on the machine",
            crate::remote::Pairing::Failed => "the set-up stopped before it was done",
            crate::remote::Pairing::None => "not paired yet",
        };
        let ca = remote.ca_fingerprint.as_deref().map_or_else(
            || {
                " · CA fingerprint not recorded (paired before it was kept; Pair again records it)"
                    .to_owned()
            },
            |print| format!(" · CA {print}"),
        );
        rows.push(("Pairing", format!("{pairing}{ca}")));
    }
    rows.push(("Status", line.text.clone()));
    if let Some(error) = &line.error {
        rows.push(("Problem", error.clone()));
    }
    rows.push((
        "Store",
        line.store.as_deref().map_or_else(
            || "none yet: made when its first record arrives".to_owned(),
            crate::app::home_short,
        ),
    ));
    if let Some(id) = &remote.machine_id {
        rows.push(("Machine id", id.clone()));
    }
    let mut y = place.y;
    for (label, value) in rows {
        let _ = c.text(sans(12.5, 400), label, (place.x, y, 20.0), t.ink4, None);
        let ink = if label == "Problem" {
            t.red_ink
        } else {
            t.ink2
        };
        y += c
            .wrapped(
                mono(12.0, 400),
                &value,
                (place.x + label_w, y + 1.0, 18.0),
                ink,
                place.w - label_w,
            )
            .max(20.0)
            + 4.0;
    }
    y + 12.0 - place.y
}

/// The form: Add machine, or a kept machine's settings to change. Answers
/// its bottom.
fn form(app: &App, c: &mut Frame<'_, '_>, t: &Theme, place: Rect) -> f32 {
    let editing = app.form.editing.clone();
    let title = editing.as_deref().map_or_else(
        || "Add machine".to_owned(),
        |name| format!("Machine {name}"),
    );
    let _ = c.text(
        sans(13.0, 600),
        &title,
        (place.x, place.y, 20.0),
        t.ink3,
        None,
    );
    if editing.is_some() {
        let _ = button(
            app,
            c,
            t,
            Rect::new(place.right() - 170.0, place.y - 6.0, 170.0, 30.0),
            "Add another machine",
            Action::FormNew,
        );
    }
    let card_top = place.y + 28.0;
    let x = place.x + 22.0;
    let inner = place.w - 44.0;
    let mut y = card_top + 18.0;
    let label_w = 96.0;
    let field_x = x + label_w;
    let field_w = 440.0_f32.min(inner - label_w - 200.0).max(260.0);
    let note_w = (inner - label_w - field_w - 12.0).max(80.0);
    if editing.is_some() {
        y += details(app, c, t, Rect::new(x, y, inner, 0.0), label_w);
    }
    // How the log comes.
    let receiver = crate::tls::receiver().is_some();
    let kinds = [
        (
            Kind::Ssh,
            "Live over SSH",
            "lode -f on the machine over ssh, kept here",
            true,
        ),
        (
            Kind::Tls,
            "Forwarded over TLS",
            if receiver {
                "lode's upload to lode-receive here; set up over ssh"
            } else {
                "Unavailable: needs lode-receive installed here"
            },
            receiver,
        ),
    ];
    let option_w = ((inner - 12.0) / 2.0).min(320.0);
    for (n, (kind, label, note, enabled)) in kinds.into_iter().enumerate() {
        let option = Rect::new(x + (option_w + 12.0) * n as f32, y, option_w, 50.0);
        choice(
            app,
            c,
            t,
            option,
            (label, note),
            app.form.kind == kind,
            enabled,
            Action::FormKind(kind),
        );
    }
    y += 66.0;
    let host_hint = "e.g. 192.168.9.193 or box.lan";
    let fields = [
        (FormField::Name, "Name", "made from the host"),
        (FormField::Host, "Host", host_hint),
        (FormField::User, "User", "e.g. root"),
        (FormField::Port, "Port", "e.g. 22"),
    ];
    for (field, label, placeholder) in fields {
        let _ = c.text(sans(13.0, 400), label, (x, y, 32.0), t.ink3, None);
        if field == FormField::Name
            && let Some(name) = &editing
        {
            // A kept machine's name ties it to its store and pairing.
            let w = c.text(mono(12.5, 600), name, (field_x, y, 32.0), t.ink, None);
            let _ = c.text(
                sans(11.5, 400),
                "its name stays: it ties the machine to its store and pairing",
                (field_x + w + 14.0, y, 32.0),
                t.ink4,
                None,
            );
            y += 42.0;
            continue;
        }
        input(
            app,
            c,
            t,
            field,
            Rect::new(field_x, y, field_w, 32.0),
            placeholder,
            note_w,
        );
        y += 42.0;
    }
    // The key: ssh's own, a file, or pasted.
    let _ = c.text(sans(13.0, 400), "Key", (x, y, 32.0), t.ink3, None);
    let mut cx = field_x;
    for (key, label, w) in [
        (KeyChoice::Agent, "ssh's own keys and the agent", 236.0),
        (KeyChoice::File, "A key file", 120.0),
        (KeyChoice::Paste, "Paste a key", 128.0),
    ] {
        let option = Rect::new(cx, y, w, 32.0);
        choice(
            app,
            c,
            t,
            option,
            (label, ""),
            app.form.key == key,
            true,
            Action::FormKeyChoice(key),
        );
        cx += w + 8.0;
    }
    y += 42.0;
    match app.form.key {
        KeyChoice::Agent => {
            y += c.wrapped(
                sans(12.0, 400),
                "ssh logs in with the keys it uses in a terminal: ~/.ssh's own and the agent's.",
                (field_x, y, 18.0),
                t.ink4,
                inner - label_w,
            ) + 10.0;
        }
        KeyChoice::File => {
            let label = if app.picking.is_some() {
                "Choosing…"
            } else {
                "Choose…"
            };
            let right = button(
                app,
                c,
                t,
                Rect::new(field_x, y, 96.0, 32.0),
                label,
                Action::FormChooseKey,
            );
            input(
                app,
                c,
                t,
                FormField::Key,
                Rect::new(right + 8.0, y, field_w - 104.0, 32.0),
                "no file chosen",
                note_w,
            );
            y += 42.0;
            if let Some(note) = &app.picker_note {
                y += c.wrapped(
                    sans(12.0, 400),
                    note,
                    (field_x, y, 18.0),
                    t.ink3,
                    inner - label_w,
                ) + 4.0;
            }
            if let Some(files) = &app.key_files {
                if files.is_empty() {
                    let _ = c.text(
                        sans(12.0, 400),
                        "~/.ssh has no private keys.",
                        (field_x, y, 22.0),
                        t.ink4,
                        None,
                    );
                    y += 26.0;
                }
                for (index, file) in files.iter().enumerate() {
                    let row = Rect::new(field_x, y, field_w, 26.0);
                    let action = Action::FormPickKey(index);
                    if app.hover.as_ref() == Some(&action) {
                        c.fill(row, 5.0, t.hover);
                    }
                    let _ = c.text(
                        mono(12.0, 400),
                        &crate::app::home_short(file),
                        (row.x + 10.0, row.y, row.h),
                        t.ink2,
                        Some(row.w - 20.0),
                    );
                    c.hit(row, action);
                    y += 26.0;
                }
                y += 8.0;
            }
        }
        KeyChoice::Paste => {
            let area = Rect::new(field_x, y, field_w, 7.0 * 16.0 + 16.0);
            let focused = app.form.focus == Some(FormField::Paste);
            c.boxed(area, 6.0, t.bg, if focused { t.blue } else { t.line3 });
            let value = app.form.value(FormField::Paste);
            if value.is_empty() {
                let said = app.form.kept_key.as_deref().map_or_else(
                    || "Paste the private key here (Ctrl+V, or the Paste button). Its body is shown as dots.".to_owned(),
                    |kept| format!("A pasted key is kept for it in {}. Paste another here to replace it.", crate::app::home_short(std::path::Path::new(kept))),
                );
                let _ = c.wrapped(
                    sans(12.5, 400),
                    &said,
                    (area.x + 10.0, area.y + 8.0, 16.0),
                    t.ink5,
                    area.w - 20.0,
                );
            } else {
                let lines = masked_key(value);
                let shown = lines.len().min(7);
                let mut last_w = 0.0;
                for (n, line) in lines.iter().take(7).enumerate() {
                    let line = if n == 6 && lines.len() > 7 {
                        format!("… {} lines in all", lines.len())
                    } else {
                        line.clone()
                    };
                    last_w = c.text(
                        mono(11.0, 400),
                        &line,
                        (area.x + 10.0, area.y + 8.0 + 16.0 * n as f32, 16.0),
                        t.ink2,
                        Some(area.w - 20.0),
                    );
                }
                if focused {
                    let at_end = value.ends_with('\n');
                    let row = if at_end {
                        shown.min(6)
                    } else {
                        shown.saturating_sub(1)
                    };
                    let caret_x = if at_end { 0.0 } else { last_w };
                    c.fill(
                        Rect::new(
                            area.x + 11.0 + caret_x,
                            area.y + 10.0 + 16.0 * row as f32,
                            1.5,
                            12.0,
                        ),
                        0.0,
                        t.ink,
                    );
                }
            }
            c.hit(area, Action::FormFocus(FormField::Paste));
            let right = area.right() + 12.0;
            let _ = button(
                app,
                c,
                t,
                Rect::new(right, y, 80.0, 30.0),
                "Paste",
                Action::FormPaste,
            );
            let show = !value.trim().is_empty() || app.form.error.is_some();
            let mut note_y = y + 40.0;
            if show && let Some(why) = app.form.problem(FormField::Paste) {
                note_y += c.wrapped(
                    sans(12.0, 400),
                    why,
                    (right, note_y, 17.0),
                    t.red_ink,
                    note_w,
                );
            }
            let name = app.form.value(FormField::Name).trim();
            let kept = format!(
                "Kept in ~/.config/ferrix/lode/keys/{}, mode 0600. A passphrase is never kept.",
                if name.is_empty() { "<name>" } else { name }
            );
            let _ = c.wrapped(
                sans(11.5, 400),
                &kept,
                (right, note_y + 4.0, 16.0),
                t.ink4,
                note_w,
            );
            y = area.bottom() + 12.0;
        }
    }
    // TLS: how the machine reaches this one.
    if app.form.kind == Kind::Tls {
        let _ = c.text(sans(13.0, 400), "Server", (x, y, 32.0), t.ink3, None);
        input(
            app,
            c,
            t,
            FormField::Server,
            Rect::new(field_x, y, field_w, 32.0),
            "this machine's address, as the machine reaches it",
            note_w,
        );
        y += 38.0;
        let mut cx = field_x;
        let said = c.text(
            sans(11.5, 400),
            "How the machine reaches this one. Found here:",
            (cx, y, 24.0),
            t.ink4,
            None,
        );
        cx += said + 8.0;
        for (index, choice) in app.reach.iter().enumerate() {
            let w = c.width(mono(11.5, 400), choice) + 18.0;
            let chip = Rect::new(cx, y + 1.0, w, 22.0);
            let action = Action::FormServer(index);
            let on = app.form.value(FormField::Server) == choice;
            let hover = app.hover.as_ref() == Some(&action);
            c.boxed(
                chip,
                11.0,
                if on {
                    t.blue_bg.over(t.bg)
                } else if hover {
                    t.hover
                } else {
                    t.bg
                },
                if on { t.blue } else { t.line3 },
            );
            c.text_center(mono(11.5, 400), choice, chip, t.ink2);
            c.hit(chip, action);
            cx += w + 6.0;
        }
        y += 34.0;
    }
    y += 4.0;
    y += c.wrapped(
        sans(12.0, 400),
        "Keys only: the viewer never asks for a password and keeps none. ssh runs with BatchMode, and with IdentitiesOnly when a key is named.",
        (x, y, 18.0),
        t.ink4,
        inner,
    );
    y += 14.0;
    let add = Rect::new(x, y, 150.0, 34.0);
    c.fill(add, 6.0, t.ink);
    let word = match (&editing, app.form.kind) {
        (Some(_), _) => "Save",
        (None, Kind::Tls) => "Pair…",
        (None, Kind::Ssh) => "Add and follow",
    };
    c.text_center(sans(13.0, 500), word, add, t.inv);
    c.hit(add, Action::FormAdd);
    let mut right = add.right();
    if editing.is_some() && app.form.kind == Kind::Tls {
        right = button(
            app,
            c,
            t,
            Rect::new(add.right() + 10.0, y, 120.0, 34.0),
            "Pair again…",
            Action::FormPairAgain,
        );
    }
    let mut py = add.bottom() + 12.0;
    if let Some(error) = &app.form.error {
        let h = c.wrapped(
            sans(12.5, 400),
            error,
            (right + 16.0, add.y + 8.0, 18.0),
            t.red_ink,
            inner - (right - x) - 20.0,
        );
        py = py.max(add.y + 8.0 + h + 10.0);
    }
    let progress = app
        .tls_progress
        .lock()
        .map(|lines| lines.clone())
        .unwrap_or_default();
    for line in progress.iter().rev().take(4).rev() {
        let ink = if line.starts_with("stopped") {
            t.red_ink
        } else {
            t.ink3
        };
        py += c.wrapped(mono(12.0, 400), line, (x, py, 18.0), ink, inner);
    }
    let card = Rect::new(place.x, card_top, place.w, py + 10.0 - card_top);
    c.outline(card, 10.0, 1.0, t.line);
    if let Some((remote, plan)) = &app.tls_plan {
        confirm(
            c,
            t,
            Rect::new(card.x, card.y + 20.0, card.w, card.h),
            remote,
            plan,
        );
    }
    card.bottom()
}

#[cfg(test)]
mod tests {
    use super::masked_key;

    #[test]
    fn a_pasted_keys_body_is_never_drawn() {
        let lines = masked_key(
            "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1r\n-----END OPENSSH PRIVATE KEY-----\n",
        );
        assert_eq!(lines[0], "-----BEGIN OPENSSH PRIVATE KEY-----");
        assert_eq!(lines[1], "••••••••••••");
        assert!(!lines.concat().contains("b3Bl"));
    }
}
