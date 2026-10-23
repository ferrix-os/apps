//! Machines: the machines the viewer follows itself, each with its link's
//! state, and the form that adds one.

use super::{Frame, MARGIN};
use crate::app::{Action, App, FormField};
use crate::draw::{Rect, mono, sans};
use crate::remote::Kind;
use crate::theme::{self, Theme};

/// Draw the page into `area`.
pub(super) fn paint(app: &App, c: &mut Frame<'_, '_>, t: &Theme, area: Rect) {
    let x = area.x + MARGIN;
    let w = (area.w - 2.0 * MARGIN).min(980.0);
    let mut y = area.y + 28.0;
    let _ = c.text(sans(18.0, 600), "Machines", (x, y, 30.0), t.ink, None);
    y += 48.0;
    let root = app.machines_root.as_ref().map_or_else(
        || "~/.local/share/ferrix/lode-receive".to_owned(),
        |root| root.display().to_string(),
    );
    let _ = c.text(
        sans(12.5, 400),
        &format!("Each machine's log is kept in a store of its own under {root}, so it stays when the link drops."),
        (x, y, 18.0),
        t.ink3,
        Some(w),
    );
    y += 32.0;
    y = followed(app, c, t, Rect::new(x, y, w, 0.0));
    y += 28.0;
    form(app, c, t, Rect::new(x, y, w, 0.0));
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
        if index > 0 {
            c.fill(
                Rect::new(row.x + 1.0, row.y, row.w - 2.0, 1.0),
                0.0,
                t.line2,
            );
        }
        let state = app
            .links
            .get(index)
            .and_then(|link| link.as_ref())
            .map(crate::remote::Link::now)
            .unwrap_or_default();
        let dot = if state.up {
            theme::GREEN
        } else if state.error.is_some() {
            theme::RED
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
        let seen = state.last_seen.map_or_else(
            || "nothing yet".to_owned(),
            |at| format!("last record {}", crate::app::clock(at * 1_000_000, 0)),
        );
        let line = format!(
            "{} · {} records this run · {seen} UTC",
            state.state, state.records
        );
        let _ = c.text(
            sans(12.0, 400),
            &line,
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
    let h = 150.0 + 20.0 * commands.len() as f32;
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
    let _ = c.text(
        sans(13.5, 600),
        &title,
        (x, y, 22.0),
        t.ink,
        Some(panel.w - 40.0),
    );
    y += 30.0;
    for command in &commands {
        let _ = c.text(
            mono(11.5, 400),
            command,
            (x, y, 18.0),
            t.ink2,
            Some(panel.w - 40.0),
        );
        y += 20.0;
    }
    y += 6.0;
    let prints = format!(
        "receiver {} · CA {} · server {}",
        plan.listen, plan.ca_fingerprint, plan.server_fingerprint
    );
    let _ = c.text(
        mono(11.0, 400),
        &prints,
        (x, y, 18.0),
        t.ink4,
        Some(panel.w - 40.0),
    );
    y += 28.0;
    let _ = c.text(
        sans(12.0, 400),
        "Then lode-receive is checked here, and started as your own service if it is not running.",
        (x, y, 18.0),
        t.ink3,
        Some(panel.w - 40.0),
    );
    y += 28.0;
    let apply = Rect::new(x, y, 190.0, 32.0);
    c.fill(apply, 6.0, t.ink);
    c.text_center(sans(13.0, 500), "Apply on the machine", apply, t.inv);
    c.hit(apply, Action::TlsConfirm);
    let cancel = Rect::new(apply.right() + 10.0, y, 90.0, 32.0);
    c.boxed(cancel, 6.0, t.bg, t.line3);
    c.text_center(sans(13.0, 500), "Cancel", cancel, t.ink);
    c.hit(cancel, Action::TlsCancel);
}

/// The Add machine form.
fn form(app: &App, c: &mut Frame<'_, '_>, t: &Theme, place: Rect) {
    let _ = c.text(
        sans(13.0, 600),
        "Add machine",
        (place.x, place.y, 20.0),
        t.ink3,
        None,
    );
    let card = Rect::new(place.x, place.y + 28.0, place.w, 440.0);
    c.boxed(card, 10.0, t.bg, t.line);
    let x = card.x + 22.0;
    let mut y = card.y + 18.0;
    // How the log comes.
    for (n, (kind, label, note)) in [
        (
            Kind::Ssh,
            "Live over SSH",
            "lode -f on the machine over ssh, kept here",
        ),
        (
            Kind::Tls,
            "Forwarded over TLS",
            if crate::tls::receiver().is_some() {
                "lode's upload to lode-receive here; set up over ssh"
            } else {
                "needs lode-receive installed here"
            },
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let option = Rect::new(x + 300.0 * n as f32, y, 290.0, 44.0);
        let on = app.form.kind == kind;
        let enabled = kind == Kind::Ssh || crate::tls::receiver().is_some();
        if on {
            c.boxed(option, 7.0, t.blue_bg, t.blue);
        } else {
            c.boxed(option, 7.0, t.bg, t.line3);
        }
        c.dot(
            option.x + 16.0,
            option.y + 16.0,
            5.0,
            if on { t.blue } else { t.line3 },
        );
        let ink = if enabled { t.ink } else { t.ink5 };
        let _ = c.text(
            sans(13.0, 500),
            label,
            (option.x + 30.0, option.y + 5.0, 22.0),
            ink,
            None,
        );
        let _ = c.text(
            sans(11.0, 400),
            note,
            (option.x + 30.0, option.y + 23.0, 16.0),
            t.ink4,
            Some(250.0),
        );
        if enabled {
            c.hit(option, Action::FormKind(kind));
        }
    }
    y += 62.0;
    let fields = [
        (FormField::Name, "Name", "n150"),
        (FormField::Host, "Host", "192.168.9.193 or box.lan"),
        (FormField::User, "User", "root"),
        (FormField::Port, "Port", "22"),
        (
            FormField::Key,
            "Key",
            "ssh's own keys and the agent's; or ~/.ssh/id_ed25519",
        ),
    ];
    for (field, label, placeholder) in fields {
        let _ = c.text(sans(13.0, 400), label, (x, y, 32.0), t.ink3, None);
        let input = Rect::new(x + 90.0, y, 460.0, 32.0);
        let focused = app.form.focus == Some(field);
        c.boxed(input, 6.0, t.bg, if focused { t.blue } else { t.line3 });
        let value = app.form.value(field);
        let w = if value.is_empty() {
            let _ = c.text(
                mono(12.5, 400),
                placeholder,
                (input.x + 10.0, input.y, input.h),
                t.ink5,
                Some(input.w - 20.0),
            );
            0.0
        } else {
            c.text(
                mono(12.5, 400),
                value,
                (input.x + 10.0, input.y, input.h),
                t.ink,
                Some(input.w - 20.0),
            )
        };
        if focused {
            c.fill(
                Rect::new(input.x + 11.0 + w, input.y + 7.0, 1.5, 18.0),
                0.0,
                t.ink,
            );
        }
        c.hit(input, Action::FormFocus(field));
        y += 42.0;
    }
    let _ = c.text(
        sans(12.0, 400),
        "Keys only: the viewer never asks for a password and keeps none. ssh runs with BatchMode, and with IdentitiesOnly when a key is named.",
        (x, y + 4.0, 18.0),
        t.ink4,
        Some(card.w - 44.0),
    );
    y += 32.0;
    let add = Rect::new(x, y, 150.0, 34.0);
    c.fill(add, 6.0, t.ink);
    let word = if app.form.kind == Kind::Tls {
        "Pair…"
    } else {
        "Add and follow"
    };
    c.text_center(sans(13.0, 500), word, add, t.inv);
    c.hit(add, Action::FormAdd);
    let progress = app
        .tls_progress
        .lock()
        .map(|lines| lines.clone())
        .unwrap_or_default();
    let mut py = add.bottom() + 12.0;
    for line in progress.iter().rev().take(4).rev() {
        let ink = if line.starts_with("stopped") {
            t.red_ink
        } else {
            t.ink3
        };
        let _ = c.text(
            mono(12.0, 400),
            line,
            (x, py, 18.0),
            ink,
            Some(card.w - 44.0),
        );
        py += 18.0;
    }
    if let Some((remote, plan)) = &app.tls_plan {
        confirm(
            c,
            t,
            Rect::new(card.x, card.y + 20.0, card.w, card.h),
            remote,
            plan,
        );
    }
    if let Some(error) = &app.form.error {
        let _ = c.text(
            sans(12.5, 400),
            error,
            (add.right() + 16.0, add.y, add.h),
            t.red_ink,
            Some(card.w - 220.0),
        );
    }
}
