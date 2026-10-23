//! Choosing a key file: the desktop's own file dialog through the XDG
//! Desktop Portal (`org.freedesktop.portal.FileChooser.OpenFile` on the
//! session bus, spoken with the compositor's small D-Bus crate), and,
//! where no portal answers, the private keys in `~/.ssh` for the form to
//! list.
//!
//! The portal's answer comes as a `Response` signal on the request object
//! the call names; the request's path is known before the call from the
//! handle token, so the match rule is in place before the dialog can
//! answer (the portal documentation, "Request").

use std::path::{Path, PathBuf};

use compositor_dbus::{Connection, Kind, Message, Reader, Writer};

/// What choosing came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked {
    /// This file.
    File(PathBuf),
    /// The person closed the dialog.
    Cancelled,
    /// No dialog could be shown: why.
    NoPortal(String),
}

/// The portal's bus name and object.
const PORTAL: &str = "org.freedesktop.portal.Desktop";
/// Its object path.
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
/// The file chooser's interface.
const FILE_CHOOSER: &str = "org.freedesktop.portal.FileChooser";
/// The request object's interface.
const REQUEST: &str = "org.freedesktop.portal.Request";

/// Show the desktop's file dialog, opened in `folder`, and wait for the
/// person to choose. Blocks while the dialog is open: run it on a thread.
#[must_use]
pub fn choose(title: &str, folder: Option<&Path>) -> Picked {
    let Some(address) = std::env::var_os("DBUS_SESSION_BUS_ADDRESS") else {
        return Picked::NoPortal("no session bus".to_owned());
    };
    let Some(path) = compositor_dbus::session_path(&address.to_string_lossy()) else {
        return Picked::NoPortal("the session bus is not at a socket path".to_owned());
    };
    match open_file(&path, title, folder) {
        Ok(picked) => picked,
        Err(why) => Picked::NoPortal(why),
    }
}

/// This process's uid, from `/proc/self`'s owner.
fn uid() -> u32 {
    use std::os::unix::fs::MetadataExt as _;
    std::fs::metadata("/proc/self").map_or(0, |meta| meta.uid())
}

/// The request path the portal will use for `token`, from our unique name
/// (`:1.42` → `1_42`).
#[must_use]
pub fn request_path(unique: &str, token: &str) -> String {
    let sender = unique.trim_start_matches(':').replace('.', "_");
    format!("{PORTAL_PATH}/request/{sender}/{token}")
}

/// `OpenFile`'s body: `parent_window`, `title` and the options `a{sv}`:
/// the handle token, modal, and the folder to open in.
#[must_use]
pub fn open_file_body(title: &str, token: &str, folder: Option<&Path>) -> Writer {
    let mut body = Writer::new();
    body.string("");
    body.string(title);
    body.array(8, |entries| {
        entries.align(8);
        entries.string("handle_token");
        entries.signature("s");
        entries.string(token);
        entries.align(8);
        entries.string("modal");
        entries.signature("b");
        entries.bool(true);
        if let Some(folder) = folder {
            use std::os::unix::ffi::OsStrExt as _;
            entries.align(8);
            entries.string("current_folder");
            entries.signature("ay");
            entries.array(1, |bytes| {
                for &byte in folder.as_os_str().as_bytes() {
                    bytes.byte(byte);
                }
                bytes.byte(0);
            });
        }
    });
    body
}

/// A `Response` signal's body, `ua{sv}`: the response code and the first
/// of its `uris`.
///
/// # Errors
///
/// A body that does not read as one.
pub fn response(body: &[u8]) -> Result<(u32, Option<String>), String> {
    let mut reader = Reader::new(body);
    let bad = |why: compositor_dbus::Malformed| why.to_string();
    let code = reader.u32().map_err(bad)?;
    let length = usize::try_from(reader.u32().map_err(bad)?).map_err(|_| "a long array")?;
    reader.align(8).map_err(bad)?;
    let end = reader.position() + length;
    let mut uri = None;
    while reader.position() < end {
        reader.align(8).map_err(bad)?;
        let key = reader.string().map_err(bad)?;
        let signature = reader.signature().map_err(bad)?;
        if key == "uris" && signature == "as" {
            uri = reader
                .strings()
                .map_err(bad)?
                .first()
                .map(|uri| (*uri).to_owned());
        } else {
            let mut left = signature;
            while !left.is_empty() {
                left = reader.skip(left).map_err(bad)?;
            }
        }
    }
    Ok((code, uri))
}

/// A `file://` URI's path, its `%XX` escapes decoded.
#[must_use]
pub fn uri_path(uri: &str) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt as _;
    let rest = uri.strip_prefix("file://")?;
    // `file://host/path`: the path starts at the first slash.
    let rest = rest.get(rest.find('/')?..)?;
    let mut bytes = Vec::with_capacity(rest.len());
    let mut chars = rest.bytes();
    while let Some(byte) = chars.next() {
        if byte == b'%' {
            let high = chars.next()?;
            let low = chars.next()?;
            let hex = |b: u8| (b as char).to_digit(16);
            bytes.push(u8::try_from(hex(high)? * 16 + hex(low)?).ok()?);
        } else {
            bytes.push(byte);
        }
    }
    Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
}

/// The portal conversation on the bus at `socket`.
fn open_file(socket: &str, title: &str, folder: Option<&Path>) -> Result<Picked, String> {
    let mut bus =
        Connection::open(socket, uid()).map_err(|why| format!("the session bus: {why}"))?;
    let token = format!("lode{}", std::process::id());
    let mut expected = request_path(&bus.unique, &token);
    add_match(&mut bus, &expected)?;
    let call = Message::call(PORTAL, PORTAL_PATH, FILE_CHOOSER, "OpenFile")
        .with_body("ssa{sv}", open_file_body(title, &token, folder));
    let reply = bus
        .call(&call)
        .map_err(|why| format!("the file dialog portal: {why}"))?;
    // A portal older than handle tokens answers on a path of its own.
    if let Ok(handle) = reply.reader().string()
        && handle != expected
    {
        expected = handle.to_owned();
        add_match(&mut bus, &expected)?;
    }
    loop {
        let message = bus
            .receive()
            .map_err(|why| format!("the session bus: {why}"))?;
        if message.kind != Some(Kind::Signal)
            || message.member.as_deref() != Some("Response")
            || message.path.as_deref() != Some(expected.as_str())
        {
            continue;
        }
        let (code, uri) = response(&message.body)?;
        return Ok(match (code, uri.as_deref().and_then(uri_path)) {
            (0, Some(path)) => Picked::File(path),
            (0 | 1, _) => Picked::Cancelled,
            _ => Picked::NoPortal("the file dialog failed".to_owned()),
        });
    }
}

/// Ask the bus for the `Response` signals of the request at `path`.
fn add_match(bus: &mut Connection, path: &str) -> Result<(), String> {
    let mut rule = Writer::new();
    rule.string(&format!(
        "type='signal',interface='{REQUEST}',member='Response',path='{path}'"
    ));
    let call = Message::call(
        compositor_dbus::BUS_NAME,
        compositor_dbus::BUS_PATH,
        compositor_dbus::BUS_NAME,
        "AddMatch",
    )
    .with_body("s", rule);
    bus.call(&call)
        .map(|_| ())
        .map_err(|why| format!("the session bus: {why}"))
}

/// `~/.ssh`.
#[must_use]
pub fn ssh_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".ssh"))
}

/// The private keys in `dir`: files whose first line says they are one,
/// sorted.
#[must_use]
pub fn private_keys(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut keys: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_private_key_file(path))
        .collect();
    keys.sort();
    keys
}

/// Whether the file at `path` starts as a PEM or OpenSSH private key does.
/// Only its first line is read.
#[must_use]
pub fn is_private_key_file(path: &Path) -> bool {
    use std::io::Read as _;
    let mut head = [0_u8; 64];
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let got = file.read(&mut head).unwrap_or(0);
    let head = String::from_utf8_lossy(head.get(..got).unwrap_or_default());
    let first = head.lines().next().unwrap_or("");
    first.starts_with("-----BEGIN ") && first.contains("PRIVATE KEY")
}

#[cfg(test)]
mod tests {
    use super::{
        is_private_key_file, open_file_body, private_keys, request_path, response, uri_path,
    };
    use compositor_dbus::{Reader, Writer};
    use std::path::{Path, PathBuf};

    #[test]
    fn the_request_path_is_named_from_our_unique_name_and_token() {
        assert_eq!(
            request_path(":1.42", "lode7"),
            "/org/freedesktop/portal/desktop/request/1_42/lode7"
        );
    }

    #[test]
    fn open_file_names_the_token_and_the_folder() {
        let body = open_file_body("Choose", "lode7", Some(Path::new("/home/me/.ssh"))).into_bytes();
        let mut reader = Reader::new(&body);
        assert_eq!(reader.string(), Ok(""));
        assert_eq!(reader.string(), Ok("Choose"));
        let rest = reader.skip("a{sv}").map(str::len);
        assert_eq!(rest, Ok(0));
        assert_eq!(reader.position(), body.len());
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("handle_token") && text.contains("lode7"));
        assert!(text.contains("current_folder") && text.contains("/home/me/.ssh\0"));
    }

    fn signal_body(code: u32, uris: &[&str]) -> Vec<u8> {
        let mut body = Writer::new();
        body.u32(code);
        body.array(8, |entries| {
            entries.align(8);
            entries.string("choices");
            entries.signature("a(ss)");
            entries.array(8, |_| {});
            entries.align(8);
            entries.string("uris");
            entries.signature("as");
            entries.strings(uris);
        });
        body.into_bytes()
    }

    #[test]
    fn a_response_gives_the_chosen_files_path() {
        let body = signal_body(0, &["file:///home/me/.ssh/id%20box"]);
        let (code, uri) = response(&body).expect("reads");
        assert_eq!(code, 0);
        assert_eq!(
            uri.as_deref().and_then(uri_path),
            Some(PathBuf::from("/home/me/.ssh/id box"))
        );
        assert_eq!(response(&signal_body(1, &[])), Ok((1, None)));
        assert_eq!(uri_path("https://x/y"), None);
        assert_eq!(uri_path("file://host/a"), Some(PathBuf::from("/a")));
    }

    #[test]
    fn only_private_keys_are_listed_from_ssh() {
        let dir = std::env::temp_dir().join(format!("lode-picker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).expect("write");
        write("id_ed25519", "-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\n");
        write("id_ed25519.pub", "ssh-ed25519 AAAA me@x\n");
        write("known_hosts", "box ssh-ed25519 AAAA\n");
        write("old_rsa", "-----BEGIN RSA PRIVATE KEY-----\nAAAA\n");
        let keys = private_keys(&dir);
        assert_eq!(keys, vec![dir.join("id_ed25519"), dir.join("old_rsa")]);
        assert!(!is_private_key_file(&dir.join("known_hosts")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Run by hand against a portal on the session bus (a stand-in one on a
/// bus of its own): `cargo test -- --ignored choose_asks_the_portal`.
#[cfg(test)]
#[test]
#[ignore = "needs a portal on the session bus"]
fn choose_asks_the_portal() {
    let picked = choose(
        "Choose the machine's private key",
        Some(Path::new("/home/me/.ssh")),
    );
    assert_eq!(
        picked,
        Picked::File(PathBuf::from("/home/me/.ssh/id ed25519"))
    );
}
