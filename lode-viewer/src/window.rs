//! The window: the tree of [`crate::ui`] in a Wayland toplevel through
//! tole's window (Ferrix's own client runtime, `compositor-toolkit`),
//! drawn on the CPU, only what changed. The loop follows the store when
//! Live, keeps the selections (the clipboard and the primary selection,
//! which GNOME offers through `wl_data_device`), and does what the tree's
//! presses and the keys nobody typed into ask of the [`App`].
//!
//! What happens is said on standard output, a line each, which is how a
//! boot that starts the viewer knows it is up.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use compositor_toolkit::{Event, KeyboardEvent, PointerEvent};
use tole::Element as _;
use tole::window::{Window, WindowOptions};

use crate::app::{Action, App, Page};
use crate::cli::say;
use crate::selection::{Selections, Which};
use crate::ui::{View, fonts};

/// The window's `app_id`: its `.desktop` file's `StartupWMClass`.
pub const APP_ID: &str = "rocks.magical.lode-viewer";

/// The middle button's evdev code: it pastes the primary selection.
const BUTTON_MIDDLE: u32 = 0x112;

/// How often Live reads the store: `lode -f`'s quarter second is for a
/// terminal; a window redraws less.
const FOLLOW_EVERY: Duration = Duration::from_millis(1000);

/// How long a toast stays.
const TOAST_FOR: Duration = Duration::from_secs(3);

/// Write `bytes` to the descriptor a paste handed over, and close it.
pub(crate) fn write_and_close(fd: i32, bytes: &[u8]) {
    use std::io::Write as _;
    use std::os::fd::FromRawFd as _;
    // SAFETY: the descriptor arrived with the compositor's `send` event and
    // nothing else in this process holds it; the file owns and closes it.
    #[allow(unsafe_code)]
    let mut pipe = unsafe { std::fs::File::from_raw_fd(fd) };
    let _ = pipe.write_all(bytes);
}

/// What the loop holds beside the tree.
struct Loop {
    /// The tree and the state.
    view: View,
    /// The state, shared with the tree.
    app: Rc<RefCell<App>>,
    /// The clipboard and the primary selection.
    selections: Selections,
    /// When the toast was put up.
    toast_since: Option<Instant>,
    /// Whether the window should close.
    done: bool,
    /// Timer turns so far.
    ticks: u32,
    /// Whether the state changed since it was last shown.
    dirty: bool,
    /// Whether a field's paste is on its way from the clipboard.
    pasting: bool,
    /// Whether the table is to follow its newest rows once the state is
    /// shown: End, or Live.
    follow_next: bool,
}

/// Open the window and run until it is closed. Answers the exit status.
#[must_use]
pub fn run(app: App) -> i32 {
    let fonts = fonts();
    if fonts.faces().is_empty() {
        say(
            "err: no font was found, so the window has no words (LODE_VIEWER_FONTS, /usr/share/fonts)",
        );
    }
    say(&format!(
        "{} records in {} boots, {} sources",
        app.log.entries.len(),
        app.log.boots.len(),
        app.log.sources.len()
    ));
    let background = crate::ui::paint::ink(crate::theme::Theme::of(app.mode).bg);
    let app = Rc::new(RefCell::new(app));
    let Some(mut view) = View::new(Rc::clone(&app), fonts) else {
        say("err: the window's tree could not be made");
        return 1;
    };
    let options = WindowOptions {
        title: "lode".to_owned(),
        app_id: APP_ID.to_owned(),
        size: (1440, 960),
        background,
        min_size: (960, 600),
        resize_edges: true,
        clipboard: false,
    };
    let started = Instant::now();
    let mut window = match Window::open(view.shell.common(), options) {
        Ok(window) => window,
        Err(error) => {
            say(&format!("err: no compositor to open a window on: {error}"));
            if std::env::var_os("WAYLAND_DISPLAY").is_none()
                && std::env::var_os("DISPLAY").is_some()
            {
                say(
                    "err: this is an X11 session, and lode-viewer speaks Wayland only: log in to a \
                     Wayland session (GNOME, Plasma, Hyprland, sway), or draw a frame with --render",
                );
            }
            return 1;
        }
    };
    let timer = window.client().add_timer(FOLLOW_EVERY, Some(FOLLOW_EVERY));
    let waker = window.client().waker().ok();
    let selections = Selections::bind(window.client(), waker.clone());
    view.ui.set_clipboard_external(selections.has_clipboard());
    app.borrow_mut().waker = waker;
    let mut state = Loop {
        view,
        app,
        selections,
        toast_since: None,
        done: false,
        ticks: 0,
        dirty: false,
        pasting: false,
        follow_next: false,
    };
    let mut up = false;
    while !state.done && !window.is_closed() {
        let events = match window.dispatch(&mut state.view.ui, None) {
            Ok(events) => events,
            Err(error) => {
                say(&format!("err: the compositor went away: {error}"));
                return 1;
            }
        };
        if !up && let Some(size) = window.size() {
            up = true;
            let lines = state.app.borrow().lines.len();
            say(&format!(
                "the window is up, {}x{}, {lines} lines in the table, first frame in {:.0} ms",
                size.0,
                size.1,
                started.elapsed().as_secs_f64() * 1e3
            ));
        }
        for event in events {
            match event {
                Event::Timer(id) if id == timer => state.tick(),
                Event::Object {
                    object,
                    interface,
                    opcode,
                    args,
                } => {
                    let _ =
                        state
                            .selections
                            .event(window.client(), object, interface, opcode, &args);
                }
                Event::Keyboard(KeyboardEvent::Key(key)) if key.pressed => {
                    // A key that came back in the batch after `/` gave the
                    // query the keys was given before the query had them:
                    // it is the query's now.
                    let event = tole::KeyEvent {
                        key: std::rc::Rc::from(key.keysym.unwrap_or("")),
                        text: std::rc::Rc::from(key.text.as_str()),
                        shift: key.modifiers.shift,
                        control: key.modifiers.control,
                        alt: key.modifiers.alt,
                        repeat: key.repeat,
                    };
                    if state.view.typing() && state.view.ui.key_press(&event) {
                        state.dirty = true;
                        continue;
                    }
                    state.key(key.keysym.unwrap_or_default(), key.modifiers.control);
                }
                Event::Pointer(PointerEvent::Button {
                    button: BUTTON_MIDDLE,
                    pressed: true,
                    ..
                }) => state.middle(window.client()),
                Event::CloseRequested(_) | Event::Closed(_) => state.done = true,
                _ => {}
            }
        }
        let (done, asked) = state.view.step();
        for action in asked {
            match action {
                Action::WindowMove => window.start_move(),
                Action::WindowMaximize => window.toggle_maximized(),
                Action::WindowMinimize => window.minimize(),
                _ => state.done = true,
            }
        }
        for action in &done {
            state.after(action);
        }
        let serial = window.key_serial();
        state.selection_work(window.client(), serial);
        if state.dirty && !state.done {
            state.dirty = false;
            state.view.show();
            // After the state's own scroll is shown, so that does not
            // count as scrolling away from the end.
            if core::mem::take(&mut state.follow_next) {
                let ui = &mut state.view.ui;
                state.view.logs.follow.set(ui, true);
            }
            if let Err(error) = window.refresh(&mut state.view.ui) {
                say(&format!("err: a frame could not be drawn: {error}"));
                return 1;
            }
        }
    }
    0
}

impl Loop {
    /// Do `action`, from a key.
    fn act(&mut self, action: &Action) {
        self.app.borrow_mut().act(action);
        self.after(action);
    }

    /// What follows an action done: the toast's clock, and the query said.
    fn after(&mut self, action: &Action) {
        if self.app.borrow().toast.is_some() && self.toast_since.is_none() {
            self.toast_since = Some(Instant::now());
        }
        if matches!(action, Action::QueryAccept) {
            self.say_query();
        }
        if matches!(action, Action::Live) {
            self.follow_next = true;
        }
        self.dirty = true;
    }

    /// Say what the query found, as a boot that types one reads it.
    fn say_query(&self) {
        let app = self.app.borrow();
        match &app.query.error {
            Some(error) => say(&format!(
                "where `{}` does not parse: {error}",
                app.query.text
            )),
            None => say(&format!(
                "where `{}`: {} of {} records",
                app.query.text.trim(),
                app.found.len(),
                app.log.of_boot(app.boot).len()
            )),
        }
    }

    /// The timer: follow the store when Live, and take a stale toast down.
    fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        let mut app = self.app.borrow_mut();
        // The links' states change on their own threads.
        if app.page == Page::Machines {
            self.dirty = true;
        }
        if app.poll_machines(self.ticks) {
            self.dirty = true;
        }
        if self.ticks.is_multiple_of(10)
            && let Some(root) = app.machines_root.clone()
            && app.rescan_machines(&root)
        {
            say(&format!("{} machines now", app.machines.len()));
            // A viewer with nothing to show shows the machine that came.
            if app.log.entries.is_empty() {
                let index = app.machine;
                app.open_machine(index);
                app.live = true;
            }
            self.dirty = true;
        }
        if app.live {
            let added = app.follow();
            if added > 0 {
                say(&format!("live: {added} new records"));
                self.dirty = true;
            }
        }
        // A toast with a button stays twice as long.
        let lasts = if app.toast_action.is_some() {
            TOAST_FOR * 2
        } else {
            TOAST_FOR
        };
        if let Some(since) = self.toast_since
            && since.elapsed() >= lasts
        {
            app.toast = None;
            app.toast_action = None;
            self.toast_since = None;
            self.dirty = true;
        }
    }

    /// A key nobody typed into took.
    fn key(&mut self, keysym: &str, control: bool) {
        if control && matches!(keysym, "q" | "w") {
            self.done = true;
            return;
        }
        let view = self.app.borrow().table_view;
        match keysym {
            "slash" => {
                self.act(&Action::Page(Page::Logs));
                self.act(&Action::EditQuery);
                self.view.focus_query();
            }
            "Escape" => {
                let mut app = self.app.borrow_mut();
                app.selected = None;
                app.boot_menu = false;
                app.machine_menu = false;
            }
            "Down" | "j" => self.app.borrow_mut().move_selection(1),
            "Up" | "k" => self.app.borrow_mut().move_selection(-1),
            "Next" | "Page_Down" => self.app.borrow_mut().scroll_by(view * 0.9),
            "Prior" | "Page_Up" => self.app.borrow_mut().scroll_by(-view * 0.9),
            "Home" => self.app.borrow_mut().scroll = 0.0,
            "End" => {
                self.app.borrow_mut().scroll_by(f32::MAX / 4.0);
                self.follow_next = true;
                self.dirty = true;
            }
            "Left" => self.app.borrow_mut().act(&Action::Step(-10)),
            "Right" => self.app.borrow_mut().act(&Action::Step(10)),
            "F2" => {
                let in_use = self.app.borrow().saved_in_use;
                if let Some(index) = in_use {
                    self.act(&Action::SavedRename(index));
                }
            }
            "o" => self.app.borrow_mut().page = Page::Overview,
            "l" => self.app.borrow_mut().page = Page::Logs,
            "t" => {
                let mut app = self.app.borrow_mut();
                app.mode = match app.mode {
                    crate::theme::Mode::Light => crate::theme::Mode::Dark,
                    crate::theme::Mode::Dark => crate::theme::Mode::Light,
                };
            }
            _ => return,
        }
        self.dirty = true;
    }

    /// The middle button: the primary selection, pasted into the field
    /// under the pointer, which the press gave the keys to.
    fn middle(&mut self, client: &mut compositor_toolkit::Client) {
        if self.view.ui.active_focus_item().is_some() && self.view.typing() {
            let _ = self.selections.paste(client, Which::Primary);
        }
    }

    /// Copies and pastes: the fields' own (tole's), a record copied as
    /// JSON or its cursor, and what arrived.
    fn selection_work(&mut self, client: &mut compositor_toolkit::Client, serial: u32) {
        if let Some(text) = self.view.ui.take_copied() {
            let length = text.chars().count();
            if self.selections.copy(client, text.to_string(), serial) {
                say(&format!("copied {length} characters from a field"));
            }
        }
        if !self.pasting && self.view.ui.take_paste_request() {
            self.pasting = self.selections.paste(client, Which::Clipboard);
            if !self.pasting {
                // Nothing to paste: the field stops waiting.
                self.view.ui.paste("");
                self.app.borrow_mut().toast =
                    Some("nothing to paste: the clipboard has no text".to_owned());
                self.toast_since = Some(Instant::now());
                say("a paste was asked for, and the clipboard has no text");
                self.dirty = true;
            }
        }
        let wanted = self.app.borrow_mut().want_paste.take();
        if let Some(which) = wanted
            && !self.selections.paste(client, which)
            && which == Which::Clipboard
        {
            self.app.borrow_mut().toast =
                Some("nothing to paste: the clipboard has no text".to_owned());
            self.toast_since = Some(Instant::now());
            self.dirty = true;
        }
        while let Some(text) = self.selections.take() {
            // How much, never what: it may be a private key.
            say(&format!("pasted {} characters", text.chars().count()));
            self.view.ui.paste(&text);
            self.pasting = false;
            self.dirty = true;
        }
        let copied = self.app.borrow_mut().clipboard.take();
        if let Some(text) = copied {
            if !self.selections.copy(client, text.clone(), serial) {
                self.app.borrow_mut().toast =
                    Some("no clipboard here: printed on standard output".to_owned());
                self.toast_since = Some(Instant::now());
            }
            say(&format!("copied: {text}"));
            self.dirty = true;
        }
    }
}
