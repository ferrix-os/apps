//! The window's tree, in tole (Ferrix's `docs/UI-FRAMEWORK.md`): the
//! markup under `ui/` is the header, the sidebar and the page's rows;
//! this module hands it the [`State`] after every change and turns what
//! was pressed back into [`Action`]s.

use std::path::Path;
use std::rc::Rc;

use tole::{Color, Element as _, Ui};

use crate::look::{ACCENTS, Palette};
use crate::pages::{Kind, Line, Nav, State};

tole::include_ui!();

/// What a press in the tree asks.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// The search typed.
    Search(String),
    /// Sidebar row `n` pressed.
    Navigate(Nav),
    /// The page's mode (Simple, Advanced) chosen.
    Mode(usize),
    /// Option `option` of row `row` chosen.
    Pick(Line, usize),
    /// Row `row`'s switch or tick turned.
    Flip(Line),
    /// Row `row`'s slider moved to a value.
    Slide(Line, f32),
    /// Text typed into row `row`.
    Edited(Line, String),
    /// Return in row `row`'s field.
    Accepted(Line),
    /// Row `row`'s button pressed.
    Pressed(Line),
}

/// The fonts the window draws with: the design's Figtree and JetBrains
/// Mono, from where the package installs them or, on a development host,
/// beside the sources; then the system's.
#[must_use]
pub fn fonts() -> tole::Fonts {
    let mut fonts = tole::Fonts::system();
    let beside = std::env::current_exe().ok().and_then(|exe| {
        let prefix = exe.parent()?.parent()?.to_path_buf();
        Some(
            prefix
                .join("share/settings/fonts")
                .to_string_lossy()
                .into_owned(),
        )
    });
    for dir in [
        std::env::var("SETTINGS_FONTS").ok(),
        Some("/usr/share/fonts/settings".to_owned()),
        beside,
        Some(concat!(env!("CARGO_MANIFEST_DIR"), "/share/fonts").to_owned()),
    ]
    .into_iter()
    .flatten()
    {
        let path = Path::new(&dir);
        if path.is_dir() && !has_family(&fonts, "Figtree") {
            let _ = fonts.add_dir(path);
        }
    }
    fonts
}

/// Whether `fonts` has a face of `family`.
fn has_family(fonts: &tole::Fonts, family: &str) -> bool {
    fonts
        .faces()
        .iter()
        .any(|face| face.families.iter().any(|name| name == family))
}

/// A string as the tree holds it.
fn s(value: impl AsRef<str>) -> Rc<str> {
    Rc::from(value.as_ref())
}

/// A colour from `rrggbb`.
fn color(hex: &str) -> Color {
    let value = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    Color::hex(value)
}

/// The tree and what the program keeps of it.
pub struct View {
    /// The tree.
    pub ui: Ui,
    /// The window.
    pub shell: Shell,
    /// The rows last shown, by index.
    rows: Vec<Line>,
    /// The sidebar's rows last shown, by index.
    nav: Vec<Nav>,
    /// The palette last set: dark, and the accent.
    look: Option<(bool, usize)>,
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View").finish_non_exhaustive()
    }
}

impl View {
    /// The tree, drawn with `fonts`; `None` when the markup's items could
    /// not be made, a fault of this program.
    #[must_use]
    pub fn new(fonts: tole::Fonts) -> Option<Self> {
        let mut ui = Ui::new(fonts);
        let shell = Shell::new(&mut ui, None)?;
        Some(View {
            ui,
            shell,
            rows: Vec::new(),
            nav: Vec::new(),
            look: None,
        })
    }

    /// The window's size in logical pixels.
    pub fn resize(&mut self, (width, height): (u32, u32)) {
        let common = self.shell.common();
        common.width.set(&mut self.ui, width as f32);
        common.height.set(&mut self.ui, height as f32);
    }

    /// Hand the tree `state`.
    pub fn show(&mut self, state: &State) {
        let ui = &mut self.ui;
        if self.look != Some((state.dark, state.accent)) {
            let hue = ACCENTS.get(state.accent).map_or(250.0, |a| a.1);
            Palette::new(state.dark, hue).apply(ui, state.dark);
            self.look = Some((state.dark, state.accent));
        }
        let nav = state.nav();
        let rows: Vec<NavRow> = nav
            .iter()
            .map(|item| match *item {
                Nav::Group(group) => NavRow {
                    group: true,
                    label: s(group.label()),
                    open: state.is_open(group),
                    on: false,
                    badge: 0,
                    icon: i32::try_from(group.index()).unwrap_or(0),
                },
                Nav::Page(page) => NavRow {
                    group: false,
                    label: s(page.label()),
                    open: false,
                    on: page == state.page,
                    badge: i32::try_from(state.badge(page)).unwrap_or(0),
                    icon: 0,
                },
            })
            .collect();
        if nav != self.nav {
            self.shell.nav.set(ui, rows);
            self.nav = nav;
        } else {
            self.shell.nav.set(ui, rows);
        }
        self.shell.no_results.set(ui, state.no_results());
        if *self.shell.query.get(ui) != *state.query {
            self.shell.query.set(ui, s(&state.query));
        }
        let who = if state.hostname.is_empty() {
            state.user.clone()
        } else {
            format!("{} on {}", state.user, state.hostname)
        };
        self.shell.who.set(ui, s(who));
        self.shell.title.set(ui, s(state.page.label()));
        self.shell.subtitle.set(ui, s(state.page.subtitle()));
        self.shell.modes.set(ui, Vec::new());
        let lines = state.rows();
        if lines != self.rows {
            let tree: Vec<PageRow> = lines.iter().map(row).collect();
            self.shell.rows.set(ui, tree);
            self.rows = lines;
        }
    }

    /// What the tree asked since the last call.
    pub fn actions(&mut self) -> Vec<Action> {
        let emitted = self.ui.take_signals();
        let mut out = Vec::new();
        let line =
            |rows: &[Line], i: i32| usize::try_from(i).ok().and_then(|i| rows.get(i)).cloned();
        for each in &emitted {
            let Some(signal) = self.shell.signal(each) else {
                continue;
            };
            let action = match signal {
                ShellSignal::Searched(text) => Some(Action::Search(text.to_string())),
                ShellSignal::Navigate(i) => usize::try_from(i)
                    .ok()
                    .and_then(|i| self.nav.get(i))
                    .map(|nav| Action::Navigate(*nav)),
                ShellSignal::PickMode(i) => usize::try_from(i).ok().map(Action::Mode),
                ShellSignal::Pick(row, option) => line(&self.rows, row)
                    .zip(usize::try_from(option).ok())
                    .map(|(row, option)| Action::Pick(row, option)),
                ShellSignal::Flip(row) => line(&self.rows, row).map(Action::Flip),
                ShellSignal::Slide(row, value) => {
                    line(&self.rows, row).map(|row| Action::Slide(row, value))
                }
                ShellSignal::Edited(row, text) => {
                    line(&self.rows, row).map(|row| Action::Edited(row, text.to_string()))
                }
                ShellSignal::Accepted(row) => line(&self.rows, row).map(Action::Accepted),
                ShellSignal::Pressed(row) => line(&self.rows, row).map(Action::Pressed),
            };
            out.extend(action);
        }
        out
    }

    /// Give the page's first line to type in the keys, as game mode's
    /// filter has them when the page opens.
    pub fn focus_field(&mut self) {
        let mut stack = vec![self.shell.id()];
        let search = self.search_input();
        while let Some(item) = stack.pop() {
            if matches!(self.ui.kind(item), Some(tole::Kind::TextInput(_))) && Some(item) != search
            {
                self.ui.force_active_focus(item);
                return;
            }
            stack.extend(self.ui.children(item).iter().rev().copied());
        }
    }

    /// The search's line.
    fn search_input(&self) -> Option<tole::ItemId> {
        let mut stack = vec![self.shell.id()];
        while let Some(item) = stack.pop() {
            if matches!(self.ui.kind(item), Some(tole::Kind::TextInput(_))) {
                return Some(item);
            }
            stack.extend(self.ui.children(item).iter().rev().copied());
        }
        None
    }

    /// One whole frame at `scale`, for `--render` and the tests.
    #[must_use]
    pub fn frame(
        &mut self,
        (width, height): (u32, u32),
        scale: f32,
    ) -> Option<tole::tiny_skia::Pixmap> {
        self.resize((width, height));
        self.ui.set_scale(scale);
        self.ui.update();
        self.ui.render((width, height), Color::TRANSPARENT)
    }
}

/// `line` as the markup's `PageRow`.
fn row(line: &Line) -> PageRow {
    PageRow {
        kind: match line.kind {
            Kind::Section => RowKind::Section,
            Kind::Choice => RowKind::Choice,
            Kind::Switch => RowKind::Switch,
            Kind::Slider => RowKind::Slider,
            Kind::Check => RowKind::Check,
            Kind::Info => RowKind::Info,
            Kind::App => RowKind::App,
            Kind::Accents => RowKind::Accents,
            Kind::Empty => RowKind::Empty,
            Kind::Field => RowKind::Field,
            Kind::Action => RowKind::Action,
            Kind::Layouts => RowKind::Layouts,
            Kind::Preview => RowKind::Preview,
            Kind::Monitors => RowKind::Monitors,
            Kind::Name => RowKind::Name,
        },
        title: s(&line.title),
        desc: s(&line.desc),
        note: s(line.note.text()),
        options: line.options.iter().map(s).collect(),
        values: line.values.iter().map(s).collect(),
        colors: line.colors.iter().map(|c| color(c)).collect(),
        nums: line.nums.clone(),
        current: line.current,
        on: line.on,
        value: line.value,
        from: line.from,
        to: line.to,
        step: line.step,
        shown: s(&line.shown),
        tag: s(&line.tag),
        enabled: line.enabled,
    }
}
