//! The window: the state and the tree joined to a Wayland toplevel by
//! tole's window.
//!
//! One loop. The tree's presses become changes to the [`State`]; a change
//! that asks something of the system ([`Effect`]) is done at once and
//! shown only once it has been; then the tree is handed the state again.
//! What happens is said on standard output, a line each, which is how the
//! boot that drives this window knows what it did.

use std::path::PathBuf;

use compositor_desktop::game_mode::{self, GameMode};
use compositor_toolkit::{Event, KeyboardEvent};
use tole::Element as _;
use tole::window::{Window, WindowOptions};

use crate::pages::{Effect, Page, State};
use crate::ui::{Action, View};

/// The window's `app_id`: its `.desktop` file's `StartupWMClass`.
pub const APP_ID: &str = "rocks.magical.settings";

/// How a window is opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// The page it opens on.
    pub page: Page,
    /// Whether it is dark.
    pub dark: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            page: Page::default(),
            dark: true,
        }
    }
}

/// Say one line of what the program did.
pub(crate) fn say(text: &str) {
    use std::io::Write as _;
    let mut out = std::io::stdout();
    let _ = writeln!(out, "settings: {text}");
    let _ = out.flush();
}

/// The environment, as the libraries ask for it.
pub(crate) fn var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// The state a window opens with: the page, the applications and the
/// user's game-mode file, who is signed in and where.
#[must_use]
pub fn state(options: &Options, var: &dyn Fn(&str) -> Option<String>) -> State {
    let mut state = State::new(options.page);
    state.dark = options.dark;
    state.user = var("USER")
        .or_else(|| var("LOGNAME"))
        .unwrap_or_else(|| "you".to_owned());
    // SETTINGS_HOSTNAME stands in for the kernel's name, so a picture
    // drawn for the README says the same on every host.
    state.hostname = var("SETTINGS_HOSTNAME")
        .or_else(|| std::fs::read_to_string("/proc/sys/kernel/hostname").ok())
        .map(|name| name.trim().to_owned())
        .unwrap_or_default();
    state.games.apps = crate::apps::installed(var);
    state.games.file = game_mode::path(var);
    if let Some(path) = &state.games.file {
        match GameMode::load(path) {
            Ok((set, complaints)) => {
                for complaint in complaints {
                    say(&format!("warn: {}: {complaint}", path.display()));
                }
                state.games.set = set;
            }
            Err(error) => say(&format!("err: {}: {error}", path.display())),
        }
    }
    state
}

/// Do `effect`, keeping what it changed in `state`: answers what to say.
pub fn apply(state: &mut State, effect: &Effect) -> String {
    match effect {
        Effect::GameMode { id, on } => {
            let Some(path) = state.games.file.clone() else {
                return "err: there is no home folder for the game-mode file".to_owned();
            };
            match crate::store::switch(&path, id, *on) {
                Ok(set) => {
                    state.games.set = set;
                    format!(
                        "game mode {} for {id}; wrote {}",
                        if *on { "on" } else { "off" },
                        path.display()
                    )
                }
                Err(why) => format!("err: {why}"),
            }
        }
    }
}

/// Act on `action`: answers whether the page changed, so its field takes
/// the keys.
pub fn act(state: &mut State, action: Action) -> bool {
    let before = state.page;
    let effect = match action {
        Action::Search(text) => {
            state.search(&text);
            None
        }
        Action::Navigate(nav) => {
            state.navigate(nav);
            None
        }
        Action::Mode(_) | Action::Pick(..) | Action::Slide(..) | Action::Pressed(_) => None,
        Action::Flip(row) => state.flip(&row),
        Action::Edited(row, text) => {
            state.edited(&row, &text);
            None
        }
        Action::Accepted(row) => state.accept(&row),
    };
    if let Some(effect) = effect {
        let said = apply(state, &effect);
        say(&said);
        state.status = Some(said);
    }
    state.page != before
}

/// Open the window and run until it is closed. Answers the exit status.
#[must_use]
pub fn run(options: &Options) -> i32 {
    let mut state = state(options, &var);
    let file = state.games.file.as_ref().map_or_else(
        || "no file".to_owned(),
        |p: &PathBuf| p.display().to_string(),
    );
    say(&format!(
        "{} applications listed, {} in game mode ({file})",
        state.games.apps.len(),
        state
            .games
            .apps
            .iter()
            .filter(|app| state.games.set.contains(&app.id))
            .count()
    ));
    let fonts = crate::ui::fonts();
    let Some(mut view) = View::new(fonts) else {
        say("err: the window's tree could not be made");
        return 1;
    };
    view.show(&state);
    let background = crate::look::Palette::new(state.dark, 250.0).bg;
    let window_options = WindowOptions {
        title: "Settings".to_owned(),
        app_id: APP_ID.to_owned(),
        size: (1180, 820),
        background,
        min_size: (760, 520),
        ..WindowOptions::default()
    };
    let mut window = match Window::open(view.shell.common(), window_options) {
        Ok(window) => window,
        Err(error) => {
            say(&format!("err: no compositor to open a window on: {error}"));
            return 1;
        }
    };
    if state.page == Page::Game {
        view.focus_field();
    }
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
            say(&format!(
                "the window is up, {w}x{h}, on {}",
                state.page.label()
            ));
        }
        for event in events {
            match event {
                Event::CloseRequested(_) | Event::Closed(_) => return 0,
                Event::Keyboard(KeyboardEvent::Key(key))
                    if key.pressed
                        && key.modifiers.control
                        && matches!(key.keysym, Some("q" | "w")) =>
                {
                    return 0;
                }
                _ => {}
            }
        }
        let actions = view.actions();
        if actions.is_empty() {
            continue;
        }
        let mut moved = false;
        for action in actions {
            moved |= act(&mut state, action);
        }
        view.show(&state);
        if moved && state.page == Page::Game {
            view.focus_field();
        }
        if let Err(error) = window.refresh(&mut view.ui) {
            say(&format!("err: a frame could not be drawn: {error}"));
            return 1;
        }
    }
    0
}
