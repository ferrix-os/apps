//! The window's tree, in tole (Ferrix's `docs/UI-FRAMEWORK.md`): the
//! markup under `ui/` is the header, the page's rows and the foot; this
//! module hands it the [`State`] after every change and turns what was
//! pressed back into [`Action`]s.

use std::path::Path;
use std::rc::Rc;

use tole::{Color, Element as _, Ui};

use crate::pages::{Kind, Line, Page, STEPS, State};

tole::include_ui!();

/// What a press in the tree asks.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Row `row`'s card or its button `option` pressed.
    Pressed(Line, usize),
    /// Text typed into row `row`.
    Edited(Line, String),
    /// Return in row `row`'s field.
    Accepted(Line),
    /// Back.
    Back,
    /// Next.
    Next,
}

/// The fonts the window draws with: Settings' Figtree and JetBrains
/// Mono, from where the package installs them or, on a development host,
/// beside the sources; then the system's.
#[must_use]
pub fn fonts() -> tole::Fonts {
    let mut fonts = tole::Fonts::system();
    for dir in [
        std::env::var("INSTALLER_FONTS").ok(),
        Some("/usr/share/fonts/installer".to_owned()),
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

/// Only the faces beside the sources: what the tests and the screenshots
/// draw with, the same on every host.
#[must_use]
pub fn own_fonts() -> tole::Fonts {
    let mut fonts = tole::Fonts::new();
    let _ = fonts.add_dir(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/share/fonts"
    )));
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

/// How many characters of a warning's words go on one of its lines.
const WARNING_WIDTH: usize = 78;

/// And of a chosen card's warning, under its words.
const RADIO_WIDTH: usize = 72;

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
    rows_page: Option<Page>,
    /// The page whose first field last took the keys.
    focused: Option<Page>,
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
        shell.mark.set(&mut ui, mark());
        Some(View {
            ui,
            shell,
            rows: Vec::new(),
            model,
            rows_page: None,
            focused: None,
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
        let now = state.page.step();
        let steps: Vec<StepRow> = STEPS
            .iter()
            .enumerate()
            .map(|(at, label)| StepRow {
                label: s(label),
                state: match at.cmp(&now) {
                    std::cmp::Ordering::Less => 0,
                    std::cmp::Ordering::Equal => 1,
                    std::cmp::Ordering::Greater => 2,
                },
            })
            .collect();
        self.shell.steps.set(ui, steps);
        self.shell.title.set(ui, s(state.page.title()));
        self.shell.subtitle.set(ui, s(state.page.subtitle()));
        self.shell.foot.set(ui, state.has_foot());
        self.shell.can_back.set(ui, state.page != Page::Welcome);
        self.shell.has_next.set(ui, state.has_next());
        self.shell.can_next.set(ui, state.can_next());
        self.shell.next_label.set(ui, s(state.next_label()));
        self.shell.back_label.set(ui, s(state.back_label()));
        self.shell
            .status
            .set(ui, s(state.status.as_deref().unwrap_or("")));
        let lines = state.rows();
        let grew = lines.len() > self.rows.len();
        if self.rows_page != Some(state.page) {
            // Another page: its rows anew, from the top.
            self.model.set_vec(lines.iter().map(row).collect());
            self.rows_page = Some(state.page);
            self.focused = None;
            if let Some(list) = self.list() {
                self.ui.update();
                // From the top, or for the zones where the one chosen is.
                let chosen = (state.page == Page::Zone)
                    .then(|| lines.iter().position(|l| l.kind == Kind::Pick && l.on))
                    .flatten()
                    .map_or(0, |at| at.saturating_sub(3));
                self.ui.position_view_at_index(list, chosen);
            }
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
            // The log, open, follows its last line.
            if grew
                && state.details
                && matches!(state.page, Page::Installing)
                && let Some(list) = self.list()
            {
                self.ui.update();
                self.ui.position_view_at_end(list);
            }
        }
        self.rows = lines;
    }

    /// Scroll the page's list to show row `index`, for a screenshot.
    pub fn reveal(&mut self, index: usize) {
        if let Some(list) = self.list() {
            self.ui.update();
            self.ui.position_view_at_index(list, index);
        }
    }

    /// The page's list.
    fn list(&self) -> Option<tole::ItemId> {
        let mut stack = vec![self.shell.id()];
        while let Some(item) = stack.pop() {
            if matches!(self.ui.kind(item), Some(tole::Kind::ListView(_))) {
                return Some(item);
            }
            stack.extend(self.ui.children(item).iter().rev().copied());
        }
        None
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
                ShellSignal::Pressed(row, option) => line(&self.rows, row)
                    .zip(usize::try_from(option).ok())
                    .map(|(row, option)| Action::Pressed(row, option)),
                ShellSignal::Edited(row, text) => {
                    line(&self.rows, row).map(|row| Action::Edited(row, text.to_string()))
                }
                ShellSignal::Accepted(row) => line(&self.rows, row).map(Action::Accepted),
                ShellSignal::Back => Some(Action::Back),
                ShellSignal::Next => Some(Action::Next),
            };
            out.extend(action);
        }
        out
    }

    /// Give the page's first line to type in the keys, once a page: whether
    /// one took them. Its item is made only once the window has a size
    /// and the tree has updated, so a window asks again until it is.
    pub fn focus_field(&mut self, page: Page) -> bool {
        if self.focused == Some(page) {
            return true;
        }
        self.ui.update();
        let Some(body) = self.list() else {
            return false;
        };
        let mut stack = vec![body];
        while let Some(item) = stack.pop() {
            let shown = self.ui.common(item).is_some_and(|c| self.ui.get(c.visible));
            if !shown {
                continue;
            }
            if matches!(self.ui.kind(item), Some(tole::Kind::TextInput(_))) {
                self.ui.force_active_focus(item);
                self.focused = Some(page);
                return true;
            }
            stack.extend(self.ui.children(item).iter().rev().copied());
        }
        false
    }

    /// One whole frame at `scale`, for `--screenshot` and the tests.
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
    // A warning's words go in lines of their own, as Text draws one line.
    let values = match line.kind {
        Kind::Warning => crate::pages::wrap(&line.desc, WARNING_WIDTH),
        Kind::Radio => crate::pages::wrap(&line.note, RADIO_WIDTH),
        _ => line.values.clone(),
    };
    PageRow {
        kind: match line.kind {
            Kind::Hero => RowKind::Hero,
            Kind::Section => RowKind::Section,
            Kind::Radio => RowKind::Radio,
            Kind::Pick => RowKind::Pick,
            Kind::Field => RowKind::Field,
            Kind::Warning => RowKind::Warning,
            Kind::Info => RowKind::Info,
            Kind::Choice => RowKind::Choice,
            Kind::Progress => RowKind::Progress,
            Kind::Log => RowKind::Log,
            Kind::Buttons => RowKind::Buttons,
            Kind::Link => RowKind::Link,
            Kind::Empty => RowKind::Empty,
        },
        title: s(&line.title),
        desc: s(&line.desc),
        tag: s(&line.tag),
        shown: s(&line.shown),
        note: s(&line.note),
        level: line.level,
        on: line.on,
        enabled: line.enabled,
        options: line.options.iter().map(s).collect(),
        values: values.iter().map(s).collect(),
        current: line.current,
        value: line.value,
    }
}

/// Ferrix's mark (Ferrix's `docs/brand/logo-mark.svg`), drawn to fill
/// the canvas it is given, keeping its shape: the F running out into the
/// X in Nocturne's text, the slash in the brand's rust.
#[must_use]
pub fn mark() -> tole::CanvasPainter {
    tole::CanvasPainter::new(|p| {
        // The mark's own box, without the SVG's margin.
        const W: f32 = 613.0;
        const H: f32 = 498.0;
        let (w, h) = p.size();
        let k = (w / W).min(h / H);
        let (dx, dy) = ((w - W * k) / 2.0, (h - H * k) / 2.0);
        let at = |points: &[(f32, f32)]| -> Vec<(f32, f32)> {
            points
                .iter()
                .map(|&(x, y)| (dx + x * k, dy + y * k))
                .collect()
        };
        let grey = Color::hex(0x00e9_e9ed);
        let rust = Color::hex(0x00ff_7a2b);
        // The F and the X's long stroke.
        p.polygon(
            &at(&[
                (0.0, 0.0),
                (460.0, 0.0),
                (370.0, 98.0),
                (103.0, 98.0),
                (103.0, 173.0),
                (300.0, 173.0),
                (573.0, 498.0),
                (434.0, 498.0),
                (242.0, 271.0),
                (103.0, 271.0),
                (103.0, 498.0),
                (0.0, 498.0),
            ]),
            grey,
        );
        // The chip under the middle bar.
        p.polygon(
            &at(&[
                (127.5, 414.0),
                (238.0, 296.0),
                (295.0, 364.0),
                (172.0, 498.0),
                (127.5, 498.0),
            ]),
            grey,
        );
        // The rust slash.
        p.polygon(
            &at(&[(492.0, 0.0), (613.0, 0.0), (385.0, 246.0), (328.0, 177.0)]),
            rust,
        );
    })
}
