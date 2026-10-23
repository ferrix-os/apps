//! The painters: what the pages draw from the records, which are loops
//! over data and so Rust's work (`docs/UI-FRAMEWORK.md` §3.5) -- the
//! track's swim lanes, the tiles' sparklines, the activity and rate
//! charts, and the table's strip. Each reads the [`App`] when the frame
//! is made, through a [`Pen`] that draws as the viewer always drew.

use std::cell::RefCell;
use std::rc::Rc;

use tole::{CanvasPainter, Painter};

use crate::app::{App, clock, clock_hm};
use crate::lines::Line;
use crate::stats::{self, Bucket, CHATTER_LIMIT_PER_S, Lane};
use crate::theme::{self, Color, Theme};

/// The state, shared by the loop and what the frame reads.
pub type Shared = Rc<RefCell<App>>;

/// A rectangle in a canvas's logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Left.
    pub x: f32,
    /// Top.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl Rect {
    /// A rectangle.
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect { x, y, w, h }
    }

    /// The right edge.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    /// The bottom edge.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    fn tole(self) -> tole::Rect {
        tole::Rect::new(self.x, self.y, self.w, self.h)
    }
}

/// The design's two faces at a size and weight.
#[must_use]
pub fn sans(px: f32, weight: u16) -> tole::Font {
    tole::Font::new(SANS, (px * 4.0).trunc() / 4.0, weight)
}

/// The monospaced face.
#[must_use]
pub fn mono(px: f32, weight: u16) -> tole::Font {
    tole::Font::new(MONO, (px * 4.0).trunc() / 4.0, weight)
}

/// The families asked for, best first: the design's, then what an image
/// without them carries. The same as `Theme.sansFamily`.
pub const SANS: &str = "IBM Plex Sans, Inter, Liberation Sans, sans-serif";
/// The same for the monospaced face.
pub const MONO: &str = "IBM Plex Mono, Liberation Mono, monospace";

/// A colour of the palette, as tole has it.
#[must_use]
pub fn ink(color: Color) -> tole::Color {
    tole::Color::rgba(color.0, color.1, color.2, color.3)
}

/// The viewer's drawing calls over a canvas's [`Painter`].
pub struct Pen<'a, 'p> {
    /// The painter.
    pub p: &'a mut Painter<'p>,
}

impl Pen<'_, '_> {
    /// `place` filled, its corners rounded by `radius`.
    pub fn fill(&mut self, place: Rect, radius: f32, color: Color) {
        if place.w > 0.0 && place.h > 0.0 && color.3 != 0 {
            self.p.fill_rect(place.tole(), radius, ink(color));
        }
    }

    /// `place`'s outline, `width` wide, inside its edge.
    pub fn outline(&mut self, place: Rect, radius: f32, width: f32, color: Color) {
        self.p.stroke_rect(place.tole(), radius, width, ink(color));
    }

    /// The polygon of `points`, filled.
    pub fn polygon(&mut self, points: &[(f32, f32)], color: Color) {
        self.p.polygon(points, ink(color));
    }

    /// A path of `points`, stroked with flat ends, dashed if `dash` is not
    /// zero.
    pub fn polyline(&mut self, points: &[(f32, f32)], width: f32, color: Color, dash: f32) {
        self.p.polyline(
            points,
            width,
            ink(color),
            tole::LineStyle {
                cap: tole::LineCap::Butt,
                miter: false,
                dash,
            },
        );
    }

    /// A dot of `radius` at (`x`, `y`).
    pub fn dot(&mut self, x: f32, y: f32, radius: f32, color: Color) {
        self.p.circle((x, y), radius, ink(color));
    }

    /// `place` striped at 45° in `color` on `under`: the design's held
    /// spans, three pixels of each.
    pub fn hatch(&mut self, place: Rect, color: Color, under: Color) {
        self.fill(place, 0.0, under);
        self.p.push_clip(place.tole());
        let mut x = place.x - place.h;
        let width = 3.0 * std::f32::consts::FRAC_1_SQRT_2 * 1.4;
        while x < place.right() {
            self.p.polyline(
                &[(x, place.bottom()), (x + place.h, place.y)],
                width,
                ink(color),
                tole::LineStyle {
                    cap: tole::LineCap::Butt,
                    miter: true,
                    dash: 0.0,
                },
            );
            x += 6.0;
        }
        self.p.pop_clip();
    }

    /// How wide `text` is in `font`.
    pub fn width(&mut self, font: &tole::Font, text: &str) -> f32 {
        self.p.text_width(text, font)
    }

    /// `text` in a line `height` tall from (`x`, `y`), cut to `max`.
    pub fn text(
        &mut self,
        font: &tole::Font,
        text: &str,
        line: (f32, f32, f32),
        color: Color,
        max: Option<f32>,
    ) -> f32 {
        self.p.text_in(text, font, line, ink(color), max)
    }

    /// `text` right-aligned to `right`.
    pub fn text_right(
        &mut self,
        font: &tole::Font,
        text: &str,
        (right, y, height): (f32, f32, f32),
        color: Color,
    ) -> f32 {
        let width = self.width(font, text);
        self.text(font, text, (right - width, y, height), color, None)
    }
}

/// A painter over the shared state.
fn painter(app: &Shared, paint: fn(&App, &mut Pen<'_, '_>)) -> CanvasPainter {
    let app = Rc::clone(app);
    CanvasPainter::new(move |p| {
        if let Ok(app) = app.try_borrow() {
            paint(&app, &mut Pen { p });
        }
    })
}

/// The track's lanes, axis, brush and cursor, in the track card's
/// coordinates.
#[must_use]
pub fn lanes(app: &Shared) -> CanvasPainter {
    painter(app, |app, c| {
        let (w, _) = c.p.size();
        let card = Rect::new(0.0, 0.0, w, 148.0);
        track_lanes(app, c, &Theme::of(app.mode), card);
    })
}

/// How many lanes the track shows.
#[must_use]
pub fn lane_rows(app: &App, width: f32) -> usize {
    let _ = width;
    let mut names = std::collections::HashSet::new();
    for entry in app.log.of_boot(app.boot) {
        let _ = names.insert(entry.row.source);
        if names.len() >= 5 {
            break;
        }
    }
    names.len().min(5)
}

/// The track below its buttons.
fn track_lanes(app: &App, c: &mut Pen<'_, '_>, t: &Theme, card: Rect) {
    let lanes_top = card.y + 50.0;
    let left = card.x + 98.0;
    let right = card.right() - 18.0;
    let width = right - left;
    let (start, span) = app.extent();
    let (from, to) = app.window();
    let zone = app.zone();
    let entries = app.log.of_boot(app.boot);
    let columns = (width / 3.0).max(1.0) as usize;
    let lanes = stats::lanes(entries, &app.log.sources, start, span, columns);
    let lane_h = 17.0;
    let lane_rows = lanes.len().min(5);
    for (row, lane) in lanes.iter().take(lane_rows).enumerate() {
        let y = lanes_top + lane_h * row as f32;
        let _ = c.text(
            &mono(11.0, 400),
            &lane.name,
            (card.x + 18.0, y, 12.0),
            t.ink3,
            Some(76.0),
        );
        draw_lane(c, t, lane, Rect::new(left, y + 1.0, width, 10.0));
    }
    let axis_y = lanes_top + lane_h * lane_rows as f32 + 2.0;
    for step in 0..5 {
        let fraction = step as f32 / 4.0;
        let at = start + (f64::from(fraction) * span as f64) as u64;
        let text = if step == 4 && app.live {
            "now".to_owned()
        } else if span < 600_000_000 {
            clock(at, zone)
        } else {
            clock_hm(at, zone)
        };
        let ink = if step == 4 && app.live {
            theme::GREEN
        } else {
            t.ink5
        };
        let font = mono(10.5, 400);
        let w = c.width(&font, &text);
        let tx = (left + width * fraction - if step == 4 { w } else { 0.0 }).max(left);
        let _ = c.text(&font, &text, (tx, axis_y, 14.0), ink, None);
    }
    // The brush over the window, and the cursor.
    let fraction =
        |at: u64| ((at.saturating_sub(start)) as f64 / span as f64).clamp(0.0, 1.0) as f32;
    let (bl, br) = (left + width * fraction(from), left + width * fraction(to));
    let brush = Rect::new(
        bl,
        lanes_top - 3.0,
        (br - bl).max(4.0),
        lane_h * lane_rows as f32 + 4.0,
    );
    if !app.whole_boot() {
        c.fill(brush, 4.0, t.blue_bg);
        c.outline(brush, 4.0, 1.5, t.blue);
        for edge in [brush.x, brush.right()] {
            let handle = Rect::new(edge - 4.0, brush.y + brush.h / 2.0 - 8.0, 8.0, 16.0);
            c.fill(handle, 3.0, t.blue);
        }
    }
    let cx = left + width * fraction(app.cursor.clamp(from, to));
    let cursor_x = if app.whole_boot() { right } else { cx };
    c.fill(
        Rect::new(
            cursor_x - 1.0,
            lanes_top - 7.0,
            2.0,
            lane_h * lane_rows as f32 + 12.0,
        ),
        0.0,
        t.ink,
    );
    c.polygon(
        &[
            (cursor_x - 5.0, lanes_top - 10.0),
            (cursor_x + 5.0, lanes_top - 10.0),
            (cursor_x, lanes_top - 4.0),
        ],
        t.ink,
    );
}

/// One swim lane: where its source was busy, held, warned and failed.
fn draw_lane(c: &mut Pen<'_, '_>, t: &Theme, lane: &Lane, place: Rect) {
    let columns = lane.columns.len().max(1);
    let step = place.w / columns as f32;
    let busy: usize = lane.columns.iter().copied().max().unwrap_or(0);
    // Where the source was heard at all: a thin line, its quiet gaps of a
    // few columns bridged; where it was busy, a thicker bar over it.
    for (dense, bridge, top, height) in [(false, 6, 4.0, 2.0), (true, 2, 2.0, 6.0)] {
        let heard = |count: usize| {
            if dense {
                count > 0 && count * 4 > busy
            } else {
                count > 0
            }
        };
        let at = |column: usize| lane.columns.get(column).copied().unwrap_or(0);
        let mut column = 0;
        while column < columns {
            if !heard(at(column)) {
                column += 1;
                continue;
            }
            let mut end = column + 1;
            let mut quiet = 0;
            while end < columns && quiet <= bridge {
                if heard(at(end)) {
                    quiet = 0;
                } else {
                    quiet += 1;
                }
                end += 1;
            }
            let last = end - quiet;
            let x = place.x + step * column as f32;
            let w = (step * (last - column) as f32).max(step);
            c.fill(Rect::new(x, place.y + top, w, height), 1.0, t.line3);
            column = end;
        }
    }
    for &(since, until) in &lane.held {
        let x = place.x + place.w * since;
        let w = (place.w * (until - since)).max(4.0);
        let span = Rect::new(x, place.y + 1.0, w, 8.0);
        c.hatch(span, theme::AMBER, t.bg);
        c.outline(span, 0.0, 1.0, theme::AMBER_EDGE);
    }
    // Errors and warnings as blocks: a column's worst, and a run of
    // columns alike one bar, a change of kind a change of colour with no
    // gap between.
    let mut worst = vec![Severity::None; columns];
    let column_of = |at: f32| ((at * columns as f32) as usize).min(columns - 1);
    for &at in &lane.warnings {
        if let Some(slot) = worst.get_mut(column_of(at)) {
            *slot = (*slot).max(Severity::Warning);
        }
    }
    for &at in &lane.errors {
        if let Some(slot) = worst.get_mut(column_of(at)) {
            *slot = Severity::Error;
        }
    }
    let spans = worst
        .iter()
        .enumerate()
        .map(|(column, kind)| (step * column as f32, step * (column + 1) as f32, *kind));
    for (from, to, kind) in runs(spans) {
        let ink = match kind {
            Severity::Error => theme::RED,
            _ => theme::AMBER,
        };
        c.fill(
            Rect::new(place.x + from, place.y + 1.0, (to - from).max(1.0), 8.0),
            0.0,
            ink,
        );
    }
}

/// How bad a line or a column of the log is, as its blocks show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Nothing to show.
    None,
    /// A warning, or lines held.
    Warning,
    /// An error or worse, or a crash.
    Error,
}

/// The runs of `spans` (each its start, end and severity, in order and
/// touching): neighbours of one severity joined into one, and those of no
/// severity left out. Each run is as long as what it covers.
pub fn runs(spans: impl Iterator<Item = (f32, f32, Severity)>) -> Vec<(f32, f32, Severity)> {
    let mut out: Vec<(f32, f32, Severity)> = Vec::new();
    for (from, to, kind) in spans {
        if kind == Severity::None {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.2 == kind && (from - last.1).abs() < 1e-3 => last.1 = to,
            _ => out.push((from, to, kind)),
        }
    }
    out
}

/// How bad a line of the table is.
#[must_use]
pub fn line_severity(app: &App, line: &Line) -> Severity {
    let of = |index: usize| {
        app.log.entries.get(index).map_or(Severity::None, |entry| {
            if entry.is_error() || entry.row.kind == crate::data::Kind::Crash {
                Severity::Error
            } else if entry.is_warning() {
                Severity::Warning
            } else {
                Severity::None
            }
        })
    };
    match line {
        Line::Held(..) => Severity::Warning,
        Line::Mark(_) => Severity::None,
        Line::Record(index) | Line::Note(index) | Line::Detail(index, _) => of(*index),
        Line::Fold { first, .. } => of(*first),
    }
}

/// The most bars the activity chart draws.
pub const BARS: usize = 120;

/// The Overview's buckets of time, and their width.
#[must_use]
pub fn buckets(app: &App) -> (Vec<Bucket>, u64) {
    let (start, span) = app.extent();
    let width = stats::bucket_width(span, 10_000_000, BARS);
    let count = usize::try_from(span / width + 1).unwrap_or(1);
    (
        stats::buckets(app.log.of_boot(app.boot), start, width, count),
        width,
    )
}

/// The tiles' sparklines, in the row of tiles' coordinates.
#[must_use]
pub fn sparks(app: &Shared) -> CanvasPainter {
    painter(app, |app, c| {
        let t = Theme::of(app.mode);
        let (row_w, _) = c.p.size();
        let (buckets, _) = buckets(app);
        let w = (row_w - 3.0 * 16.0) / 4.0;
        let series: [(Vec<f32>, Color); 4] = [
            (
                buckets.iter().map(|b| b.errors as f32).collect(),
                theme::RED,
            ),
            (
                buckets.iter().map(|b| b.warnings as f32).collect(),
                theme::AMBER,
            ),
            (buckets.iter().map(|b| b.total() as f32).collect(), t.ink),
            (buckets.iter().map(|b| b.held as f32).collect(), t.ink4),
        ];
        for (index, (series, stroke)) in series.iter().enumerate() {
            let x = (w + 16.0) * index as f32 + 20.0;
            spark(c, Rect::new(x, 58.0, w - 40.0, 30.0), series, *stroke);
        }
    })
}

/// A sparkline of `series` in `place`: a line and the area under it.
fn spark(c: &mut Pen<'_, '_>, place: Rect, series: &[f32], ink: Color) {
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

/// The activity chart, in its card's coordinates: a bar each bucket,
/// grey for its lines, an amber cap for warnings and a red mark for
/// errors; the window shaded; the boot before as outlines when asked.
#[must_use]
pub fn activity(app: &Shared) -> CanvasPainter {
    painter(app, |app, c| {
        let t = Theme::of(app.mode);
        let (w, h) = c.p.size();
        let place = Rect::new(0.0, 0.0, w, h);
        let (buckets, width) = buckets(app);
        let previous = app.boot.checked_sub(1);
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
        if !app.whole_boot() {
            let (start, span) = app.extent();
            let (from, to) = app.window();
            let fx =
                |at: u64| chart.x + chart.w * ((at - start) as f32 / span as f32).clamp(0.0, 1.0);
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
        let font = mono(10.5, 400);
        for step in 0..4 {
            let i = (n.saturating_sub(1)) * step / 3;
            let at = start + width * i as u64;
            let text = clock(at, zone);
            let x = chart.x + slot * i as f32;
            let tx = if step == 3 {
                x + slot - c.width(&font, &text)
            } else {
                x
            };
            let _ = c.text(
                &font,
                &text,
                (tx, chart.bottom() + 10.0, 14.0),
                t.ink4,
                None,
            );
        }
        if app.compare && previous.is_some() {
            let _ = c.text(
                &sans(11.0, 400),
                "outlined: boot −1, from its own start",
                (chart.x, place.bottom() - 22.0, 14.0),
                t.ink4,
                None,
            );
        }
    })
}

/// A bucket's width in words.
#[must_use]
pub fn per(width: u64) -> String {
    let seconds = width / 1_000_000;
    if seconds.is_multiple_of(60) && seconds >= 60 {
        format!("{} min", seconds / 60)
    } else {
        format!("{seconds} s")
    }
}

/// The log rate, in its card's coordinates: lines a second per bucket,
/// the limiter's chatter limit dashed, and where the limiter held lines
/// shaded.
#[must_use]
pub fn rate(app: &Shared) -> CanvasPainter {
    painter(app, |app, c| {
        let t = Theme::of(app.mode);
        let (w, h) = c.p.size();
        let place = Rect::new(0.0, 0.0, w, h);
        let (buckets, width) = buckets(app);
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
            &mono(10.5, 400),
            &format!("{most:.0}/s"),
            (chart.x, chart.y - 16.0, 14.0),
            t.ink4,
            None,
        );
    })
}

/// The strip beside the table: where errors and warnings are among the
/// lines, a block for each run of lines alike, as tall as the lines it
/// stands for and at least a pixel. The part in view is an outline the
/// page draws over it.
#[must_use]
pub fn minimap(app: &Shared) -> CanvasPainter {
    painter(app, |app, c| {
        let (w, h) = c.p.size();
        let place = Rect::new(0.0, 0.0, w, h);
        let t = Theme::of(app.mode);
        c.fill(place, 3.0, t.bg2);
        // Each line's share of the strip, as tall as the line is; lines
        // alike in a row one block.
        let total = app.table_height.max(1.0);
        let scale = place.h / total;
        let spans = app.lines.iter().zip(&app.tops).map(|(line, &top)| {
            (
                top * scale,
                (top + line.height()) * scale,
                line_severity(app, line),
            )
        });
        for (from, to, kind) in runs(spans) {
            let ink = match kind {
                Severity::Error => theme::RED,
                _ => theme::AMBER,
            };
            c.fill(
                Rect::new(place.x, place.y + from, place.w, (to - from).max(1.0)),
                0.0,
                ink,
            );
        }
    })
}
