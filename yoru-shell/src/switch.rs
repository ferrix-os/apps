//! The wallpaper switcher on the screen (`yoru-shell --walls`): an
//! overlay layer surface over every window, taking the keyboard while it
//! is up, as Yoru's Alt+I and the dock's Wallpapers button open it.
//!
//! Started again while it is up, it closes: the first one listens on
//! `$XDG_RUNTIME_DIR/yoru-walls.sock` (`/tmp/yoru-walls-UID.sock` with no
//! runtime folder), and a second that reaches it there is the key pressed
//! again. The colours are read off the pictures on a thread of their own,
//! so that the keys stay quick, and kept in
//! `~/.cache/yoru-shell/walls` against each file's time.

use std::collections::BTreeMap;
use std::os::fd::AsRawFd as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use compositor_toolkit::Event;
use tole::Color;
use tole::Element as _;
use tole::window::{
    Anchor, KeyboardInteractivity, Layer, LayerOptions, Margin, Window, WindowOptions,
};

use crate::hypr::Hypr;
use crate::picker::{Asked, View};
use crate::walls::{self, Look, Picker};
use crate::window::say;

/// The surface's namespace.
pub const NAMESPACE: &str = "yoru-walls";

/// The socket a second start finds the first by.
fn socket() -> Option<PathBuf> {
    if let Some(dir) = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|d| !d.is_empty())
    {
        return Some(Path::new(&dir).join("yoru-walls.sock"));
    }
    // No runtime folder: /tmp, by the person's own number.
    use std::os::unix::fs::MetadataExt as _;
    let uid = std::fs::metadata("/proc/self").ok()?.uid();
    Some(PathBuf::from(format!("/tmp/yoru-walls-{uid}.sock")))
}

/// The person's home.
fn home() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// Where the colours read are kept.
fn cache() -> Option<PathBuf> {
    let base = std::env::var("XDG_CACHE_HOME")
        .ok()
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".cache")))?;
    Some(base.join("yoru-shell/walls"))
}

/// A file's time, in seconds.
fn mtime(path: &str) -> u64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_secs())
}

/// The colours kept: path to (time, look).
#[must_use]
pub fn read_cache(text: &str) -> BTreeMap<String, (u64, Look)> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let time = fields.next()?.parse().ok()?;
            let hue = fields.next()?;
            let hue = if hue == "-" {
                None
            } else {
                Some(hue.parse().ok()?)
            };
            let lum = fields.next()?.parse().ok()?;
            let chroma = fields.next()?.parse().ok()?;
            let path = fields.next()?.to_owned();
            Some((path, (time, Look { hue, lum, chroma })))
        })
        .collect()
}

/// The colours kept, as the file holds them.
#[must_use]
pub fn write_cache(kept: &BTreeMap<String, (u64, Look)>) -> String {
    kept.iter()
        .map(|(path, (time, look))| {
            let hue = look.hue.map_or("-".to_owned(), |h| format!("{h:.1}"));
            format!(
                "{time}\t{hue}\t{:.3}\t{:.3}\t{path}\n",
                look.lum, look.chroma
            )
        })
        .collect()
}

/// The switcher over what is found, the one in force in hand.
#[must_use]
pub fn found(hypr: Option<&Hypr>) -> Picker {
    let own = home().map(|h| h.join(walls::OWN));
    let (mut found, folders) = walls::find(Path::new(walls::SYSTEM), own.as_deref());
    let kept = cache()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| read_cache(&text))
        .unwrap_or_default();
    for wall in &mut found {
        if wall.video {
            wall.look = Some(Look::NIGHT);
        } else if let Some((time, look)) = kept.get(&wall.path)
            && *time == mtime(&wall.path)
        {
            wall.look = Some(*look);
        }
    }
    let current = hypr
        .and_then(|hypr| hypr.ask("settings"))
        .and_then(|text| walls::in_force(&text))
        .unwrap_or_else(|| walls::DEFAULT.to_owned());
    Picker::new(found, folders, &current)
}

/// Read the colours of the pictures that have none, off the program's
/// loop (a picture decoded on a slow machine takes long enough for a key
/// held to repeat); each is sent as it is read.
fn read_all(picker: &Picker) -> Option<std::sync::mpsc::Receiver<(String, Look)>> {
    let wanted: Vec<String> = picker
        .walls
        .iter()
        .filter(|wall| wall.look.is_none())
        .map(|wall| wall.path.clone())
        .collect();
    if wanted.is_empty() {
        return None;
    }
    let (send, receive) = std::sync::mpsc::channel();
    let _ = std::thread::Builder::new()
        .name("walls-colours".to_owned())
        .spawn(move || {
            for path in wanted {
                let look = walls::look_of(Path::new(&path)).unwrap_or(Look::NIGHT);
                if send.send((path, look)).is_err() {
                    return;
                }
            }
        });
    Some(receive)
}

/// Take the colours read so far; whether any came. When the last has
/// come they are kept.
fn take_read(picker: &mut Picker, from: &std::sync::mpsc::Receiver<(String, Look)>) -> bool {
    let mut came = false;
    while let Ok((path, look)) = from.try_recv() {
        for wall in picker.walls.iter_mut().filter(|wall| wall.path == path) {
            wall.look = Some(look);
        }
        came = true;
    }
    if came
        && picker.walls.iter().all(|wall| wall.look.is_some())
        && let Some(path) = cache()
    {
        let kept: BTreeMap<String, (u64, Look)> = picker
            .walls
            .iter()
            .filter(|wall| !wall.video)
            .filter_map(|wall| Some((wall.path.clone(), (mtime(&wall.path), wall.look?))))
            .collect();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, write_cache(&kept));
    }
    came
}

/// Set the one in hand as every screen's wallpaper; whether hyprix took
/// it.
fn apply(hypr: Option<&Hypr>, picker: &Picker) -> bool {
    let Some(wall) = picker.held().and_then(|i| picker.walls.get(i)) else {
        return false;
    };
    let line = format!("setting wallpaper {}", wall.setting());
    let done = hypr
        .and_then(|hypr| hypr.ask(&line))
        .is_some_and(|said| said.trim() == "ok");
    say(&format!(
        "wallpaper set: {} ({}): {}",
        wall.name,
        wall.path,
        if done { "ok" } else { "refused" }
    ));
    done
}

/// Run the switcher until it is closed; the exit status.
#[must_use]
pub fn run() -> i32 {
    let listener = match socket() {
        Some(path) => {
            if UnixStream::connect(&path).is_ok() {
                say("the wallpaper switcher was up: closed");
                return 0;
            }
            let _ = std::fs::remove_file(&path);
            UnixListener::bind(&path)
                .ok()
                .map(|listener| (listener, path))
        }
        None => None,
    };
    if let Some((listener, _)) = &listener {
        let _ = listener.set_nonblocking(true);
    }
    let code = shown(listener.as_ref().map(|(listener, _)| listener));
    if let Some((_, path)) = listener {
        let _ = std::fs::remove_file(path);
    }
    code
}

/// The switcher's loop.
fn shown(listener: Option<&UnixListener>) -> i32 {
    let hypr = Hypr::from_env();
    let mut picker = found(hypr.as_ref());
    let Some(mut view) = View::new(crate::look::fonts()) else {
        say("err: the switcher's tree could not be made");
        return 1;
    };
    view.show(&picker);
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
        title: "Wallpapers".to_owned(),
        app_id: NAMESPACE.to_owned(),
        size: (1280, 800),
        background: Color::rgba(0, 0, 0, 0),
        clipboard: false,
        layer: Some(layer),
        ..WindowOptions::default()
    };
    let mut window = match Window::open(view.walls.common(), options) {
        Ok(window) => window,
        Err(error) => {
            say(&format!(
                "err: no compositor to put the switcher on: {error}"
            ));
            return 1;
        }
    };
    let watch = listener.map(|listener| window.client().watch_fd(listener.as_raw_fd()));
    let mut size = None;
    let reading = read_all(&picker);
    loop {
        let wanted = picker.walls.iter().any(|wall| wall.look.is_none());
        let wait = (wanted && reading.is_some()).then_some(Duration::from_millis(150));
        let happened = match window.dispatch(&mut view.ui, wait) {
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
                    "the wallpaper switcher is up, {}x{}, {} wallpapers in {} folders, {} in hand",
                    now.0,
                    now.1,
                    picker.walls.len(),
                    picker.folders.len(),
                    picker
                        .held()
                        .and_then(|i| picker.walls.get(i))
                        .map_or("none", |wall| wall.name.as_str())
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
                    say("the wallpaper switcher was asked again: closed");
                    return 0;
                }
                _ => {}
            }
        }
        if let Some(from) = &reading
            && take_read(&mut picker, from)
        {
            changed = true;
        }
        let before = picker.clone();
        for asked in view.take(&mut picker) {
            match asked {
                Asked::Close => {
                    say("the wallpaper switcher closed, the wallpaper kept");
                    return 0;
                }
                Asked::Again => {
                    say("the wallpaper switcher was asked again: closed");
                    return 0;
                }
                Asked::Apply => {
                    if apply(hypr.as_ref(), &picker) {
                        return 0;
                    }
                }
                Asked::Named(name) => {
                    let made = home().map(|h| h.join(walls::OWN).join(&name));
                    let done = made.is_some_and(|dir| std::fs::create_dir_all(dir).is_ok());
                    say(&format!(
                        "folder {name}: {}",
                        if done { "made" } else { "refused" }
                    ));
                }
            }
        }
        if changed || picker != before {
            if picker.pos != before.pos || picker.layout != before.layout {
                let held = picker.held().and_then(|i| picker.walls.get(i));
                say(&format!(
                    "in hand: {} ({:?}, {} of {}, was {})",
                    held.map_or("none", |wall| wall.name.as_str()),
                    picker.layout,
                    picker.pos + 1,
                    picker.list().len(),
                    before.pos + 1
                ));
            }
            view.show(&picker);
            if let Err(error) = window.refresh(&mut view.ui) {
                say(&format!("err: the compositor went away: {error}"));
                return 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{read_cache, write_cache};
    use crate::walls::Look;

    #[test]
    fn the_colours_kept_read_back() {
        let mut kept = std::collections::BTreeMap::new();
        kept.insert(
            "/a b/c.jpg".to_owned(),
            (
                7,
                Look {
                    hue: Some(250.0),
                    lum: 0.25,
                    chroma: 0.125,
                },
            ),
        );
        kept.insert(
            "/g.png".to_owned(),
            (
                9,
                Look {
                    hue: None,
                    lum: 0.5,
                    chroma: 0.0,
                },
            ),
        );
        assert_eq!(read_cache(&write_cache(&kept)), kept);
        assert!(read_cache("garbage\n1\tx\n").is_empty());
    }
}
