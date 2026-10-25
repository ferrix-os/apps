//! The stash panel on the screen (`yoru-shell --stash-panel`, the dock's
//! Stash button): an overlay layer whose panel sits over the dock, the
//! stash's files as tiles (`ui/stash.tole`). A pick opens the file -- a
//! folder in Files (shelf), anything else with `xdg-open` -- and closes the
//! panel; the folder button opens the stash's folder in Files; clear
//! empties it. Started again while it is up, it
//! closes, as the switcher does.

use std::os::fd::AsRawFd as _;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use compositor_toolkit::Event;
use tole::Color;
use tole::Element as _;
use tole::window::{
    Anchor, KeyboardInteractivity, Layer, LayerOptions, Margin, Window, WindowOptions,
};

use crate::look::icon;
use crate::stash::{self, Item};
use crate::ui::{Stash, StashSignal, Tile};
use crate::window::say;

/// The surface's namespace.
pub const NAMESPACE: &str = "yoru-stash";

/// How many characters of a name fit a tile's line at 11.5 pixels.
const LINE: usize = 11;

/// A name on a tile's two lines: broken after a `-`, `_`, `.` or space
/// where one is near enough the end of the first line, else at its end;
/// the second line is elided by the tree.
#[must_use]
pub fn two_lines(name: &str) -> (String, String) {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= LINE {
        return (name.to_owned(), String::new());
    }
    let head = chars.get(..LINE).unwrap_or(&chars);
    let at = head
        .iter()
        .rposition(|c| matches!(c, '-' | '_' | '.' | ' '))
        .filter(|&at| at >= LINE / 2)
        .map_or(LINE, |at| at + 1);
    let first: String = chars.get(..at).unwrap_or(&chars).iter().collect();
    let second: String = chars.get(at..).unwrap_or_default().iter().collect();
    (first.trim_end().to_owned(), second.trim_start().to_owned())
}

/// A string as the tree holds it.
fn s(value: impl AsRef<str>) -> Rc<str> {
    Rc::from(value.as_ref())
}

/// The panel's height for `count` files.
#[must_use]
pub fn panel_height(count: usize) -> f32 {
    let rows = count.div_ceil(4).clamp(1, 3) as f32;
    80.0 + rows * 96.0 + (rows - 1.0) * 10.0 + 12.0
}

/// The tree and what the program keeps of it.
pub struct View {
    /// The tree.
    pub ui: Ui,
    /// The panel.
    pub stash: Stash,
    /// Its size.
    size: (f32, f32),
}

use tole::Ui;

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View").finish_non_exhaustive()
    }
}

/// What a press in the panel asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asked {
    /// Close.
    Close,
    /// Open the stash's folder.
    Folder,
    /// Empty the stash.
    Clear,
    /// Open file `n` of the list.
    Open(usize),
    /// Take file `n` out.
    Remove(usize),
}

impl View {
    /// The tree, drawn with `fonts`.
    #[must_use]
    pub fn new(fonts: tole::Fonts) -> Option<Self> {
        let mut ui = Ui::new(fonts);
        let stash = Stash::new(&mut ui, None)?;
        stash.ic_folder.set(&mut ui, s(icon("folder-open")));
        stash.ic_trash.set(&mut ui, s(icon("trash")));
        stash.ic_close.set(&mut ui, s(icon("x")));
        stash.ic_diamond.set(&mut ui, s(icon("diamond")));
        Some(View {
            ui,
            stash,
            size: (1280.0, 800.0),
        })
    }

    /// Its size in logical pixels.
    pub fn resize(&mut self, (width, height): (u32, u32)) {
        self.size = (width as f32, height as f32);
        let common = self.stash.common();
        common.width.set(&mut self.ui, width as f32);
        common.height.set(&mut self.ui, height as f32);
    }

    /// Hand the tree the stash's `items`.
    pub fn show(&mut self, items: &[Item]) {
        let ui = &mut self.ui;
        let (width, height) = self.size;
        let tall = panel_height(items.len());
        self.stash.panel_x.set(ui, ((width - 340.0) / 2.0).max(0.0));
        // Over the dock, which floats 6 above the foot and is 60 high.
        self.stash
            .panel_y
            .set(ui, (height - 66.0 - 18.0 - tall).max(0.0));
        self.stash.panel_h.set(ui, tall);
        self.stash.count.set(
            ui,
            s(match items.len() {
                1 => "1 item".to_owned(),
                n => format!("{n} items"),
            }),
        );
        self.stash.empty.set(ui, items.is_empty());
        let tiles = items
            .iter()
            .take(12)
            .map(|item| {
                let (first, second) = two_lines(&item.name);
                Tile {
                    icon: s(icon(item.kind.icon())),
                    tint: Color::hex(item.kind.tint()),
                    first: s(first),
                    second: s(second),
                    gone: item.gone,
                }
            })
            .collect::<Vec<_>>();
        self.stash.tiles.set(ui, tiles);
    }

    /// What the tree asked since the last call.
    pub fn take(&mut self) -> Vec<Asked> {
        let emitted = self.ui.take_signals();
        let mut asked = Vec::new();
        for each in &emitted {
            let Some(signal) = self.stash.signal(each) else {
                continue;
            };
            let index = |i: i32| usize::try_from(i).ok();
            asked.extend(match signal {
                StashSignal::Closed => Some(Asked::Close),
                StashSignal::OpenFolder => Some(Asked::Folder),
                StashSignal::ClearAll => Some(Asked::Clear),
                StashSignal::Pick(i) => index(i).map(Asked::Open),
                StashSignal::Remove(i) => index(i).map(Asked::Remove),
            });
        }
        asked
    }
}

/// Files, the file manager (shelf), which opens a folder on Yoru (the
/// customer's choice: Files is shelf).
pub const FILES: &str = "/bin/shelf";

/// What opens `path`: shelf for a folder where the image has it, else
/// `xdg-open`.
#[must_use]
pub fn opener(path: &Path, files: &Path) -> std::path::PathBuf {
    if path.is_dir() && files.is_file() {
        files.to_path_buf()
    } else {
        std::path::PathBuf::from("xdg-open")
    }
}

/// Open `path`, a folder in Files, anything else with `xdg-open`; whether
/// it started.
fn open(path: &Path) -> bool {
    std::process::Command::new(opener(path, Path::new(FILES)))
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

/// Open `item`'s file, saying so; whether it opened.
fn opened(item: &Item) -> bool {
    let done = open(&item.target);
    say(&format!(
        "stash: open {}: {}",
        item.target.display(),
        if done { "ok" } else { "no xdg-open" }
    ));
    done
}

/// Run the panel until it is closed; the exit status.
#[must_use]
pub fn run() -> i32 {
    crate::switch::once(NAMESPACE, shown).unwrap_or_else(|| {
        say("the stash panel was up: closed");
        0
    })
}

/// The panel's loop.
fn shown(listener: Option<&UnixListener>) -> i32 {
    let Some(dir) = stash::folder() else {
        say("err: no stash without $HOME");
        return 1;
    };
    let mut items = stash::list(&dir);
    let Some(mut view) = View::new(crate::look::fonts()) else {
        say("err: the stash's tree could not be made");
        return 1;
    };
    view.show(&items);
    let layer = LayerOptions {
        output: None,
        layer: Layer::Overlay,
        namespace: NAMESPACE.to_owned(),
        size: (0, 0),
        anchor: Anchor::TOP
            .with(Anchor::BOTTOM)
            .with(Anchor::LEFT)
            .with(Anchor::RIGHT),
        exclusive_zone: -1,
        margin: Margin::default(),
        keyboard: KeyboardInteractivity::Exclusive,
    };
    let options = WindowOptions {
        title: "Stash".to_owned(),
        app_id: NAMESPACE.to_owned(),
        size: (1280, 800),
        background: Color::rgba(0, 0, 0, 0),
        clipboard: false,
        layer: Some(layer),
        ..WindowOptions::default()
    };
    let mut window = match Window::open(view.stash.common(), options) {
        Ok(window) => window,
        Err(error) => {
            say(&format!("err: no compositor to put the stash on: {error}"));
            return 1;
        }
    };
    let watch = listener.map(|listener| window.client().watch_fd(listener.as_raw_fd()));
    let mut size = None;
    loop {
        let happened = match window.dispatch(&mut view.ui, None) {
            Ok(happened) => happened,
            Err(error) => {
                say(&format!("err: the compositor went away: {error}"));
                return 1;
            }
        };
        let mut changed = false;
        if let Some(now) = window.size()
            && size != Some(now)
        {
            if size.is_none() {
                say(&format!(
                    "the stash panel is up, {}x{}, {} items",
                    now.0,
                    now.1,
                    items.len()
                ));
            }
            size = Some(now);
            view.resize(now);
            changed = true;
        }
        for event in happened {
            match event {
                Event::Closed(_) | Event::CloseRequested(_) => return 0,
                Event::Readable(id) if Some(id) == watch => {
                    say("the stash panel was asked again: closed");
                    return 0;
                }
                _ => {}
            }
        }
        for asked in view.take() {
            match asked {
                Asked::Close => {
                    say("the stash panel closed");
                    return 0;
                }
                Asked::Folder => {
                    let done = open(&dir);
                    say(&format!(
                        "stash: open the folder {}: {}",
                        dir.display(),
                        if done { "ok" } else { "no xdg-open" }
                    ));
                    if done {
                        return 0;
                    }
                }
                Asked::Clear => {
                    stash::clear(&dir);
                    say("stash: cleared");
                    items = stash::list(&dir);
                    changed = true;
                }
                Asked::Open(n) => {
                    if items.get(n).is_some_and(opened) {
                        return 0;
                    }
                }
                Asked::Remove(n) => {
                    if let Some(item) = items.get(n) {
                        stash::remove(&dir, &item.link);
                        say(&format!("stash: took out {}", item.name));
                    }
                    items = stash::list(&dir);
                    changed = true;
                }
            }
        }
        if changed {
            view.show(&items);
            if let Err(error) = window.refresh(&mut view.ui) {
                say(&format!("err: the compositor went away: {error}"));
                return 1;
            }
        }
    }
}

/// `yoru-shell --stash FILE...`: put the files in the stash; the exit
/// status.
#[must_use]
pub fn add(files: &[String]) -> i32 {
    let Some(dir) = stash::folder() else {
        eprintln!("yoru-shell: no stash without $HOME");
        return 1;
    };
    let files: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    match stash::add(&dir, &files) {
        Ok(n) => {
            say(&format!("stash: {n} added, {} in all", stash::count(&dir)));
            i32::from(n == 0 && !files.is_empty())
        }
        Err(error) => {
            eprintln!("yoru-shell: {}: {error}", dir.display());
            1
        }
    }
}

#[cfg(test)]
mod opens {
    use super::opener;
    use std::path::Path;

    /// A folder opens in Files where shelf is there, a file with xdg-open.
    #[test]
    fn a_folder_opens_in_files() {
        let dir = std::env::temp_dir();
        let me = std::env::current_exe().expect("this test");
        assert_eq!(opener(&dir, &me), me);
        assert_eq!(opener(&dir, Path::new("/no/shelf")), Path::new("xdg-open"));
        assert_eq!(opener(&me, &me), Path::new("xdg-open"));
    }
}

#[cfg(test)]
mod tests {
    use super::{Asked, View, panel_height, two_lines};
    use crate::stash::{Item, Kind};

    #[test]
    fn names_break_where_a_reader_would() {
        assert_eq!(two_lines("lib.rs"), ("lib.rs".into(), String::new()));
        assert_eq!(
            two_lines("receipt-0417.pdf"),
            ("receipt-".into(), "0417.pdf".into())
        );
        assert_eq!(
            two_lines("Weekly plan 12.md"),
            ("Weekly".into(), "plan 12.md".into())
        );
        assert_eq!(
            two_lines("abcdefghijklmnop"),
            ("abcdefghijk".into(), "lmnop".into())
        );
    }

    fn item(name: &str, kind: Kind) -> Item {
        Item {
            link: name.to_owned(),
            target: format!("/f/{name}").into(),
            name: name.to_owned(),
            kind,
            gone: false,
        }
    }

    /// The panel's tiles take presses: a pick, the remove, and the head's
    /// three buttons.
    #[test]
    fn presses_on_the_panel() {
        let mut view = View::new(crate::look::fonts()).expect("made");
        view.resize((1000, 800));
        let items = vec![
            item("receipt-0417.pdf", Kind::Pdf),
            item("song.flac", Kind::Audio),
        ];
        view.show(&items);
        let top = 800.0 - 66.0 - 18.0 - panel_height(2);
        let left = (1000.0 - 340.0) / 2.0;
        let click = |view: &mut View, x: f32, y: f32| {
            view.ui.update();
            view.ui.pointer_move(x, y);
            view.ui.pointer_press();
            view.ui.pointer_release();
            view.take()
        };
        // The second tile's icon.
        assert_eq!(
            click(&mut view, left + 14.0 + 79.0 + 36.0, top + 80.0 + 30.0),
            vec![Asked::Open(1)]
        );
        // The head: folder, clear, close, from the right.
        assert_eq!(
            click(&mut view, left + 340.0 - 14.0 - 14.0, top + 28.0),
            vec![Asked::Close]
        );
        assert_eq!(
            click(
                &mut view,
                left + 340.0 - 14.0 - 28.0 - 6.0 - 14.0,
                top + 28.0
            ),
            vec![Asked::Clear]
        );
        assert_eq!(
            click(
                &mut view,
                left + 340.0 - 14.0 - 2.0 * 34.0 - 14.0,
                top + 28.0
            ),
            vec![Asked::Folder]
        );
        // Outside the panel.
        assert_eq!(click(&mut view, 20.0, 20.0), vec![Asked::Close]);
    }
}

#[cfg(test)]
mod batched {
    use super::{Asked, View, panel_height};
    use crate::stash::{Item, Kind};

    /// A slow machine reads the pointer's move, press and release in one
    /// go, the panel just drawn: the pick still lands.
    #[test]
    fn a_pick_read_with_its_moves_in_one_go() {
        let mut view = View::new(crate::look::fonts()).expect("made");
        view.resize((1024, 768));
        let items: Vec<Item> = ["a.md", "b.rs", "c.flac", "d.pdf"]
            .iter()
            .map(|name| Item {
                link: (*name).to_owned(),
                target: format!("/f/{name}").into(),
                name: (*name).to_owned(),
                kind: Kind::Other,
                gone: false,
            })
            .collect();
        view.show(&items);
        let top = 768.0 - 84.0 - panel_height(4);
        let left = (1024.0 - 340.0) / 2.0;
        view.ui.update();
        let _ = view.ui.render_into(
            &mut tole::Frame::new((1024, 768), 1.0, tole::Color::hex(0)).expect("a frame"),
        );
        let draw = |view: &mut View| {
            view.ui.update();
            let _ = view.ui.render_into(
                &mut tole::Frame::new((1024, 768), 1.0, tole::Color::hex(0)).expect("a frame"),
            );
        };
        view.ui.pointer_move(left + 10.0, top + 110.0);
        draw(&mut view);
        view.ui.pointer_move(left + 50.0, top + 110.0);
        draw(&mut view);
        view.ui.pointer_press();
        draw(&mut view);
        let pressed = view.take();
        view.ui.pointer_release();
        draw(&mut view);
        assert_eq!(pressed, vec![]);
        assert_eq!(view.take(), vec![Asked::Open(0)]);
    }
}
