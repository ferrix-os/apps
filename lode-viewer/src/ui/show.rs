//! The state, handed to the tree after every change, and what the tree's
//! signals ask of the state.

use std::rc::Rc;

use super::{
    BUDGET, ChipItem, Facet, FieldRow, LogsPageSignal, MachinesPageSignal, MenuRow, NavItem,
    OverviewPageSignal, SIDEBAR, SavedItem, ShellSignal, SourceShare, Tile, View, WindowLine,
    paint,
};
use crate::app::{Action, App, Origin, Page, ZOOMS, clock, clock_ms};
use crate::data::{Kind, LEVELS, level_name};
use crate::lines::group;
use crate::query::Chip;
use crate::stats;
use crate::theme::Mode;

/// What the tree was last given beyond its properties, which compare
/// themselves.
#[derive(Debug, Default)]
pub(super) struct Shown {
    /// The scrolls the pages were last at: the Overview's, the table's
    /// and Machines'.
    pub scrolls: (f32, f32, f32),
    /// What the table's lines were made from, to say when they changed.
    lines: Option<LinesKey>,
}

/// What the table's lines depend on.
#[derive(Debug, PartialEq)]
struct LinesKey {
    count: usize,
    height: u32,
    boot: usize,
    live: bool,
    found: usize,
    first: Option<usize>,
    entries: usize,
}

/// `value` as the markup's string.
fn s(value: impl AsRef<str>) -> Rc<str> {
    Rc::from(value.as_ref())
}

/// A size, in the unit that reads best.
#[must_use]
pub fn bytes(size: u64) -> String {
    if size >= 1 << 30 {
        format!("{:.0} GiB", size as f64 / f64::from(1_u32 << 30))
    } else if size >= 1 << 20 {
        format!("{:.1} MiB", size as f64 / f64::from(1_u32 << 20))
    } else {
        format!("{} KiB", size / 1024)
    }
}

/// A level's name and number: `3 · err`.
#[must_use]
pub fn level_label(level: u8) -> String {
    if level >= 8 {
        "unspecified".to_owned()
    } else {
        format!("{level} · {}", level_name(level))
    }
}

/// The heading's and the track's properties, the same on both pages.
macro_rules! heading_and_track {
    ($page:expr, $ui:expr, $app:expr, $lanes:expr) => {{
        let app: &App = $app;
        let ui: &mut tole::Ui = $ui;
        $page.has_machines.set(ui, !app.machines.is_empty());
        $page.machine_name.set(ui, s(&app.origin_name));
        let (from, to) = app
            .boot()
            .map_or((0, 0), |boot| (boot.first_us, boot.last_us));
        let end = if app.boot + 1 == app.log.boots.len() && app.live {
            "now".to_owned()
        } else {
            clock(to, app.zone())
        };
        $page
            .boot_label
            .set(ui, s(format!("Boot {}", app.offset_of_boot_label())));
        $page
            .boot_rest
            .set(ui, s(format!(" · {} → {end}", clock(from, app.zone()))));
        $page.live_state.set(
            ui,
            match (&app.origin, app.live) {
                (Origin::Store(_), true) => 0,
                (Origin::Store(_), false) => 1,
                (Origin::File(_), _) => 2,
            },
        );
        $page.live.set(ui, app.live);
        let (from, to) = app.window();
        let zone = app.zone();
        let label = if app.live && app.whole_boot() {
            format!("live · {}", clock(to, zone))
        } else {
            format!(
                "{} → {} · {} s",
                clock(from, zone),
                clock(to, zone),
                (to - from).div_ceil(1_000_000)
            )
        };
        $page.track_label.set(ui, s(label));
        let zoom = ZOOMS.iter().position(|&(_, seconds)| {
            if seconds == 0 {
                app.whole_boot()
            } else {
                app.span == seconds * 1_000_000 && !app.whole_boot()
            }
        });
        $page
            .zoom
            .set(ui, zoom.map_or(-1, |at| i32::try_from(at).unwrap_or(-1)));
        $page.lane_rows.set(ui, $lanes);
    }};
}

/// Hand `view` what `app` holds.
pub(super) fn show(view: &mut View, app: &App) {
    let ui = &mut view.ui;
    let theme = super::fx_controls::Theme::instance(ui);
    theme.dark.set(ui, app.mode == Mode::Dark);
    shell(view, app);
    let ui = &mut view.ui;
    let lanes = stats::lanes(
        app.log.of_boot(app.boot),
        &app.log.sources,
        app.extent().0,
        app.extent().1,
        1,
    )
    .len()
    .min(5);
    let lanes = i32::try_from(lanes).unwrap_or(5);
    for (page, common) in [
        (Page::Overview, view.overview.base.common),
        (Page::Logs, view.logs.base.common),
        (Page::Machines, view.machines.base.common),
    ] {
        common.visible.set(ui, app.page == page);
    }
    // The painters read the state when the frame is made: new ones say
    // it changed.
    let shared = Rc::clone(&view.app);
    match app.page {
        Page::Overview => {
            heading_and_track!(view.overview, ui, app, lanes);
            view.overview.lanes.set(ui, paint::lanes(&shared));
            overview(view, app);
        }
        Page::Logs => {
            heading_and_track!(view.logs, ui, app, lanes);
            view.logs.lanes.set(ui, paint::lanes(&shared));
            logs(view, app);
        }
        Page::Machines => super::machines_show::show(view, app),
    }
    let ui = &mut view.ui;
    let (overview, logs, machines) = view.shown.scrolls;
    if (app.overview_scroll - overview).abs() > 0.01 {
        view.overview.scroll.set(ui, app.overview_scroll);
    }
    if (app.scroll - logs).abs() > 0.01 {
        view.logs.scroll.set(ui, app.scroll);
    }
    if (app.machines_scroll - machines).abs() > 0.01 {
        view.machines.scroll.set(ui, app.machines_scroll);
    }
    view.shown.scrolls = (app.overview_scroll, app.scroll, app.machines_scroll);
}

/// The header, the sidebar, the menus and the toast.
fn shell(view: &mut View, app: &App) {
    let ui = &mut view.ui;
    let shell = view.shell;
    let host = app
        .boot()
        .map_or_else(String::new, |boot| boot.host.clone());
    shell.place.set(
        ui,
        s(format!(
            "{} · {} · boot {}",
            if host.is_empty() { "ferrix" } else { &host },
            app.origin_name,
            app.log.offset_of(app.boot)
        )),
    );
    let crashes = app.crashes();
    shell.nav.set(
        ui,
        vec![
            nav("Overview", app.page == Page::Overview, 0),
            nav("Logs", app.page == Page::Logs, 0),
            nav("Crashes", false, crashes),
            nav("Machines", app.page == Page::Machines, 0),
        ],
    );
    shell.saved.set(
        ui,
        app.saved
            .iter()
            .enumerate()
            .map(|(index, saved)| SavedItem {
                name: s(&saved.name),
                on: app.saved_in_use == Some(index),
            })
            .collect(),
    );
    shell
        .notices
        .set(ui, app.notices.iter().rev().take(3).map(s).collect());
    let (running, line, tier) = match &app.origin {
        Origin::Store(_) if !app.machines.is_empty() => {
            let mib = app.status.used as f32 / 1_048_576.0;
            (
                app.live,
                format!("{} machines · {mib:.0} MiB here", app.machines.len()),
                if app.live {
                    "received · following".to_owned()
                } else {
                    "received · Live is off".to_owned()
                },
            )
        }
        Origin::Store(_) => {
            let mib = app.status.used as f32 / 1_048_576.0;
            let state = if app.status.running {
                "lode running"
            } else {
                "lode not running"
            };
            (
                app.status.running,
                format!("{state} · {mib:.0} MiB used"),
                format!("{} tier · {} budget", app.status.tier, bytes(BUDGET)),
            )
        }
        Origin::File(path) => {
            let size = std::fs::metadata(path).map_or(0, |meta| meta.len());
            (
                false,
                format!("file · {} records", group(app.log.entries.len() as u64)),
                format!("{} · not live", bytes(size)),
            )
        }
    };
    shell.running.set(ui, running);
    shell.foot_line.set(ui, s(line));
    shell.foot_tier.set(ui, s(tier));
    let used = match &app.origin {
        Origin::Store(_) => (app.status.used as f32 / BUDGET as f32).clamp(0.0, 1.0),
        Origin::File(_) => 0.0,
    };
    shell.used.set(ui, used);
    shell.boot_menu_open.set(ui, app.boot_menu);
    shell.boot_menu_x.set(
        ui,
        SIDEBAR + 36.0 + if app.machines.is_empty() { 80.0 } else { 220.0 },
    );
    if app.boot_menu {
        let zone = app.zone();
        shell.boots.set(
            ui,
            app.log
                .boots
                .iter()
                .enumerate()
                .rev()
                .take(12)
                .map(|(index, boot)| MenuRow {
                    title: s(format!("Boot {}", app.log.offset_of(index))),
                    note: s(format!(
                        "{} → {} · {} records · {}",
                        clock(boot.first_us, zone),
                        clock(boot.last_us, zone),
                        group(boot.range.len() as u64),
                        &boot.hex()[..8]
                    )),
                    on: index == app.boot,
                })
                .collect(),
        );
    }
    shell.machine_menu_open.set(ui, app.machine_menu);
    if app.machine_menu {
        shell.machines.set(
            ui,
            app.machines
                .iter()
                .take(12)
                .enumerate()
                .map(|(row, machine)| MenuRow {
                    title: s(&machine.name),
                    note: s(&machine.note),
                    on: row == app.machine,
                })
                .collect(),
        );
    }
    shell
        .toast
        .set(ui, app.toast.as_deref().map_or_else(|| s(""), s));
}

/// A page of the sidebar.
fn nav(label: &str, on: bool, badge: usize) -> NavItem {
    NavItem {
        label: s(label),
        on,
        badge: if badge > 0 {
            s(badge.to_string())
        } else {
            s("")
        },
    }
}

/// The Overview's own.
fn overview(view: &mut View, app: &App) {
    let ui = &mut view.ui;
    let page = view.overview;
    let shared = Rc::clone(&view.app);
    let entries = app.log.of_boot(app.boot);
    page.empty.set(ui, entries.is_empty());
    if entries.is_empty() {
        return;
    }
    let zone = app.zone();
    let (_, span) = app.extent();
    let (_, width) = paint::buckets(app);
    let figures = stats::tiles(entries, span);
    let held = if figures.held >= 1000 {
        format!("{:.1}k", figures.held as f32 / 1000.0)
    } else {
        figures.held.to_string()
    };
    let rate = if figures.rate >= 10.0 {
        format!("{:.0}", figures.rate)
    } else {
        format!("{:.1}", figures.rate)
    };
    let last = |at: Option<u64>| {
        at.map_or_else(
            || "none this boot".to_owned(),
            |at| format!("last {}", clock(at, zone)),
        )
    };
    page.tiles.set(
        ui,
        vec![
            Tile {
                figure: s(figures.errors.to_string()),
                word: s("errors"),
                foot: s(last(figures.last_error)),
                kind: 0,
            },
            Tile {
                figure: s(figures.warnings.to_string()),
                word: s("warnings"),
                foot: s(last(figures.last_warning)),
                kind: 1,
            },
            Tile {
                figure: s(rate),
                word: s("lines/s"),
                foot: s(format!(
                    "peak {} at {}",
                    figures.peak.0,
                    clock(figures.peak.1, zone)
                )),
                kind: 2,
            },
            Tile {
                figure: s(held),
                word: s("held"),
                foot: s("by the limiter"),
                kind: 3,
            },
        ],
    );
    page.sparks.set(ui, paint::sparks(&shared));
    page.activity_note.set(
        ui,
        s(format!(
            "per {} · grey info · amber warning · red error",
            paint::per(width)
        )),
    );
    page.compare.set(
        ui,
        match (app.boot.checked_sub(1), app.compare) {
            (None, _) => -1,
            (Some(_), false) => 0,
            (Some(_), true) => 1,
        },
    );
    page.activity.set(ui, paint::activity(&shared));
    page.rate.set(ui, paint::rate(&shared));
    let shares = stats::top_sources(entries, app.window(), 5);
    let most = shares.first().map_or(1, |share| share.lines).max(1) as f32;
    page.sources.set(
        ui,
        shares
            .iter()
            .map(|share| SourceShare {
                name: s(app.log.sources.name(share.source)),
                count: s(group(share.lines as u64)),
                fraction: share.lines as f32 / most,
                errors: share.errors > 0,
            })
            .collect(),
    );
    let (from, to) = app.window();
    let (order, total) = stats::worst_first(entries, (from, to));
    page.window_title.set(
        ui,
        s(format!(
            "In the window · {} → {} · {} lines, warnings and worse first",
            clock(from, zone),
            clock(to, zone),
            group(total as u64)
        )),
    );
    page.lines.set(
        ui,
        order
            .iter()
            .take(6)
            .filter_map(|&index| entries.get(index))
            .map(|entry| WindowLine {
                level: i32::from(entry.row.level),
                crash: entry.row.kind == Kind::Crash,
                time: s(clock_ms(entry.time(), zone)),
                source: s(app.log.sources.name(entry.row.source)),
                message: s(match entry.row.kind {
                    Kind::Held(count) => {
                        format!("{} lines held · {}", group(count), entry.message())
                    }
                    _ => entry.message().into_owned(),
                }),
            })
            .collect(),
    );
    page.show_all.set(
        ui,
        if total == 0 {
            s("")
        } else {
            s(format!("Show all {} in Logs →", group(total as u64)))
        },
    );
}

/// Logs' own.
fn logs(view: &mut View, app: &App) {
    let ui = &mut view.ui;
    let page = view.logs;
    page.edited.set(ui, app.query_edited());
    let text = if app.editing {
        app.draft.as_str()
    } else {
        app.query.text.as_str()
    };
    let shown = text.trim().strip_prefix("where ").unwrap_or(text.trim());
    if !app.editing && &*page.query.get(ui) != shown {
        page.query.set(ui, s(shown));
    }
    page.editing.set(ui, app.editing);
    let saved = app
        .saved_in_use
        .and_then(|index| app.saved.get(index))
        .map(|saved| {
            if app.query_edited() {
                format!("@{} · edited", saved.name)
            } else {
                format!("@{}", saved.name)
            }
        })
        .unwrap_or_default();
    page.saved_label.set(ui, s(saved));
    page.query_error
        .set(ui, s(app.query.error.as_deref().unwrap_or("")));
    // The chips.
    let zone = app.zone();
    page.boot_chip
        .set(ui, s(format!("boot {}", app.log.offset_of(app.boot))));
    page.window_chip.set(
        ui,
        if app.whole_boot() {
            s("")
        } else {
            let (from, to) = app.window();
            s(format!("{} → {}", clock(from, zone), clock(to, zone)))
        },
    );
    page.chips.set(
        ui,
        app.query
            .chips
            .iter()
            .map(|chip| ChipItem {
                label: s(chip_label(app, chip.clone())),
            })
            .collect(),
    );
    let suggestion = app
        .facets
        .sources
        .first()
        .map(|&(source, _)| Chip::NotSource(source))
        .filter(|chip| !app.query.chips.contains(chip) && app.query.chips.len() < 4);
    page.suggestion.set(
        ui,
        match suggestion {
            Some(Chip::NotSource(source)) => s(format!("+ not {}", app.log.sources.name(source))),
            _ => s(""),
        },
    );
    let boot_total = app.log.of_boot(app.boot).len();
    page.count.set(
        ui,
        s(format!(
            "{} of {} records",
            group(app.found.len() as u64),
            group(boot_total as u64)
        )),
    );
    // The facets.
    let mut levels = Vec::new();
    for (level, name) in LEVELS.iter().enumerate() {
        let count = app.facets.levels.get(level).copied().unwrap_or(0);
        if count == 0 && !matches!(level, 3..=6) {
            continue;
        }
        levels.push(Facet {
            name: s(*name),
            count: s(group(count as u64)),
            on: app.query.levels.get(level).copied().unwrap_or(true),
            level: i32::try_from(level).unwrap_or(8),
        });
    }
    page.levels.set(ui, levels);
    let chosen = |chip: Chip| app.query.chips.contains(&chip);
    let facet = |name: String, count: usize, on: bool| Facet {
        name: s(name),
        count: s(group(count as u64)),
        on,
        level: 0,
    };
    page.sources.set(
        ui,
        app.facets
            .sources
            .iter()
            .take(6)
            .map(|&(source, count)| {
                facet(
                    app.log.sources.name(source).to_owned(),
                    count,
                    chosen(Chip::Source(source)),
                )
            })
            .collect(),
    );
    page.more_sources.set(
        ui,
        i32::try_from(app.facets.sources.len().saturating_sub(6)).unwrap_or(0),
    );
    page.transports.set(
        ui,
        app.facets
            .transports
            .iter()
            .take(5)
            .map(|&(transport, count)| {
                facet(
                    app.log.transports.name(transport).to_owned(),
                    count,
                    chosen(Chip::Transport(transport)),
                )
            })
            .collect(),
    );
    page.sessions.set(
        ui,
        app.facets
            .sessions
            .iter()
            .take(4)
            .map(|&(session, count)| {
                facet(
                    format!("session {}", app.log.sessions.name(session)),
                    count,
                    chosen(Chip::Session(session)),
                )
            })
            .collect(),
    );
    // The table.
    let key = LinesKey {
        count: app.lines.len(),
        height: app.table_height.to_bits(),
        boot: app.boot,
        live: app.live,
        found: app.found.len(),
        first: app.found.first().copied(),
        entries: app.log.entries.len(),
    };
    if view.shown.lines.as_ref() != Some(&key) {
        view.lines.reset();
        view.shown.lines = Some(key);
        page.minimap.set(ui, paint::minimap(&view.app));
    }
    page.table_height.set(ui, app.table_height);
    page.selected.set(
        ui,
        app.selected
            .map_or(-1, |index| i32::try_from(index).unwrap_or(-1)),
    );
    pane(view, app);
}

/// A chip's words.
fn chip_label(app: &App, chip: Chip) -> String {
    match chip {
        Chip::Source(source) => format!("source {}", app.log.sources.name(source)),
        Chip::NotSource(source) => format!("not {}", app.log.sources.name(source)),
        Chip::Transport(transport) => {
            format!("transport = {}", app.log.transports.name(transport))
        }
        Chip::Session(session) => format!("session {}", app.log.sessions.name(session)),
        Chip::Crashes => "has crash".to_owned(),
    }
}

/// The record's pane: its level and message, then every field that fits.
fn pane(view: &mut View, app: &App) {
    let ui = &mut view.ui;
    let page = view.logs;
    let entry = app.selected.and_then(|index| app.log.entries.get(index));
    page.pane_open.set(ui, entry.is_some());
    let Some(entry) = entry else {
        return;
    };
    page.pane_level.set(ui, i32::from(entry.row.level));
    page.pane_crash.set(ui, entry.row.kind == Kind::Crash);
    page.pane_title
        .set(ui, s(format!("Record {}", group(entry.record.seqnum))));
    let face = paint::mono(13.0, 400);
    let message = entry.message().into_owned();
    let wrapped = wrap(ui, &face, &message, 340.0 - 24.0, 8);
    let mut y = 28.0 + 38.0 + 14.0 + 20.0 * wrapped.len() as f32 + 14.0;
    page.pane_message.set(ui, wrapped.iter().map(s).collect());
    let note = entry.row.note.map_or_else(Vec::new, |note| {
        let text = format!(
            "Stored as {}; text says “{}”. Shown as {} by a display rule.",
            level_name(note.stored),
            note.word,
            level_name(entry.row.level)
        );
        // Measured in the message's face, as the pane always measured it.
        wrap(ui, &paint::mono(13.0, 400), &text, 340.0 - 20.0, 3)
    });
    if !note.is_empty() {
        y += 10.0 + 18.0 * note.len() as f32 + 12.0;
    }
    page.pane_note.set(ui, note.iter().map(s).collect());
    let mut fields: Vec<(String, String)> =
        vec![("PRIORITY".to_owned(), level_label(entry.stored_level()))];
    for field in &entry.record.fields {
        if field.name == "MESSAGE" {
            continue;
        }
        let value = match std::str::from_utf8(&field.value) {
            Ok(text) => text.replace('\n', " \\n "),
            Err(_) => format!("[{} bytes of binary data]", field.value.len()),
        };
        fields.push((field.name.clone(), value));
    }
    let mut boot = String::new();
    ferrix_lode_core::segment::push_hex(&mut boot, &entry.record.boot_id);
    let civil = ferrix_lode_core::time::Civil::at(entry.time(), entry.record.utc_offset_s);
    fields.push((
        "__REALTIME".to_owned(),
        format!(
            "{:04}-{:02}-{:02} {} {}",
            civil.year,
            civil.month,
            civil.day,
            clock_ms(entry.time(), entry.record.utc_offset_s),
            civil.offset()
        ),
    ));
    fields.push((
        "__MONOTONIC".to_owned(),
        format!("{:.6} s", entry.record.monotonic_us as f64 / 1e6),
    ));
    fields.push(("_HOSTNAME".to_owned(), entry.record.host.to_string()));
    fields.push(("_BOOT_ID".to_owned(), boot));
    fields.push(("__SEQNUM".to_owned(), entry.record.seqnum.to_string()));
    fields.push((
        "__CURSOR".to_owned(),
        ferrix_lode_core::cursor::of(&entry.record),
    ));
    let height = view.shell.base.common.height.get(&mut view.ui) - super::HEADER;
    let bottom = height - 120.0;
    let fit = fields
        .iter()
        .take_while(|_| {
            let fits = y + 20.0 <= bottom;
            y += 20.0;
            fits
        })
        .count();
    let ui = &mut view.ui;
    page.pane_more.set(ui, fit < fields.len());
    page.pane_fields.set(
        ui,
        fields
            .into_iter()
            .take(fit)
            .map(|(name, value)| FieldRow {
                name: s(name),
                value: s(value),
            })
            .collect(),
    );
}

/// `text` broken into lines no wider than `width` in `face`, at most
/// `most` of them, the last cut with `…`.
fn wrap(ui: &mut tole::Ui, face: &tole::Font, text: &str, width: f32, most: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if ui.text_width(face, &candidate) <= width || line.is_empty() {
                line = candidate;
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_owned();
            }
        }
        lines.push(line);
    }
    if lines.len() > most {
        lines.truncate(most);
        if let Some(last) = lines.last_mut() {
            last.push_str(" …");
        }
    }
    lines
}

/// What a press on the shell asks.
pub(super) fn shell_action(app: &App, signal: ShellSignal) -> Vec<Action> {
    let action = match signal {
        ShellSignal::Theme(dark) => Action::Theme(if dark { Mode::Dark } else { Mode::Light }),
        ShellSignal::MoveWindow => Action::WindowMove,
        ShellSignal::MaximizeWindow => Action::WindowMaximize,
        ShellSignal::MinimizeWindow => Action::WindowMinimize,
        ShellSignal::CloseWindow => Action::WindowClose,
        ShellSignal::Navigate(index) => match index {
            0 => Action::Page(Page::Overview),
            1 => Action::Page(Page::Logs),
            2 => Action::Crashes,
            _ => Action::Page(Page::Machines),
        },
        ShellSignal::SavedQuery(index) => Action::Saved(usize::try_from(index).unwrap_or(0)),
        ShellSignal::NewSaved => Action::NewSaved,
        ShellSignal::Boot(row) => {
            let index = app
                .log
                .boots
                .len()
                .checked_sub(1 + usize::try_from(row).unwrap_or(0));
            match index {
                Some(index) => Action::Boot(index),
                None => return Vec::new(),
            }
        }
        ShellSignal::Machine(row) => Action::Machine(usize::try_from(row).unwrap_or(0)),
        ShellSignal::AddMachine => Action::FormFocus(crate::app::FormField::Name),
    };
    vec![action]
}

/// The heading's and the track's signals, the same on both pages.
macro_rules! common_actions {
    ($signal:expr, $enum:ident, $app:expr) => {
        match $signal {
            $enum::MachineMenu => Some(Action::MachineMenu),
            $enum::BootMenu => Some(Action::BootMenu),
            $enum::PrevProblem => Some(Action::PrevProblem),
            $enum::NextProblem => Some(Action::NextProblem),
            $enum::Step(seconds) => Some(Action::Step(i64::from(seconds))),
            $enum::ZoomTo(seconds) => Some(Action::Zoom(u64::try_from(seconds).unwrap_or(0))),
            $enum::GoLive => Some(Action::Live),
            $enum::CursorAt(fraction) => Some(Action::Track(fraction)),
            $enum::ZoomWheel(delta) => Some(zoom_wheel($app, delta)),
            _ => None,
        }
    };
}

/// The wheel over the track: zoom out when turned down, in when up.
fn zoom_wheel(app: &App, delta: f32) -> Action {
    let zooms: [u64; 4] = [30, 60, 300, 0];
    let now = zooms
        .iter()
        .position(|&z| z * 1_000_000 == app.span || (z == 0 && app.whole_boot()))
        .unwrap_or(1);
    let next = if delta > 0.0 {
        (now + 1).min(3)
    } else {
        now.saturating_sub(1)
    };
    Action::Zoom(zooms.get(next).copied().unwrap_or(0))
}

/// What a press on the Overview asks.
pub(super) fn overview_action(app: &App, signal: OverviewPageSignal) -> Vec<Action> {
    if let Some(action) = common_actions!(signal.clone(), OverviewPageSignal, app) {
        return vec![action];
    }
    let entries = app.log.of_boot(app.boot);
    match signal {
        OverviewPageSignal::ToggleCompare => vec![Action::Compare],
        OverviewPageSignal::Source(index) => {
            let shares = stats::top_sources(entries, app.window(), 5);
            shares
                .get(usize::try_from(index).unwrap_or(usize::MAX))
                .map(|share| vec![Action::Source(share.source)])
                .unwrap_or_default()
        }
        OverviewPageSignal::Line(index) => {
            let (order, _) = stats::worst_first(entries, app.window());
            let base = app.boot().map_or(0, |boot| boot.range.start);
            order
                .get(usize::try_from(index).unwrap_or(usize::MAX))
                .map(|&at| vec![Action::Select(base + at)])
                .unwrap_or_default()
        }
        OverviewPageSignal::WindowInLogs => vec![Action::WindowInLogs],
        _ => Vec::new(),
    }
}

/// What a press or a key on Logs asks.
pub(super) fn logs_action(app: &App, signal: LogsPageSignal) -> Vec<Action> {
    if let Some(action) = common_actions!(signal.clone(), LogsPageSignal, app) {
        return vec![action];
    }
    let index = |i: i32| usize::try_from(i).unwrap_or(usize::MAX);
    let action = match signal {
        LogsPageSignal::Save => Action::Save,
        LogsPageSignal::EditQuery => Action::EditQuery,
        LogsPageSignal::QueryEdited(text) => Action::QueryText(text.to_string()),
        LogsPageSignal::QueryAccepted => Action::QueryAccept,
        LogsPageSignal::QueryCancelled => Action::QueryCancel,
        LogsPageSignal::RemoveChip(i) => Action::RemoveChip(index(i)),
        LogsPageSignal::AddSuggestion => match app.facets.sources.first() {
            Some(&(source, _)) => Action::AddChip(Chip::NotSource(source)),
            None => return Vec::new(),
        },
        LogsPageSignal::Level(level) => Action::Level(u8::try_from(level).unwrap_or(8)),
        LogsPageSignal::Source(i) => match app.facets.sources.get(index(i)) {
            Some(&(source, _)) => Action::Source(source),
            None => return Vec::new(),
        },
        LogsPageSignal::Transport(i) => match app.facets.transports.get(index(i)) {
            Some(&(transport, _)) => Action::Transport(transport),
            None => return Vec::new(),
        },
        LogsPageSignal::Session(i) => match app.facets.sessions.get(index(i)) {
            Some(&(session, _)) => Action::Session(session),
            None => return Vec::new(),
        },
        LogsPageSignal::Select(record) => Action::Select(index(record)),
        LogsPageSignal::Expand(record) => Action::Expand(index(record)),
        LogsPageSignal::MinimapAt(fraction) => Action::MinimapAt(fraction),
        LogsPageSignal::ClosePane => Action::CloseDetail,
        LogsPageSignal::Around => return app.selected.map(Action::Around).into_iter().collect(),
        LogsPageSignal::OnlySource => {
            return app.selected.map(Action::OnlySource).into_iter().collect();
        }
        LogsPageSignal::CopyJson => {
            return app.selected.map(Action::CopyJson).into_iter().collect();
        }
        LogsPageSignal::CopyCursor => {
            return app.selected.map(Action::CopyCursor).into_iter().collect();
        }
        _ => return Vec::new(),
    };
    vec![action]
}

/// What a press on Machines asks.
pub(super) fn machines_action(app: &App, signal: MachinesPageSignal) -> Vec<Action> {
    super::machines_show::action(app, signal)
}
