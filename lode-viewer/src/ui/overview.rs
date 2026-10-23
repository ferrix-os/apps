//! The Overview: a boot at a glance. The track, the tiles, the activity
//! per bucket of time (the boot before beside it when asked), the rate
//! against the limiter, the busiest sources and the window's lines,
//! warnings and worse first.

use super::{Frame, MARGIN, TRACK, badge, page_heading, track};
use crate::app::{Action, App, clock, clock_ms};
use crate::data::{Entry, Kind};
use crate::draw::{Rect, mono, sans};
use crate::lines::group;
use crate::stats::{self, Bucket, CHATTER_LIMIT_PER_S};
use crate::theme::{self, Color, Theme};

/// The most bars the activity chart draws.
const BARS: usize = 120;

/// Draw the page into `area`.
pub(super) fn paint(app: &App, c: &mut Frame<'_, '_>, t: &Theme, area: Rect) {
    let inner = Rect::new(
        area.x + MARGIN,
        area.y + 28.0 - app.overview_scroll,
        area.w - 2.0 * MARGIN,
        area.h,
    );
    let entries = app.log.of_boot(app.boot);
    let _ = page_heading(app, c, t, inner, "Overview");
    let mut y = inner.y + 54.0;
    track(app, c, t, Rect::new(inner.x, y, inner.w, TRACK));
    y += TRACK + 24.0;
    if entries.is_empty() {
        let _ = c.text(
            sans(14.0, 400),
            "No records: nothing was read from the store or the file.",
            (inner.x, y, 24.0),
            t.ink3,
            None,
        );
        return;
    }
    let (start, span) = app.extent();
    let width = stats::bucket_width(span, 10_000_000, BARS);
    let count = usize::try_from(span / width + 1).unwrap_or(1);
    let buckets = stats::buckets(entries, start, width, count);
    let figures = stats::tiles(entries, span);
    tiles(
        app,
        c,
        t,
        Rect::new(inner.x, y, inner.w, 132.0),
        &figures,
        &buckets,
    );
    y += 132.0 + 24.0;
    activity(
        app,
        c,
        t,
        Rect::new(inner.x, y, inner.w, 216.0),
        &buckets,
        width,
    );
    y += 216.0 + 24.0;
    let rate_w = ((inner.w - 16.0) * 0.6).floor();
    rate(
        app,
        c,
        t,
        Rect::new(inner.x, y, rate_w, 212.0),
        &buckets,
        width,
    );
    sources(
        app,
        c,
        t,
        Rect::new(inner.x + rate_w + 16.0, y, inner.w - rate_w - 16.0, 212.0),
    );
    y += 212.0 + 28.0;
    window_list(app, c, t, Rect::new(inner.x, y, inner.w, 300.0), entries);
}

/// A card: a bordered, rounded box with a title and a note at its right.
fn card(c: &mut Frame<'_, '_>, t: &Theme, place: Rect, title: &str, note: &str) {
    c.boxed(place, 12.0, t.bg, t.line);
    let _ = c.text(
        sans(13.0, 600),
        title,
        (place.x + 24.0, place.y + 20.0, 22.0),
        t.ink,
        None,
    );
    let _ = c.text_right(
        sans(11.0, 400),
        note,
        (place.right() - 24.0, place.y + 20.0, 22.0),
        t.ink4,
    );
}

/// A tile: its figure, its word, the figure's colour, its line's colour,
/// the line across the boot, and its foot.
type Tile = (String, &'static str, Color, Color, Vec<f32>, String);

/// The four tiles: errors, warnings, lines a second and lines held, each
/// with its line across the boot.
fn tiles(
    app: &App,
    c: &mut Frame<'_, '_>,
    t: &Theme,
    row: Rect,
    figures: &stats::Tiles,
    buckets: &[Bucket],
) {
    let zone = app.zone();
    let w = (row.w - 3.0 * 16.0) / 4.0;
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
    let errors: Vec<f32> = buckets.iter().map(|b| b.errors as f32).collect();
    let warnings: Vec<f32> = buckets.iter().map(|b| b.warnings as f32).collect();
    let lines: Vec<f32> = buckets.iter().map(|b| b.total() as f32).collect();
    let helds: Vec<f32> = buckets.iter().map(|b| b.held as f32).collect();
    let last = |at: Option<u64>| {
        at.map_or_else(
            || "none this boot".to_owned(),
            |at| format!("last {}", clock(at, zone)),
        )
    };
    let list: [Tile; 4] = [
        (
            figures.errors.to_string(),
            "errors",
            t.red_ink,
            theme::RED,
            errors,
            last(figures.last_error),
        ),
        (
            figures.warnings.to_string(),
            "warnings",
            t.amber_ink,
            theme::AMBER,
            warnings,
            last(figures.last_warning),
        ),
        (
            rate,
            "lines/s",
            t.ink,
            t.ink,
            lines,
            format!("peak {} at {}", figures.peak.0, clock(figures.peak.1, zone)),
        ),
        (
            held,
            "held",
            t.ink3,
            t.ink4,
            helds,
            "by the limiter".to_owned(),
        ),
    ];
    for (index, (number, label, ink, stroke, series, foot)) in list.into_iter().enumerate() {
        let place = Rect::new(row.x + (w + 16.0) * index as f32, row.y, w, row.h);
        c.boxed(place, 12.0, t.bg, t.line);
        let nx = place.x + 20.0;
        let nw = c.text(
            sans(28.0, 600),
            &number,
            (nx, place.y + 16.0, 36.0),
            ink,
            None,
        );
        let _ = c.text(
            sans(13.0, 400),
            label,
            (nx + nw + 8.0, place.y + 24.0, 24.0),
            t.ink3,
            None,
        );
        spark(
            c,
            Rect::new(nx, place.y + 58.0, place.w - 40.0, 30.0),
            &series,
            stroke,
        );
        let _ = c.text(
            sans(11.5, 400),
            &foot,
            (nx, place.y + 98.0, 18.0),
            t.ink4,
            Some(place.w - 40.0),
        );
    }
}

/// A sparkline of `series` in `place`: a line and the area under it.
fn spark(c: &mut Frame<'_, '_>, place: Rect, series: &[f32], ink: Color) {
    let most = series.iter().copied().fold(0.0_f32, f32::max).max(1.0);
    let n = series.len().max(2);
    let points: Vec<(f32, f32)> = series
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            (
                place.x + place.w * i as f32 / (n - 1) as f32,
                place.bottom() - (v / most) * place.h,
            )
        })
        .collect();
    let mut area = points.clone();
    area.push((place.right(), place.bottom()));
    area.push((place.x, place.bottom()));
    c.polygon(&area, ink.alpha(0.12));
    c.polyline(&points, 1.5, ink, 0.0);
}

/// The activity chart: a bar each bucket, grey for its lines, an amber cap
/// for warnings and a red mark for errors; the window shaded; the boot
/// before as outlines when asked.
fn activity(
    app: &App,
    c: &mut Frame<'_, '_>,
    t: &Theme,
    place: Rect,
    buckets: &[Bucket],
    width: u64,
) {
    let note = format!("per {} · grey info · amber warning · red error", per(width));
    card(c, t, place, "Activity", &note);
    // Compare with the boot before.
    let label = "Compare with boot −1";
    let bw = c.width(sans(11.5, 500), label) + 20.0;
    let note_w = c.width(sans(11.0, 400), &note);
    let toggle = Rect::new(
        place.right() - 24.0 - note_w - 16.0 - bw,
        place.y + 18.0,
        bw,
        24.0,
    );
    let previous = app.boot.checked_sub(1);
    if previous.is_some() {
        if app.compare {
            c.fill(toggle, 5.0, t.ink);
            c.text_center(sans(11.5, 500), label, toggle, t.inv);
        } else {
            c.boxed(toggle, 5.0, t.bg, t.line3);
            c.text_center(sans(11.5, 500), label, toggle, t.ink2);
        }
        c.hit(toggle, Action::Compare);
    } else {
        c.dashed_outline(toggle, 5.0, t.line3);
        c.text_center(sans(11.5, 500), "No boot −1 kept", toggle, t.ink5);
    }
    let chart = Rect::new(place.x + 24.0, place.y + 60.0, place.w - 48.0, 110.0);
    let earlier: Vec<Bucket> = match (
        app.compare,
        previous.and_then(|index| app.log.boots.get(index).map(|boot| (index, boot))),
    ) {
        (true, Some((index, boot))) => {
            stats::buckets(app.log.of_boot(index), boot.first_us, width, buckets.len())
        }
        _ => Vec::new(),
    };
    let most = buckets
        .iter()
        .chain(earlier.iter())
        .map(Bucket::total)
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    for step in 1..=2 {
        let y = chart.bottom() - chart.h * step as f32 / 2.0 * 0.8;
        c.fill(Rect::new(chart.x, y, chart.w, 1.0), 0.0, t.line2);
    }
    let n = buckets.len().max(1);
    let slot = chart.w / n as f32;
    let gap = (slot * 0.2).clamp(1.0, 10.0);
    let bar_h = |total: usize| (total as f32 / most) * chart.h * 0.8;
    // The window's span.
    if !app.whole_boot() {
        let (start, span) = app.extent();
        let (from, to) = app.window();
        let fx = |at: u64| chart.x + chart.w * ((at - start) as f32 / span as f32).clamp(0.0, 1.0);
        c.fill(
            Rect::new(
                fx(from),
                chart.y - 6.0,
                (fx(to) - fx(from)).max(2.0),
                chart.h + 8.0,
            ),
            4.0,
            t.blue_bg,
        );
    }
    for (i, bucket) in buckets.iter().enumerate() {
        let x = chart.x + slot * i as f32 + gap / 2.0;
        let w = (slot - gap).max(1.0);
        let h = bar_h(bucket.total()).max(if bucket.total() > 0 { 2.0 } else { 0.0 });
        let top = chart.bottom() - h;
        c.fill(Rect::new(x, top, w, h), 1.0, t.bar);
        if bucket.warnings > 0 {
            c.fill(Rect::new(x, top - 5.0, w, 3.5), 1.5, theme::AMBER);
        }
        if bucket.errors > 0 {
            let ey = top - if bucket.warnings > 0 { 14.0 } else { 9.0 };
            let ew = w.clamp(6.0, 20.0);
            c.fill(
                Rect::new(x + (w - ew) / 2.0, ey - 3.0, ew, 7.0),
                3.5,
                theme::RED,
            );
        }
        if let Some(before) = earlier.get(i)
            && before.total() > 0
        {
            let h = bar_h(before.total()).max(2.0);
            c.outline(Rect::new(x, chart.bottom() - h, w, h), 1.0, 1.0, t.ink4);
        }
    }
    let (start, _) = app.extent();
    let zone = app.zone();
    for step in 0..4 {
        let i = (n.saturating_sub(1)) * step / 3;
        let at = start + width * i as u64;
        let text = clock(at, zone);
        let x = chart.x + slot * i as f32;
        let tx = if step == 3 {
            x + slot - c.width(mono(10.5, 400), &text)
        } else {
            x
        };
        let _ = c.text(
            mono(10.5, 400),
            &text,
            (tx, chart.bottom() + 10.0, 14.0),
            t.ink4,
            None,
        );
    }
    if app.compare && previous.is_some() {
        let _ = c.text(
            sans(11.0, 400),
            "outlined: boot −1, from its own start",
            (chart.x, place.bottom() - 22.0, 14.0),
            t.ink4,
            None,
        );
    }
}

/// A bucket's width in words.
fn per(width: u64) -> String {
    let seconds = width / 1_000_000;
    if seconds.is_multiple_of(60) && seconds >= 60 {
        format!("{} min", seconds / 60)
    } else {
        format!("{seconds} s")
    }
}

/// The log rate: lines a second per bucket, the limiter's chatter limit
/// dashed, and where the limiter held lines shaded.
fn rate(app: &App, c: &mut Frame<'_, '_>, t: &Theme, place: Rect, buckets: &[Bucket], width: u64) {
    card(
        c,
        t,
        place,
        "Log rate",
        "lines/s · dashed = limiter (chatter, per source)",
    );
    let chart = Rect::new(
        place.x + 24.0,
        place.y + 58.0,
        place.w - 48.0,
        place.h - 90.0,
    );
    let seconds = (width as f32 / 1e6).max(1.0);
    let rates: Vec<f32> = buckets.iter().map(|b| b.total() as f32 / seconds).collect();
    let most = rates
        .iter()
        .copied()
        .fold(CHATTER_LIMIT_PER_S * 1.25, f32::max);
    let n = rates.len().max(2);
    let x_of = |i: usize| chart.x + chart.w * i as f32 / (n - 1) as f32;
    for (i, bucket) in buckets.iter().enumerate() {
        if bucket.held > 0 {
            let x = x_of(i.saturating_sub(1));
            c.fill(
                Rect::new(x, chart.y, (x_of(i) - x).max(4.0), chart.h),
                0.0,
                t.amber_bg,
            );
        }
    }
    let mut area: Vec<(f32, f32)> = rates
        .iter()
        .enumerate()
        .map(|(i, &r)| (x_of(i), chart.bottom() - (r / most) * chart.h))
        .collect();
    area.push((chart.right(), chart.bottom()));
    area.push((chart.x, chart.bottom()));
    c.polygon(&area, t.ink);
    let limit_y = chart.bottom() - (CHATTER_LIMIT_PER_S / most) * chart.h;
    c.polyline(
        &[(chart.x, limit_y), (chart.right(), limit_y)],
        1.2,
        theme::AMBER,
        4.0,
    );
    c.fill(Rect::new(chart.x, chart.bottom(), chart.w, 1.5), 0.0, t.ink);
    let _ = c.text(
        mono(10.5, 400),
        &format!("{:.0}/s", most),
        (chart.x, chart.y - 16.0, 14.0),
        t.ink4,
        None,
    );
    let _ = app;
}

/// The busiest sources in the window, red when they had errors.
fn sources(app: &App, c: &mut Frame<'_, '_>, t: &Theme, place: Rect) {
    card(
        c,
        t,
        place,
        "Top sources",
        "in the window · red = has errors",
    );
    let shares = stats::top_sources(app.log.of_boot(app.boot), app.window(), 5);
    let most = shares.first().map_or(1, |share| share.lines).max(1) as f32;
    let mut y = place.y + 56.0;
    for share in shares {
        let name = app.log.sources.name(share.source);
        let row = Rect::new(place.x + 24.0, y, place.w - 48.0, 22.0);
        let _ = c.text(
            mono(12.5, 400),
            name,
            (row.x, row.y, row.h),
            t.ink,
            Some(88.0),
        );
        let count = group(share.lines as u64);
        let _ = c.text_right(mono(12.0, 400), &count, (row.right(), row.y, row.h), t.ink4);
        let bar = Rect::new(row.x + 96.0, row.y + 8.0, row.w - 96.0 - 56.0, 6.0);
        c.fill(bar, 3.0, t.line2);
        let fill = if share.errors > 0 { theme::RED } else { t.bar };
        c.fill(
            Rect::new(
                bar.x,
                bar.y,
                (bar.w * share.lines as f32 / most).max(6.0),
                6.0,
            ),
            3.0,
            fill,
        );
        c.hit(row, Action::Source(share.source));
        y += 29.0;
    }
}

/// The window's lines, warnings and worse first, and the way to all of
/// them in Logs.
fn window_list(app: &App, c: &mut Frame<'_, '_>, t: &Theme, place: Rect, entries: &[Entry]) {
    let (from, to) = app.window();
    let zone = app.zone();
    let (order, total) = stats::worst_first(entries, (from, to));
    let title = format!(
        "In the window · {} → {} · {} lines, warnings and worse first",
        clock(from, zone),
        clock(to, zone),
        group(total as u64)
    );
    let _ = c.text(
        sans(13.0, 600),
        &title,
        (place.x, place.y, 20.0),
        t.ink2,
        None,
    );
    let mut y = place.y + 28.0;
    let base = app.boot().map_or(0, |boot| boot.range.start);
    for (index, entry) in order
        .iter()
        .take(6)
        .filter_map(|&index| entries.get(index).map(|entry| (index, entry)))
    {
        let row = Rect::new(place.x, y, place.w, 30.0);
        let bg = match entry.row.level {
            0..=3 => t.red_bg,
            4 => t.amber_bg,
            _ => t.bg,
        };
        c.fill(row, 4.0, bg);
        badge(
            c,
            t,
            row.x + 10.0,
            row.y + 7.0,
            entry.row.level,
            entry.row.kind,
        );
        let _ = c.text(
            mono(12.5, 400),
            &clock_ms(entry.time(), zone),
            (row.x + 40.0, row.y, row.h),
            t.ink4,
            None,
        );
        let source = app.log.sources.name(entry.row.source);
        let _ = c.text(
            mono(12.5, 400),
            source,
            (row.x + 148.0, row.y, row.h),
            t.ink,
            Some(96.0),
        );
        let message = match entry.row.kind {
            Kind::Held(count) => format!("{} lines held · {}", group(count), entry.message()),
            _ => entry.message().into_owned(),
        };
        let _ = c.text(
            mono(12.5, 400),
            &message,
            (row.x + 256.0, row.y, row.h),
            t.ink,
            Some(row.w - 270.0),
        );
        // Pressing a line opens it in Logs.
        let global = base + index;
        c.hit(row, Action::Select(global));
        y += 35.0;
    }
    if total == 0 {
        let _ = c.text(
            sans(13.0, 400),
            "Nothing in the window.",
            (place.x, y, 24.0),
            t.ink4,
            None,
        );
        return;
    }
    let link = format!("Show all {} in Logs →", group(total as u64));
    let w = c.text(
        sans(13.0, 400),
        &link,
        (place.x + 8.0, y + 2.0, 22.0),
        t.blue,
        None,
    );
    c.hit(Rect::new(place.x, y, w + 16.0, 26.0), Action::WindowInLogs);
}

/// How tall the page is, for its scroll.
#[must_use]
pub fn height() -> f32 {
    28.0 + 54.0
        + TRACK
        + 24.0
        + 132.0
        + 24.0
        + 216.0
        + 24.0
        + 212.0
        + 28.0
        + 28.0
        + 6.0 * 35.0
        + 60.0
}
