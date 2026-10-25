//! Files dropped on the dock: `wl_data_device` as a drop target, through
//! the toolkit's objects of a program's own. The compositor offers what a
//! drag carries; the dock takes `text/uri-list` (a file manager's or a
//! browser's files), says it would copy it, follows the pointer over its
//! surface, and on the drop reads the list and hands it on. Where the
//! pointer was decides what the drop is for -- the dock asks its tree
//! whether that was the Stash button.

use std::io::Read as _;
use std::os::fd::AsRawFd as _;
use std::time::Duration;

use compositor_toolkit::protocol::core::{
    self as wl, wl_data_device as device, wl_data_device_manager as manager, wl_data_offer as offer,
};
use compositor_toolkit::{Client, ObjectId, Value};

/// What a drag's files are offered as.
pub const URI_LIST: &str = "text/uri-list";

/// `wl_data_device_manager.dnd_action.copy`.
const COPY: u32 = 1;

/// What happened to a drag over the dock.
#[derive(Clone, Debug, PartialEq)]
pub enum Drag {
    /// It is over the surface at `(x, y)`, carrying files or not.
    Over(f32, f32, bool),
    /// It left.
    Left,
    /// It was let go at `(x, y)` with this `text/uri-list`.
    Dropped(f32, f32, String),
}

/// The seat's data device, and the drag over the dock.
#[derive(Debug)]
pub struct Target {
    /// The device.
    device: ObjectId,
    /// Every live offer's types.
    types: Vec<(ObjectId, String)>,
    /// The offer of the drag over the dock, the serial of its `enter`
    /// and whether it carries files.
    over: Option<(ObjectId, u32, bool)>,
    /// Where the pointer was last.
    at: (f32, f32),
    /// The manager's version.
    version: u32,
}

impl Target {
    /// The data device, when the compositor has `wl_data_device_manager`.
    pub fn bind(client: &mut Client) -> Option<Self> {
        let seat = client.seat()?;
        let (bound, version) = client.bind(&wl::WL_DATA_DEVICE_MANAGER, 3, None).ok()?;
        let made = client.new_object(&wl::WL_DATA_DEVICE, version);
        client
            .request(
                bound,
                manager::request::GET_DATA_DEVICE,
                &[Value::NewId(made), Value::Object(seat)],
            )
            .ok()?;
        client.adopt_new_ids(made, device::event::DATA_OFFER, &wl::WL_DATA_OFFER, version);
        let _ = client.flush();
        Some(Target {
            device: made,
            types: Vec::new(),
            over: None,
            at: (0.0, 0.0),
            version,
        })
    }

    /// An event for one of the target's objects: `None` when it was not
    /// one, `Some(None)` when it was and changed nothing the program sees.
    pub fn event(
        &mut self,
        client: &mut Client,
        object: ObjectId,
        opcode: u16,
        args: &[Value],
    ) -> Option<Option<Drag>> {
        let float = |at: usize| match args.get(at) {
            Some(Value::Fixed(v)) => *v as f32,
            _ => 0.0,
        };
        if object == self.device {
            return Some(match opcode {
                device::event::ENTER => {
                    let serial = match args.first() {
                        Some(Value::Uint(serial)) => *serial,
                        _ => 0,
                    };
                    let named = args
                        .get(4)
                        .and_then(Value::as_object)
                        .filter(|o| !o.is_null());
                    let files = named.is_some_and(|o| {
                        self.types
                            .iter()
                            .any(|(of, mime)| *of == o && mime == URI_LIST)
                    });
                    if let Some(offered) = named {
                        self.take(client, offered, serial, files);
                    }
                    self.at = (float(2), float(3));
                    Some(Drag::Over(self.at.0, self.at.1, files))
                }
                device::event::MOTION => {
                    self.at = (float(1), float(2));
                    let files = self.over.is_some_and(|(_, _, files)| files);
                    Some(Drag::Over(self.at.0, self.at.1, files))
                }
                device::event::LEAVE => {
                    if let Some((offered, _, _)) = self.over.take() {
                        self.forget(client, offered);
                    }
                    Some(Drag::Left)
                }
                device::event::DROP => {
                    let Some((offered, _, files)) = self.over.take() else {
                        return Some(None);
                    };
                    let text = if files {
                        read(client, offered)
                    } else {
                        String::new()
                    };
                    if files && self.version >= 3 {
                        let _ = client.request(offered, offer::request::FINISH, &[]);
                    }
                    self.forget(client, offered);
                    files.then_some(Drag::Dropped(self.at.0, self.at.1, text))
                }
                device::event::SELECTION => {
                    // The clipboard's offer, which the dock does not read.
                    if let Some(named) = args.first().and_then(Value::as_object)
                        && !named.is_null()
                    {
                        self.forget(client, named);
                    }
                    None
                }
                _ => None,
            });
        }
        if opcode == offer::event::OFFER
            && let Some(mime) = args.first().and_then(Value::as_str)
        {
            self.types.push((object, mime.to_owned()));
            return Some(None);
        }
        if self.types.iter().any(|(of, _)| *of == object) {
            // `source_actions`, `action`: nothing to do.
            return Some(None);
        }
        None
    }

    /// The drag of `offered` came over the dock: say it would copy files,
    /// and nothing else.
    fn take(&mut self, client: &mut Client, offered: ObjectId, serial: u32, files: bool) {
        let mime = files.then(|| URI_LIST.to_owned());
        let _ = client.request(
            offered,
            offer::request::ACCEPT,
            &[Value::Uint(serial), Value::Str(mime)],
        );
        if self.version >= 3 {
            let actions = if files { COPY } else { 0 };
            let _ = client.request(
                offered,
                offer::request::SET_ACTIONS,
                &[Value::Uint(actions), Value::Uint(actions)],
            );
        }
        let _ = client.flush();
        self.over = Some((offered, serial, files));
    }

    /// Destroy `offered` and forget its types.
    fn forget(&mut self, client: &mut Client, offered: ObjectId) {
        let _ = client.request(offered, offer::request::DESTROY, &[]);
        let _ = client.flush();
        self.types.retain(|(of, _)| *of != offered);
    }
}

/// The `text/uri-list` of `offered`, waiting a moment for the program
/// that dragged it to write it.
fn read(client: &mut Client, offered: ObjectId) -> String {
    let Ok((mut reading, writing)) = std::os::unix::net::UnixStream::pair() else {
        return String::new();
    };
    let sent = client.request(
        offered,
        offer::request::RECEIVE,
        &[
            Value::Str(Some(URI_LIST.to_owned())),
            Value::Fd(writing.as_raw_fd()),
        ],
    );
    let _ = client.flush();
    drop(writing);
    if sent.is_err() {
        return String::new();
    }
    let _ = reading.set_read_timeout(Some(Duration::from_millis(1500)));
    let mut bytes = Vec::new();
    let _ = reading.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}
