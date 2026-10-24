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

use crate::backend::{Desktop, Monitor, System};
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
pub fn state(
    options: &Options,
    var: &dyn Fn(&str) -> Option<String>,
    system: &mut dyn System,
) -> State {
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
    // What is in force -- Yoru's theme, the person's hyprland.conf -- and
    // over it what Settings itself kept, as the pages compare them.
    state.values = system.options(crate::backend::OPTION_KEYS);
    state.values.extend(system.settings());
    state.accent = state.desktop_accent().unwrap_or(0);
    state.monitors = system.monitors();
    state.typing_in = system.layout_now().unwrap_or_else(|| {
        let first = state.kb_layouts().into_iter().next().unwrap_or_default();
        crate::desktop::LAYOUTS
            .iter()
            .find(|(id, _)| *id == first)
            .map_or(first, |(_, name)| (*name).to_owned())
    });
    let idle = crate::backend::hypridle_path(var).and_then(|p| system.read(&p));
    state.idle = crate::desktop::Idle::read(idle.as_deref());
    state.about = about(var);
    state.machine =
        crate::machine::Machine::read(&ferrix_settings_client::manifest::Manifest::read());
    state
}

/// What the computer runs, for About: Ferrix's version, the kernel's
/// profile, whether the configuration is the certified one.
fn about(var: &dyn Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    let release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let field = |name: &str| {
        release.lines().find_map(|line| {
            line.strip_prefix(name)
                .and_then(|rest| rest.strip_prefix('='))
                .map(|v| v.trim_matches('"').to_owned())
        })
    };
    let version = var("SETTINGS_VERSION")
        .or_else(|| field("VERSION"))
        .or_else(|| field("PRETTY_NAME"))
        .unwrap_or_else(|| "unknown".to_owned());
    let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
    let word = |name: &str| {
        cmdline
            .split_whitespace()
            .find_map(|w| w.strip_prefix(name).and_then(|v| v.strip_prefix('=')))
            .map(str::to_owned)
    };
    let profile = var("SETTINGS_PROFILE")
        .or_else(|| word("ferrix.profile"))
        .unwrap_or_else(|| "desktop".to_owned());
    let certified = if word("ferrix.checks").as_deref() == Some("run") {
        "Certified"
    } else {
        "Not certified"
    };
    vec![
        ("Version".to_owned(), version),
        ("Kernel profile".to_owned(), profile),
        ("Reference configuration".to_owned(), certified.to_owned()),
    ]
}

/// The desktop Settings changes: hyprix's, or for `SETTINGS_FAKE=1` (a
/// drawing for the README) one made up as the design shows it.
#[must_use]
pub fn system(var: &dyn Fn(&str) -> Option<String>) -> Box<dyn System> {
    if var("SETTINGS_FAKE").as_deref() != Some("1") {
        return Box::new(Desktop);
    }
    let mut fake = crate::backend::Fake::default();
    for (key, value) in [
        (
            "general:col.active_border",
            crate::desktop::accent_colours(250.0)[0].clone(),
        ),
        ("decoration:rounding", "8".to_owned()),
        ("input:follow_mouse", "1".to_owned()),
    ] {
        let _ = fake.kept.insert(key.to_owned(), value);
    }
    let modes = |list: &[&str]| list.iter().map(|m| (*m).to_owned()).collect();
    fake.displays = vec![
        Monitor {
            name: "eDP-1".to_owned(),
            model: "13.5\" panel".to_owned(),
            width: 2560,
            height: 1600,
            refresh: 60.0,
            scale: 1.5,
            modes: modes(&[
                "2560x1600@60.00Hz",
                "2560x1600@120.00Hz",
                "1920x1200@60.00Hz",
                "1280x800@60.00Hz",
            ]),
            ..Monitor::default()
        },
        Monitor {
            name: "DP-1".to_owned(),
            model: "Dell U2723QE".to_owned(),
            width: 3840,
            height: 2160,
            refresh: 60.0,
            scale: 1.5,
            x: 1707,
            modes: modes(&[
                "3840x2160@60.00Hz",
                "3840x2160@30.00Hz",
                "2560x1440@60.00Hz",
                "1920x1080@60.00Hz",
            ]),
            ..Monitor::default()
        },
    ];
    Box::new(fake)
}

/// Do `effect`, keeping what it changed in `state`: answers what to say.
pub fn apply(
    state: &mut State,
    effect: &Effect,
    system: &mut dyn System,
    var: &dyn Fn(&str) -> Option<String>,
) -> String {
    let said = match effect {
        Effect::GameMode { id, on } => {
            let Some(path) = state.games.file.clone() else {
                return "err: there is no home folder for the game-mode file".to_owned();
            };
            return match crate::store::switch(&path, id, *on) {
                Ok(set) => {
                    state.games.set = set;
                    format!(
                        "game mode {} for {id}; wrote {}",
                        if *on { "on" } else { "off" },
                        path.display()
                    )
                }
                Err(why) => format!("err: {why}"),
            };
        }
        Effect::Set(pairs) => {
            for (key, value) in pairs {
                if let Err(why) = system.set(key, value) {
                    return format!("err: {key} = {value}: {why}");
                }
            }
            let list: Vec<String> = pairs.iter().map(|(k, v)| format!("{k} = {v}")).collect();
            format!("set {}", list.join(", "))
        }
        Effect::Wall { set, reset } => {
            for key in reset {
                if let Err(why) = system.reset(key) {
                    return format!("err: reset {key}: {why}");
                }
            }
            for (key, value) in set {
                if let Err(why) = system.set(key, value) {
                    return format!("err: {key} = {value}: {why}");
                }
            }
            let list: Vec<String> = set.iter().map(|(k, v)| format!("{k} = {v}")).collect();
            format!("set {}", list.join(", "))
        }
        Effect::Monitor(line) => match system.monitor(line) {
            Ok(()) => {
                state.monitors = system.monitors();
                format!("monitor {line}")
            }
            Err(why) => return format!("err: monitor {line}: {why}"),
        },
        Effect::NextLayout => match system.next_layout() {
            Ok(()) => {
                if let Some(now) = system.layout_now() {
                    state.typing_in = now;
                }
                format!("typing in {}", state.typing_in)
            }
            Err(why) => return format!("err: switchxkblayout: {why}"),
        },
        Effect::Change(request) => match system.change(request) {
            Ok(text) => {
                state.machine_changed(request);
                text
            }
            Err(why) => return format!("err: {why}"),
        },
        Effect::Idle { lock, off } => {
            let Some(path) = crate::backend::hypridle_path(var) else {
                return "err: there is no home folder for hypridle.conf".to_owned();
            };
            if let Err(why) = system.write(&path, &crate::desktop::Idle::text(*lock, *off)) {
                return format!("err: {why}");
            }
            format!("wrote {}", path.display())
        }
    };
    state.applied(effect);
    said
}

/// Act on `action`: answers whether the page changed, so its field takes
/// the keys.
pub fn act(
    state: &mut State,
    action: Action,
    system: &mut dyn System,
    var: &dyn Fn(&str) -> Option<String>,
) -> bool {
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
        Action::Mode(mode) => {
            state.advanced = mode == 1;
            None
        }
        Action::Pick(row, option) => state.pick(&row, option),
        Action::Slide(row, value) => state.slide(&row, value),
        Action::Pressed(row) => {
            state.press_opens(&row);
            state.press(&row)
        }
        Action::Browse => {
            state.want_chooser = true;
            None
        }
        Action::Flip(row) => state.flip(&row),
        Action::Edited(row, text) => {
            state.edited(&row, &text);
            None
        }
        Action::Accepted(row) => state.accept(&row),
        Action::Chosen(path) => state.wall_chosen(&path),
    };
    if let Some(effect) = effect {
        let said = apply(state, &effect, system, var);
        say(&said);
        state.status = Some(said);
    }
    state.page != before
}

/// Open the window and run until it is closed. Answers the exit status.
#[must_use]
pub fn run(options: &Options) -> i32 {
    let mut system = system(&var);
    let mut state = state(options, &var, system.as_mut());
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
    // Game mode's filter takes the keys once its row is made, which is
    // once the window has a size.
    let mut focus = state.page == Page::Game;
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
        if focus && up && view.focus_field() {
            focus = false;
            let _ = window.refresh(&mut view.ui);
        }
        let actions = view.actions();
        if actions.is_empty() {
            continue;
        }
        let mut moved = false;
        for action in actions {
            moved |= act(&mut state, action, system.as_mut(), &var);
        }
        view.show(&state);
        if moved && state.page == Page::Game {
            focus = !view.focus_field();
        }
        if std::mem::take(&mut state.want_chooser) {
            view.open_chooser();
        }
        if let Err(error) = window.refresh(&mut view.ui) {
            say(&format!("err: a frame could not be drawn: {error}"));
            return 1;
        }
    }
    0
}
