//! The window: the [`App`] and the painter joined to a Wayland toplevel
//! through the desktop's own client runtime (`compositor-toolkit`), drawn
//! on the CPU into shared memory -- no GPU is asked for -- and the same
//! frame drawn into a PNG for `--render`.
//!
//! What happens is said on standard output, a line each, which is how a
//! boot that starts the viewer knows it is up.

use std::path::Path;
use std::time::{Duration, Instant};

use compositor_text::tiny_skia::Pixmap;
use compositor_toolkit::{
    Client, CursorShape, Event, KeyboardEvent, PointerEvent, SurfaceId, ToplevelOptions,
};

use crate::app::{Action, App, Page};
use crate::cli::say;
use crate::draw::{Canvas, Rect, Text};

/// The window's `app_id`: its `.desktop` file's `StartupWMClass`.
pub const APP_ID: &str = "rocks.magical.lode-viewer";

/// The left button's evdev code.
const BUTTON_LEFT: u32 = 0x110;

/// How often Live reads the store: `lode -f`'s quarter second is for a
/// terminal; a window redraws less.
const FOLLOW_EVERY: Duration = Duration::from_millis(1000);

/// How long a toast stays.
const TOAST_FOR: Duration = Duration::from_secs(3);

/// Draw one frame of `app` into a new pixmap of `size` logical pixels at
/// `scale`; answers it and the regions it left.
pub fn frame(
    app: &App,
    text: &mut Text,
    size: (u32, u32),
    scale: u32,
) -> Option<(Pixmap, Vec<(Rect, Action)>)> {
    let mut pixmap = Pixmap::new(size.0 * scale, size.1 * scale)?;
    let hits = {
        let mut target = pixmap.as_mut();
        let mut canvas = Canvas::new(&mut target, text, scale as f32);
        crate::ui::paint(app, &mut canvas);
        canvas.hits
    };
    Some((pixmap, hits))
}

/// `--render`: one frame into a PNG. Answers the exit status.
#[must_use]
pub fn render(app: App, out: &Path, size: (u32, u32), scale: u32) -> i32 {
    let mut text = Text::load();
    if text.faces() == 0 {
        say("err: no font was found (LODE_VIEWER_FONTS, /usr/share/fonts)");
    }
    let started = Instant::now();
    let Some((pixmap, _)) = frame(&app, &mut text, size, scale) else {
        say("err: a frame of that size cannot be made");
        return 1;
    };
    let drawn = started.elapsed();
    let png = crate::png::encode(pixmap.width(), pixmap.height(), pixmap.data());
    match std::fs::write(out, png) {
        Ok(()) => {
            say(&format!(
                "drew {}x{} at scale {scale} in {:.1} ms into {}",
                size.0,
                size.1,
                drawn.as_secs_f64() * 1e3,
                out.display()
            ));
            0
        }
        Err(error) => {
            say(&format!("err: {}: {error}", out.display()));
            1
        }
    }
}

/// `--bench`'s frames: each page drawn five times, the first with no
/// cache; answers the times in words.
#[must_use]
pub fn bench_frames(app: &mut App, size: (u32, u32)) -> String {
    let mut text = Text::load();
    let mut out = Vec::new();
    for page in [Page::Logs, Page::Overview] {
        app.page = page;
        let mut times = Vec::new();
        for _ in 0..5 {
            let started = Instant::now();
            let _ = frame(app, &mut text, size, 1);
            times.push(started.elapsed().as_secs_f64() * 1e3);
        }
        let first = times.first().copied().unwrap_or(0.0);
        let rest: f64 = times.iter().skip(1).sum();
        out.push(format!(
            "{page:?} first {first:.0} ms then {:.0} ms",
            rest / 4.0
        ));
    }
    out.join(", ")
}

/// The clipboard, through `ext_data_control_v1`: the record a Copy took
/// is offered as the selection, and written to whoever pastes it.
struct Clipboard {
    /// The manager, bound.
    manager: compositor_toolkit::ObjectId,
    /// The seat's data device.
    device: compositor_toolkit::ObjectId,
    /// The source offered now, and its text.
    offered: Option<(compositor_toolkit::ObjectId, String)>,
}

/// The types a copy is offered as.
const TEXT_TYPES: [&str; 3] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"];

impl Clipboard {
    /// The clipboard, when the compositor offers `ext_data_control`.
    fn bind(client: &mut Client) -> Option<Self> {
        use compositor_toolkit::Value;
        use compositor_toolkit::protocol::ext_data_control::{
            self as control, ext_data_control_manager_v1 as manager,
        };
        let seat = client.seat()?;
        let (bound, _) = client
            .bind(&control::EXT_DATA_CONTROL_MANAGER_V1, 1, None)
            .ok()?;
        let device = client.new_object(&control::EXT_DATA_CONTROL_DEVICE_V1, 1);
        client
            .request(
                bound,
                manager::request::GET_DATA_DEVICE,
                &[Value::NewId(device), Value::Object(seat)],
            )
            .ok()?;
        Some(Clipboard {
            manager: bound,
            device,
            offered: None,
        })
    }

    /// Offer `text` as the selection.
    fn copy(&mut self, client: &mut Client, text: String) -> bool {
        use compositor_toolkit::Value;
        use compositor_toolkit::protocol::ext_data_control::{
            self as control, ext_data_control_device_v1 as device,
            ext_data_control_manager_v1 as manager, ext_data_control_source_v1 as source,
        };
        let made = client.new_object(&control::EXT_DATA_CONTROL_SOURCE_V1, 1);
        let mut sent = client
            .request(
                self.manager,
                manager::request::CREATE_DATA_SOURCE,
                &[Value::NewId(made)],
            )
            .is_ok();
        for mime in TEXT_TYPES {
            sent &= client
                .request(
                    made,
                    source::request::OFFER,
                    &[Value::Str(Some(mime.to_owned()))],
                )
                .is_ok();
        }
        sent &= client
            .request(
                self.device,
                device::request::SET_SELECTION,
                &[Value::Object(made)],
            )
            .is_ok();
        let _ = client.flush();
        if let Some((old, _)) = self.offered.replace((made, text)) {
            let _ = client.request(old, source::request::DESTROY, &[]);
        }
        sent
    }

    /// An event for one of the clipboard's objects: someone pastes, or the
    /// selection went to another program. Answers whether it was ours.
    fn event(
        &mut self,
        client: &mut Client,
        object: compositor_toolkit::ObjectId,
        opcode: u16,
        args: &[compositor_toolkit::Value],
    ) -> bool {
        use compositor_toolkit::Value;
        use compositor_toolkit::protocol::ext_data_control::ext_data_control_source_v1 as source;
        let Some((offered, text)) = &self.offered else {
            return false;
        };
        if object != *offered {
            return false;
        }
        if opcode == source::event::SEND {
            if let Some(Value::Fd(fd)) = args.get(1) {
                write_and_close(*fd, text.as_bytes());
            }
        } else if opcode == source::event::CANCELLED {
            let _ = client.request(*offered, source::request::DESTROY, &[]);
            self.offered = None;
        }
        true
    }
}

/// Write `bytes` to the descriptor a paste handed over, and close it.
fn write_and_close(fd: i32, bytes: &[u8]) {
    use std::io::Write as _;
    use std::os::fd::FromRawFd as _;
    // SAFETY: the descriptor arrived with the compositor's `send` event and
    // nothing else in this process holds it; the file owns and closes it.
    #[allow(unsafe_code)]
    let mut pipe = unsafe { std::fs::File::from_raw_fd(fd) };
    let _ = pipe.write_all(bytes);
}

/// What the loop holds beside the app.
struct Window {
    /// The clipboard, when the compositor has one to offer to.
    clipboard: Option<Clipboard>,
    /// A copy waiting to be offered.
    copied: Option<String>,
    /// The state.
    app: App,
    /// The fonts.
    text: Text,
    /// The surface.
    surface: SurfaceId,
    /// Its size in logical pixels.
    size: Option<(u32, u32)>,
    /// The last frame's regions.
    hits: Vec<(Rect, Action)>,
    /// Where the pointer is.
    pointer: (f32, f32),
    /// Whether the left button is down on the track, to drag the cursor.
    dragging: Option<Action>,
    /// Whether the picture is out of date.
    dirty: bool,
    /// When the toast was put up.
    toast_since: Option<Instant>,
    /// Whether the window should close.
    done: bool,
}

/// Open the window and run until it is closed. Answers the exit status.
#[must_use]
pub fn run(app: App) -> i32 {
    let mut client = match Client::connect() {
        Ok(client) => client,
        Err(error) => {
            say(&format!("err: no compositor to open a window on: {error}"));
            return 1;
        }
    };
    let surface = match client.toplevel(&ToplevelOptions {
        title: "lode".to_owned(),
        app_id: APP_ID.to_owned(),
        size: (1440, 960),
        min_size: (960, 600),
        ..ToplevelOptions::default()
    }) {
        Ok(surface) => surface,
        Err(error) => {
            say(&format!("err: the compositor gave no window: {error}"));
            return 1;
        }
    };
    let text = Text::load();
    if text.faces() == 0 {
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
    let timer = client.add_timer(FOLLOW_EVERY, Some(FOLLOW_EVERY));
    let clipboard = Clipboard::bind(&mut client);
    let mut window = Window {
        clipboard,
        copied: None,
        app,
        text,
        surface,
        size: None,
        hits: Vec::new(),
        pointer: (-1.0, -1.0),
        dragging: None,
        dirty: false,
        toast_since: None,
        done: false,
    };
    let mut drawn = false;
    while !window.done {
        let events = match client.dispatch(None) {
            Ok(events) => events,
            Err(error) => {
                say(&format!("err: the compositor went away: {error}"));
                return 1;
            }
        };
        for event in events {
            match event {
                Event::Timer(id) if id == timer => window.tick(),
                Event::Object {
                    object,
                    opcode,
                    args,
                    ..
                } => {
                    if let Some(clipboard) = &mut window.clipboard {
                        let _ = clipboard.event(&mut client, object, opcode, &args);
                    }
                }
                event => window.handle(&mut client, event),
            }
        }
        if let Some(text) = window.copied.take() {
            let offered = window
                .clipboard
                .as_mut()
                .is_some_and(|clipboard| clipboard.copy(&mut client, text.clone()));
            if !offered {
                window.app.toast = Some("no clipboard here: printed on standard output".to_owned());
                window.toast_since = Some(Instant::now());
                window.dirty = true;
            }
            say(&format!("copied: {text}"));
        }
        if window.dirty && !window.done {
            window.dirty = false;
            let Some(size) = window.size else {
                continue;
            };
            let scale = client.scale(surface);
            let started = Instant::now();
            let (app, text) = (&window.app, &mut window.text);
            let mut hits = Vec::new();
            let result = client.draw(surface, |pixmap| {
                let mut canvas = Canvas::new(pixmap, text, scale.max(1) as f32);
                crate::ui::paint(app, &mut canvas);
                hits = canvas.hits;
            });
            window.hits = hits;
            match result {
                Ok(true) if !drawn => {
                    drawn = true;
                    say(&format!(
                        "the window is up, {}x{} at scale {scale}, {} lines in the table, first frame in {:.0} ms",
                        size.0,
                        size.1,
                        window.app.lines.len(),
                        started.elapsed().as_secs_f64() * 1e3
                    ));
                }
                Ok(_) => {}
                Err(error) => {
                    say(&format!("err: a frame could not be drawn: {error}"));
                    return 1;
                }
            }
        }
    }
    client.destroy(surface);
    let _ = client.flush();
    0
}

impl Window {
    /// The timer: follow the store when Live, and take a stale toast down.
    fn tick(&mut self) {
        if self.app.live {
            let added = self.app.follow();
            if added > 0 {
                say(&format!("live: {added} new records"));
                self.dirty = true;
            }
        }
        if let Some(since) = self.toast_since
            && since.elapsed() >= TOAST_FOR
        {
            self.app.toast = None;
            self.toast_since = None;
            self.dirty = true;
        }
    }

    /// The action under (`x`, `y`) in the last frame.
    fn hit(&self, x: f32, y: f32) -> Option<Action> {
        self.hits
            .iter()
            .rev()
            .find(|(place, _)| place.contains(x, y))
            .map(|(_, action)| action.clone())
    }

    /// Do `action`, pressed at `x`.
    fn act(&mut self, action: &Action, x: f32, y: f32) {
        if let Action::Minimap(top, height) = *action {
            let view = self.view_height();
            self.app.minimap(top, height, y, view);
        } else {
            self.app.act(action, x);
        }
        if let Action::Select(index) = *action
            && self.app.page == Page::Overview
        {
            self.app.page = Page::Logs;
            self.app.refresh();
            self.app.selected = Some(index);
            self.app.scroll_to(index);
        }
        if self.app.toast.is_some() && self.toast_since.is_none() {
            self.toast_since = Some(Instant::now());
        }
        if let Some(text) = self.app.clipboard.take() {
            self.copied = Some(text);
        }
        self.dirty = true;
    }

    /// Say what the query found, as a boot that types one reads it.
    fn say_query(&self) {
        match &self.app.query.error {
            Some(error) => say(&format!(
                "where `{}` does not parse: {error}",
                self.app.query.text
            )),
            None => say(&format!(
                "where `{}`: {} of {} records",
                self.app.query.text.trim(),
                self.app.found.len(),
                self.app.log.of_boot(self.app.boot).len()
            )),
        }
    }

    /// The table's visible height.
    fn view_height(&self) -> f32 {
        let (w, h) = self.size.unwrap_or((1440, 960));
        crate::ui::logs_table(&self.app, w as f32, h as f32).h
    }

    /// One event.
    fn handle(&mut self, client: &mut Client, event: Event) {
        match event {
            Event::Configure {
                surface,
                width,
                height,
            } if surface == self.surface => {
                let size = (width.max(1), height.max(1));
                self.size = Some(size);
                self.dirty = true;
            }
            Event::Scale { surface, .. } if surface == self.surface => self.dirty = true,
            Event::CloseRequested(surface) | Event::Closed(surface) if surface == self.surface => {
                self.done = true;
            }
            Event::Keyboard(KeyboardEvent::Key(key)) if key.pressed => {
                let keysym = key.keysym.unwrap_or_default();
                if key.modifiers.control && matches!(keysym, "q" | "w") {
                    self.done = true;
                    return;
                }
                if self.app.editing {
                    self.dirty |= self.app.type_key(keysym, &key.text);
                    if !self.app.editing && matches!(keysym, "Return" | "KP_Enter") {
                        self.say_query();
                    }
                    return;
                }
                self.key(keysym);
            }
            Event::Pointer(pointer) => self.point(client, pointer),
            _ => {}
        }
    }

    /// A key, while nothing is typed into.
    fn key(&mut self, keysym: &str) {
        let view = self.view_height();
        match keysym {
            "slash" => self.act(&Action::EditQuery, 0.0, 0.0),
            "Escape" => {
                self.app.selected = None;
                self.app.boot_menu = false;
            }
            "Down" | "j" => self.move_selection(1),
            "Up" | "k" => self.move_selection(-1),
            "Next" | "Page_Down" => self.scroll(view * 0.9),
            "Prior" | "Page_Up" => self.scroll(-view * 0.9),
            "Home" => self.app.scroll = 0.0,
            "End" => self.scroll(f32::MAX / 4.0),
            "Left" => self.app.act(&Action::Step(-10), 0.0),
            "Right" => self.app.act(&Action::Step(10), 0.0),
            "o" => self.app.page = Page::Overview,
            "l" => self.app.page = Page::Logs,
            "t" => {
                self.app.mode = match self.app.mode {
                    crate::theme::Mode::Light => crate::theme::Mode::Dark,
                    crate::theme::Mode::Dark => crate::theme::Mode::Light,
                };
            }
            _ => return,
        }
        self.dirty = true;
    }

    /// Select the record `by` records on from the selected one.
    fn move_selection(&mut self, by: isize) {
        let records: Vec<usize> = self
            .app
            .lines
            .iter()
            .filter_map(|line| match line {
                crate::lines::Line::Record(index) => Some(*index),
                _ => None,
            })
            .collect();
        if records.is_empty() {
            return;
        }
        let at = self
            .app
            .selected
            .and_then(|selected| records.iter().position(|&index| index == selected))
            .map_or(0, |at| at.saturating_add_signed(by).min(records.len() - 1));
        let Some(&chosen) = records.get(at) else {
            return;
        };
        self.app.page = Page::Logs;
        self.app.selected = Some(chosen);
        // Keep it in view.
        if let Some(top) = self
            .app
            .lines
            .iter()
            .zip(&self.app.tops)
            .find(|(line, _)| **line == crate::lines::Line::Record(chosen))
            .map(|(_, top)| *top)
        {
            let view = self.view_height();
            if top < self.app.scroll {
                self.app.scroll = top;
            } else if top + 28.0 > self.app.scroll + view {
                self.app.scroll = top + 28.0 - view;
            }
        }
    }

    /// Scroll the page under the pointer by `by` pixels.
    fn scroll(&mut self, by: f32) {
        let (w, h) = self.size.unwrap_or((1440, 960));
        match self.app.page {
            Page::Logs => {
                let view = crate::ui::logs_table(&self.app, w as f32, h as f32).h;
                let most = (self.app.table_height - view).max(0.0);
                self.app.scroll = (self.app.scroll + by).clamp(0.0, most);
            }
            Page::Overview => {
                let most = (crate::ui::overview_height() - (h as f32 - crate::ui::HEADER)).max(0.0);
                self.app.overview_scroll = (self.app.overview_scroll + by).clamp(0.0, most);
            }
        }
    }

    /// The pointer moved, pressed or scrolled.
    fn point(&mut self, client: &mut Client, pointer: PointerEvent) {
        match pointer {
            PointerEvent::Enter { surface, x, y } if surface == self.surface => {
                client.set_cursor(CursorShape::Default);
                self.pointer = (x as f32, y as f32);
            }
            PointerEvent::Motion { surface, x, y } if surface == self.surface => {
                self.pointer = (x as f32, y as f32);
                if let Some(action) = self.dragging.clone() {
                    self.act(&action, x as f32, y as f32);
                }
            }
            PointerEvent::Button {
                surface,
                x,
                y,
                button: BUTTON_LEFT,
                pressed,
                ..
            } if surface == self.surface => {
                let (x, y) = (x as f32, y as f32);
                if !pressed {
                    self.dragging = None;
                    return;
                }
                let Some(action) = self.hit(x, y) else {
                    if self.app.editing {
                        self.app.commit_draft();
                        self.dirty = true;
                    }
                    return;
                };
                if matches!(action, Action::Track(..) | Action::Minimap(..)) {
                    self.dragging = Some(action.clone());
                }
                self.act(&action, x, y);
            }
            PointerEvent::Axis {
                surface,
                vertical,
                discrete,
                ..
            } if surface == self.surface => {
                // Over the track the wheel zooms, as the design's does;
                // anywhere else it scrolls.
                if let Some(Action::Track(..)) = self.hit(self.pointer.0, self.pointer.1) {
                    let zooms: [u64; 4] = [30, 60, 300, 0];
                    let now = zooms
                        .iter()
                        .position(|&z| {
                            z * 1_000_000 == self.app.span || (z == 0 && self.app.whole_boot())
                        })
                        .unwrap_or(1);
                    let next = if vertical > 0.0 {
                        (now + 1).min(3)
                    } else {
                        now.saturating_sub(1)
                    };
                    let zoom = zooms.get(next).copied().unwrap_or(0);
                    self.app.act(&Action::Zoom(zoom), 0.0);
                } else {
                    let rows = if discrete.0 != 0 {
                        discrete.0 as f32 * 3.0 * 28.0
                    } else {
                        vertical as f32 * 2.0
                    };
                    self.scroll(rows);
                }
                self.dirty = true;
            }
            _ => {}
        }
    }
}
