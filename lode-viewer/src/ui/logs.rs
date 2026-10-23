//! Logs: the query, its chips and count, the facets, the table with its
//! held, folded and marked lines and display-rule notes, the strip that
//! shows where the problems are, and a record's every field.

use super::{DETAIL, Frame, MARGIN, TRACK, badge, level_label, page_heading, track};
use crate::app::{Action, App, clock, clock_ms};
use crate::data::{Kind, LEVELS, Mark, level_name};
use crate::draw::{Rect, mono, sans};
use crate::lines::{Line, group, held_since};
use crate::query::Chip;
use crate::theme::{self, Theme};

/// Where the table is, for the window to scroll it: its top, its height
/// and the strip's place, given the window's size.
#[must_use]
pub fn table_area(app: &App, width: f32, height: f32) -> Rect {
    let x = super::SIDEBAR + MARGIN + 196.0;
    let right = width - MARGIN - if app.selected.is_some() { DETAIL } else { 0.0 } - 20.0;
    let top = super::HEADER + 28.0 + 54.0 + TRACK + 20.0 + 42.0 + 14.0 + 26.0 + 16.0;
    Rect::new(
        x,
        top + 30.0,
        (right - x).max(100.0),
        (height - top - 30.0 - 24.0).max(60.0),
    )
}

/// Draw the page into `area`.
pub(super) fn paint(app: &App, c: &mut Frame<'_, '_>, t: &Theme, area: Rect) {
    let (width, height) = c.size();
    let right = width - MARGIN - if app.selected.is_some() { DETAIL } else { 0.0 };
    let inner = Rect::new(
        area.x + MARGIN,
        area.y + 28.0,
        right - area.x - MARGIN,
        area.h,
    );
    let save_x = page_heading(app, c, t, inner, "Logs");
    // Save, enabled once the saved query in use was changed.
    let edited = app.query_edited();
    let save = Rect::new(save_x - 70.0, inner.y, 62.0, 30.0);
    if edited {
        c.fill(save, 6.0, t.ink);
        c.text_center(sans(13.0, 500), "Save", save, t.inv);
        c.hit(save, Action::Save);
    } else {
        c.boxed(save, 6.0, t.bg2, t.line);
        c.text_center(sans(13.0, 500), "Save", save, t.ink5);
    }
    let mut y = inner.y + 54.0;
    track(app, c, t, Rect::new(inner.x, y, inner.w, TRACK));
    y += TRACK + 20.0;
    query_box(app, c, t, Rect::new(inner.x, y, inner.w, 42.0));
    y += 42.0 + 14.0;
    chips(app, c, t, Rect::new(inner.x, y, inner.w, 26.0));
    y += 26.0 + 16.0;
    facets(app, c, t, Rect::new(inner.x, y, 180.0, height - y - 24.0));
    let table = table_area(app, width, height);
    table_view(app, c, t, Rect::new(table.x, y, table.w, height - y - 24.0));
    minimap(
        app,
        c,
        t,
        Rect::new(table.right() + 8.0, y, 12.0, height - y - 24.0),
        table.h,
    );
    if let Some(index) = app.selected {
        detail(
            app,
            c,
            t,
            Rect::new(
                width - DETAIL,
                super::HEADER,
                DETAIL,
                height - super::HEADER,
            ),
            index,
        );
    }
}

/// The query: `where`, the expression, and the saved query it came from.
fn query_box(app: &App, c: &mut Frame<'_, '_>, t: &Theme, place: Rect) {
    let edge = if app.query.error.is_some() {
        theme::RED
    } else if app.editing {
        t.blue
    } else {
        t.ink
    };
    c.boxed(place, 6.0, t.bg, edge);
    let x = place.x + 12.0;
    let wx =
        x + c.text(
            mono(13.0, 400),
            "where",
            (x, place.y, place.h),
            t.ink4,
            None,
        ) + 9.0;
    let text = if app.editing {
        &app.draft
    } else {
        &app.query.text
    };
    let shown = text.trim().strip_prefix("where ").unwrap_or(text.trim());
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
    let saved_w = c.width(sans(12.0, 400), &saved);
    let room = place.right() - wx - saved_w - 30.0;
    let tw = if shown.is_empty() && !app.editing {
        c.text(
            mono(13.0, 400),
            "lode's filter language: priority <= warning or id = \"hyprix*\"",
            (wx, place.y, place.h),
            t.ink5,
            Some(room),
        );
        0.0
    } else {
        c.text(
            mono(13.0, 400),
            shown,
            (wx, place.y, place.h),
            t.ink,
            Some(room),
        )
    };
    if app.editing {
        c.fill(
            Rect::new(wx + tw + 1.0, place.y + 11.0, 1.5, 20.0),
            0.0,
            t.ink,
        );
    }
    let _ = c.text_right(
        sans(12.0, 400),
        &saved,
        (place.right() - 14.0, place.y, place.h),
        t.ink4,
    );
    if let Some(error) = &app.query.error {
        let _ = c.text(
            sans(11.5, 400),
            &format!("does not parse: {error}"),
            (place.x + 12.0, place.bottom() + 1.0, 14.0),
            t.red_ink,
            Some(place.w - 24.0),
        );
    }
    c.hit(place, Action::EditQuery);
}

/// One chip: its label and its cross.
fn chip(
    c: &mut Frame<'_, '_>,
    t: &Theme,
    x: f32,
    y: f32,
    label: &str,
    remove: Option<Action>,
) -> f32 {
    let w = c.width(sans(12.0, 400), label) + if remove.is_some() { 36.0 } else { 22.0 };
    let place = Rect::new(x, y, w, 26.0);
    c.boxed(place, 13.0, t.bg, t.line3);
    let _ = c.text(sans(12.0, 400), label, (x + 11.0, y, 26.0), t.ink, None);
    if let Some(action) = remove {
        let cx = place.right() - 14.0;
        let _ = c.text(sans(12.0, 400), "×", (cx - 3.0, y, 26.0), t.ink4, None);
        c.hit(place, action);
    }
    w + 8.0
}

/// The chips: the boot, the window, each condition; one suggested; and the
/// count at the right.
fn chips(app: &App, c: &mut Frame<'_, '_>, t: &Theme, row: Rect) {
    let mut x = row.x;
    let zone = app.zone();
    x += chip(
        c,
        t,
        x,
        row.y,
        &format!("boot {}", app.log.offset_of(app.boot)),
        None,
    );
    if !app.whole_boot() {
        let (from, to) = app.window();
        x += chip(
            c,
            t,
            x,
            row.y,
            &format!("{} → {}", clock(from, zone), clock(to, zone)),
            Some(Action::Zoom(0)),
        );
    }
    for (index, each) in app.query.chips.iter().enumerate() {
        let label = match *each {
            Chip::Source(source) => format!("source {}", app.log.sources.name(source)),
            Chip::NotSource(source) => format!("not {}", app.log.sources.name(source)),
            Chip::Transport(transport) => {
                format!("transport = {}", app.log.transports.name(transport))
            }
            Chip::Session(session) => format!("session {}", app.log.sessions.name(session)),
            Chip::Crashes => "has crash".to_owned(),
        };
        x += chip(c, t, x, row.y, &label, Some(Action::RemoveChip(index)));
    }
    // A suggestion: leave out the busiest source the table shows.
    if let Some(&(source, _)) = app.facets.sources.first() {
        let suggestion = Chip::NotSource(source);
        if !app.query.chips.contains(&suggestion) && app.query.chips.len() < 4 {
            let label = format!("+ not {}", app.log.sources.name(source));
            let w = c.width(sans(12.0, 400), &label) + 22.0;
            let place = Rect::new(x, row.y, w, 26.0);
            c.dashed_outline(place, 13.0, t.line3);
            let _ = c.text(
                sans(12.0, 400),
                &label,
                (x + 11.0, row.y, 26.0),
                t.ink4,
                None,
            );
            c.hit(place, Action::AddChip(suggestion));
        }
    }
    let total: usize = app.facets.levels.iter().sum();
    let boot_total = app.log.of_boot(app.boot).len();
    let count = format!(
        "{} of {} records",
        group(app.found.len() as u64),
        group(boot_total as u64)
    );
    let _ = total;
    let _ = c.text_right(sans(12.0, 400), &count, (row.right(), row.y, row.h), t.ink4);
}

/// The facets: the levels to tick, the sources, the transports and the
/// sessions, each with its count.
fn facets(app: &App, c: &mut Frame<'_, '_>, t: &Theme, column: Rect) {
    let mut y = column.y + 4.0;
    let _ = c.text(
        sans(13.0, 600),
        "Priority",
        (column.x, y, 20.0),
        t.ink,
        None,
    );
    y += 24.0;
    for (level, name) in LEVELS.iter().enumerate() {
        let level = u8::try_from(level).unwrap_or(8);
        let count = app
            .facets
            .levels
            .get(usize::from(level))
            .copied()
            .unwrap_or(0);
        if count == 0 && !matches!(level, 3..=6) {
            continue;
        }
        let row = Rect::new(column.x, y, column.w, 22.0);
        let on = app
            .query
            .levels
            .get(usize::from(level))
            .copied()
            .unwrap_or(true);
        let tick = Rect::new(row.x, row.y + 4.0, 13.0, 13.0);
        if on {
            c.fill(tick, 3.0, t.ink3);
            c.polyline(
                &[
                    (tick.x + 3.0, tick.y + 6.5),
                    (tick.x + 5.5, tick.y + 9.0),
                    (tick.x + 10.0, tick.y + 3.5),
                ],
                1.6,
                t.bg,
                0.0,
            );
        } else {
            c.outline(tick, 3.0, 1.0, t.ink4);
        }
        let ink = t.level_ink(level);
        let w = c.text(
            sans(13.0, 400),
            name,
            (row.x + 20.0, row.y, row.h),
            ink,
            None,
        );
        let _ = c.text(
            sans(13.0, 400),
            &group(count as u64),
            (row.x + 26.0 + w, row.y, row.h),
            t.ink4,
            None,
        );
        c.hit(row, Action::Level(level));
        y += 22.0;
    }
    y += 16.0;
    let list = |c: &mut Frame<'_, '_>,
                y: &mut f32,
                title: &str,
                items: Vec<(String, usize, bool, Action)>,
                more: usize| {
        let _ = c.text(sans(13.0, 600), title, (column.x, *y, 20.0), t.ink, None);
        *y += 24.0;
        if items.is_empty() {
            let _ = c.text(sans(13.0, 400), "none", (column.x, *y, 22.0), t.ink5, None);
            *y += 22.0;
        }
        for (name, count, on, action) in items {
            let row = Rect::new(column.x, *y, column.w, 22.0);
            let w = c.text(
                sans(13.0, if on { 600 } else { 400 }),
                &name,
                (row.x, row.y, row.h),
                if on { t.blue } else { t.ink },
                Some(column.w - 50.0),
            );
            let _ = c.text(
                sans(13.0, 400),
                &group(count as u64),
                (row.x + w + 5.0, row.y, row.h),
                t.ink4,
                None,
            );
            c.hit(row, action);
            *y += 22.0;
        }
        if more > 0 {
            let _ = c.text(
                sans(13.0, 400),
                &format!("+ {more} more"),
                (column.x, *y, 22.0),
                t.blue,
                None,
            );
            *y += 22.0;
        }
        *y += 16.0;
    };
    let chosen = |chip: Chip| app.query.chips.contains(&chip);
    let sources: Vec<(String, usize, bool, Action)> = app
        .facets
        .sources
        .iter()
        .take(6)
        .map(|&(source, count)| {
            (
                app.log.sources.name(source).to_owned(),
                count,
                chosen(Chip::Source(source)),
                Action::Source(source),
            )
        })
        .collect();
    let more = app.facets.sources.len().saturating_sub(6);
    list(c, &mut y, "Source", sources, more);
    let transports = app
        .facets
        .transports
        .iter()
        .take(5)
        .map(|&(transport, count)| {
            (
                app.log.transports.name(transport).to_owned(),
                count,
                chosen(Chip::Transport(transport)),
                Action::Transport(transport),
            )
        })
        .collect();
    list(c, &mut y, "Transport", transports, 0);
    let sessions = app
        .facets
        .sessions
        .iter()
        .take(4)
        .map(|&(session, count)| {
            (
                format!("session {}", app.log.sessions.name(session)),
                count,
                chosen(Chip::Session(session)),
                Action::Session(session),
            )
        })
        .collect();
    list(c, &mut y, "Session", sessions, 0);
}

/// The table: its heading and the lines in view.
fn table_view(app: &App, c: &mut Frame<'_, '_>, t: &Theme, place: Rect) {
    c.boxed(place, 8.0, t.bg, t.line);
    let head = Rect::new(place.x, place.y, place.w, 30.0);
    for (label, x) in [("TIME", 42.0), ("SOURCE", 146.0), ("MESSAGE", 246.0)] {
        let _ = c.text(
            sans(11.0, 600),
            label,
            (place.x + x, head.y, head.h),
            t.ink4,
            None,
        );
    }
    c.fill(
        Rect::new(place.x + 1.0, head.bottom(), place.w - 2.0, 1.0),
        0.0,
        t.line,
    );
    let body = Rect::new(
        place.x + 1.0,
        head.bottom() + 1.0,
        place.w - 2.0,
        place.h - 32.0,
    );
    let zone = app.zone();
    // The first line in view.
    let first = app.tops.partition_point(|&top| top + 0.1 < app.scroll);
    let offset = app.tops.get(first).map_or(0.0, |&top| top - app.scroll);
    let mut y = body.y + offset.max(0.0);
    let mut drawn_to_end = true;
    for line in app.lines.iter().skip(first) {
        let h = line.height();
        if y + h > body.bottom() {
            drawn_to_end = false;
            break;
        }
        draw_line(app, c, t, line, Rect::new(body.x, y, body.w, h), zone);
        y += h;
    }
    if drawn_to_end {
        let end = if app.live {
            "— end of window · following —"
        } else if app.lines.is_empty() {
            "— nothing matches —"
        } else {
            "— end of window —"
        };
        if y + 30.0 <= body.bottom() {
            let _ = c.text(
                mono(11.5, 400),
                end,
                (body.x + 10.0, y + 6.0, 20.0),
                t.ink4,
                None,
            );
        }
    }
}

/// One line of the table.
fn draw_line(app: &App, c: &mut Frame<'_, '_>, t: &Theme, line: &Line, row: Rect, zone: i32) {
    let face = mono(12.5, 400);
    let Some(entry) = line.record().and_then(|index| app.log.entries.get(index)) else {
        return;
    };
    match line {
        Line::Record(index) => {
            let bg = match entry.row.level {
                0..=3 => Some(t.red_bg),
                4 => Some(t.amber_bg),
                _ => None,
            };
            if let Some(bg) = bg {
                c.fill(row, 0.0, bg);
            }
            if app.selected == Some(*index) {
                c.outline(row.inset(1.0), 2.0, 1.5, t.ink);
            }
            badge(
                c,
                t,
                row.x + 12.0,
                row.y + 6.0,
                entry.row.level,
                entry.row.kind,
            );
            let _ = c.text(
                face,
                &clock_ms(entry.time(), zone),
                (row.x + 42.0, row.y, row.h),
                t.ink4,
                None,
            );
            let source = app.log.sources.name(entry.row.source);
            let _ = c.text(
                face,
                source,
                (row.x + 146.0, row.y, row.h),
                t.ink,
                Some(92.0),
            );
            let message = match entry.row.kind {
                Kind::Folded(count) => {
                    let text = entry.message();
                    let rest = text
                        .split_once(": ")
                        .map_or("", |(_, rest)| rest)
                        .to_owned();
                    format!("message repeated {} times  {rest}", group(count))
                }
                Kind::Crash => entry.message().into_owned(),
                _ => entry.message().into_owned(),
            };
            let ink = if matches!(entry.row.kind, Kind::Folded(_)) {
                t.ink3
            } else {
                t.ink
            };
            let _ = c.text(
                face,
                &message.replace('\n', " \\n "),
                (row.x + 246.0, row.y, row.h),
                ink,
                Some(row.w - 256.0),
            );
            c.hit(row, Action::Select(*index));
        }
        Line::Note(index) => {
            if let Some(note) = entry.row.note {
                if entry.row.level <= 4 {
                    c.fill(
                        row,
                        0.0,
                        if entry.row.level <= 3 {
                            t.red_bg
                        } else {
                            t.amber_bg
                        },
                    );
                }
                let text = format!(
                    "Stored as {}; text says “{}”. Shown as {} by a display rule.",
                    level_name(note.stored),
                    note.word,
                    level_name(entry.row.level)
                );
                let _ = c.text(
                    sans(11.5, 400),
                    &text,
                    (row.x + 38.0, row.y, row.h - 4.0),
                    t.ink3,
                    Some(row.w - 48.0),
                );
            }
            c.hit(row, Action::Select(*index));
        }
        Line::Held(index, open) => {
            let place = Rect::new(row.x + 2.0, row.y + 2.0, row.w - 4.0, row.h - 4.0);
            c.fill(place, 4.0, t.amber_bg);
            c.dashed_outline(place, 4.0, theme::AMBER_EDGE);
            let count = match entry.row.kind {
                Kind::Held(count) => count,
                _ => 0,
            };
            let source = entry
                .record
                .get_str("SUPPRESSED_SOURCE")
                .unwrap_or("a source")
                .to_owned();
            let text = format!(
                "… {} lines held from {source} · {} → {}",
                group(count),
                clock(held_since(entry), zone),
                clock(entry.time(), zone)
            );
            let _ = c.text(
                mono(12.0, 400),
                &text,
                (place.x + 10.0, place.y, place.h),
                t.ink2,
                Some(place.w - 90.0),
            );
            let word = if *open { "Fold" } else { "Expand" };
            let _ = c.text_right(
                sans(12.0, 400),
                word,
                (place.right() - 10.0, place.y, place.h),
                t.blue,
            );
            c.hit(place, Action::Expand(*index));
        }
        Line::Detail(index, text) => {
            c.fill(
                Rect::new(row.x + 2.0, row.y, row.w - 4.0, row.h),
                0.0,
                t.amber_bg,
            );
            let _ = c.text(
                mono(11.5, 400),
                text,
                (row.x + 30.0, row.y, row.h),
                t.ink3,
                Some(row.w - 40.0),
            );
            c.hit(row, Action::Select(*index));
        }
        Line::Fold {
            first,
            more,
            last,
            lines,
            expanded,
        } => {
            let a = entry;
            let b = app.log.entries.get(*last).unwrap_or(entry);
            let source = app.log.sources.name(a.row.source);
            let folded = if *lines > *more as u64 + 1 {
                format!(", {} lines with lode's folds", group(*lines))
            } else {
                String::new()
            };
            let text = if *expanded {
                format!(
                    "{} records alike from {source}{folded}, shown",
                    group(*more as u64 + 1)
                )
            } else {
                format!(
                    "… {} more like it from {source}{folded} · {} → {}",
                    group(*more as u64),
                    clock(a.time(), zone),
                    clock(b.time(), zone)
                )
            };
            let _ = c.text(
                mono(12.0, 400),
                &text,
                (row.x + 42.0, row.y, row.h),
                t.ink3,
                Some(row.w - 120.0),
            );
            let word = if *expanded { "Fold" } else { "Expand" };
            let _ = c.text_right(
                sans(12.0, 400),
                word,
                (row.right() - 12.0, row.y, row.h),
                t.blue,
            );
            c.hit(row, Action::Expand(*first));
        }
        Line::Mark(index) => {
            let place = Rect::new(row.x + 2.0, row.y + 4.0, row.w - 4.0, row.h - 8.0);
            c.outline(place, 4.0, 1.0, t.ink);
            let what = match entry.row.kind {
                Kind::Mark(Mark::Started) => "lode started",
                Kind::Mark(Mark::Restarted) => "lode restarted",
                Kind::Mark(Mark::Lost) => "records lost",
                _ => "a segment's torn tail",
            };
            let text = format!(
                "— {what} {} · {}",
                clock(entry.time(), zone),
                entry.message()
            );
            let _ = c.text(
                mono(12.0, 400),
                &text,
                (place.x + 10.0, place.y, place.h),
                t.ink,
                Some(place.w - 20.0),
            );
            c.hit(row, Action::Select(*index));
        }
    }
}

/// The strip beside the table: where errors and warnings are among the
/// lines, and the part in view.
fn minimap(app: &App, c: &mut Frame<'_, '_>, t: &Theme, place: Rect, view: f32) {
    c.fill(place, 3.0, t.bg2);
    let total = app.table_height.max(1.0);
    let mut last = (f32::MIN, 0_u8);
    for (line, &top) in app.lines.iter().zip(&app.tops) {
        let Line::Record(index) = line else {
            if matches!(line, Line::Held(..)) {
                let y = place.y + place.h * top / total;
                c.fill(Rect::new(place.x, y, place.w, 2.0), 0.0, theme::AMBER);
            }
            continue;
        };
        let Some(level) = app.log.entries.get(*index).map(|entry| entry.row.level) else {
            continue;
        };
        if level > 4 {
            continue;
        }
        let y = place.y + place.h * top / total;
        if y - last.0 < 2.0 && last.1 <= level {
            continue;
        }
        let ink = if level <= 3 { theme::RED } else { theme::AMBER };
        c.fill(Rect::new(place.x, y, place.w, 2.0), 0.0, ink);
        last = (y, level);
    }
    if app.table_height > view {
        let y = place.y + place.h * app.scroll / total;
        let h = (place.h * view / total).max(12.0);
        c.outline(
            Rect::new(place.x - 1.0, y, place.w + 2.0, h),
            3.0,
            1.5,
            t.ink,
        );
    }
    c.hit(place, Action::Minimap(place.y, place.h));
}

/// The record's pane: its level and message, then every field.
fn detail(app: &App, c: &mut Frame<'_, '_>, t: &Theme, pane: Rect, index: usize) {
    let Some(entry) = app.log.entries.get(index) else {
        return;
    };
    c.fill(pane, 0.0, t.bg);
    c.fill(Rect::new(pane.x, pane.y, 1.0, pane.h), 0.0, t.line);
    let x = pane.x + 26.0;
    let w = pane.w - 52.0;
    let mut y = pane.y + 28.0;
    badge(c, t, x, y + 3.0, entry.row.level, entry.row.kind);
    let _ = c.text(
        sans(15.0, 600),
        &format!("Record {}", group(entry.record.seqnum)),
        (x + 26.0, y, 22.0),
        t.ink,
        None,
    );
    let close = Rect::new(pane.right() - 46.0, y - 4.0, 30.0, 30.0);
    let _ = c.text(
        sans(18.0, 400),
        "×",
        (close.x + 9.0, close.y, close.h),
        t.ink3,
        None,
    );
    c.hit(close, Action::CloseDetail);
    y += 38.0;
    // The message, wrapped.
    let message = entry.message().into_owned();
    let wrapped = wrap(c, &message, w - 24.0, 8);
    let box_h = 14.0 + 20.0 * wrapped.len() as f32;
    c.fill(Rect::new(x, y, w, box_h), 6.0, t.bg3);
    for (n, part) in wrapped.iter().enumerate() {
        let _ = c.text(
            mono(13.0, 400),
            part,
            (x + 12.0, y + 7.0 + 20.0 * n as f32, 20.0),
            t.ink,
            None,
        );
    }
    y += box_h + 14.0;
    if let Some(note) = entry.row.note {
        let text = format!(
            "Stored as {}; text says “{}”. Shown as {} by a display rule.",
            level_name(note.stored),
            note.word,
            level_name(entry.row.level)
        );
        let lines = wrap(c, &text, w - 20.0, 3);
        let h = 10.0 + 18.0 * lines.len() as f32;
        c.boxed(Rect::new(x, y, w, h), 5.0, t.amber_bg, t.amber_line);
        for (n, part) in lines.iter().enumerate() {
            let _ = c.text(
                sans(12.0, 400),
                part,
                (x + 10.0, y + 5.0 + 18.0 * n as f32, 18.0),
                t.amber_ink,
                None,
            );
        }
        y += h + 12.0;
    }
    // Every field.
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
    let bottom = pane.bottom() - 120.0;
    for (name, value) in fields {
        if y + 20.0 > bottom {
            let _ = c.text(
                sans(12.0, 400),
                "… more fields: Copy as JSON",
                (x, y, 20.0),
                t.ink4,
                None,
            );
            break;
        }
        let _ = c.text(mono(12.0, 400), &name, (x, y, 20.0), t.ink4, Some(124.0));
        let _ = c.text(
            mono(12.0, 400),
            &value,
            (x + 130.0, y, 20.0),
            t.ink,
            Some(w - 130.0),
        );
        y += 20.0;
    }
    // The four actions.
    let half = (w - 10.0) / 2.0;
    let buttons = [
        ("±30 s around", Action::Around(index)),
        ("Filter: this source", Action::OnlySource(index)),
        ("Copy as JSON", Action::CopyJson(index)),
        ("Copy cursor", Action::CopyCursor(index)),
    ];
    for (n, (label, action)) in buttons.into_iter().enumerate() {
        let place = Rect::new(
            x + (half + 10.0) * (n % 2) as f32,
            pane.bottom() - 104.0 + 46.0 * (n / 2) as f32,
            half,
            36.0,
        );
        c.boxed(place, 6.0, t.bg, t.line3);
        c.text_center(sans(12.5, 500), label, place, t.ink);
        c.hit(place, action);
    }
}

/// `text` broken into lines no wider than `width` in the message's face,
/// at most `most` of them, the last cut with `…`.
fn wrap(c: &mut Frame<'_, '_>, text: &str, width: f32, most: usize) -> Vec<String> {
    let face = mono(13.0, 400);
    let mut lines: Vec<String> = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if c.width(face, &candidate) <= width || line.is_empty() {
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
