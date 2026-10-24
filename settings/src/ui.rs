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
    /// The page's rows as the tree's model: changed row by row, so a row
    /// that did not change -- a field being typed in -- keeps its item and
    /// the keys.
    model: Rc<tole::VecModel<PageRow>>,
    /// The page the rows are of.
    rows_page: Option<crate::pages::Page>,
    /// The sidebar's rows last shown, by index.
    nav: Vec<Nav>,
    /// The palette last set: dark, and the accent.
    look: Option<(bool, usize)>,
    /// What the desktop drawn small last showed.
    preview: Option<Preview>,
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
        let model = Rc::new(tole::VecModel::new(Vec::new()));
        shell.rows.set(
            &mut ui,
            tole::ModelRc::from_rc(Rc::clone(&model) as Rc<dyn tole::Model<Row = PageRow>>),
        );
        Some(View {
            ui,
            shell,
            rows: Vec::new(),
            model,
            rows_page: None,
            nav: Vec::new(),
            look: None,
            preview: None,
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
        if state.page == crate::pages::Page::Appearance {
            self.shell.modes.set(ui, vec![s("Simple"), s("Advanced")]);
            self.shell.mode.set(ui, i32::from(state.advanced));
        } else {
            self.shell.modes.set(ui, Vec::new());
        }
        let preview = Preview::of(state);
        if self.preview != Some(preview) {
            self.shell.preview.set(ui, preview.painter());
            self.preview = Some(preview);
        }
        let lines = state.rows();
        if self.rows_page != Some(state.page) {
            // Another page: its rows anew.
            self.model.set_vec(lines.iter().map(row).collect());
            self.rows_page = Some(state.page);
        } else if lines != self.rows {
            for (at, line) in lines.iter().enumerate() {
                match self.rows.get(at) {
                    Some(old) if old == line => {}
                    Some(_) => self.model.set_row(at, row(line)),
                    None => self.model.push(row(line)),
                }
            }
            for at in (lines.len()..self.rows.len()).rev() {
                let _ = self.model.remove(at);
            }
        }
        self.rows = lines;
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
    /// filter has them when the page opens: whether there was one. Its
    /// item is made only once the window has a size and the tree has
    /// updated, so a window asks again until it is.
    pub fn focus_field(&mut self) -> bool {
        self.ui.update();
        let mut stack = vec![self.shell.id()];
        let search = self.search_input();
        while let Some(item) = stack.pop() {
            if matches!(self.ui.kind(item), Some(tole::Kind::TextInput(_))) && Some(item) != search
            {
                self.ui.force_active_focus(item);
                return true;
            }
            stack.extend(self.ui.children(item).iter().rev().copied());
        }
        false
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

/// Appearance's picture of the desktop: what it is drawn from.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Preview {
    dark: bool,
    hue: f32,
    gaps_in: f32,
    gaps_out: f32,
    rounding: f32,
    border: f32,
    see_through: bool,
}

impl Preview {
    /// The picture for `state`.
    fn of(state: &State) -> Self {
        let number = |key: &str| state.value(key).trim().parse::<f32>().unwrap_or(0.0);
        Preview {
            dark: state.dark,
            hue: ACCENTS.get(state.accent).map_or(250.0, |a| a.1),
            gaps_in: number("general:gaps_in"),
            gaps_out: number("general:gaps_out"),
            rounding: number("decoration:rounding"),
            border: number("general:border_size").max(1.0),
            see_through: number("decoration:inactive_opacity") < 0.999,
        }
    }

    /// Its painter: a bar, a wallpaper in the accent, and four windows
    /// placed as the spiral places them, the first focused.
    fn painter(self) -> tole::CanvasPainter {
        tole::CanvasPainter::new(move |p| {
            use crate::look::color;
            let (w, h) = p.size();
            let hue = self.hue;
            let whole = tole::Rect::new(0.0, 0.0, w, h);
            p.fill_rect(whole, 12.0, color(0.2, 0.04, 260.0));
            p.push_clip(whole);
            // The wallpaper: soft light in the accent and two near it.
            for (cx, cy, r, l, c, dh) in [
                (0.18, 0.22, 0.6, 0.6, 0.12, -15.0),
                (0.82, 0.78, 0.6, 0.52, 0.13, 35.0),
                (0.7, 0.12, 0.45, 0.66, 0.09, -45.0),
            ] {
                for step in 0..12 {
                    let t = step as f32 / 12.0;
                    let mut ink = color(l, c, hue + dh);
                    ink.a = (40.0 * (1.0 - t)) as u8;
                    p.circle((w * cx, h * cy), w * r * (1.0 - t * 0.9) * 0.6, ink);
                }
            }
            // The bar.
            let bar = 20.0;
            p.fill_rect(
                tole::Rect::new(0.0, 0.0, w, bar),
                0.0,
                color(0.17, 0.01, 260.0),
            );
            let mono = tole::Font::new("JetBrains Mono", 10.0, 400);
            let ink = color(0.9, 0.003, 260.0);
            p.text("1 2 3", &mono, (10.0, 14.0), ink);
            let clock = "23:14";
            let tw = p.text_width(clock, &mono);
            p.text(clock, &mono, (w - tw - 10.0, 14.0), ink);
            // The windows, scaled from the gaps chosen.
            let k = 0.5;
            let (gi, go) = (self.gaps_in * k, self.gaps_out * k + 4.0);
            let area = tole::Rect::new(go, bar + go, w - 2.0 * go, h - bar - 2.0 * go);
            let half = (area.w - gi) / 2.0;
            let top = (area.h - gi) / 2.0;
            let quarter = (half - gi) / 2.0;
            let windows = [
                ("term", tole::Rect::new(area.x, area.y, half, area.h)),
                (
                    "files",
                    tole::Rect::new(area.x + half + gi, area.y, half, top),
                ),
                (
                    "lode viewer",
                    tole::Rect::new(area.x + half + gi, area.y + top + gi, quarter, top),
                ),
                (
                    "fuzzel",
                    tole::Rect::new(
                        area.x + half + gi + quarter + gi,
                        area.y + top + gi,
                        quarter,
                        top,
                    ),
                ),
            ];
            let radius = self.rounding * k;
            for (i, (name, r)) in windows.iter().enumerate() {
                let mut fill = color(0.19, 0.005, 260.0);
                if self.see_through && i > 0 {
                    fill.a = 0xd8;
                }
                p.fill_rect(*r, radius, fill);
                let edge = if i == 0 {
                    color(0.72, 0.13, hue)
                } else {
                    color(0.36, 0.03, hue)
                };
                p.stroke_rect(*r, radius, self.border.min(2.0), edge);
                p.text(
                    name,
                    &mono,
                    (r.x + 10.0, r.y + 18.0),
                    color(0.85, 0.003, 260.0),
                );
            }
            p.pop_clip();
            let _ = self.dark;
        })
    }
}
