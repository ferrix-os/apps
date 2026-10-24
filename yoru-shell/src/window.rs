//! The dock on the screen: the state and the tree joined to a layer-shell
//! surface by tole's window, and kept up to date from hyprix.
//!
//! One loop. hyprix's event socket is watched, and an event that moves a
//! workspace or a window has the dock ask again (`j/workspaces`,
//! `j/activeworkspace`, `j/clients`); a timer each second moves the clock
//! and the pomodoro; a press becomes a dispatcher. What happens is said on
//! standard output, a line each, which is how a boot that drives the dock
//! knows what it did.

use std::io::Read as _;
use std::os::fd::AsRawFd as _;
use std::time::{Duration, SystemTime};

use compositor_toolkit::Event;
use tole::Color;
use tole::Element as _;
use tole::window::{
    Anchor, KeyboardInteractivity, Layer, LayerOptions, Margin, Window, WindowOptions,
};

use crate::hypr::{self, Hypr};
use crate::json::Json;
use crate::model::{self, State, Tool};
use crate::ui::{Action, HEIGHT, View};

/// The surface's namespace, which `layerrule`s match: Yoru's theme blurs
/// what is behind every `yoru-` surface.
pub const NAMESPACE: &str = "yoru-dock";

/// The launcher's look in Yoru's theme, which fuzzel is started with.
const PALETTE: &str = "/usr/share/hyprix/themes/yoru-fuzzel.ini";

/// How far above the screen's foot the dock floats, and in from its sides.
const MARGIN: i32 = 6;

/// Say one line of what the program did.
pub fn say(text: &str) {
    use std::io::Write as _;
    let mut out = std::io::stdout();
    let _ = writeln!(out, "yoru-shell: {text}");
    let _ = out.flush();
}

/// Seconds since 1970 in the local zone.
fn now() -> i64 {
    let utc = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0));
    utc + model::offset(std::env::var("TZ").ok().as_deref())
}

/// The applications installed, as `(id, command, window class)`.
fn installed() -> Vec<(String, String, Option<String>)> {
    let var = |name: &str| std::env::var(name).ok();
    let search = compositor_desktop::entry::Search::listing(&var);
    compositor_desktop::entry::find_programs(&search)
        .into_iter()
        .filter(|app| app.visible)
        .filter_map(|app| {
            let exec = model::command(app.exec.as_deref()?);
            Some((app.id?, exec, app.app_id))
        })
        .collect()
}

/// Ask hyprix again for the workspaces and the windows.
fn refresh(hypr: Option<&Hypr>, state: &mut State) {
    let Some(hypr) = hypr else {
        return;
    };
    let ask = |what: &str| hypr.ask(what).and_then(|text| Json::parse(&text));
    let active = ask("j/activeworkspace")
        .and_then(|json| json.get("id").and_then(Json::int))
        .unwrap_or(1);
    if let Some(list) = ask("j/workspaces") {
        state.workspaces(&list, active);
    }
    if let Some(list) = ask("j/clients") {
        state.clients(&list);
    }
}

/// Do what a press asked.
fn act(hypr: Option<&Hypr>, state: &mut State, action: Action) {
    let dispatch = |what: &str| {
        let done = hypr.is_some_and(|hypr| hypr.dispatch(what));
        say(&format!(
            "dispatch {what}: {}",
            if done { "ok" } else { "refused" }
        ));
    };
    match action {
        Action::Search | Action::Tool(Tool::Search) => {
            // fuzzel in Yoru's look where the theme put it (step Y3).
            if std::path::Path::new(PALETTE).exists() {
                dispatch(&format!("exec /bin/fuzzel --config {PALETTE}"));
            } else {
                dispatch("exec /bin/fuzzel");
            }
        }
        Action::Space(at) => {
            if let Some(space) = state.spaces.get(at) {
                dispatch(&format!("workspace {}", space.id));
            }
        }
        Action::Launch(at) => {
            if let Some(launcher) = state.launchers.get(at) {
                let exec = launcher.exec.clone();
                dispatch(&format!("exec {exec}"));
            }
        }
        Action::Tool(Tool::Scratchpad) => dispatch("togglespecialworkspace scratch"),
        Action::Tool(Tool::Settings) => dispatch("exec settings"),
        Action::Focus => {
            state.focus = !state.focus;
            let line = format!("decoration:dim_inactive {}", i32::from(state.focus));
            let done = hypr.is_some_and(|hypr| hypr.keyword(&line));
            say(&format!(
                "focus mode {}: {}",
                if state.focus { "on" } else { "off" },
                if done { "ok" } else { "refused" }
            ));
        }
        Action::Timer => {
            state.pomodoro.toggle();
            say(&format!(
                "pomodoro {} at {}",
                if state.pomodoro.running {
                    "started"
                } else {
                    "paused"
                },
                state.pomodoro.label()
            ));
        }
    }
}

/// Read what the event socket has into `pending`; false when it closed.
fn drain(stream: &mut std::os::unix::net::UnixStream, pending: &mut String) -> bool {
    let mut bytes = [0u8; 4096];
    loop {
        match stream.read(&mut bytes) {
            Ok(0) => return false,
            Ok(n) => pending.push_str(&String::from_utf8_lossy(bytes.get(..n).unwrap_or_default())),
            Err(_) => return true,
        }
    }
}

/// Run the dock until the compositor goes away; the exit status.
#[must_use]
pub fn run() -> i32 {
    let hypr = Hypr::from_env();
    if hypr.is_none() {
        say(
            "no hyprix here (HYPRLAND_INSTANCE_SIGNATURE is not set): the workspaces stay as they are",
        );
    }
    let mut state = State::new(model::launchers(&installed()), now());
    refresh(hypr.as_ref(), &mut state);
    let Some(mut view) = View::new(crate::look::fonts()) else {
        say("err: the dock's tree could not be made");
        return 1;
    };
    view.show(&state);
    let layer = LayerOptions {
        output: None,
        layer: Layer::Top,
        namespace: NAMESPACE.to_owned(),
        size: (0, HEIGHT),
        anchor: Anchor::BOTTOM.with(Anchor::LEFT).with(Anchor::RIGHT),
        exclusive_zone: i32::try_from(HEIGHT).unwrap_or(60) + MARGIN,
        margin: Margin {
            top: 0,
            right: 2 * MARGIN,
            bottom: MARGIN,
            left: 2 * MARGIN,
        },
        keyboard: KeyboardInteractivity::None,
    };
    let options = WindowOptions {
        title: "Yoru".to_owned(),
        app_id: NAMESPACE.to_owned(),
        size: (1280, HEIGHT),
        background: Color::rgba(0, 0, 0, 0),
        clipboard: false,
        layer: Some(layer),
        ..WindowOptions::default()
    };
    let mut window = match Window::open(view.dock.common(), options) {
        Ok(window) => window,
        Err(error) => {
            say(&format!("err: no compositor to put the dock on: {error}"));
            return 1;
        }
    };
    let mut events = hypr.as_ref().and_then(Hypr::events);
    let watch = events
        .as_ref()
        .map(|stream| window.client().watch_fd(stream.as_raw_fd()));
    let tick = window
        .client()
        .add_timer(Duration::from_secs(1), Some(Duration::from_secs(1)));
    let mut pending = String::new();
    let mut up = false;
    loop {
        let happened = match window.dispatch(&mut view.ui, None) {
            Ok(happened) => happened,
            Err(error) => {
                say(&format!("err: the compositor went away: {error}"));
                return 1;
            }
        };
        if !up && let Some((w, h)) = window.size() {
            up = true;
            say(&format!(
                "the dock is up, {w}x{h}, {} workspaces, {} launchers",
                state.spaces.len(),
                state.launchers.len()
            ));
        }
        let mut changed = false;
        for event in happened {
            match event {
                Event::Closed(_) | Event::CloseRequested(_) => return 0,
                Event::Timer(timer) if timer == tick => {
                    state.clock = model::Clock::at(now());
                    if state.pomodoro.tick() {
                        say(&format!("pomodoro: {} begins", state.pomodoro.mode()));
                    }
                    changed = true;
                }
                Event::Readable(id) if Some(id) == watch => {
                    if let Some(stream) = events.as_mut()
                        && !drain(stream, &mut pending)
                    {
                        say("hyprix's event socket closed");
                        watch
                            .into_iter()
                            .for_each(|w| window.client().unwatch_fd(w));
                        events = None;
                    }
                    let mut moved = false;
                    while let Some(end) = pending.find('\n') {
                        let line: String = pending.drain(..=end).collect();
                        moved |= hypr::wanted(line.trim_end());
                    }
                    if moved {
                        refresh(hypr.as_ref(), &mut state);
                        changed = true;
                    }
                }
                _ => {}
            }
        }
        for action in view.actions() {
            act(hypr.as_ref(), &mut state, action);
            refresh(hypr.as_ref(), &mut state);
            changed = true;
        }
        if changed {
            view.show(&state);
            if let Err(error) = window.refresh(&mut view.ui) {
                say(&format!("err: the compositor went away: {error}"));
                return 1;
            }
        }
    }
}
