//! The window: the model and the painter joined to a Wayland toplevel.
//!
//! One loop. The compositor's events become calls on the [`Model`]; a switch
//! that turns is written to the user's file at once ([`crate::store`]) and
//! shown only once it has been; then, if anything changed, one frame is
//! drawn. What happens is said on standard output, a line each, which is
//! how the boot that drives this window knows what it did.

use std::path::PathBuf;

use compositor_desktop::game_mode::{self, GameMode};
use compositor_text::Fonts;
use compositor_toolkit::{
    Client, CursorShape, Event, Key, KeyboardEvent, PointerEvent, SurfaceId, ToplevelOptions,
};

use crate::layout::{self, Geometry, Hit};
use crate::model::{Change, Model, Page};
use crate::paint::Painter;

/// The window's `app_id`: its `.desktop` file's `StartupWMClass`.
pub const APP_ID: &str = "rocks.magical.settings";

/// The left button's evdev code.
const BUTTON_LEFT: u32 = 0x110;

/// How a window is opened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// The page it opens on.
    pub page: Page,
}

/// Say one line of what the program did.
pub(crate) fn say(text: &str) {
    println!("settings: {text}");
}

/// The environment, as the libraries ask for it.
pub(crate) fn var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Everything the loop holds.
struct App {
    /// What is shown.
    model: Model,
    /// What draws it.
    painter: Painter,
    /// Where things are, once the compositor has given a size.
    geometry: Option<Geometry>,
    /// The user's game-mode file, if there is a user to have one.
    file: Option<PathBuf>,
    /// Where the pointer is.
    pointer: (f32, f32),
    /// Whether the picture is out of date.
    dirty: bool,
    /// Whether the window should close.
    done: bool,
}

/// Open the window and run until it is closed. Answers the exit status.
#[must_use]
pub fn run(options: &Options) -> i32 {
    let apps = crate::apps::installed(&var);
    let file = game_mode::path(&var);
    let games = match &file {
        Some(path) => match GameMode::load(path) {
            Ok((games, complaints)) => {
                for complaint in complaints {
                    say(&format!("warn: {}: {complaint}", path.display()));
                }
                games
            }
            Err(error) => {
                say(&format!("err: {}: {error}", path.display()));
                GameMode::default()
            }
        },
        None => {
            say(
                "warn: neither XDG_CONFIG_HOME nor HOME is set, so there is no file to keep the switches in",
            );
            GameMode::default()
        }
    };
    let mut client = match Client::connect() {
        Ok(client) => client,
        Err(error) => {
            say(&format!("err: no compositor to open a window on: {error}"));
            return 1;
        }
    };
    let window = match client.toplevel(&ToplevelOptions {
        title: "Settings".to_owned(),
        app_id: APP_ID.to_owned(),
        size: layout::PREFERRED,
        min_size: (480, 360),
        ..ToplevelOptions::default()
    }) {
        Ok(window) => window,
        Err(error) => {
            say(&format!("err: the compositor gave no window: {error}"));
            return 1;
        }
    };
    let painter = Painter::new(Fonts::system(), &apps, &var);
    if painter.faces() == 0 {
        say(
            "err: no font was found, so the window has no words (FERRIX_FONT_DIRS, /usr/share/fonts, /usr/share/ferrix/fonts)",
        );
    }
    let mut model = Model::new(apps, games);
    model.page = options.page;
    say(&format!(
        "{} applications listed with {} icons, {} in game mode ({})",
        model.apps.len(),
        painter.icons_found(),
        model.games_installed(),
        file.as_ref()
            .map_or_else(|| "no file".to_owned(), |path| path.display().to_string()),
    ));
    let mut app = App {
        model,
        painter,
        geometry: None,
        file,
        pointer: (-1.0, -1.0),
        dirty: false,
        done: false,
    };
    let mut drawn = false;
    while !app.done {
        let events = match client.dispatch(None) {
            Ok(events) => events,
            Err(error) => {
                say(&format!("err: the compositor went away: {error}"));
                return 1;
            }
        };
        for event in events {
            app.handle(&mut client, window, event);
        }
        if app.dirty && !app.done {
            app.dirty = false;
            match app.draw(&mut client, window) {
                Ok(true) if !drawn => {
                    drawn = true;
                    let (width, height) = client.size(window).unwrap_or_default();
                    say(&format!(
                        "the window is up, {width}x{height} at scale {}, {} rows in view",
                        client.scale(window),
                        app.geometry.map_or(0, |geometry| geometry.visible_rows()),
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
    client.destroy(window);
    let _ = client.flush();
    0
}

impl App {
    /// How many rows are in view.
    fn visible(&self) -> usize {
        self.geometry.map_or(0, |geometry| geometry.visible_rows())
    }

    /// Draw one frame.
    fn draw(
        &mut self,
        client: &mut Client,
        window: SurfaceId,
    ) -> Result<bool, compositor_toolkit::Error> {
        let Some(geometry) = self.geometry else {
            return Ok(false);
        };
        let scale = client.scale(window);
        let (painter, model) = (&mut self.painter, &self.model);
        client.draw(window, |pixmap| {
            painter.draw(pixmap, model, &geometry, scale);
        })
    }

    /// One event.
    fn handle(&mut self, client: &mut Client, window: SurfaceId, event: Event) {
        match event {
            Event::Configure {
                surface,
                width,
                height,
            } if surface == window => {
                self.geometry = Some(self.painter.geometry(width, height));
                let visible = self.visible();
                self.model.reveal(visible);
                self.dirty = true;
            }
            Event::Scale { surface, .. } if surface == window => self.dirty = true,
            Event::CloseRequested(surface) | Event::Closed(surface) if surface == window => {
                self.done = true;
            }
            Event::Keyboard(KeyboardEvent::Key(key)) if key.pressed => self.key(&key),
            Event::Pointer(pointer) => self.point(client, window, pointer),
            _ => {}
        }
    }

    /// A key went down, or repeated.
    fn key(&mut self, key: &Key) {
        let visible = self.visible();
        let page = isize::try_from(visible.max(1)).unwrap_or(1);
        let held = key.modifiers.control || key.modifiers.alt || key.modifiers.logo;
        let before = (self.model.selected, self.model.scroll);
        match key.keysym.unwrap_or_default() {
            "q" | "w" if key.modifiers.control => {
                self.done = true;
                return;
            }
            "Up" | "KP_Up" => self.model.move_by(-1, visible),
            "Down" | "KP_Down" => self.model.move_by(1, visible),
            "Page_Up" | "KP_Page_Up" | "Prior" | "KP_Prior" => self.model.move_by(-page, visible),
            "Page_Down" | "KP_Page_Down" | "Next" | "KP_Next" => self.model.move_by(page, visible),
            "Home" | "KP_Home" => self.model.move_to_end(false, visible),
            "End" | "KP_End" => self.model.move_to_end(true, visible),
            "Return" | "KP_Enter" => {
                if !key.repeat {
                    let change = self.model.toggle_selected();
                    self.turn(change);
                }
                return;
            }
            // A switch turns on the space bar wherever there is one; here
            // only while nothing is typed, since a name may have a space.
            "space" if self.model.filter.is_empty() && !held => {
                if !key.repeat {
                    let change = self.model.toggle_selected();
                    self.turn(change);
                }
                return;
            }
            "BackSpace" => {
                self.dirty |= self.model.backspace();
                self.model.reveal(visible);
                return;
            }
            "Escape" => {
                self.dirty |= self.model.clear_filter();
                self.model.reveal(visible);
                return;
            }
            _ if !held && !key.text.is_empty() => {
                self.model.type_text(&key.text);
                self.model.reveal(visible);
                self.dirty = true;
                return;
            }
            _ => return,
        }
        self.dirty |= before != (self.model.selected, self.model.scroll);
    }

    /// The pointer moved, pressed or scrolled.
    fn point(&mut self, client: &mut Client, window: SurfaceId, pointer: PointerEvent) {
        let Some(geometry) = self.geometry else {
            return;
        };
        let visible = geometry.visible_rows();
        match pointer {
            PointerEvent::Enter { surface, x, y } if surface == window => {
                // A client says what the pointer looks like over it.
                client.set_cursor(CursorShape::Default);
                self.pointer = (x as f32, y as f32);
                self.hover(&geometry);
            }
            PointerEvent::Motion { surface, x, y } if surface == window => {
                self.pointer = (x as f32, y as f32);
                self.hover(&geometry);
            }
            PointerEvent::Leave { surface } if surface == window => {
                self.pointer = (-1.0, -1.0);
                self.hover(&geometry);
            }
            PointerEvent::Button {
                surface,
                x,
                y,
                button: BUTTON_LEFT,
                pressed: true,
                ..
            } if surface == window => {
                self.pointer = (x as f32, y as f32);
                let shown = self.model.shown_len();
                match geometry.hit(x as f32, y as f32, self.model.scroll, shown) {
                    Hit::Page(page) => {
                        self.dirty |= self.model.page != page;
                        self.model.page = page;
                    }
                    Hit::Row(row) => {
                        self.dirty |= self.model.selected != row;
                        self.model.select(row);
                    }
                    Hit::Switch(row) => {
                        self.model.select(row);
                        let change = self.model.toggle(row);
                        self.turn(change);
                    }
                    Hit::Nothing => {}
                }
            }
            PointerEvent::Axis {
                surface,
                vertical,
                discrete,
                ..
            } if surface == window => {
                // A wheel's click is a row; a touchpad's distance is rows of
                // the height a row has.
                let rows = if discrete.0 != 0 {
                    isize::try_from(discrete.0).unwrap_or(0)
                } else {
                    (vertical / f64::from(layout::ROW)).round() as isize
                };
                let before = self.model.scroll;
                self.model.scroll_by(rows, visible);
                if before != self.model.scroll {
                    self.dirty = true;
                    self.hover(&geometry);
                }
            }
            _ => {}
        }
    }

    /// Work out which row the pointer is over.
    fn hover(&mut self, geometry: &Geometry) {
        let (x, y) = self.pointer;
        let hovered = match geometry.hit(x, y, self.model.scroll, self.model.shown_len()) {
            Hit::Row(row) | Hit::Switch(row) => Some(row),
            Hit::Page(_) | Hit::Nothing => None,
        };
        if hovered != self.model.hovered {
            self.model.hovered = hovered;
            self.dirty = true;
        }
    }

    /// The user turned a switch: write it, and show it once it is written.
    fn turn(&mut self, change: Option<Change>) {
        let Some(change) = change else {
            return;
        };
        self.dirty = true;
        let Some(path) = &self.file else {
            self.model.refused(
                "there is no home directory to keep the file in (HOME is not set)".to_owned(),
            );
            say("err: a switch was turned and there is no file to write it to");
            return;
        };
        match crate::store::switch(path, &change.id, change.on) {
            Ok(games) => {
                say(&format!(
                    "game mode {} for {}; wrote {}",
                    if change.on { "on" } else { "off" },
                    change.id,
                    path.display()
                ));
                self.model.written(&change, games);
            }
            Err(why) => {
                say(&format!("err: {why}"));
                self.model.refused(why);
            }
        }
    }
}
