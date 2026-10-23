//! One frame: the header, the sidebar and the page, drawn whole from the
//! [`App`], back to front, after the design (`Lode App.dc.html`): its
//! sizes, its palette ([`crate::theme`]) and its two faces.

mod logs;
mod overview;

use crate::app::{Action, App, Origin, Page, ZOOMS, clock, clock_hm};
use crate::data::{Kind, level_name};
use crate::draw::{Canvas, Rect, mono, sans};
use crate::lines::group;
use crate::stats::Lane;
use crate::theme::{self, Color, Mode, Theme};

/// The header's height.
pub const HEADER: f32 = 38.0;
/// The sidebar's width.
pub const SIDEBAR: f32 = 220.0;
/// The page's margin left and right.
pub const MARGIN: f32 = 36.0;
/// The record pane's width.
pub const DETAIL: f32 = 392.0;
/// The track card's height.
pub const TRACK: f32 = 148.0;
/// A retention class's budget when nothing says otherwise: lode's
/// `system` class, 1 GiB (`docs/LOGGING.md` §8.5).
pub const BUDGET: u64 = 1 << 30;

/// The canvas the viewer draws on.
pub type Frame<'a, 'p> = Canvas<'a, 'p, Action>;

/// Draw `app` whole.
pub fn paint(app: &App, c: &mut Frame<'_, '_>) {
    let t = Theme::of(app.mode);
    let (width, height) = c.size();
    c.clear(t.bg);
    let page = Rect::new(SIDEBAR, HEADER, width - SIDEBAR, height - HEADER);
    match app.page {
        Page::Overview => overview::paint(app, c, &t, page),
        Page::Logs => logs::paint(app, c, &t, page),
    }
    sidebar(app, c, &t, Rect::new(0.0, HEADER, SIDEBAR, height - HEADER));
    header(app, c, &t, width);
    if app.boot_menu {
        boot_menu(app, c, &t);
    }
    if app.machine_menu {
        machine_menu(app, c, &t);
    }
    if let Some(toast) = &app.toast {
        let w = c.width(sans(12.5, 500), toast) + 28.0;
        let place = Rect::new((width - w) / 2.0, height - 46.0, w, 30.0);
        c.fill(place, 15.0, t.ink);
        c.text_center(sans(12.5, 500), toast, place, t.inv);
    }
    if c.text.faces() == 0 {
        // No font: say so in shapes, the one way left.
        c.fill(Rect::new(0.0, 0.0, width, 4.0), 0.0, theme::RED);
    }
}

/// The bar across the top: the name, where the records are from, and the
/// palette's switch.
fn header(app: &App, c: &mut Frame<'_, '_>, t: &Theme, width: f32) {
    let bar = Rect::new(0.0, 0.0, width, HEADER);
    c.fill(bar, 0.0, t.bg3);
    c.fill(Rect::new(0.0, HEADER - 1.0, width, 1.0), 0.0, t.line);
    let logo = Rect::new(14.0, 11.0, 16.0, 16.0);
    c.fill(logo, 4.0, t.ink);
    c.text_center(mono(10.0, 600), "L", logo, t.inv);
    let mut x = 40.0;
    x += c.text(sans(13.0, 600), "lode", (x, 0.0, HEADER), t.ink, None) + 10.0;
    let host = app
        .boot()
        .map_or_else(String::new, |boot| boot.host.clone());
    let offset = app.log.offset_of(app.boot);
    let place = format!(
        "{} · {} · boot {}",
        if host.is_empty() { "ferrix" } else { &host },
        app.origin_name,
        offset
    );
    let _ = c.text(
        sans(13.0, 400),
        &place,
        (x, 0.0, HEADER),
        t.ink4,
        Some(width - x - 200.0),
    );
    // Light | Dark.
    let group = Rect::new(width - 132.0, 8.0, 120.0, 22.0);
    c.boxed(group, 6.0, t.bg3, t.line3);
    for (index, (label, mode)) in [("Light", Mode::Light), ("Dark", Mode::Dark)]
        .into_iter()
        .enumerate()
    {
        let half = Rect::new(group.x + 60.0 * index as f32, group.y, 60.0, group.h);
        let on = app.mode == mode;
        if on {
            c.fill(half.inset(1.0), 5.0, t.ink);
        }
        let ink = if on { t.inv } else { t.ink2 };
        // The sun and the moon are drawn: the design's glyphs are not in
        // Plex.
        let mark_x = half.x + 12.0;
        if mode == Mode::Light {
            c.dot(mark_x, half.y + 11.0, 3.0, ink);
        } else {
            c.dot(mark_x, half.y + 11.0, 3.5, ink);
            c.dot(
                mark_x + 1.6,
                half.y + 9.8,
                3.0,
                if on { t.ink } else { t.bg3 },
            );
        }
        let _ = c.text(
            sans(12.0, 400),
            label,
            (half.x + 19.0, half.y, half.h),
            ink,
            None,
        );
        c.hit(half, Action::Theme(mode));
    }
}

/// The sidebar: the pages, the saved queries and lode's state.
fn sidebar(app: &App, c: &mut Frame<'_, '_>, t: &Theme, bar: Rect) {
    c.fill(bar, 0.0, t.bg2);
    c.fill(Rect::new(bar.right() - 1.0, bar.y, 1.0, bar.h), 0.0, t.line);
    let mut y = bar.y + 22.0;
    let crashes = app.crashes();
    let pages: [(&str, Action, bool, usize); 3] = [
        (
            "Overview",
            Action::Page(Page::Overview),
            app.page == Page::Overview,
            0,
        ),
        ("Logs", Action::Page(Page::Logs), app.page == Page::Logs, 0),
        ("Crashes", Action::Crashes, false, crashes),
    ];
    for (label, action, on, badge) in pages {
        let place = Rect::new(14.0, y, SIDEBAR - 28.0, 36.0);
        if on {
            c.fill(place, 7.0, t.ink);
        }
        let _ = c.text(
            sans(13.5, if on { 500 } else { 400 }),
            label,
            (place.x + 12.0, place.y, place.h),
            if on { t.inv } else { t.ink2 },
            None,
        );
        if badge > 0 {
            let text = badge.to_string();
            let w = c.width(mono(11.0, 600), &text).max(8.0) + 10.0;
            let pill = Rect::new(place.right() - 12.0 - w, place.y + 9.0, w, 18.0);
            c.fill(pill, 9.0, theme::RED);
            c.text_center(mono(11.0, 600), &text, pill, theme::WHITE);
        }
        c.hit(place, action);
        y += 40.0;
    }
    y += 16.0;
    let _ = c.text(
        sans(11.0, 600),
        "SAVED QUERIES",
        (26.0, y, 20.0),
        t.ink5,
        None,
    );
    y += 28.0;
    for (index, saved) in app.saved.iter().enumerate() {
        let place = Rect::new(14.0, y, SIDEBAR - 28.0, 30.0);
        if app.saved_in_use == Some(index) {
            c.fill(place, 6.0, t.hover);
        }
        let _ = c.text(
            mono(12.5, 400),
            &format!("@{}", saved.name),
            (place.x + 12.0, place.y, place.h),
            t.ink2,
            Some(place.w - 24.0),
        );
        c.hit(place, Action::Saved(index));
        y += 32.0;
    }
    let new = Rect::new(14.0, y, SIDEBAR - 28.0, 30.0);
    let _ = c.text(
        mono(12.5, 400),
        "+ new",
        (new.x + 12.0, new.y, new.h),
        t.blue,
        None,
    );
    c.hit(new, Action::NewSaved);
    // lode's state at the foot.
    let foot = bar.bottom() - 92.0;
    let mut notice_y = foot - 8.0;
    for notice in app.notices.iter().rev().take(3) {
        notice_y -= 34.0;
        let _ = c.text(
            sans(11.0, 400),
            notice,
            (26.0, notice_y, 16.0),
            t.red_ink,
            Some(SIDEBAR - 40.0),
        );
    }
    c.fill(Rect::new(14.0, foot, SIDEBAR - 28.0, 1.0), 0.0, t.line);
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
    c.dot(
        30.0,
        foot + 26.0,
        4.0,
        if running { theme::GREEN } else { theme::IDLE },
    );
    let _ = c.text(
        sans(12.0, 400),
        &line,
        (40.0, foot + 16.0, 20.0),
        t.ink3,
        Some(SIDEBAR - 54.0),
    );
    let meter = Rect::new(26.0, foot + 44.0, SIDEBAR - 52.0, 4.0);
    c.fill(meter, 2.0, t.line);
    let used = match &app.origin {
        Origin::Store(_) => (app.status.used as f32 / BUDGET as f32).clamp(0.0, 1.0),
        Origin::File(_) => 0.0,
    };
    c.fill(
        Rect::new(meter.x, meter.y, (meter.w * used).max(4.0), 4.0),
        2.0,
        t.ink3,
    );
    let _ = c.text(
        sans(12.0, 400),
        &tier,
        (26.0, foot + 56.0, 20.0),
        t.ink4,
        None,
    );
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

/// The page's heading: its name, the boot picker and, at the right, the
/// Live state. Answers where the right-hand side begins.
pub(super) fn page_heading(
    app: &App,
    c: &mut Frame<'_, '_>,
    t: &Theme,
    area: Rect,
    title: &str,
) -> f32 {
    let y = area.y;
    let mut x = area.x + c.text(sans(18.0, 600), title, (area.x, y, 30.0), t.ink, None) + 16.0;
    // The machine, when the viewer holds several (lode-receive's stores).
    if !app.machines.is_empty() {
        let name = app
            .machines
            .get(app.machine)
            .map_or("?", |machine| machine.name.as_str());
        let w = c.width(sans(13.0, 500), name) + 40.0;
        let picker = Rect::new(x, y + 1.0, w, 28.0);
        c.boxed(picker, 6.0, t.bg, t.line3);
        c.dot(picker.x + 12.0, picker.y + 14.0, 3.5, theme::GREEN);
        let _ = c.text(
            sans(13.0, 500),
            name,
            (picker.x + 20.0, picker.y, picker.h),
            t.ink,
            None,
        );
        let cx = picker.right() - 12.0;
        c.polygon(
            &[(cx - 4.0, y + 13.0), (cx + 4.0, y + 13.0), (cx, y + 18.0)],
            t.ink,
        );
        c.hit(picker, Action::MachineMenu);
        x = picker.right() + 8.0;
    }
    let (from, to) = app
        .boot()
        .map_or((0, 0), |boot| (boot.first_us, boot.last_us));
    let offset = app.offset_of_boot_label();
    let end = if app.boot + 1 == app.log.boots.len() && app.live {
        "now".to_owned()
    } else {
        clock(to, app.zone())
    };
    let label = format!("Boot {offset}");
    let rest = format!(" · {} → {end}", clock(from, app.zone()));
    let w = c.width(sans(13.0, 400), &label) + c.width(sans(13.0, 400), &rest) + 36.0;
    let picker = Rect::new(x, y + 1.0, w, 28.0);
    c.boxed(picker, 6.0, t.bg, t.line3);
    let lx = picker.x
        + 10.0
        + c.text(
            sans(13.0, 400),
            &label,
            (picker.x + 10.0, picker.y, picker.h),
            t.ink,
            None,
        );
    let _ = c.text(
        sans(13.0, 400),
        &rest,
        (lx, picker.y, picker.h),
        t.ink4,
        None,
    );
    let cx = picker.right() - 14.0;
    c.polygon(
        &[(cx - 4.0, y + 13.0), (cx + 4.0, y + 13.0), (cx, y + 18.0)],
        t.ink,
    );
    c.hit(picker, Action::BootMenu);
    // Live, or why not.
    let (dot, word, ink) = match (&app.origin, app.live) {
        (Origin::Store(_), true) => (theme::GREEN, "Live", theme::GREEN),
        (Origin::Store(_), false) => (theme::IDLE, "Paused", t.ink4),
        (Origin::File(_), _) => (theme::IDLE, "From a file", t.ink4),
    };
    let w = c.width(sans(13.0, 500), word);
    let right = area.right();
    let _ = c.text(sans(13.0, 500), word, (right - w, y, 30.0), ink, None);
    c.dot(right - w - 10.0, y + 15.0, 4.0, dot);
    right - w - 24.0
}

/// The boots, to pick one.
fn boot_menu(app: &App, c: &mut Frame<'_, '_>, t: &Theme) {
    let x = SIDEBAR + MARGIN + if app.machines.is_empty() { 80.0 } else { 220.0 };
    let y = HEADER + 28.0 + 34.0;
    let count = app.log.boots.len();
    let place = Rect::new(x, y, 420.0, 12.0 + 32.0 * count.min(12) as f32);
    c.fill(
        Rect::new(place.x + 2.0, place.y + 3.0, place.w, place.h),
        8.0,
        t.fill,
    );
    c.boxed(place, 8.0, t.bg, t.line3);
    for (row, (index, boot)) in app.log.boots.iter().enumerate().rev().take(12).enumerate() {
        let line = Rect::new(
            place.x + 6.0,
            place.y + 6.0 + 32.0 * row as f32,
            place.w - 12.0,
            32.0,
        );
        if index == app.boot {
            c.fill(line, 5.0, t.hover);
        }
        let zone = app.zone();
        let _ = c.text(
            sans(13.0, 500),
            &format!("Boot {}", app.log.offset_of(index)),
            (line.x + 10.0, line.y, line.h),
            t.ink,
            None,
        );
        let _ = c.text(
            mono(12.0, 400),
            &format!(
                "{} → {} · {} records · {}",
                clock(boot.first_us, zone),
                clock(boot.last_us, zone),
                group(boot.range.len() as u64),
                &boot.hex()[..8]
            ),
            (line.x + 76.0, line.y, line.h),
            t.ink4,
            Some(line.w - 86.0),
        );
        c.hit(line, Action::Boot(index));
    }
}

/// The machines, to pick one.
fn machine_menu(app: &App, c: &mut Frame<'_, '_>, t: &Theme) {
    let x = SIDEBAR + MARGIN + 80.0;
    let y = HEADER + 28.0 + 34.0;
    let count = app.machines.len().min(12);
    let place = Rect::new(x, y, 380.0, 12.0 + 32.0 * count as f32);
    c.fill(
        Rect::new(place.x + 2.0, place.y + 3.0, place.w, place.h),
        8.0,
        t.fill,
    );
    c.boxed(place, 8.0, t.bg, t.line3);
    for (row, machine) in app.machines.iter().take(12).enumerate() {
        let line = Rect::new(
            place.x + 6.0,
            place.y + 6.0 + 32.0 * row as f32,
            place.w - 12.0,
            32.0,
        );
        if row == app.machine {
            c.fill(line, 5.0, t.hover);
        }
        let _ = c.text(
            sans(13.0, 500),
            &machine.name,
            (line.x + 10.0, line.y, line.h),
            t.ink,
            Some(150.0),
        );
        let _ = c.text(
            mono(12.0, 400),
            &machine.note,
            (line.x + 170.0, line.y, line.h),
            t.ink4,
            Some(line.w - 180.0),
        );
        c.hit(line, Action::Machine(row));
    }
}

/// A button with an edge.
pub(super) fn button(c: &mut Frame<'_, '_>, t: &Theme, place: Rect, label: &str, action: Action) {
    c.boxed(place, 5.0, t.bg, t.line3);
    c.text_center(mono(12.0, 400), label, place, t.ink2);
    c.hit(place, action);
}

/// The time track: the controls, the swim lanes, the window's brush and
/// the cursor.
pub(super) fn track(app: &App, c: &mut Frame<'_, '_>, t: &Theme, card: Rect) {
    c.boxed(card, 12.0, t.bg2, t.line);
    let row_y = card.y + 14.0;
    let mut x = card.x + 18.0;
    // The step buttons: to the problem before, back, forward, to the next.
    let prev = Rect::new(x, row_y, 40.0, 24.0);
    c.boxed(prev, 5.0, t.bg, t.line3);
    skip_mark(c, prev.x + 9.0, prev.y + 12.0, false, t.ink2);
    c.dot(prev.x + 28.0, prev.y + 12.0, 3.5, theme::RED);
    c.hit(prev, Action::PrevProblem);
    x += 46.0;
    button(
        c,
        t,
        Rect::new(x, row_y, 46.0, 24.0),
        "−10s",
        Action::Step(-10),
    );
    x += 52.0;
    button(
        c,
        t,
        Rect::new(x, row_y, 46.0, 24.0),
        "+10s",
        Action::Step(10),
    );
    x += 52.0;
    let next = Rect::new(x, row_y, 40.0, 24.0);
    c.boxed(next, 5.0, t.bg, t.line3);
    c.dot(next.x + 12.0, next.y + 12.0, 3.5, theme::RED);
    skip_mark(c, next.x + 22.0, next.y + 12.0, true, t.ink2);
    c.hit(next, Action::NextProblem);
    x += 52.0;
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
    let _ = c.text(mono(12.0, 400), &label, (x, row_y, 24.0), t.ink, None);
    // Live, then the zooms, from the right.
    let live = Rect::new(card.right() - 18.0 - 52.0, row_y, 52.0, 24.0);
    c.fill(live, 5.0, if app.live { theme::GREEN } else { theme::IDLE });
    c.dot(live.x + 12.0, live.y + 12.0, 3.5, t.bg);
    let _ = c.text(
        sans(12.0, 400),
        "Live",
        (live.x + 20.0, live.y, live.h),
        theme::WHITE,
        None,
    );
    c.hit(live, Action::Live);
    let zooms = Rect::new(live.x - 8.0 - 156.0, row_y, 156.0, 24.0);
    c.boxed(zooms, 5.0, t.bg, t.line3);
    let mut zx = zooms.x + 1.0;
    for (label, seconds) in ZOOMS {
        let w = if seconds == 0 { 45.0 } else { 36.0 };
        let cell = Rect::new(zx, zooms.y + 1.0, w, 22.0);
        let on = if seconds == 0 {
            app.whole_boot()
        } else {
            app.span == seconds * 1_000_000 && !app.whole_boot()
        };
        if on {
            c.fill(cell, 3.0, t.ink);
        }
        c.text_center(
            mono(12.0, 400),
            label,
            cell,
            if on { t.inv } else { t.ink2 },
        );
        c.hit(cell, Action::Zoom(seconds));
        zx += w;
    }
    // The lanes.
    let lanes_top = card.y + 50.0;
    let left = card.x + 98.0;
    let right = card.right() - 18.0;
    let width = right - left;
    let (start, span) = app.extent();
    let entries = app.log.of_boot(app.boot);
    let columns = (width / 3.0).max(1.0) as usize;
    let lanes = crate::stats::lanes(entries, &app.log.sources, start, span, columns);
    let lane_h = 17.0;
    let lane_rows = lanes.len().min(5);
    for (row, lane) in lanes.iter().take(lane_rows).enumerate() {
        let y = lanes_top + lane_h * row as f32;
        let _ = c.text(
            mono(11.0, 400),
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
        let w = c.width(mono(10.5, 400), &text);
        let tx = (left + width * fraction - if step == 4 { w } else { 0.0 }).max(left);
        let _ = c.text(mono(10.5, 400), &text, (tx, axis_y, 14.0), ink, None);
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
    c.hit(
        Rect::new(
            left,
            lanes_top - 8.0,
            width,
            lane_h * lane_rows as f32 + 10.0,
        ),
        Action::Track(left, width),
    );
}

/// A skip mark: a bar and a triangle, pointing right when `forward`.
fn skip_mark(c: &mut Frame<'_, '_>, x: f32, y: f32, forward: bool, ink: Color) {
    if forward {
        c.polygon(&[(x, y - 4.0), (x + 6.0, y), (x, y + 4.0)], ink);
        c.fill(Rect::new(x + 6.0, y - 4.0, 1.5, 8.0), 0.0, ink);
    } else {
        c.fill(Rect::new(x, y - 4.0, 1.5, 8.0), 0.0, ink);
        c.polygon(&[(x + 7.5, y - 4.0), (x + 1.5, y), (x + 7.5, y + 4.0)], ink);
    }
}

/// One swim lane: where its source was busy, held, warned and failed.
fn draw_lane(c: &mut Frame<'_, '_>, t: &Theme, lane: &Lane, place: Rect) {
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
    let mut last = f32::MIN;
    for &at in &lane.warnings {
        let x = place.x + place.w * at;
        if x - last >= 6.0 {
            c.dot(x, place.y + 5.0, 3.5, theme::AMBER);
            last = x;
        }
    }
    let mut last = f32::MIN;
    for &at in &lane.errors {
        let x = place.x + place.w * at;
        if x - last >= 7.0 {
            c.dot(x, place.y + 5.0, 4.5, theme::RED);
            last = x;
        }
    }
}

/// A level's badge: a letter in a small square, filled for errors and
/// warnings, outlined for the rest.
pub(super) fn badge(c: &mut Frame<'_, '_>, t: &Theme, x: f32, y: f32, level: u8, kind: Kind) {
    let place = Rect::new(x, y, 16.0, 16.0);
    let letter = match (kind, level) {
        (Kind::Crash, _) => "C",
        (_, 0..=3) => "E",
        (_, 4) => "W",
        (_, 5) => "N",
        (_, 6) => "I",
        (_, 7) => "D",
        _ => "?",
    };
    match level {
        0..=3 => {
            c.fill(place, 3.0, theme::RED);
            c.text_center(mono(10.0, 600), letter, place, theme::WHITE);
        }
        4 => {
            c.fill(place, 3.0, theme::AMBER);
            c.text_center(mono(10.0, 600), letter, place, Color::hex(0x1a_1a_1a));
        }
        _ => {
            let ink = if level == 5 { t.blue } else { t.ink4 };
            c.outline(place, 3.0, 1.0, ink);
            c.text_center(mono(10.0, 600), letter, place, ink);
        }
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

impl App {
    /// The UTC offset times are shown at: the boot's own, as lode stored
    /// it.
    #[must_use]
    pub fn zone(&self) -> i32 {
        self.log
            .of_boot(self.boot)
            .first()
            .map_or(0, |entry| entry.record.utc_offset_s)
    }

    /// The boot's offset as the picker says it.
    #[must_use]
    pub fn offset_of_boot_label(&self) -> i64 {
        self.log.offset_of(self.boot)
    }
}

/// Where the Logs table's lines are, in a window `width` by `height`.
#[must_use]
pub fn logs_table(app: &App, width: f32, height: f32) -> Rect {
    logs::table_area(app, width, height)
}

/// How tall the Overview is.
#[must_use]
pub fn overview_height() -> f32 {
    overview::height()
}
