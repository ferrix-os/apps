//! The window: the state and the tree joined to a Wayland toplevel by
//! tole's window.
//!
//! One loop. The tree's presses become changes to the [`State`]; a change
//! that asks something of the system ([`Effect`]) is started at once, and
//! what the daemon answers comes back over a channel, so the window keeps
//! drawing while it scans the disks or installs. What happens is said on
//! standard output, a line each.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use compositor_toolkit::{Event, KeyboardEvent};
use tole::Element as _;
use tole::window::{Window, WindowOptions};

use crate::pages::{Effect, Page, State};
use crate::protocol;
use crate::ui::{Action, View};

/// The window's `app_id`: its `.desktop` file's `StartupWMClass`.
pub const APP_ID: &str = "rocks.magical.installer";

/// The window's size.
pub const SIZE: (u32, u32) = (900, 640);

/// Say one line of what the program did.
pub(crate) fn say(text: &str) {
    use std::io::Write as _;
    let mut out = std::io::stdout();
    let _ = writeln!(out, "ferrix-installer: {text}");
    let _ = out.flush();
}

/// What the daemon's side sends back to the window.
#[derive(Debug)]
pub enum Answer {
    /// The disks, or why not.
    Disks(Result<protocol::Disks, String>),
    /// A line of the install's stream.
    Install(protocol::Event),
}

/// What the program does with an effect: the window's own, or a test's.
pub trait System {
    /// Make the live session type in `layout` and `variant`.
    fn keyboard(&mut self, layout: &str, variant: &str);
    /// Ask for the disks; the answer comes as [`Answer::Disks`].
    fn disks(&mut self);
    /// Install; the stream comes as [`Answer::Install`]s.
    fn install(&mut self, install: protocol::Install);
    /// Restart the machine.
    ///
    /// # Errors
    ///
    /// The daemon could not do it, in words.
    fn reboot(&mut self) -> Result<(), String>;
}

/// The running system: `hyprctl` and the daemon on its socket.
#[derive(Debug)]
pub struct Live {
    /// The daemon's socket.
    pub socket: PathBuf,
    /// Where answers go.
    pub answers: Sender<Answer>,
}

impl System for Live {
    fn keyboard(&mut self, layout: &str, variant: &str) {
        let lines = crate::keyboard::hyprctl(layout, variant);
        let _ = std::thread::spawn(move || {
            for args in lines {
                // The live session follows when it can; an installer
                // started outside one simply does not.
                let _ = std::process::Command::new("/bin/hyprctl")
                    .args(&args)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        });
        say(&format!("keyboard {layout} {variant}"));
    }

    fn disks(&mut self) {
        let socket = self.socket.clone();
        let answers = self.answers.clone();
        let _ = std::thread::spawn(move || {
            let _ = answers.send(Answer::Disks(protocol::disks(&socket)));
        });
    }

    fn install(&mut self, install: protocol::Install) {
        let socket = self.socket.clone();
        let answers = self.answers.clone();
        say(&format!("installing on {}", install.dev));
        let _ = std::thread::spawn(move || {
            protocol::install(&socket, &install, &mut |event| {
                let _ = answers.send(Answer::Install(event));
            });
        });
    }

    fn reboot(&mut self) -> Result<(), String> {
        protocol::reboot(&self.socket)
    }
}

/// What [`act`] asks of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Go on.
    Stay,
    /// Close the window.
    Close,
}

/// Do `effect` through `system`.
pub fn apply(state: &mut State, effect: Effect, system: &mut dyn System) -> Outcome {
    state.status = None;
    match effect {
        Effect::Close => return Outcome::Close,
        Effect::Keyboard(layout, variant) => system.keyboard(&layout, &variant),
        Effect::Disks => {
            state.disks = None;
            system.disks();
        }
        Effect::Install(install) => system.install(install),
        Effect::Reboot => {
            if let Err(why) = system.reboot() {
                say(&format!("err: reboot: {why}"));
                state.status = Some(format!("Could not restart: {why}"));
            }
        }
    }
    Outcome::Stay
}

/// Act on `action`.
pub fn act(state: &mut State, action: Action, system: &mut dyn System) -> Outcome {
    let effect = match action {
        Action::Pressed(row, option) => state.press(&row.key, option),
        Action::Edited(row, text) => {
            state.edited(&row.key, &text);
            None
        }
        Action::Accepted(row) => state.accept(&row.key),
        Action::Back => {
            state.back();
            None
        }
        Action::Next => state.go_next(),
    };
    match effect {
        Some(effect) => apply(state, effect, system),
        None => Outcome::Stay,
    }
}

/// Take what the daemon answered into `state`.
pub fn answer(state: &mut State, answer: Answer) {
    match answer {
        Answer::Disks(disks) => {
            if let Err(why) = &disks {
                say(&format!("err: disks: {why}"));
            }
            state.set_disks(disks);
        }
        Answer::Install(event) => {
            match &event {
                protocol::Event::Done => say("installed"),
                protocol::Event::Error(why) => say(&format!("err: install: {why}")),
                _ => {}
            }
            state.event(event);
        }
    }
}

/// The state a window opens with: the zones read.
#[must_use]
pub fn state(zoneinfo: &Path) -> State {
    State {
        zones: crate::zones::read(zoneinfo),
        ..State::default()
    }
}

/// Open the window and run until it is closed. Answers the exit status.
#[must_use]
pub fn run() -> i32 {
    let var = |name: &str| std::env::var(name).ok();
    let (send, answers): (Sender<Answer>, Receiver<Answer>) = channel();
    let mut system = Live {
        socket: protocol::socket(&var),
        answers: send,
    };
    let mut state = state(Path::new(crate::zones::ZONEINFO));
    // The disks are asked for at once, so the Disk page has them.
    let _ = apply(&mut state, Effect::Disks, &mut system);
    let Some(mut view) = View::new(crate::ui::fonts()) else {
        say("err: the window's tree could not be made");
        return 1;
    };
    view.show(&state);
    let options = WindowOptions {
        title: "Install Ferrix".to_owned(),
        app_id: APP_ID.to_owned(),
        size: SIZE,
        background: tole::Color::hex(0x0016_1826),
        min_size: (760, 560),
        ..WindowOptions::default()
    };
    let mut window = match Window::open(view.shell.common(), options) {
        Ok(window) => window,
        Err(error) => {
            say(&format!("err: no compositor to open a window on: {error}"));
            return 1;
        }
    };
    let mut up = false;
    while !window.is_closed() {
        // Wake now and then while the daemon may be answering.
        let events = match window.dispatch(&mut view.ui, Some(Duration::from_millis(100))) {
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
        for event in events {
            match event {
                Event::CloseRequested(_) | Event::Closed(_) => {
                    if matches!(state.page, Page::Installing) {
                        // Closing mid-install would leave a half-written
                        // disk with no one watching: say so, stay open.
                        state.status =
                            Some("Ferrix is being installed; wait until it is done.".to_owned());
                    } else {
                        return 0;
                    }
                }
                Event::Keyboard(KeyboardEvent::Key(key))
                    if key.pressed
                        && key.modifiers.control
                        && matches!(key.keysym, Some("q" | "w"))
                        && !matches!(state.page, Page::Installing) =>
                {
                    return 0;
                }
                _ => {}
            }
        }
        let mut changed = false;
        while let Ok(each) = answers.try_recv() {
            answer(&mut state, each);
            changed = true;
        }
        for action in view.actions() {
            changed = true;
            if act(&mut state, action, &mut system) == Outcome::Close {
                say("try Ferrix: the installer closes");
                return 0;
            }
        }
        if !changed {
            continue;
        }
        view.show(&state);
        if up && matches!(state.page, Page::Keyboard | Page::You | Page::Zone) {
            let _ = view.focus_field(state.page);
        }
        if let Err(error) = window.refresh(&mut view.ui) {
            say(&format!("err: a frame could not be drawn: {error}"));
            return 1;
        }
    }
    0
}
