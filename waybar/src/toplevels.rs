//! The compositor's window list, through
//! `zwlr_foreign_toplevel_management_v1`: what `wlr/taskbar` draws.
//!
//! The bar binds the manager on its own connection, as waybar does, and is
//! given a handle for every window, with its title, application id and
//! state, and told again whenever one changes. A click on a taskbar button
//! is a request back on that handle. This is the protocol's half: the
//! module ([`crate::modules::taskbar`]) is handed the list and asks for the
//! requests through its host, so it is tested without a compositor.

use compositor_toolkit::protocol::foreign_toplevel::{
    ZWLR_FOREIGN_TOPLEVEL_HANDLE_V1, ZWLR_FOREIGN_TOPLEVEL_MANAGER_V1,
    zwlr_foreign_toplevel_handle_v1 as handle, zwlr_foreign_toplevel_manager_v1 as manager,
};
use compositor_toolkit::{Client, ObjectId, Value};

/// One window, as the compositor last described it whole.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Toplevel {
    /// Its handle, which a request names.
    pub id: u32,
    /// Its application id.
    pub app_id: String,
    /// Its title.
    pub title: String,
    /// Whether it is the focused window.
    pub active: bool,
    /// Whether it is minimized.
    pub minimized: bool,
    /// Whether it is maximized.
    pub maximized: bool,
    /// Whether it is fullscreen.
    pub fullscreen: bool,
}

/// What a click asks of a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// `activate`: focus it, bringing it back if it is minimized.
    Activate,
    /// `close`: ask it to close.
    Close,
    /// `set_minimized` or `unset_minimized`.
    Minimize(bool),
    /// `set_maximized` or `unset_maximized`.
    Maximize(bool),
    /// `set_fullscreen` or `unset_fullscreen`.
    Fullscreen(bool),
}

/// The version of the protocol asked for: 3 has everything a taskbar uses.
const VERSION: u32 = 3;

/// The manager and every window it has told of.
#[derive(Debug)]
pub struct Toplevels {
    manager: ObjectId,
    /// Every handle, in the order the compositor made them, with what has
    /// arrived for it since its last `done` and what that `done` made
    /// whole.
    windows: Vec<(ObjectId, Toplevel, Option<Toplevel>)>,
}

impl Toplevels {
    /// Bind the manager, if the compositor has one.
    pub fn bind(client: &mut Client) -> Option<Self> {
        let (manager, version) = client
            .bind(&ZWLR_FOREIGN_TOPLEVEL_MANAGER_V1, VERSION, None)
            .ok()?;
        // Each handle is a new id in the manager's `toplevel`, and its own
        // events follow it in the same read.
        client.adopt_new_ids(
            manager,
            manager::event::TOPLEVEL,
            &ZWLR_FOREIGN_TOPLEVEL_HANDLE_V1,
            version,
        );
        let _ = client.flush();
        Some(Self {
            manager,
            windows: Vec::new(),
        })
    }

    /// The windows the compositor has described whole, in the order it
    /// made them.
    #[must_use]
    pub fn list(&self) -> Vec<Toplevel> {
        self.windows
            .iter()
            .filter_map(|(_, _, done)| done.clone())
            .collect()
    }

    /// Take one of the program's own objects' events. Gives whether the
    /// list changed.
    pub fn event(
        &mut self,
        client: &mut Client,
        object: ObjectId,
        opcode: u16,
        args: &[Value],
    ) -> bool {
        if object == self.manager {
            if opcode == manager::event::TOPLEVEL
                && let Some(Value::Object(made) | Value::NewId(made)) = args.first()
            {
                let fresh = Toplevel {
                    id: made.0,
                    ..Toplevel::default()
                };
                self.windows.push((*made, fresh, None));
            }
            return false;
        }
        let Some(at) = self.windows.iter().position(|(held, _, _)| *held == object) else {
            return false;
        };
        let Some((_, pending, done)) = self.windows.get_mut(at) else {
            return false;
        };
        match opcode {
            handle::event::TITLE => {
                if let Some(Value::Str(Some(title))) = args.first() {
                    pending.title.clone_from(title);
                }
                false
            }
            handle::event::APP_ID => {
                if let Some(Value::Str(Some(app_id))) = args.first() {
                    pending.app_id.clone_from(app_id);
                }
                false
            }
            handle::event::STATE => {
                if let Some(Value::Array(bytes)) = args.first() {
                    let states: Vec<u32> = bytes
                        .chunks_exact(4)
                        .filter_map(|word| word.try_into().ok().map(u32::from_ne_bytes))
                        .collect();
                    pending.active = states.contains(&handle::state::ACTIVATED);
                    pending.minimized = states.contains(&handle::state::MINIMIZED);
                    pending.maximized = states.contains(&handle::state::MAXIMIZED);
                    pending.fullscreen = states.contains(&handle::state::FULLSCREEN);
                }
                false
            }
            handle::event::DONE => {
                let changed = done.as_ref() != Some(pending);
                *done = Some(pending.clone());
                changed
            }
            handle::event::CLOSED => {
                let was = self.windows.remove(at);
                // The handle is the client's to destroy once it is closed.
                let _ = client.request(object, handle::request::DESTROY, &[]);
                was.2.is_some()
            }
            _ => false,
        }
    }

    /// Send `request` about window `id`.
    pub fn request(&self, client: &mut Client, id: u32, request: Request) {
        let Some((object, _, _)) = self.windows.iter().find(|(held, _, _)| held.0 == id) else {
            return;
        };
        let object = *object;
        let sent = match request {
            Request::Activate => {
                let Some(seat) = client.seat() else {
                    return;
                };
                client.request(object, handle::request::ACTIVATE, &[Value::Object(seat)])
            }
            Request::Close => client.request(object, handle::request::CLOSE, &[]),
            Request::Minimize(on) => client.request(
                object,
                if on {
                    handle::request::SET_MINIMIZED
                } else {
                    handle::request::UNSET_MINIMIZED
                },
                &[],
            ),
            Request::Maximize(on) => client.request(
                object,
                if on {
                    handle::request::SET_MAXIMIZED
                } else {
                    handle::request::UNSET_MAXIMIZED
                },
                &[],
            ),
            Request::Fullscreen(on) => {
                if on {
                    // The output it should go fullscreen on: none, the
                    // compositor's choice.
                    client.request(
                        object,
                        handle::request::SET_FULLSCREEN,
                        &[Value::Object(ObjectId::NULL)],
                    )
                } else {
                    client.request(object, handle::request::UNSET_FULLSCREEN, &[])
                }
            }
        };
        if sent.is_ok() {
            let _ = client.flush();
        }
    }
}
