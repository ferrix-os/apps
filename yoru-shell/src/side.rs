//! A widget sidebar on the screen (`yoru-shell --sidebar left|right`): a
//! layer surface at the edge, two pixels wide until the pointer reaches
//! it, then the column (`ui/sidebar.tole`), which closes again a moment
//! after the pointer leaves it -- unless the side is pinned, when it stays
//! open and keeps its room from the windows. What is on it is the person's
//! file (`crate::widgets`), read again when the other side changes it.

use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

use compositor_toolkit::Event;
use tole::Element as _;
use tole::window::{
    Anchor, KeyboardInteractivity, Layer, LayerOptions, Margin, Window, WindowOptions,
};
use tole::{Color, Ui};

use crate::look::icon;
use crate::quick::{Change, Quick};
use crate::ui::{
    AddRow, Cell, DayRow, HourRow, QuickAccent, QuickSlider, QuickTile, Sidebar, SidebarSignal,
    Tab, TaskRow,
};
use crate::widgets::{self, Arrangement, Kind, Side, Task};
use crate::window::say;

/// The column's width, its margins included.
pub const OPEN: u32 = 336;
/// The edge's strip while closed.
pub const CLOSED: u32 = 2;
/// The dock's room at the foot, which the column stays above.
const DOCK_ROOM: i32 = 66;
/// How long the column stays after the pointer leaves it.
const LINGER: Duration = Duration::from_millis(600);

/// A string as the tree holds it.
fn s(value: impl AsRef<str>) -> Rc<str> {
    Rc::from(value.as_ref())
}

/// What the column shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    /// Which side it is.
    pub side: Side,
    /// What is where.
    pub arrangement: Arrangement,
    /// The to-do list.
    pub tasks: Vec<Task>,
    /// Local time, seconds since 1970.
    pub now: i64,
    /// Whether the column is open.
    pub open: bool,
    /// The weather setting, the place's name, and what was last read.
    pub weather: crate::weather::Weather,
    /// The place's name as Open-Meteo gave it.
    pub place: String,
    /// The forecast last read.
    pub forecast: Option<crate::weather::Forecast>,
    /// When it was read, or why it could not be.
    pub status: String,
    /// The quick settings, as last read.
    pub quick: Quick,
}

/// The tree.
pub struct View {
    /// The tree.
    pub ui: Ui,
    /// The column.
    pub side: Sidebar,
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View").finish_non_exhaustive()
    }
}

/// What a press in the column asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asked {
    /// Show widget `n` of this side.
    Tab(usize),
    /// Pin or unpin.
    Pin,
    /// Move the shown widget to the other side.
    Move,
    /// Take the shown widget away.
    Remove,
    /// Add the `n`th widget on neither side.
    Add(usize),
    /// Tick task `n`, or untick it.
    Toggle(usize),
    /// Delete task `n`.
    Delete(usize),
    /// A new task.
    Task(String),
    /// Weather on, for the place typed.
    WeatherOn(String),
    /// Weather off: no request from now on.
    WeatherOff,
    /// A quick setting changed.
    Quick(Change),
}

impl View {
    /// The tree, drawn with `fonts`.
    #[must_use]
    pub fn new(fonts: tole::Fonts) -> Option<Self> {
        let mut ui = Ui::new(fonts);
        let side = Sidebar::new(&mut ui, None)?;
        for (prop, name) in [
            (&side.ic_check, "check-square"),
            (&side.ic_box, "square"),
            (&side.ic_trash, "trash"),
            (&side.ic_close, "x"),
            (&side.ic_plus, "plus"),
            (&side.ic_calendar, "calendar-blank"),
            (&side.ic_pin, "map-pin"),
            (&side.ic_clock, "clock"),
        ] {
            prop.set(&mut ui, s(icon(name)));
        }
        Some(View { ui, side })
    }

    /// Its size.
    pub fn resize(&mut self, (width, height): (u32, u32)) {
        let common = self.side.common();
        common.width.set(&mut self.ui, width as f32);
        common.height.set(&mut self.ui, height as f32);
        self.side
            .panel_h
            .set(&mut self.ui, (height as f32 - 16.0).max(0.0));
    }

    /// Hand the tree `shown`.
    pub fn show(&mut self, shown: &Shown) {
        let ui = &mut self.ui;
        let v = &self.side;
        let side = shown.side;
        let a = &shown.arrangement;
        v.open.set(ui, shown.open);
        v.right.set(ui, side == Side::Right);
        let current = a.shown(side);
        let tabs = a
            .on(side)
            .iter()
            .map(|kind| Tab {
                icon: s(icon(kind.icon())),
                label: s(kind.label()),
                on: Some(*kind) == current,
            })
            .collect::<Vec<_>>();
        v.tabs.set(ui, tabs);
        let pinned = a.is_pinned(side);
        v.pin_icon.set(
            ui,
            s(icon(if pinned { "push-pin-slash" } else { "push-pin" })),
        );
        v.pin_title.set(
            ui,
            s(if pinned {
                "Unpin sidebar · show on hover only"
            } else {
                "Pin sidebar · reserve its space"
            }),
        );
        v.label.set(
            ui,
            s(current.map_or(String::new(), |k| k.label().to_uppercase())),
        );
        v.move_icon.set(
            ui,
            s(icon(if side == Side::Left {
                "arrow-right"
            } else {
                "arrow-left"
            })),
        );
        v.kind.set(
            ui,
            match current {
                Some(Kind::Clock) => 0,
                Some(Kind::Weather) => 1,
                Some(Kind::Settings) => 2,
                Some(Kind::Todo) => 3,
                None => -1,
            },
        );
        let of_day = shown.now.rem_euclid(86_400);
        v.time.set(
            ui,
            s(format!("{:02}:{:02}", of_day / 3600, of_day / 60 % 60)),
        );
        v.seconds.set(ui, s(format!("{:02}", of_day % 60)));
        v.date.set(ui, s(crate::model::Clock::at(shown.now).date));
        let (title, cells) = widgets::month(shown.now);
        v.month_title.set(ui, s(title.to_uppercase()));
        let cells = cells
            .into_iter()
            .map(|(day, in_month, today)| Cell {
                day: s(day.to_string()),
                in_month,
                today,
            })
            .collect::<Vec<_>>();
        v.cells.set(ui, cells);
        let rows = shown
            .tasks
            .iter()
            .map(|task| TaskRow {
                text: s(&task.text),
                done: task.done,
            })
            .collect::<Vec<_>>();
        v.tasks.set(ui, rows);
        v.count_line.set(ui, s(widgets::done_line(&shown.tasks)));
        let adds = a
            .hidden()
            .into_iter()
            .map(|kind| AddRow {
                label: s(kind.label()),
            })
            .collect::<Vec<_>>();
        v.adds.set(ui, adds);
        self.show_weather(shown);
        self.show_quick(&shown.quick);
    }

    /// The quick settings' part: the sliders, the tiles, the accents.
    fn show_quick(&mut self, quick: &Quick) {
        let ui = &mut self.ui;
        let v = &self.side;
        let slider =
            |name: &str, label: &str, value: u32, shown: String, enabled: bool| QuickSlider {
                icon: s(icon(name)),
                label: s(label),
                value: value as f32,
                shown: s(shown),
                enabled,
            };
        let brightness = quick
            .backlight
            .as_ref()
            .map(crate::quick::Backlight::percent);
        v.q_sliders.set(
            ui,
            vec![
                slider(
                    "sun",
                    "Brightness",
                    brightness.unwrap_or(0),
                    brightness.map_or_else(|| "No backlight".to_owned(), |p| format!("{p}%")),
                    brightness.is_some(),
                ),
                slider(
                    "speaker-high",
                    "Volume",
                    0,
                    "Not here yet".to_owned(),
                    false,
                ),
                slider(
                    "microphone",
                    "Microphone",
                    0,
                    "No capture yet".to_owned(),
                    false,
                ),
            ],
        );
        let on_off = |on: bool| if on { "On" } else { "Off" };
        let tile = |name: &str, label: &str, sub: &str, on: bool, enabled: bool| QuickTile {
            icon: s(icon(name)),
            label: s(label),
            sub: s(sub),
            on,
            enabled,
        };
        v.q_tiles.set(
            ui,
            vec![
                tile(
                    "wifi-high",
                    "Wi-Fi",
                    if quick.wireless {
                        "Not yet"
                    } else {
                        "None found"
                    },
                    false,
                    false,
                ),
                tile("bluetooth", "Bluetooth", "Not yet", false, false),
                tile(
                    "bell-slash",
                    "Do not disturb",
                    on_off(quick.quiet),
                    quick.quiet,
                    true,
                ),
                tile("moon", "Night light", "Not yet", false, false),
                tile(
                    "crosshair-simple",
                    "Focus mode",
                    on_off(quick.focus),
                    quick.focus,
                    true,
                ),
                tile(
                    "squares-four",
                    "Auto-tile",
                    if quick.tiled { "Tiling" } else { "Off" },
                    quick.tiled,
                    true,
                ),
            ],
        );
        let accents = crate::quick::ACCENTS
            .iter()
            .enumerate()
            .map(|(n, (name, hue))| {
                let [r, g, b] = crate::quick::oklch(0.72, 0.13, *hue);
                QuickAccent {
                    name: s(name),
                    colour: Color::rgba(r, g, b, 0xff),
                    on: quick.accent == Some(n),
                }
            })
            .collect::<Vec<_>>();
        v.q_accents.set(ui, accents);
        v.q_line.set(ui, s(&quick.said));
    }

    /// The weather tab's part of `shown`.
    fn show_weather(&mut self, shown: &Shown) {
        use crate::weather::{condition, degrees};
        let ui = &mut self.ui;
        let v = &self.side;
        v.weather_on.set(ui, shown.weather.on);
        v.w_place.set(ui, s(&shown.place));
        v.w_status.set(ui, s(&shown.status));
        let Some(made) = &shown.forecast else {
            v.w_cond.set(ui, s(""));
            v.w_now.set(ui, s("–"));
            v.w_icon.set(ui, s(""));
            v.w_line.set(ui, s(""));
            v.w_days.set(ui, Vec::new());
            v.w_hours.set(ui, Vec::new());
            return;
        };
        let (label, glyph) = condition(made.code, made.day);
        v.w_cond.set(ui, s(label));
        v.w_now.set(ui, s(degrees(made.now)));
        v.w_icon.set(ui, s(icon(glyph)));
        let (high, low) = made
            .days
            .first()
            .map_or((f64::NAN, f64::NAN), |(_, _, h, l)| (*h, *l));
        v.w_line.set(
            ui,
            s(format!(
                "Feels like {} · H {} · L {}",
                degrees(made.feels),
                degrees(high),
                degrees(low)
            )),
        );
        let days = made
            .days
            .iter()
            .take(7)
            .map(|(day, code, high, low)| DayRow {
                day: s(day),
                icon: s(icon(condition(*code, true).1)),
                high: s(degrees(*high)),
                low: s(degrees(*low)),
            })
            .collect::<Vec<_>>();
        v.w_days.set(ui, days);
        let hours = made
            .hours
            .iter()
            .take(8)
            .map(|(hour, code, temp)| HourRow {
                hour: s(hour),
                icon: s(icon(condition(*code, made.day).1)),
                temp: s(degrees(*temp)),
            })
            .collect::<Vec<_>>();
        v.w_hours.set(ui, hours);
    }

    /// What the tree asked since the last call.
    pub fn take(&mut self) -> Vec<Asked> {
        let emitted = self.ui.take_signals();
        let mut asked = Vec::new();
        let index = |i: i32| usize::try_from(i).ok();
        for each in &emitted {
            let Some(signal) = self.side.signal(each) else {
                continue;
            };
            asked.extend(match signal {
                SidebarSignal::Tab(i) => index(i).map(Asked::Tab),
                SidebarSignal::Pin => Some(Asked::Pin),
                SidebarSignal::MoveIt => Some(Asked::Move),
                SidebarSignal::RemoveIt => Some(Asked::Remove),
                SidebarSignal::Add(i) => index(i).map(Asked::Add),
                SidebarSignal::Toggle(i) => index(i).map(Asked::Toggle),
                SidebarSignal::Drop(i) => index(i).map(Asked::Delete),
                SidebarSignal::Task(text) => {
                    let text = text.trim().to_owned();
                    (!text.is_empty()).then_some(Asked::Task(text))
                }
                SidebarSignal::WeatherStart(text) => Some(Asked::WeatherOn(text.trim().to_owned())),
                SidebarSignal::WeatherStop => Some(Asked::WeatherOff),
                SidebarSignal::QuickSlide(i, value) => match i {
                    0 => Some(Asked::Quick(Change::Brightness(
                        value.clamp(0.0, 100.0).round() as u32,
                    ))),
                    _ => None,
                },
                SidebarSignal::QuickTile(i) => match i {
                    2 => Some(Asked::Quick(Change::Quiet)),
                    4 => Some(Asked::Quick(Change::Focus)),
                    5 => Some(Asked::Quick(Change::Tile)),
                    _ => None,
                },
                SidebarSignal::QuickAccent(i) => index(i).map(|n| Asked::Quick(Change::Accent(n))),
            });
        }
        asked
    }
}

/// Do what was asked of `shown`; whether the arrangement and whether the
/// tasks changed.
pub fn apply(shown: &mut Shown, asked: &Asked) -> (bool, bool) {
    let side = shown.side;
    let a = &mut shown.arrangement;
    match asked {
        Asked::Tab(n) => {
            if let Some(kind) = a.on(side).get(*n).copied() {
                a.show(side, kind);
            }
            (true, false)
        }
        Asked::Pin => {
            a.toggle_pin(side);
            (true, false)
        }
        Asked::Move => {
            a.move_shown(side);
            (true, false)
        }
        Asked::Remove => {
            a.remove_shown(side);
            (true, false)
        }
        Asked::Add(n) => {
            if let Some(kind) = a.hidden().get(*n).copied() {
                a.add(side, kind);
            }
            (true, false)
        }
        Asked::Toggle(n) => {
            if let Some(task) = shown.tasks.get_mut(*n) {
                task.done = !task.done;
            }
            (false, true)
        }
        Asked::Delete(n) => {
            if *n < shown.tasks.len() {
                let _ = shown.tasks.remove(*n);
            }
            (false, true)
        }
        Asked::Task(text) => {
            shown.tasks.push(Task {
                text: text.clone(),
                done: false,
            });
            (false, true)
        }
        Asked::WeatherOn(typed) => {
            shown.weather.place = crate::weather::place(typed);
            shown.weather.on = shown.weather.place.is_some();
            shown.place = typed.clone();
            shown.forecast = None;
            shown.status = "Asking Open-Meteo…".to_owned();
            (false, false)
        }
        Asked::WeatherOff => {
            shown.weather.on = false;
            shown.forecast = None;
            shown.status.clear();
            (false, false)
        }
        Asked::Quick(_) => (false, false),
    }
}

/// Local time in seconds.
fn now() -> i64 {
    let utc = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0));
    utc + crate::model::offset(std::env::var("TZ").ok().as_deref())
}

/// A file's time, to notice the other side's changes.
fn stamp(path: Option<&std::path::Path>) -> Option<SystemTime> {
    std::fs::metadata(path?).and_then(|m| m.modified()).ok()
}

/// The layer for `side`, open or closed, pinned or not.
fn layer(side: Side, open: bool, pinned: bool) -> LayerOptions {
    let edge = match side {
        Side::Left => Anchor::LEFT,
        Side::Right => Anchor::RIGHT,
    };
    let width = if open { OPEN } else { CLOSED };
    LayerOptions {
        output: None,
        layer: Layer::Top,
        namespace: format!("yoru-sidebar-{}", side.id()),
        size: (width, 0),
        anchor: edge.with(Anchor::TOP).with(Anchor::BOTTOM),
        exclusive_zone: if pinned {
            i32::try_from(OPEN).unwrap_or(336)
        } else {
            0
        },
        margin: Margin {
            top: 0,
            right: 0,
            bottom: DOCK_ROOM,
            left: 0,
        },
        keyboard: if open {
            KeyboardInteractivity::OnDemand
        } else {
            KeyboardInteractivity::None
        },
    }
}

/// Run `side`'s sidebar until the compositor goes away; the exit status.
#[must_use]
pub fn run(side: Side) -> i32 {
    let file = widgets::arrangement_file();
    let todo = widgets::todo_file();
    let read_arrangement = || {
        Arrangement::read(
            &file
                .as_ref()
                .and_then(|f| std::fs::read_to_string(f).ok())
                .unwrap_or_default(),
        )
    };
    let read_tasks = || {
        widgets::tasks(
            &todo
                .as_ref()
                .and_then(|f| std::fs::read_to_string(f).ok())
                .unwrap_or_default(),
        )
    };
    let arrangement = read_arrangement();
    let pinned = arrangement.is_pinned(side);
    let mut shown = Shown {
        side,
        arrangement,
        tasks: read_tasks(),
        now: now(),
        open: pinned,
        weather: crate::weather::Weather::default(),
        place: String::new(),
        forecast: None,
        status: String::new(),
        quick: Quick::default(),
    };
    let hypr = crate::hypr::Hypr::from_env();
    shown.quick = crate::quick::read(hypr.as_ref());
    let weather_file = widgets::weather_file();
    let (weather, place) = crate::weather::Weather::read(
        &weather_file
            .as_ref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .unwrap_or_default(),
    );
    shown.weather = weather;
    shown.place = place;
    // The forecast is asked for only while it is on and a place was typed,
    // on a thread, again each half hour.
    let (send, answers) = std::sync::mpsc::channel();
    let mut asked_at: Option<Instant> = None;
    let Some(mut view) = View::new(crate::look::fonts()) else {
        say("err: the sidebar's tree could not be made");
        return 1;
    };
    view.show(&shown);
    let options = WindowOptions {
        title: format!("Yoru {} sidebar", side.id()),
        app_id: format!("yoru-sidebar-{}", side.id()),
        size: (OPEN, 600),
        background: Color::rgba(0, 0, 0, 0),
        clipboard: true,
        layer: Some(layer(side, shown.open, pinned)),
        ..WindowOptions::default()
    };
    let mut window = match Window::open(view.side.common(), options) {
        Ok(window) => window,
        Err(error) => {
            say(&format!(
                "err: no compositor to put the sidebar on: {error}"
            ));
            return 1;
        }
    };
    let tick = window
        .client()
        .add_timer(Duration::from_secs(1), Some(Duration::from_secs(1)));
    let mut seen = (stamp(file.as_deref()), stamp(todo.as_deref()));
    let mut left_at: Option<Instant> = None;
    let mut up = false;
    loop {
        let due = shown.weather.on
            && shown.arrangement.on(side).contains(&Kind::Weather)
            && asked_at.is_none_or(|at| at.elapsed() >= Duration::from_secs(1800));
        if due {
            asked_at = Some(Instant::now());
            let weather = shown.weather.clone();
            let send = send.clone();
            let _ = std::thread::Builder::new()
                .name("weather".to_owned())
                .spawn(move || {
                    let _ = send.send(crate::weather::fetch(&weather));
                });
        }
        while let Ok(answer) = answers.try_recv() {
            match answer {
                Ok((found, forecast)) => {
                    if let Some((place, label)) = found {
                        shown.weather.place = Some(place);
                        shown.place = label;
                    }
                    shown.forecast = Some(forecast);
                    let of_day = now().rem_euclid(86_400);
                    shown.status = format!(
                        "From Open-Meteo at {:02}:{:02}",
                        of_day / 3600,
                        of_day / 60 % 60
                    );
                    say(&format!("weather: {} read", shown.place));
                }
                Err(why) => {
                    shown.status = why.clone();
                    say(&format!("weather: {why}"));
                }
            }
            if let Some(f) = &weather_file {
                widgets::keep(f, &shown.weather.text(&shown.place));
            }
            view.show(&shown);
            let _ = window.refresh(&mut view.ui);
        }
        let wait = if (shown.open && left_at.is_some())
            || asked_at.is_some_and(|at| at.elapsed() < Duration::from_secs(60))
        {
            Some(Duration::from_millis(150))
        } else {
            None
        };
        let happened = match window.dispatch(&mut view.ui, wait) {
            Ok(happened) => happened,
            Err(error) => {
                say(&format!("err: the compositor went away: {error}"));
                return 1;
            }
        };
        let mut changed = false;
        if let Some(size) = window.size() {
            if !up {
                up = true;
                say(&format!(
                    "the {} sidebar is up, {} widgets",
                    side.id(),
                    shown.arrangement.on(side).len()
                ));
            }
            view.resize(size);
        }
        for event in happened {
            match event {
                Event::Closed(_) | Event::CloseRequested(_) => return 0,
                Event::Timer(timer) if timer == tick => {
                    shown.now = now();
                    let stamps = (stamp(file.as_deref()), stamp(todo.as_deref()));
                    if stamps != seen {
                        seen = stamps;
                        shown.arrangement = read_arrangement();
                        shown.tasks = read_tasks();
                    }
                    changed = shown.open;
                }
                _ => {}
            }
        }
        // Open when the pointer reaches the edge; closed a moment after it
        // leaves, unless pinned.
        let over = view.ui.pointer_at().is_some();
        let pinned = shown.arrangement.is_pinned(side);
        let wanted = if over || pinned {
            left_at = None;
            true
        } else if shown.open {
            let since = *left_at.get_or_insert_with(Instant::now);
            since.elapsed() < LINGER
        } else {
            false
        };
        for asked in view.take() {
            if let Asked::Quick(change) = &asked {
                let said = crate::quick::change(hypr.as_ref(), &shown.quick, change);
                shown.quick = crate::quick::read(hypr.as_ref());
                let q = &shown.quick;
                say(&format!(
                    "quick: {said}; now focus {}, tiled {}, accent {:?}, quiet {}",
                    q.focus, q.tiled, q.accent, q.quiet
                ));
                shown.quick.said = said;
            }
            let (arranged, tasked) = apply(&mut shown, &asked);
            if arranged && let Some(f) = &file {
                widgets::keep(f, &shown.arrangement.text());
            }
            if tasked && let Some(f) = &todo {
                widgets::keep(f, &widgets::tasks_text(&shown.tasks));
            }
            if matches!(asked, Asked::WeatherOn(_) | Asked::WeatherOff) {
                asked_at = None;
                if let Some(f) = &weather_file {
                    widgets::keep(f, &shown.weather.text(&shown.place));
                }
            }
            seen = (stamp(file.as_deref()), stamp(todo.as_deref()));
            say(&format!("{} sidebar: {asked:?}", side.id()));
            changed = true;
        }
        let pinned_now = shown.arrangement.is_pinned(side);
        if wanted != shown.open || pinned_now != pinned {
            shown.open = wanted || pinned_now;
            if shown.open {
                // What another program changed since, read again.
                let said = std::mem::take(&mut shown.quick.said);
                shown.quick = crate::quick::read(hypr.as_ref());
                shown.quick.said = said;
            }
            window.set_layer(layer(side, shown.open, pinned_now));
            say(&format!(
                "the {} sidebar {}",
                side.id(),
                if shown.open { "opened" } else { "closed" }
            ));
            changed = true;
        }
        if changed {
            view.show(&shown);
            if let Err(error) = window.refresh(&mut view.ui) {
                say(&format!("err: the compositor went away: {error}"));
                return 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Asked, Shown, View, apply};
    use crate::widgets::{Arrangement, Kind, Side, Task};

    fn shown() -> Shown {
        Shown {
            side: Side::Right,
            arrangement: Arrangement::default(),
            tasks: vec![Task {
                text: "Water the plants".to_owned(),
                done: false,
            }],
            now: crate::widgets::days_from_civil(2026, 10, 25) * 86_400 + 9 * 3600,
            open: true,
            weather: crate::weather::Weather::default(),
            place: String::new(),
            forecast: None,
            status: String::new(),
            quick: crate::quick::Quick::default(),
        }
    }

    #[test]
    fn presses_change_what_is_where_and_the_tasks() {
        let mut view = View::new(crate::look::fonts()).expect("made");
        view.resize((super::OPEN, 900));
        let mut state = shown();
        view.show(&state);
        let click = |view: &mut View, x: f32, y: f32| {
            view.ui.update();
            view.ui.pointer_move(x, y);
            view.ui.pointer_press();
            view.ui.pointer_release();
            view.take()
        };
        // The first task's box ticks it; the to-do list is shown.
        let asked = click(&mut view, 8.0 + 16.0 + 20.0, 8.0 + 90.0 + 26.0 + 17.0);
        assert_eq!(asked, vec![Asked::Toggle(0)]);
        let _ = apply(&mut state, &asked[0]);
        assert!(state.tasks[0].done);
        // The pin.
        let asked = click(&mut view, 8.0 + 320.0 - 12.0 - 15.0, 8.0 + 12.0 + 15.0);
        assert_eq!(asked, vec![Asked::Pin]);
        let _ = apply(&mut state, &asked[0]);
        assert!(state.arrangement.is_pinned(Side::Right));
        // Move: the to-do list goes left.
        let asked = click(
            &mut view,
            8.0 + 16.0 + 288.0 - 28.0 - 4.0 - 14.0,
            8.0 + 54.0 + 14.0,
        );
        assert_eq!(asked, vec![Asked::Move]);
        let _ = apply(&mut state, &asked[0]);
        assert_eq!(state.arrangement.left.last(), Some(&Kind::Todo));
        let _ = apply(&mut state, &Asked::Task("Call the bank".to_owned()));
        assert_eq!(state.tasks.len(), 2);
    }

    /// The quick settings' tiles and colours ask their changes; the tiles
    /// this machine lacks, and a slider without a backlight, ask nothing.
    #[test]
    fn quick_settings_ask_what_they_change() {
        use crate::quick::Change;
        let mut view = View::new(crate::look::fonts()).expect("made");
        view.resize((super::OPEN, 900));
        let mut state = shown();
        state.arrangement.show(Side::Right, Kind::Settings);
        view.show(&state);
        let mut click = |x: f32, y: f32| {
            view.ui.update();
            view.ui.pointer_move(x, y);
            view.ui.pointer_press();
            view.ui.pointer_release();
            view.take()
        };
        let (left, right, top) = (
            8.0 + 16.0 + 70.0,
            8.0 + 16.0 + 147.0 + 70.0,
            8.0 + 90.0 + 148.0,
        );
        assert_eq!(
            click(left, top + 112.0 + 25.0),
            vec![Asked::Quick(Change::Focus)]
        );
        assert_eq!(
            click(right, top + 112.0 + 25.0),
            vec![Asked::Quick(Change::Tile)]
        );
        assert_eq!(
            click(left, top + 56.0 + 25.0),
            vec![Asked::Quick(Change::Quiet)]
        );
        assert_eq!(click(left, top + 25.0), Vec::<Asked>::new(), "Wi-Fi");
        assert_eq!(
            click(right, top + 56.0 + 25.0),
            Vec::<Asked>::new(),
            "night light"
        );
        assert_eq!(
            click(8.0 + 16.0 + 40.0 * 2.0 + 14.0, 8.0 + 90.0 + 402.0 + 14.0),
            vec![Asked::Quick(Change::Accent(2))]
        );
        assert_eq!(
            click(8.0 + 16.0 + 144.0, 8.0 + 90.0 + 26.0),
            Vec::<Asked>::new(),
            "no backlight"
        );
    }
}
