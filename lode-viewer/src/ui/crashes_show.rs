//! The Crashes page, handed to the tree: the filters, the groups, a
//! group's crashes, and the crash shown -- its fields, its screenshot and
//! the log around it.

use std::path::PathBuf;
use std::rc::Rc;

use super::{AroundRow, CrashGroupRow, CrashRow, CrashesPageSignal, View};
use crate::app::{Action, App, Origin, clock, clock_ms};
use crate::crashes::CrashKind;
use crate::lines::group;

/// `value` as the markup's string.
fn s(value: impl AsRef<str>) -> Rc<str> {
    Rc::from(value.as_ref())
}

/// The screenshot last read, by its file: read once, drawn as often as
/// the page is.
#[derive(Debug, Default)]
pub(super) struct Capture {
    /// The file, and its picture when it could be read.
    shown: Option<(PathBuf, Option<tole::Image>)>,
}

/// Where lode's captures may be: beside the store, or the export.
fn capture_dirs(app: &App) -> Vec<PathBuf> {
    match &app.origin {
        Origin::Store(store) => store.dirs().to_vec(),
        Origin::File(path) => path
            .parent()
            .map(|dir| vec![dir.to_path_buf()])
            .unwrap_or_default(),
    }
}

/// Hand `view` the Crashes page from `app`.
pub(super) fn show(view: &mut View, app: &App) {
    let page = view.crashes;
    let (groups, crashes) = app.crash_groups();
    let all = app.crash_list();
    let (programs, signals) = crate::crashes::choices(&all);
    let ui = &mut view.ui;
    page.all_boots.set(ui, app.crash_all);
    let mut list = vec![s("Every program")];
    list.extend(programs.iter().map(s));
    page.programs.set(ui, list);
    page.program_on.set(
        ui,
        app.crash_program
            .as_ref()
            .and_then(|wanted| programs.iter().position(|p| p == wanted))
            .map_or(0, |at| i32::try_from(at + 1).unwrap_or(0)),
    );
    let mut list = vec![s("Every signal")];
    list.extend(signals.iter().map(s));
    page.signals.set(ui, list);
    page.signal_on.set(
        ui,
        app.crash_signal
            .as_ref()
            .and_then(|wanted| signals.iter().position(|p| p == wanted))
            .map_or(0, |at| i32::try_from(at + 1).unwrap_or(0)),
    );
    let shown: usize = groups.iter().map(|g| g.crashes.len()).sum();
    page.summary.set(
        ui,
        s(format!(
            "{} crashes in {} groups{}",
            group(shown as u64),
            group(groups.len() as u64),
            if app.crash_all {
                ", every boot"
            } else {
                ", this boot"
            }
        )),
    );
    page.empty.set(
        ui,
        s(if all.is_empty() {
            if app.crash_all {
                "Nothing crashed in any boot kept."
            } else {
                "Nothing crashed in this boot."
            }
        } else {
            "No crash is of that program and signal."
        }),
    );
    let zone = app.zone();
    let selected = app.crash_group.clone();
    page.groups.set(
        ui,
        groups
            .iter()
            .map(|g| CrashGroupRow {
                program: s(&g.program),
                detail: s(format!(
                    "{} · {} · last {}",
                    g.address,
                    g.signal,
                    clock(g.last, zone)
                )),
                count: s(format!("{}×", group(g.crashes.len() as u64))),
                app_error: g.kind == CrashKind::AppError,
                selected: selected.as_ref().is_some_and(|(p, a, sig)| {
                    *p == g.program && *a == g.address && *sig == g.signal
                }),
            })
            .collect(),
    );
    let current = selected.as_ref().and_then(|(p, a, sig)| {
        groups
            .iter()
            .find(|g| g.program == *p && g.address == *a && g.signal == *sig)
    });
    page.group_title.set(
        ui,
        s(current.map_or_else(String::new, |g| {
            format!(
                "{} · {} · {}×",
                g.program,
                g.signal,
                group(g.crashes.len() as u64)
            )
        })),
    );
    page.crashes.set(
        ui,
        current.map_or_else(Vec::new, |g| {
            g.crashes
                .iter()
                .enumerate()
                .filter_map(|(at, &index)| {
                    let crash = crashes.get(index)?;
                    Some(CrashRow {
                        time: s(clock_ms(crash.time, zone)),
                        detail: s(format!(
                            "{}{}{}",
                            crash
                                .pid
                                .as_deref()
                                .map_or_else(String::new, |pid| format!("pid {pid} · ")),
                            crash.address,
                            crash
                                .trap
                                .as_deref()
                                .map_or_else(String::new, |trap| format!(" · {trap}"))
                        )),
                        selected: at == app.crash_pick,
                    })
                })
                .collect()
        }),
    );
    let shown = current.and_then(|_| app.crash_shown());
    page.has_crash.set(ui, shown.is_some());
    let Some(crash) = shown else {
        return;
    };
    let rows = super::show::field_rows(ui, app, crash.index);
    page.fields.set(ui, rows);
    // The log around it: 30 s each side, at most 200 lines.
    let from = crash.time.saturating_sub(30_000_000);
    let to = crash.time + 30_000_000;
    let around: Vec<AroundRow> = app
        .log
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.time() >= from && entry.time() <= to)
        .take(200)
        .map(|(index, entry)| AroundRow {
            level: i32::from(entry.row.level),
            time: s(clock_ms(entry.time(), zone)),
            source: s(app.log.sources.name(entry.row.source)),
            text: s(entry.message().replace('\n', " \\n ")),
            here: index == crash.index,
        })
        .collect();
    page.around.set(ui, around);
    // Its screenshot, when lode kept one.
    let file = crate::crashes::capture_of(&app.log, crash.seqnum, &capture_dirs(app));
    let image = match (&file, &view.capture.shown) {
        (Some(path), Some((kept, image))) if kept == path => image.clone(),
        (Some(path), _) => {
            let image = crate::crashes::read_capture(path)
                .and_then(|(w, h, rgba)| tole::Image::from_rgba(w, h, &rgba));
            view.capture.shown = Some((path.clone(), image.clone()));
            image
        }
        (None, _) => None,
    };
    let ui = &mut view.ui;
    page.capture_note.set(
        ui,
        s(match (&file, &image) {
            (None, _) => "No screenshot was kept of it.",
            (Some(_), None) => "Its screenshot could not be read.",
            (Some(_), Some(_)) => "",
        }),
    );
    page.capture.set(
        ui,
        tole::CanvasPainter::new(move |p| {
            let Some(image) = &image else {
                return;
            };
            let (w, h) = p.size();
            let (iw, ih) = image.size();
            let fit = ((w - 16.0) / iw as f32).min((h - 16.0) / ih as f32);
            let (dw, dh) = (iw as f32 * fit, ih as f32 * fit);
            p.image(
                tole::Rect::new((w - dw) / 2.0, (h - dh) / 2.0, dw, dh),
                image,
            );
        }),
    );
}

/// What a press on the Crashes page asks.
pub(super) fn action(app: &App, signal: CrashesPageSignal) -> Vec<Action> {
    let at = |i: i32| usize::try_from(i).unwrap_or(0);
    let action = match signal {
        CrashesPageSignal::Scope(all) => Action::CrashScope(all),
        CrashesPageSignal::Program(i) => Action::CrashProgram(at(i)),
        CrashesPageSignal::Kind(i) => Action::CrashSignal(at(i)),
        CrashesPageSignal::Group(i) => Action::CrashGroup(at(i)),
        CrashesPageSignal::Crash(i) => Action::CrashPick(at(i)),
        CrashesPageSignal::OpenInLogs => Action::CrashInLogs,
        CrashesPageSignal::CopyField(i) => {
            return match app.crash_shown() {
                Some(crash) => vec![Action::Select(crash.index), Action::CopyField(at(i))],
                None => Vec::new(),
            };
        }
    };
    vec![action]
}
