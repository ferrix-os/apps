//! Machines, handed to the tree: the followed machines' rows, the form's
//! fields and what is wrong with them, the details of a kept machine, and
//! the TLS set-up to confirm. Words that wrap are broken here, at the
//! widths the markup gives them.

use std::rc::Rc;

use super::{DetailLine, MachinesPageSignal, ProgressLine, RemoteRow, View, paint};
use crate::app::{Action, App, FormField, KeyChoice};
use crate::remote::{Kind, Pairing};

/// `value` as the markup's string.
fn s(value: impl AsRef<str>) -> Rc<str> {
    Rc::from(value.as_ref())
}

/// `text` broken at `width` in `font`.
fn wrap(ui: &mut tole::Ui, font: &tole::Font, text: &str, width: f32) -> Vec<Rc<str>> {
    super::wrap_with(text, width, |line| ui.text_width(font, line))
        .into_iter()
        .map(s)
        .collect()
}

/// The page's width for its form, as the markup has it.
fn form_width(view: &mut View) -> f32 {
    let page = view.machines.base.common.width.get(&mut view.ui);
    (page - 72.0).min(980.0)
}

/// The fields in their markup's order: the inputs under the page, in tree
/// order, are these.
const ORDER: [FormField; 7] = FormField::ALL;

/// Hand `view` Machines from `app`.
pub(super) fn show(view: &mut View, app: &App) {
    let w = form_width(view);
    let inner = w - 44.0;
    let field_w = 440.0_f32.min(inner - 96.0 - 200.0).max(260.0);
    let note_w = (inner - 96.0 - field_w - 12.0).max(80.0);
    let page = view.machines;
    let ui = &mut view.ui;
    let root = app.machines_root.as_ref().map_or_else(
        || "~/.local/share/ferrix/lode-receive".to_owned(),
        |root| root.display().to_string(),
    );
    let intro = wrap(
        ui,
        &paint::sans(12.5, 400),
        &format!(
            "Each machine's log is kept in a store of its own under {root}, so it stays when the link drops."
        ),
        w,
    );
    page.intro.set(ui, intro);
    page.remotes.set(
        ui,
        app.remotes
            .iter()
            .enumerate()
            .map(|(index, remote)| {
                let state = app.remote_line(index);
                RemoteRow {
                    name: s(&remote.name),
                    at: s(format!(
                        "{} {}@{}:{}",
                        remote.kind.word(),
                        remote.user,
                        remote.host,
                        remote.port
                    )),
                    state: s(&state.text),
                    error: s(state.error.as_deref().unwrap_or("")),
                    health: if state.error.is_some() {
                        2
                    } else if state.up {
                        0
                    } else {
                        1
                    },
                    selected: app.selected_remote == Some(index),
                }
            })
            .collect(),
    );
    let editing = app.form.editing.clone().unwrap_or_default();
    page.editing.set(ui, s(&editing));
    let details = details(ui, app, inner - 96.0);
    page.details.set(ui, details);
    page.tls.set(ui, app.form.kind == Kind::Tls);
    let receiver = crate::tls::receiver().is_some();
    page.receiver.set(ui, receiver);
    let option_w = ((inner - 12.0) / 2.0).min(320.0);
    let note = paint::sans(11.0, 400);
    let ssh_note = wrap(
        ui,
        &note,
        "lode -f on the machine over ssh, kept here",
        option_w - 40.0,
    );
    page.ssh_note.set(ui, ssh_note);
    let tls_note = wrap(
        ui,
        &note,
        if receiver {
            "lode's upload to lode-receive here; set up over ssh"
        } else {
            "Unavailable: needs lode-receive installed here"
        },
        option_w - 40.0,
    );
    page.tls_note.set(ui, tls_note);
    page.key.set(
        ui,
        match app.form.key {
            KeyChoice::Agent => 0,
            KeyChoice::File => 1,
            KeyChoice::Paste => 2,
        },
    );
    // The fields' values: what was typed is the state's already.
    let values = [
        (page.name, FormField::Name),
        (page.host, FormField::Host),
        (page.user, FormField::User),
        (page.port, FormField::Port),
        (page.key_file, FormField::Key),
        (page.server, FormField::Server),
        (page.pasted, FormField::Paste),
    ];
    for (prop, field) in values {
        let value = app.form.value(field);
        if &*prop.get(ui) != value {
            prop.set(ui, s(value));
        }
    }
    // What is wrong, as it is typed: an empty field says so only once Add
    // was pressed.
    let problem_font = paint::sans(12.0, 400);
    let problems = [
        (page.name_problem, FormField::Name),
        (page.host_problem, FormField::Host),
        (page.user_problem, FormField::User),
        (page.port_problem, FormField::Port),
        (page.key_problem, FormField::Key),
        (page.server_problem, FormField::Server),
        (page.paste_problem, FormField::Paste),
    ];
    for (prop, field) in problems {
        let value = app.form.value(field);
        let show = !value.trim().is_empty() || app.form.error.is_some();
        let lines = match app.form.problem(field) {
            Some(why) if show => wrap(ui, &problem_font, why, note_w),
            _ => Vec::new(),
        };
        prop.set(ui, lines);
    }
    let name = app.form.value(FormField::Name).trim();
    let kept = format!(
        "Kept in ~/.config/ferrix/lode/keys/{}, mode 0600. A passphrase is never kept.",
        if name.is_empty() { "<name>" } else { name }
    );
    let kept = wrap(ui, &paint::sans(11.5, 400), &kept, note_w);
    page.paste_kept.set(ui, kept);
    let placeholder = app.form.kept_key.as_deref().map_or_else(
        || {
            "Paste the private key here (Ctrl+V, or the Paste button). Its body is shown as dots."
                .to_owned()
        },
        |kept| {
            format!(
                "A pasted key is kept for it in {}. Paste another here to replace it.",
                crate::app::home_short(std::path::Path::new(kept))
            )
        },
    );
    let placeholder = wrap(ui, &paint::sans(12.5, 400), &placeholder, field_w - 20.0);
    page.paste_placeholder.set(ui, placeholder);
    let agent = wrap(
        ui,
        &paint::sans(12.0, 400),
        "ssh logs in with the keys it uses in a terminal: ~/.ssh's own and the agent's.",
        inner - 96.0,
    );
    page.agent_note.set(ui, agent);
    page.reach.set(ui, app.reach.iter().map(s).collect());
    page.reach_on.set(
        ui,
        app.reach
            .iter()
            .position(|choice| choice == app.form.value(FormField::Server))
            .map_or(-1, |at| i32::try_from(at).unwrap_or(-1)),
    );
    let keys_only = wrap(
        ui,
        &paint::sans(12.0, 400),
        "Keys only: the viewer never asks for a password and keeps none. ssh runs with BatchMode, and with IdentitiesOnly when a key is named.",
        inner,
    );
    page.keys_only.set(ui, keys_only);
    let word = match (&app.form.editing, app.form.kind) {
        (Some(_), _) => "Save",
        (None, Kind::Tls) => "Pair…",
        (None, Kind::Ssh) => "Add and follow",
    };
    page.add_word.set(ui, s(word));
    let right = if app.form.editing.is_some() && app.form.kind == Kind::Tls {
        280.0
    } else {
        150.0
    };
    page.error_x.set(ui, right);
    let error = app.form.error.as_deref().map_or_else(Vec::new, |error| {
        wrap(ui, &paint::sans(12.5, 400), error, inner - right - 20.0)
    });
    page.error.set(ui, error);
    let progress = app
        .tls_progress
        .lock()
        .map(|lines| lines.clone())
        .unwrap_or_default();
    let mono = paint::mono(12.0, 400);
    let mut lines = Vec::new();
    for line in progress.iter().rev().take(4).rev() {
        for part in wrap(ui, &mono, line, inner) {
            lines.push(ProgressLine {
                text: part,
                stopped: line.starts_with("stopped"),
            });
        }
    }
    page.progress.set(ui, lines);
    confirm(view, app, w);
    focus(view, app);
}

/// What is kept of the machine the form shows beyond its fields: its
/// pairing, its store and what arrives.
fn details(ui: &mut tole::Ui, app: &App, width: f32) -> Vec<DetailLine> {
    let Some(index) = app.selected_remote else {
        return Vec::new();
    };
    let Some(remote) = app.remotes.get(index) else {
        return Vec::new();
    };
    let line = app.remote_line(index);
    let mut rows: Vec<(&str, String)> = Vec::new();
    if remote.kind == Kind::Tls {
        let pairing = match remote.pairing {
            Pairing::Paired => "paired",
            Pairing::Applying => "being set up on the machine",
            Pairing::Failed => "the set-up stopped before it was done",
            Pairing::None => "not paired yet",
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
    let font = paint::mono(12.0, 400);
    let mut out = Vec::new();
    for (label, value) in rows {
        let lines = wrap(ui, &font, &value, width);
        let count = lines.len();
        let total = (18.0 * count as f32).max(20.0) + 4.0;
        for (n, text) in lines.into_iter().enumerate() {
            let last = n + 1 == count;
            out.push(DetailLine {
                label: s(if n == 0 { label } else { "" }),
                text,
                problem: label == "Problem",
                height: if last {
                    total - 18.0 * (count - 1) as f32
                } else {
                    18.0
                },
            });
        }
    }
    out
}

/// The TLS set-up shown, to confirm.
fn confirm(view: &mut View, app: &App, w: f32) {
    let page = view.machines;
    let ui = &mut view.ui;
    page.confirming.set(ui, app.tls_plan.is_some());
    let Some((remote, plan)) = &app.tls_plan else {
        return;
    };
    let inner = w - 64.0;
    let title = format!(
        "These changes will be made on {}@{}:{} as it is shown, nothing more:",
        remote.user, remote.host, remote.port
    );
    let title = wrap(ui, &paint::sans(13.5, 600), &title, inner);
    page.confirm_title.set(ui, title);
    let font = paint::mono(11.5, 400);
    let mut commands = Vec::new();
    for command in crate::tls::remote_commands(plan) {
        let lines = wrap(ui, &font, &command, inner);
        let count = lines.len();
        for (n, text) in lines.into_iter().enumerate() {
            commands.push(DetailLine {
                label: s(""),
                text,
                problem: false,
                height: if n + 1 == count { 22.0 } else { 18.0 },
            });
        }
    }
    page.commands.set(ui, commands);
    let prints = format!(
        "receiver {} · CA {} · server {}",
        plan.listen, plan.ca_fingerprint, plan.server_fingerprint
    );
    let prints = wrap(ui, &paint::mono(11.0, 400), &prints, inner);
    page.prints.set(ui, prints);
    let then = wrap(
        ui,
        &paint::sans(12.0, 400),
        "Then lode-receive is checked here, and started as your own service if it is not running.",
        inner,
    );
    page.then.set(ui, then);
}

/// The text inputs under the page, in tree order.
fn inputs(ui: &tole::Ui, root: tole::ItemId) -> Vec<tole::ItemId> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(item) = stack.pop() {
        if ui.text_input(item).is_some() {
            out.push(item);
        }
        stack.extend(ui.children(item).iter().rev());
    }
    out
}

/// Give the keys to the field the state says is typed into, when that
/// changed.
fn focus(view: &mut View, app: &App) {
    use tole::Element as _;
    let wanted = app.form.focus;
    if view.shown_focus == wanted {
        return;
    }
    view.shown_focus = wanted;
    let Some(field) = wanted else {
        return;
    };
    let items = inputs(&view.ui, view.machines.id());
    if let Some(&item) = ORDER
        .iter()
        .position(|each| *each == field)
        .and_then(|at| items.get(at))
    {
        view.ui.force_active_focus(item);
    }
}

/// The field a markup index names.
fn field(index: i32) -> FormField {
    ORDER
        .get(usize::try_from(index).unwrap_or(0))
        .copied()
        .unwrap_or(FormField::Name)
}

/// What a press or an edit on Machines asks.
pub(super) fn action(app: &App, signal: MachinesPageSignal) -> Vec<Action> {
    let index = |i: i32| usize::try_from(i).unwrap_or(usize::MAX);
    let action = match signal {
        MachinesPageSignal::OpenRemote(i) => Action::OpenRemote(index(i)),
        MachinesPageSignal::RemoveRemote(i) => Action::RemoveRemote(index(i)),
        MachinesPageSignal::Select(i) => Action::SelectRemote(index(i)),
        MachinesPageSignal::Another => Action::FormNew,
        MachinesPageSignal::Kind(tls) => Action::FormKind(if tls { Kind::Tls } else { Kind::Ssh }),
        MachinesPageSignal::KeyChoice(choice) => Action::FormKeyChoice(match choice {
            0 => KeyChoice::Agent,
            1 => KeyChoice::File,
            _ => KeyChoice::Paste,
        }),
        MachinesPageSignal::FieldEdited(at, text) => Action::FormText(field(at), text.to_string()),
        MachinesPageSignal::FieldPressed(at) => Action::FormFocus(field(at)),
        MachinesPageSignal::ChooseKey => Action::FormChooseKey,
        MachinesPageSignal::KeyChosen(path) => Action::FormText(
            FormField::Key,
            crate::app::home_short(std::path::Path::new(&*path)),
        ),
        MachinesPageSignal::Paste => Action::FormPaste,
        MachinesPageSignal::ServerChoice(i) => Action::FormServer(index(i)),
        MachinesPageSignal::Add => Action::FormAdd,
        MachinesPageSignal::PairAgain => Action::FormPairAgain,
        MachinesPageSignal::Confirm => Action::TlsConfirm,
        MachinesPageSignal::Cancel => Action::TlsCancel,
    };
    let _ = app;
    vec![action]
}
