//! shelf in a window on the Wayland desktop it runs on: the keys, the
//! timer that brings folder sizes in and takes the toast away, and a frame
//! drawn after each change.

use std::time::{Duration, Instant};

use compositor_toolkit::{Event, KeyboardEvent};
use tole::Element as _;
use tole::window::{Window, WindowOptions};

use crate::app::{Action, App};
use crate::cli::say;
use crate::view::{View, fonts};

/// The window's app id, as the desktop names it.
pub const APP_ID: &str = "rocks.magical.shelf";

/// How often folder sizes are looked for.
const TICK: Duration = Duration::from_millis(250);

/// How long a toast stays.
const TOAST_FOR: Duration = Duration::from_millis(2600);

/// What a key does when no text field took it.
fn key_action(app: &App, key: &str, shift: bool, control: bool, alt: bool) -> Option<Action> {
    Some(match key {
        "Down" => Action::Step(1),
        "Up" if alt => Action::Up(app.focus()),
        "Up" => Action::Step(-1),
        "Return" | "KP_Enter" => Action::OpenPrimary,
        "BackSpace" => Action::Up(app.focus()),
        "Left" if alt => Action::Back,
        "Right" if alt => Action::Forward,
        "Escape" => Action::ClearSelection,
        "Delete" => Action::Trash,
        "F2" => Action::Rename,
        "F5" => Action::CopyOver,
        "F6" => Action::MoveOver,
        "a" | "A" if control => Action::SelectAll,
        "n" | "N" if control && shift => Action::NewFolder,
        "h" | "H" if control => Action::Setting("hidden".to_owned()),
        _ => return None,
    })
}

/// Run shelf until its window closes; the exit status.
#[must_use]
pub fn run(mut app: App) -> i32 {
    let fonts = fonts();
    if fonts.faces().is_empty() {
        say("err: no font was found, so the window has no words (SHELF_FONTS, /usr/share/fonts)");
    }
    let Some(mut view) = View::new(fonts) else {
        say("err: the window's tree could not be made");
        return 1;
    };
    view.show(&mut app);
    let options = WindowOptions {
        title: "Files".to_owned(),
        app_id: APP_ID.to_owned(),
        size: (1240, 800),
        background: tole::Color::hex(if app.settings.dark {
            0x1c_1d_21
        } else {
            0xff_ff_ff
        }),
        min_size: (900, 560),
        resize_edges: true,
        clipboard: true,
        ..WindowOptions::default()
    };
    let mut window = match Window::open(view.shelf.common(), options) {
        Ok(window) => window,
        Err(error) => {
            say(&format!("err: no compositor to open a window on: {error}"));
            return 1;
        }
    };
    let timer = window.client().add_timer(TICK, Some(TICK));
    let mut toast_since: Option<Instant> = None;
    let mut up = false;
    while !window.is_closed() {
        let events = match window.dispatch(&mut view.ui, None) {
            Ok(events) => events,
            Err(error) => {
                say(&format!("err: the compositor went away: {error}"));
                return 1;
            }
        };
        if !up && let Some((w, h)) = window.size() {
            up = true;
            say(&format!("the window is up, {w}x{h}"));
        }
        let mut dirty = false;
        let mut actions: Vec<Action> = Vec::new();
        for event in events {
            match event {
                Event::Timer(id) if id == timer => {
                    if app.sizes.poll() {
                        dirty = true;
                    }
                    if toast_since.is_some_and(|since| since.elapsed() > TOAST_FOR) {
                        app.toast = None;
                        toast_since = None;
                        dirty = true;
                    }
                }
                Event::Keyboard(KeyboardEvent::Key(key)) if key.pressed => {
                    let name = key.keysym.unwrap_or_default();
                    let (shift, control, alt) = (
                        key.modifiers.shift,
                        key.modifiers.control,
                        key.modifiers.alt,
                    );
                    if view.dialog_open() {
                        continue;
                    }
                    // Keys of the window's own, before a field sees them.
                    let own = match name {
                        "Tab" if app.tab().dual && !shift => Some(Action::Focus(1 - app.focus())),
                        "t" | "T" if control => Some(Action::NewTab),
                        "w" | "W" if control => Some(Action::CloseTab(app.active)),
                        "F2" => Some(Action::Rename),
                        "Delete" if !view.typing() => Some(Action::Trash),
                        "Left" | "Right" if alt => key_action(&app, name, shift, control, alt),
                        _ => None,
                    };
                    if let Some(action) = own {
                        actions.push(action);
                        continue;
                    }
                    if view.typing()
                        && !matches!(name, "Up" | "Down" | "Return" | "KP_Enter" | "Escape")
                    {
                        continue;
                    }
                    if let Some(action) = key_action(&app, name, shift, control, alt) {
                        actions.push(action);
                    }
                }
                Event::CloseRequested(_) | Event::Closed(_) => return 0,
                _ => {}
            }
        }
        actions.extend(view.actions());
        for action in actions {
            app.act(action);
            dirty = true;
        }
        if app.toast.is_some() && toast_since.is_none() {
            toast_since = Some(Instant::now());
        }
        if dirty {
            view.show(&mut app);
            if let Err(error) = window.refresh(&mut view.ui) {
                say(&format!("err: a frame could not be drawn: {error}"));
                return 1;
            }
        }
    }
    0
}
