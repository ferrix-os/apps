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
use crate::ui::{AddRow, Cell, Sidebar, SidebarSignal, Tab, TaskRow};
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
    };
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
        let wait = (shown.open && left_at.is_some()).then_some(Duration::from_millis(150));
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
            let (arranged, tasked) = apply(&mut shown, &asked);
            if arranged && let Some(f) = &file {
                widgets::keep(f, &shown.arrangement.text());
            }
            if tasked && let Some(f) = &todo {
                widgets::keep(f, &widgets::tasks_text(&shown.tasks));
            }
            seen = (stamp(file.as_deref()), stamp(todo.as_deref()));
            say(&format!("{} sidebar: {asked:?}", side.id()));
            changed = true;
        }
        let pinned_now = shown.arrangement.is_pinned(side);
        if wanted != shown.open || pinned_now != pinned {
            shown.open = wanted || pinned_now;
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
}
