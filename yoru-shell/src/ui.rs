//! The dock's tree, in tole (Ferrix's `docs/UI-FRAMEWORK.md`): the markup
//! under `ui/` draws it; this module hands it the [`State`] after every
//! change and turns what was pressed back into [`Action`]s.

use std::rc::Rc;

use tole::{Color, Element as _, Ui};

use crate::look::icon;
use crate::model::{State, Tool};

tole::include_ui!();

/// The dock's height, its frame included, in logical pixels.
pub const HEIGHT: u32 = 60;

/// What a press in the dock asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// The search: the palette.
    Search,
    /// Workspace button `n` (its place in the row).
    Space(usize),
    /// Launcher `n`.
    Launch(usize),
    /// A tool.
    Tool(Tool),
    /// Focus mode on or off.
    Focus,
    /// The pomodoro started or paused.
    Timer,
}

/// A string as the tree holds it.
fn s(value: impl AsRef<str>) -> Rc<str> {
    Rc::from(value.as_ref())
}

/// The tree and what the program keeps of it.
pub struct View {
    /// The tree.
    pub ui: Ui,
    /// The dock.
    pub dock: Dock,
    /// The state last shown.
    shown: Option<State>,
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View").finish_non_exhaustive()
    }
}

/// The frame round the bar: Nocturne's three accents across it, at about
/// a quarter, as the design's `linear-gradient(90deg, ...)` in a rounded
/// box of radius 18. Drawn in columns, each as tall as the corners allow.
fn frame() -> tole::CanvasPainter {
    tole::CanvasPainter::new(|painter: &mut tole::Painter<'_>| {
        let (width, height) = painter.size();
        if width < 2.0 || height < 2.0 {
            return;
        }
        let stops: [(f32, [u8; 4]); 3] = [
            (0.0, [0x96, 0x8a, 0xe0, 0x42]),
            (0.5, [0x00, 0xb2, 0xba, 0x2e]),
            (1.0, [0xce, 0x7b, 0xb4, 0x42]),
        ];
        let radius = 18.0_f32.min(height / 2.0);
        let step = 2.0;
        let mut x = 0.0;
        while x < width {
            let middle = x + step / 2.0;
            let t = (middle / width).clamp(0.0, 1.0);
            let (from, to) = if t < 0.5 {
                (stops[0], stops[1])
            } else {
                (stops[1], stops[2])
            };
            let along = (t - from.0) / (to.0 - from.0);
            let mix =
                |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * along).round() as u8;
            let colour = Color::rgba(
                mix(from.1[0], to.1[0]),
                mix(from.1[1], to.1[1]),
                mix(from.1[2], to.1[2]),
                mix(from.1[3], to.1[3]),
            );
            // How far the rounded ends cut this column in, top and bottom.
            let from_edge = middle.min(width - middle);
            let inset = if from_edge < radius {
                let dx = radius - from_edge;
                radius - (radius * radius - dx * dx).max(0.0).sqrt()
            } else {
                0.0
            };
            painter.fill_rect(
                tole::Rect {
                    x,
                    y: inset,
                    w: step.min(width - x),
                    h: height - 2.0 * inset,
                },
                0.0,
                colour,
            );
            x += step;
        }
    })
}

impl View {
    /// The tree, drawn with `fonts`; `None` when the markup's items could
    /// not be made, a fault of this program.
    #[must_use]
    pub fn new(fonts: tole::Fonts) -> Option<Self> {
        let mut ui = Ui::new(fonts);
        let dock = Dock::new(&mut ui, None)?;
        dock.frame.set(&mut ui, frame());
        dock.search_icon.set(&mut ui, s(icon("magnifying-glass")));
        dock.focus_icon.set(&mut ui, s(icon("crosshair-simple")));
        Some(View {
            ui,
            dock,
            shown: None,
        })
    }

    /// The dock's size in logical pixels.
    pub fn resize(&mut self, (width, height): (u32, u32)) {
        let common = self.dock.common();
        common.width.set(&mut self.ui, width as f32);
        common.height.set(&mut self.ui, height as f32);
    }

    /// Hand the tree `state`.
    pub fn show(&mut self, state: &State) {
        if self.shown.as_ref() == Some(state) {
            return;
        }
        let ui = &mut self.ui;
        let spaces = state
            .spaces
            .iter()
            .map(|space| SpaceRow {
                label: s(space.id.to_string()),
                title: s(format!(
                    "Workspace {} · {} · Super {}",
                    space.id, space.name, space.id
                )),
                on: space.on,
                dots: i32::try_from(space.windows.clamp(0, 3)).unwrap_or(0),
            })
            .collect::<Vec<_>>();
        self.dock.spaces.set(ui, spaces);
        let launchers = state
            .launchers
            .iter()
            .map(|launcher| IconRow {
                icon: s(icon(launcher.icon)),
                title: s(&launcher.title),
                on: launcher.running,
            })
            .collect::<Vec<_>>();
        self.dock.launchers.set(ui, launchers);
        let tools = Tool::ALL
            .iter()
            .map(|(_, name, title)| IconRow {
                icon: s(icon(name)),
                title: s(*title),
                on: false,
            })
            .collect::<Vec<_>>();
        self.dock.tools.set(ui, tools);
        self.dock.focus_on.set(ui, state.focus);
        let timer = &state.pomodoro;
        self.dock
            .timer_icon
            .set(ui, s(icon(if timer.running { "pause" } else { "timer" })));
        self.dock.timer_text.set(ui, s(timer.label()));
        self.dock.timer_mode.set(ui, s(timer.mode()));
        self.dock.timer_done.set(ui, timer.done());
        self.dock.timer_running.set(ui, timer.running);
        self.dock.clock.set(ui, s(&state.clock.time));
        self.dock.date.set(ui, s(&state.clock.date));
        self.shown = Some(state.clone());
    }

    /// What the tree asked since the last call.
    pub fn actions(&mut self) -> Vec<Action> {
        let emitted = self.ui.take_signals();
        let mut actions = Vec::new();
        for each in &emitted {
            let Some(signal) = self.dock.signal(each) else {
                continue;
            };
            let index = |i: i32| usize::try_from(i).ok();
            let action = match signal {
                DockSignal::Search => Some(Action::Search),
                DockSignal::Space(i) => index(i).map(Action::Space),
                DockSignal::Launch(i) => index(i).map(Action::Launch),
                DockSignal::Tool(i) => index(i)
                    .and_then(|i| Tool::ALL.get(i))
                    .map(|(tool, _, _)| Action::Tool(*tool)),
                DockSignal::FocusMode => Some(Action::Focus),
                DockSignal::Timer => Some(Action::Timer),
            };
            actions.extend(action);
        }
        actions
    }
}
