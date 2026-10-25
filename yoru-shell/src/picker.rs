//! The wallpaper switcher's tree (`ui/walls.tole`): handed the
//! [`Picker`] after every change, with every card placed, and turning what
//! was pressed back into [`Asked`]s.

use std::rc::Rc;

use tole::{Color, Element as _, Ui};

use crate::look::icon;
use crate::ui::{Card, Chip, Dot, Sorter, Walls, WallsSignal};
use crate::walls::{COLUMNS, Colour, HUES, Layout, Picker, Sort, cover_box};

/// What a press or a key in the switcher asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asked {
    /// Close, keeping the wallpaper in force.
    Close,
    /// The key that opened it, pressed again: close.
    Again,
    /// Set the one in hand.
    Apply,
    /// A folder named so is to be made (empty: none after all).
    Named(String),
}

/// The colour dots in order: any, the hues, light, dark.
#[must_use]
pub fn dots() -> Vec<(String, Colour, Color)> {
    let mut dots = vec![("Any colour".to_owned(), Colour::Any, Color::hex(0x23_2532))];
    // The dots' own colours, sRGB near OKLCH (0.7, 0.17, hue).
    let shown: [u32; 8] = [
        0xe0_6070, 0xe0_8a4a, 0xc8_b040, 0x5f_b46a, 0x2f_b6b0, 0x5a_9ae8, 0xa0_80e0, 0xdc_74b4,
    ];
    for ((name, hue), rgb) in HUES.iter().zip(shown) {
        dots.push(((*name).to_owned(), Colour::Hue(*hue), Color::hex(rgb)));
    }
    dots.push(("Light".to_owned(), Colour::Light, Color::hex(0xf5_f3ee)));
    dots.push(("Dark".to_owned(), Colour::Dark, Color::hex(0x1c_1d2a)));
    dots
}

/// The sorts in order, with their icons and titles.
pub const SORTS: [(Sort, &str, &str); 3] = [
    (Sort::Found, "list-numbers", "Default order"),
    (Sort::Hue, "rainbow", "Sort by hue"),
    (Sort::Light, "sun-dim", "Sort by brightness"),
];

/// A string as the tree holds it.
fn s(value: impl AsRef<str>) -> Rc<str> {
    Rc::from(value.as_ref())
}

/// The shade over the desktop: the design's gradient, the ground at 60%
/// at the top, clear through the middle, 88% at the foot.
fn shade() -> tole::CanvasPainter {
    tole::CanvasPainter::new(|painter: &mut tole::Painter<'_>| {
        let (width, height) = painter.size();
        let step = 4.0;
        let mut y = 0.0;
        while y < height {
            let t = (y + step / 2.0) / height.max(1.0);
            let alpha = if t < 0.32 {
                0.6 * (1.0 - t / 0.32)
            } else if t < 0.55 {
                0.0
            } else if t < 0.8 {
                0.88 * (t - 0.55) / 0.25
            } else {
                0.88
            };
            let a = (alpha * 255.0).round() as u8;
            painter.fill_rect(
                tole::Rect {
                    x: 0.0,
                    y,
                    w: width,
                    h: step.min(height - y),
                },
                0.0,
                Color::rgba(0x16, 0x18, 0x26, a),
            );
            y += step;
        }
    })
}

/// Where the cards go, in the stage's coordinates, for a stage `width` by
/// `height`: `(index into the list shown, x, y, w, h, dimmed)`, drawn in
/// this order (the one in hand last, on top).
#[must_use]
pub fn place(picker: &Picker, width: f32, height: f32) -> Vec<(usize, f32, f32, f32, f32, f32)> {
    let count = picker.list().len();
    let mut placed = Vec::new();
    match picker.layout {
        Layout::Cover => {
            let mut order: Vec<usize> = (0..count).collect();
            let away = |k: usize| k.abs_diff(picker.pos);
            order.retain(|&k| away(k) <= 4);
            order.sort_by_key(|&k| std::cmp::Reverse(away(k)));
            for k in order {
                let offset =
                    isize::try_from(k).unwrap_or(0) - isize::try_from(picker.pos).unwrap_or(0);
                let (x, y, w, h) = cover_box(offset, width / 2.0);
                let dim = if offset == 0 { 1.0 } else { 0.72 };
                placed.push((k, x, y, w, h, dim));
            }
        }
        Layout::Grid => {
            let panel = (width - 48.0).min(1000.0);
            let gap = 14.0;
            let tile = (panel - gap * (COLUMNS as f32 - 1.0)) / COLUMNS as f32;
            let tall = tile * 10.0 / 16.0;
            let pitch = tall + 24.0 + gap;
            let left = (width - panel) / 2.0;
            let rows = ((height + gap) / pitch).floor().max(1.0) as usize;
            // The rows shown are those round the one in hand.
            let row = picker.pos / COLUMNS;
            let first = row.saturating_sub(rows - 1).min(row);
            for k in (first * COLUMNS)..count.min((first + rows) * COLUMNS) {
                let (r, c) = (k / COLUMNS - first, k % COLUMNS);
                placed.push((
                    k,
                    left + c as f32 * (tile + gap),
                    r as f32 * pitch,
                    tile,
                    tall,
                    1.0,
                ));
            }
        }
    }
    placed
}

/// The tree and what the program keeps of it.
pub struct View {
    /// The tree.
    pub ui: Ui,
    /// The switcher.
    pub walls: Walls,
    /// The cards shown, as indices into the list shown.
    shown: Vec<usize>,
    /// Its size.
    size: (f32, f32),
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View").finish_non_exhaustive()
    }
}

impl View {
    /// The tree, drawn with `fonts`.
    #[must_use]
    pub fn new(fonts: tole::Fonts) -> Option<Self> {
        let mut ui = Ui::new(fonts);
        let walls = Walls::new(&mut ui, None)?;
        walls.shade.set(&mut ui, shade());
        for (property, name) in [
            (&walls.ic_cards, "cards"),
            (&walls.ic_grid, "squares-four"),
            (&walls.ic_close, "x"),
            (&walls.ic_plus, "folder-plus"),
            (&walls.ic_left, "caret-left"),
            (&walls.ic_right, "caret-right"),
            (&walls.ic_check, "check"),
            (&walls.ic_play, "play"),
        ] {
            property.set(&mut ui, s(icon(name)));
        }
        Some(View {
            ui,
            walls,
            shown: Vec::new(),
            size: (1280.0, 800.0),
        })
    }

    /// Its size in logical pixels.
    pub fn resize(&mut self, (width, height): (u32, u32)) {
        self.size = (width as f32, height as f32);
        let common = self.walls.common();
        common.width.set(&mut self.ui, width as f32);
        common.height.set(&mut self.ui, height as f32);
    }

    /// Hand the tree `picker`.
    pub fn show(&mut self, picker: &Picker) {
        let ui = &mut self.ui;
        let w = &self.walls;
        let (width, height) = self.size;
        let panel = (width - 48.0).min(1000.0);
        w.panel_x.set(ui, (width - panel) / 2.0);
        w.panel_w.set(ui, panel);
        let stage_y = if picker.layout == Layout::Cover {
            // The cover in the middle of what the head and the foot leave.
            (162.0_f32).max((height - 112.0 + 162.0) / 2.0 - 170.0)
        } else {
            160.0
        };
        w.stage_y.set(ui, stage_y);
        w.count.set(ui, s(picker.count_line()));
        let mut chips = vec![Chip {
            name: s("All"),
            count: s(picker.count(None).to_string()),
            icon: s(icon("images")),
            on: picker.folder.is_none(),
        }];
        for folder in &picker.folders {
            let on = picker.folder.as_deref() == Some(folder.as_str());
            chips.push(Chip {
                name: s(folder),
                count: s(picker.count(Some(folder)).to_string()),
                icon: s(icon(if on { "folder-open" } else { "folder" })),
                on,
            });
        }
        w.chips.set(ui, chips);
        w.naming.set(ui, picker.naming);
        let dots = dots()
            .into_iter()
            .map(|(title, colour, shown)| Dot {
                title: s(title),
                colour: shown,
                any: colour == Colour::Any,
                on: picker.colour == colour && colour != Colour::Any,
            })
            .collect::<Vec<_>>();
        w.dots.set(ui, dots);
        let sorters = SORTS
            .iter()
            .map(|(sort, name, title)| Sorter {
                icon: s(icon(name)),
                title: s(*title),
                on: picker.sort == *sort,
            })
            .collect::<Vec<_>>();
        w.sorters.set(ui, sorters);
        w.cover.set(ui, picker.layout == Layout::Cover);
        w.grid.set(ui, picker.layout == Layout::Grid);
        let list = picker.list();
        w.empty.set(ui, list.is_empty());
        let placed = place(picker, width, height - stage_y - 112.0);
        let cards = placed
            .iter()
            .filter_map(|&(k, x, y, cw, ch, dim)| {
                let wall = picker.walls.get(*list.get(k)?)?;
                Some(Card {
                    source: s(if wall.video { "" } else { wall.path.as_str() }),
                    name: s(&wall.name),
                    x,
                    y,
                    w: cw,
                    h: ch,
                    on: k == picker.pos,
                    in_use: wall.path == picker.current,
                    video: wall.video,
                    dim,
                })
            })
            .collect::<Vec<_>>();
        self.shown = placed.iter().map(|p| p.0).collect();
        w.cards.set(ui, cards);
        let held = picker.held().and_then(|i| picker.walls.get(i));
        w.name
            .set(ui, s(held.map_or("", |wall| wall.name.as_str())));
        w.meta.set(
            ui,
            s(held.map(crate::walls::Wall::meta).unwrap_or_default()),
        );
        w.set_label.set(ui, s("Set wallpaper"));
    }

    /// Do what the tree asked since the last call to `picker`; what is
    /// left for the program.
    pub fn take(&mut self, picker: &mut Picker) -> Vec<Asked> {
        let emitted = self.ui.take_signals();
        let mut asked = Vec::new();
        let index = |i: i32| usize::try_from(i).ok();
        for each in &emitted {
            let Some(signal) = self.walls.signal(each) else {
                continue;
            };
            match signal {
                WallsSignal::Closed => asked.push(Asked::Close),
                WallsSignal::Again => asked.push(Asked::Again),
                WallsSignal::Apply => asked.push(Asked::Apply),
                WallsSignal::Layout(grid) => {
                    picker.layout = if grid == 0 {
                        Layout::Cover
                    } else {
                        Layout::Grid
                    };
                }
                WallsSignal::Flip => picker.flip(),
                WallsSignal::Folder(i) => match index(i) {
                    Some(0) => picker.show_folder(None),
                    Some(i) => {
                        let folder = picker.folders.get(i - 1).cloned();
                        if folder.is_some() {
                            picker.show_folder(folder);
                        }
                    }
                    None => {}
                },
                WallsSignal::StartNaming => picker.naming = true,
                WallsSignal::Named(text) => {
                    let text = text.trim().to_owned();
                    if picker.named(&text) {
                        asked.push(Asked::Named(text));
                    }
                }
                WallsSignal::Colour(i) => {
                    if let Some((_, colour, _)) = index(i).and_then(|i| dots().into_iter().nth(i)) {
                        picker.filter(colour);
                    }
                }
                WallsSignal::Sort(i) => {
                    if let Some((sort, _, _)) = index(i).and_then(|i| SORTS.get(i)) {
                        picker.sort_by(*sort);
                    }
                }
                WallsSignal::Pick(i) => {
                    if let Some(&k) = index(i).and_then(|i| self.shown.get(i)) {
                        picker.pos = k;
                    }
                }
                WallsSignal::Step(by) => picker.step(by as isize),
                WallsSignal::Row(down) => picker.row(down != 0),
            }
        }
        asked
    }
}

#[cfg(test)]
mod tests {
    use super::{Asked, View};
    use crate::walls::{Colour, Layout};

    fn key(view: &mut View, name: &str) {
        let _ = view.ui.key_press(&tole::KeyEvent::named(name));
    }

    fn click(view: &mut View, x: f32, y: f32) {
        view.ui.update();
        view.ui.pointer_move(x, y);
        view.ui.pointer_press();
        view.ui.pointer_release();
    }

    /// The keys browse, Tab flips to the grid, a dot filters, Return
    /// sets and Escape closes; the one in hand is ringed in the accent.
    #[test]
    fn keys_and_presses_drive_the_switcher() {
        let mut picker = crate::cli::sample_walls(false);
        assert_eq!(picker.pos, 1, "the one in force is in hand");
        let mut view = View::new(crate::look::fonts()).expect("made");
        view.ui.set_window_active(true);
        view.resize((1600, 1000));
        view.show(&picker);
        view.ui.update();
        key(&mut view, "Right");
        assert!(view.take(&mut picker).is_empty());
        assert_eq!(picker.pos, 2);
        key(&mut view, "Left");
        let _ = view.take(&mut picker);
        assert_eq!(picker.pos, 1, "Left steps one back");
        key(&mut view, "Right");
        let _ = view.take(&mut picker);
        key(&mut view, "Tab");
        let _ = view.take(&mut picker);
        assert_eq!(picker.layout, Layout::Grid);
        view.show(&picker);
        key(&mut view, "Up");
        let _ = view.take(&mut picker);
        assert_eq!(picker.pos, 0, "a row up from the third of four");
        // The first card of the grid, picked by a press, then set by one.
        view.show(&picker);
        let left = (1600.0 - 1000.0) / 2.0;
        click(&mut view, left + 300.0, 160.0 + 40.0);
        let _ = view.take(&mut picker);
        assert_eq!(picker.pos, 1, "the second card is in hand");
        view.show(&picker);
        click(&mut view, left + 300.0, 160.0 + 40.0);
        assert_eq!(view.take(&mut picker), vec![Asked::Apply]);
        // The "Dark" dot, last of eleven, filters.
        view.show(&picker);
        click(&mut view, left + 10.0 * 28.0 + 11.0, 24.0 + 86.0 + 11.0);
        let _ = view.take(&mut picker);
        assert_eq!(picker.colour, Colour::Dark);
        key(&mut view, "i");
        assert_eq!(view.take(&mut picker), vec![Asked::Again]);
        key(&mut view, "Escape");
        assert_eq!(view.take(&mut picker), vec![Asked::Close]);
    }

    /// The picture: the cover's middle card drawn from the file, ringed.
    #[test]
    fn the_cover_draws_the_pictures() {
        let picker = crate::cli::sample_walls(false);
        let frame = crate::cli::render_walls(&picker, (1600, 1000), 1.0).expect("drawn");
        let pixmap = frame.pixmap();
        let at = |x: u32, y: u32| {
            let pixel = pixmap.pixel(x, y).expect("inside").demultiply();
            (pixel.red(), pixel.green(), pixel.blue())
        };
        // Not the ground: a picture is in the middle card.
        let ground = (0x2b, 0x27, 0x41);
        let middle_y = super::place(&picker, 1600.0, 600.0)
            .last()
            .map(|p| p.2 + p.4 / 2.0)
            .expect("placed");
        let stage = 162.0_f32.max((1000.0 - 112.0 + 162.0) / 2.0 - 170.0);
        let y = (stage + middle_y) as u32;
        let differs = (700..900).filter(|&x| at(x, y) != ground).count();
        assert!(differs > 150, "the picture is drawn: {differs}");
    }
}
