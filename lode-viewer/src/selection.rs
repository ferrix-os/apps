//! The selections a person pastes from: the clipboard (`wl_data_device`,
//! Ctrl+V and Shift+Insert) and the primary selection
//! (`zwp_primary_selection_device_v1`, the middle button), and the
//! clipboard a field's Ctrl+C and Ctrl+X put text on.
//!
//! The compositor offers the selection to the window that has the
//! keyboard (GNOME's mutter does so on focus); each offer lists the types
//! it can be had as, and a paste asks for its text through a pipe, read on
//! a thread of its own so a slow source never stops the window. What was
//! pasted is never said on standard output, only how long it was: a field
//! may be a private key.

use std::collections::HashMap;
use std::io::Read as _;
use std::sync::mpsc;

use compositor_toolkit::protocol::core::{
    self as core, wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source,
};
use compositor_toolkit::protocol::primary_selection::{
    self as primary, zwp_primary_selection_device_manager_v1 as primary_manager,
    zwp_primary_selection_device_v1 as primary_device,
    zwp_primary_selection_offer_v1 as primary_offer,
};
use compositor_toolkit::{Client, ObjectId, Value, Waker};

/// The text types, best first.
pub const TEXT_TYPES: [&str; 4] = [
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
];

/// The most a paste reads: a key or a query, never a file's worth.
const MOST: u64 = 256 * 1024;

/// Which selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    /// The clipboard.
    Clipboard,
    /// The primary selection: what was last selected, pasted by the
    /// middle button.
    Primary,
}

/// One selection's device and what it offers now.
#[derive(Debug, Default)]
struct Device {
    /// The device, once bound.
    device: Option<ObjectId>,
    /// Each live offer's types.
    offers: HashMap<u32, Vec<String>>,
    /// The offer that is the selection now.
    selection: Option<ObjectId>,
}

impl Device {
    /// The text type the selection can be had as.
    fn text_type(&self) -> Option<(ObjectId, String)> {
        let offer = self.selection?;
        let types = self.offers.get(&offer.0)?;
        TEXT_TYPES
            .iter()
            .find(|wanted| types.iter().any(|offered| offered == *wanted))
            .map(|wanted| (offer, (*wanted).to_owned()))
    }
}

/// The selections, bound on the window's connection.
#[derive(Debug)]
pub struct Selections {
    /// `wl_data_device_manager`, for making a source.
    manager: Option<ObjectId>,
    /// The clipboard.
    clipboard: Device,
    /// The primary selection.
    primary: Device,
    /// The source the window offers, and its text.
    source: Option<(ObjectId, String)>,
    /// Pastes on their way: their text arrives here.
    arrived: mpsc::Receiver<String>,
    /// The sending side, a clone per paste.
    send: mpsc::Sender<String>,
    /// Wakes the window when a paste arrived.
    waker: Option<Waker>,
}

impl Selections {
    /// Bind what the compositor offers of the two.
    pub fn bind(client: &mut Client, waker: Option<Waker>) -> Self {
        let (send, arrived) = mpsc::channel();
        let mut selections = Selections {
            manager: None,
            clipboard: Device::default(),
            primary: Device::default(),
            source: None,
            arrived,
            send,
            waker,
        };
        let Some(seat) = client.seat() else {
            return selections;
        };
        if let Ok((manager, version)) = client.bind(&core::WL_DATA_DEVICE_MANAGER, 3, None) {
            let device = client.new_object(&core::WL_DATA_DEVICE, version);
            client.adopt_new_ids(
                device,
                wl_data_device::event::DATA_OFFER,
                &core::WL_DATA_OFFER,
                version,
            );
            if client
                .request(
                    manager,
                    wl_data_device_manager::request::GET_DATA_DEVICE,
                    &[Value::NewId(device), Value::Object(seat)],
                )
                .is_ok()
            {
                selections.manager = Some(manager);
                selections.clipboard.device = Some(device);
            }
        }
        if let Ok((manager, version)) =
            client.bind(&primary::ZWP_PRIMARY_SELECTION_DEVICE_MANAGER_V1, 1, None)
        {
            let device = client.new_object(&primary::ZWP_PRIMARY_SELECTION_DEVICE_V1, version);
            client.adopt_new_ids(
                device,
                primary_device::event::DATA_OFFER,
                &primary::ZWP_PRIMARY_SELECTION_OFFER_V1,
                version,
            );
            if client
                .request(
                    manager,
                    primary_manager::request::GET_DEVICE,
                    &[Value::NewId(device), Value::Object(seat)],
                )
                .is_ok()
            {
                selections.primary.device = Some(device);
            }
        }
        let _ = client.flush();
        selections
    }

    /// An event of one of the selections' objects. Answers whether it was
    /// theirs.
    pub fn event(
        &mut self,
        client: &mut Client,
        object: ObjectId,
        interface: &str,
        opcode: u16,
        args: &[Value],
    ) -> bool {
        let object_of = |at: usize| {
            args.get(at)
                .and_then(Value::as_object)
                .filter(|id| id.0 != 0)
        };
        let text_of = |at: usize| match args.get(at) {
            Some(Value::Str(Some(text))) => Some(text.clone()),
            _ => None,
        };
        match interface {
            "wl_data_device" | "zwp_primary_selection_device_v1" => {
                let (device, offer_opcode, selection_opcode, destroy) =
                    if interface == "wl_data_device" {
                        (
                            &mut self.clipboard,
                            wl_data_device::event::DATA_OFFER,
                            wl_data_device::event::SELECTION,
                            wl_data_offer::request::DESTROY,
                        )
                    } else {
                        (
                            &mut self.primary,
                            primary_device::event::DATA_OFFER,
                            primary_device::event::SELECTION,
                            primary_offer::request::DESTROY,
                        )
                    };
                if device.device != Some(object) {
                    return false;
                }
                if opcode == offer_opcode {
                    if let Some(offer) = object_of(0) {
                        let _ = device.offers.insert(offer.0, Vec::new());
                    }
                } else if opcode == selection_opcode {
                    let now = object_of(0);
                    // The offers before it are done with.
                    let old: Vec<u32> = device
                        .offers
                        .keys()
                        .copied()
                        .filter(|id| Some(*id) != now.map(|o| o.0))
                        .collect();
                    for id in old {
                        let _ = device.offers.remove(&id);
                        let _ = client.request(ObjectId(id), destroy, &[]);
                    }
                    device.selection = now;
                }
                true
            }
            "wl_data_offer" | "zwp_primary_selection_offer_v1" => {
                let device = if interface == "wl_data_offer" {
                    &mut self.clipboard
                } else {
                    &mut self.primary
                };
                let Some(types) = device.offers.get_mut(&object.0) else {
                    return false;
                };
                // `offer` is opcode 0 of both.
                if opcode == 0
                    && let Some(mime) = text_of(0)
                {
                    types.push(mime);
                }
                true
            }
            "wl_data_source" => {
                let Some((source, text)) = &self.source else {
                    return false;
                };
                if object != *source {
                    return false;
                }
                if opcode == wl_data_source::event::SEND {
                    if let Some(Value::Fd(fd)) = args.get(1) {
                        crate::window::write_and_close(*fd, text.as_bytes());
                    }
                } else if opcode == wl_data_source::event::CANCELLED {
                    let _ = client.request(*source, wl_data_source::request::DESTROY, &[]);
                    self.source = None;
                }
                true
            }
            _ => false,
        }
    }

    /// Ask for `which` selection's text; it arrives through
    /// [`Selections::take`]. Answers whether there is one to paste.
    pub fn paste(&mut self, client: &mut Client, which: Which) -> bool {
        let device = match which {
            Which::Clipboard => &self.clipboard,
            Which::Primary => &self.primary,
        };
        let Some((offer, mime)) = device.text_type() else {
            return false;
        };
        let Ok((mut reader, writer)) = std::io::pipe() else {
            return false;
        };
        let receive = match which {
            Which::Clipboard => wl_data_offer::request::RECEIVE,
            Which::Primary => primary_offer::request::RECEIVE,
        };
        use std::os::fd::AsRawFd as _;
        let asked = client
            .request(
                offer,
                receive,
                &[Value::Str(Some(mime)), Value::Fd(writer.as_raw_fd())],
            )
            .is_ok();
        // The descriptor goes with the flush; ours closes after it, so the
        // read below ends when the source closes its end.
        let _ = client.flush();
        drop(writer);
        if !asked {
            return false;
        }
        let send = self.send.clone();
        let waker = self.waker.clone();
        let _ = std::thread::Builder::new()
            .name("paste".to_owned())
            .spawn(move || {
                let mut bytes = Vec::new();
                let _ = (&mut reader).take(MOST).read_to_end(&mut bytes);
                let _ = send.send(String::from_utf8_lossy(&bytes).into_owned());
                if let Some(waker) = waker {
                    waker.wake();
                }
            });
        true
    }

    /// A paste that arrived, if one did.
    pub fn take(&mut self) -> Option<String> {
        self.arrived.try_recv().ok()
    }

    /// Put `text` on the clipboard, for the key press `serial`. Answers
    /// whether the compositor was asked.
    pub fn copy(&mut self, client: &mut Client, text: String, serial: u32) -> bool {
        let (Some(manager), Some(device)) = (self.manager, self.clipboard.device) else {
            return false;
        };
        let source = client.new_object(&core::WL_DATA_SOURCE, 3);
        let mut sent = client
            .request(
                manager,
                wl_data_device_manager::request::CREATE_DATA_SOURCE,
                &[Value::NewId(source)],
            )
            .is_ok();
        for mime in TEXT_TYPES {
            sent &= client
                .request(
                    source,
                    wl_data_source::request::OFFER,
                    &[Value::Str(Some(mime.to_owned()))],
                )
                .is_ok();
        }
        sent &= client
            .request(
                device,
                wl_data_device::request::SET_SELECTION,
                &[Value::Object(source), Value::Uint(serial)],
            )
            .is_ok();
        let _ = client.flush();
        if let Some((old, _)) = self.source.replace((source, text)) {
            let _ = client.request(old, wl_data_source::request::DESTROY, &[]);
        }
        sent
    }
}
