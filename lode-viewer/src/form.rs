//! The Add machine form, with no window: its fields, what each may hold
//! (said beside the field as it is typed, not only when Add is pressed),
//! the key's three ways, the name made from the host, and the line editing
//! every field of the viewer shares.

use std::path::{Path, PathBuf};

use crate::remote::{Kind, Remote};

/// The Add machine form's fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormField {
    /// What to call it.
    Name,
    /// Its host.
    Host,
    /// The user to log in as.
    User,
    /// The port.
    Port,
    /// The key file's path.
    Key,
    /// A key pasted in, several lines.
    Paste,
    /// How the machine reaches this one, for TLS.
    Server,
}

impl FormField {
    /// Every field, in the order Tab goes.
    pub const ALL: [FormField; 7] = [
        FormField::Name,
        FormField::Host,
        FormField::User,
        FormField::Port,
        FormField::Key,
        FormField::Paste,
        FormField::Server,
    ];

    /// Its index in [`FormField::ALL`].
    fn index(self) -> usize {
        FormField::ALL
            .iter()
            .position(|each| *each == self)
            .unwrap_or(0)
    }
}

/// How ssh logs in to the machine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyChoice {
    /// ssh's own keys and the agent's.
    #[default]
    Agent,
    /// A key file, chosen.
    File,
    /// A key pasted into the form, kept in a file of its own.
    Paste,
}

/// The Add machine form.
#[derive(Clone, PartialEq, Eq)]
pub struct Form {
    /// The values, in [`FormField::ALL`]'s order.
    pub values: [String; 7],
    /// How the log comes.
    pub kind: Kind,
    /// How ssh logs in.
    pub key: KeyChoice,
    /// The field typed into.
    pub focus: Option<FormField>,
    /// Whether the focused field's whole text is selected (Ctrl+A).
    pub all: bool,
    /// Whether the name was typed, rather than made from the host.
    pub name_typed: bool,
    /// Why the last Add was refused.
    pub error: Option<String>,
}

// The form may hold a private key: its Debug says only how long that is.
impl std::fmt::Debug for Form {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Form")
            .field("name", &self.value(FormField::Name))
            .field("host", &self.value(FormField::Host))
            .field("kind", &self.kind)
            .field("key", &self.key)
            .field(
                "pasted",
                &format!("{} bytes", self.value(FormField::Paste).len()),
            )
            .field("focus", &self.focus)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl Default for Form {
    fn default() -> Self {
        Form {
            values: [
                String::new(),
                String::new(),
                "root".to_owned(),
                "22".to_owned(),
                String::new(),
                String::new(),
                String::new(),
            ],
            kind: Kind::Ssh,
            key: KeyChoice::Agent,
            focus: None,
            all: false,
            name_typed: false,
            error: None,
        }
    }
}

/// What a key did to a line of text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    /// The text changed, or the selection did.
    Changed,
    /// Copy this.
    Copy(String),
    /// Nothing the line takes.
    Unhandled,
}

/// One key typed into `value`: Backspace, Ctrl+A, Ctrl+C, Ctrl+X and
/// printable text. `all` is whether the whole text is selected: what is
/// typed then replaces it. Nothing is inserted while Ctrl or Alt is held.
pub fn edit_line(
    value: &mut String,
    all: &mut bool,
    keysym: &str,
    text: &str,
    (control, alt): (bool, bool),
) -> Edit {
    if control {
        return match keysym {
            "a" | "A" => {
                *all = !value.is_empty();
                Edit::Changed
            }
            "c" | "C" => Edit::Copy(value.clone()),
            "x" | "X" => {
                let cut = std::mem::take(value);
                *all = false;
                Edit::Copy(cut)
            }
            _ => Edit::Unhandled,
        };
    }
    match keysym {
        "BackSpace" => {
            if std::mem::take(all) {
                value.clear();
            } else {
                let _ = value.pop();
            }
            Edit::Changed
        }
        _ if !text.is_empty() && !alt && !text.chars().any(char::is_control) => {
            if std::mem::take(all) {
                value.clear();
            }
            value.push_str(text);
            Edit::Changed
        }
        _ => Edit::Unhandled,
    }
}

/// A name made from a host: `box.lan` → `box`, `192.168.9.193` →
/// `192-168-9-193`; a-z, 0-9 and dashes, at most 32.
#[must_use]
pub fn name_from_host(host: &str) -> String {
    let host = host.trim().to_ascii_lowercase();
    let numeric = host.chars().all(|c| c.is_ascii_digit() || c == '.');
    let base = if numeric || host.contains(':') {
        host.as_str()
    } else {
        host.split('.').next().unwrap_or("")
    };
    let mut name = String::new();
    for c in base.chars() {
        let c = if c.is_ascii_lowercase() || c.is_ascii_digit() {
            c
        } else {
            '-'
        };
        if c == '-' && (name.is_empty() || name.ends_with('-')) {
            continue;
        }
        name.push(c);
    }
    name.truncate(32);
    name.trim_end_matches('-').to_owned()
}

/// Why `name` cannot name a machine, if it cannot.
#[must_use]
pub fn name_problem(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        return Some("a name is needed");
    }
    let ok = name.len() <= 32
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    (!ok).then_some("a-z, 0-9 and dashes, at most 32")
}

/// Whether `text` is a host name or address the form takes.
fn plain(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'))
}

/// Why pasted `text` cannot be used as a key, if it cannot.
#[must_use]
pub fn pasted_key_problem(text: &str) -> Option<&'static str> {
    let text = text.trim();
    if text.is_empty() {
        return Some("paste the private key here");
    }
    if text.starts_with("ssh-") || text.starts_with("ecdsa-") {
        return Some("that is a public key: paste the private one");
    }
    if !(text.starts_with("-----BEGIN ") && text.contains("PRIVATE KEY-----")) {
        return Some("not a private key: it starts -----BEGIN … PRIVATE KEY-----");
    }
    if !text.contains("-----END ") {
        return Some("the key is cut off: its -----END line is missing");
    }
    if text.contains("ENCRYPTED") || openssh_cipher(text).is_some_and(|cipher| cipher != "none") {
        return Some(
            "this key has a passphrase, which the viewer never asks for: add it to ssh-agent and use ssh's own keys",
        );
    }
    None
}

/// An OpenSSH private key's cipher name (`none` for no passphrase), read
/// from its base64 body.
fn openssh_cipher(text: &str) -> Option<String> {
    let body: String = text
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect::<Vec<_>>()
        .concat();
    if !text.contains("OPENSSH PRIVATE KEY") {
        return None;
    }
    let bytes = base64(&body)?;
    let rest = bytes.strip_prefix(b"openssh-key-v1\0")?;
    let length = u32::from_be_bytes(rest.get(..4)?.try_into().ok()?) as usize;
    let name = rest.get(4..4 + length)?;
    Some(String::from_utf8_lossy(name).into_owned())
}

/// Standard base64, padding and whitespace ignored.
fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut bits = 0_u32;
    let mut count = 0;
    for c in text.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push(u8::try_from((bits >> count) & 0xff).ok()?);
        }
    }
    Some(out)
}

/// `~/` at a path's start, the home directory.
#[must_use]
pub fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

/// Why the key file at `path` cannot be used, if it cannot.
#[must_use]
pub fn key_file_problem(path: &str) -> Option<&'static str> {
    if path.trim().is_empty() {
        return Some("choose the key file");
    }
    let path = expand_home(path.trim());
    if path.extension().is_some_and(|ext| ext == "pub") {
        return Some("that is the public half: choose the file without .pub");
    }
    if !path.is_file() {
        return Some("no such file");
    }
    if !crate::picker::is_private_key_file(&path) {
        return Some("not a private key file");
    }
    None
}

impl Form {
    /// A field's value.
    #[must_use]
    pub fn value(&self, field: FormField) -> &str {
        self.values.get(field.index()).map_or("", String::as_str)
    }

    /// A field's value, to change.
    pub fn value_mut(&mut self, field: FormField) -> Option<&mut String> {
        self.values.get_mut(field.index())
    }

    /// Set a field, as typing it would.
    pub fn set(&mut self, field: FormField, value: &str) {
        if let Some(slot) = self.value_mut(field) {
            value.clone_into(slot);
        }
        self.changed(field);
    }

    /// After `field` changed: the name follows the host until it is typed.
    pub fn changed(&mut self, field: FormField) {
        match field {
            FormField::Name => self.name_typed = !self.value(FormField::Name).is_empty(),
            FormField::Host if !self.name_typed => {
                let name = name_from_host(self.value(FormField::Host));
                if let Some(slot) = self.value_mut(FormField::Name) {
                    *slot = name;
                }
            }
            _ => {}
        }
        self.error = None;
    }

    /// The fields shown, in the order Tab goes.
    #[must_use]
    pub fn shown(&self) -> Vec<FormField> {
        let mut fields = vec![
            FormField::Name,
            FormField::Host,
            FormField::User,
            FormField::Port,
        ];
        match self.key {
            KeyChoice::Agent => {}
            KeyChoice::File => fields.push(FormField::Key),
            KeyChoice::Paste => fields.push(FormField::Paste),
        }
        if self.kind == Kind::Tls {
            fields.push(FormField::Server);
        }
        fields
    }

    /// Why `field`'s value cannot be used, if it cannot: said beside it.
    #[must_use]
    pub fn problem(&self, field: FormField) -> Option<&'static str> {
        let value = self.value(field).trim();
        match field {
            FormField::Name => name_problem(value),
            FormField::Host => (!plain(value)).then_some("a host name or address"),
            FormField::User => {
                (!value.is_empty() && !plain(value)).then_some("letters, digits, . _ -")
            }
            FormField::Port => value
                .parse::<u16>()
                .ok()
                .filter(|port| *port > 0)
                .is_none()
                .then_some("a port from 1 to 65535"),
            FormField::Key => key_file_problem(value),
            FormField::Paste => pasted_key_problem(value),
            FormField::Server => (!plain(value))
                .then_some("this machine's name or address, as the machine reaches it"),
        }
    }

    /// The first problem of the fields shown.
    #[must_use]
    pub fn first_problem(&self) -> Option<(FormField, &'static str)> {
        self.shown()
            .into_iter()
            .find_map(|field| self.problem(field).map(|why| (field, why)))
    }

    /// The settings it describes; the key is the file's path, or, for a
    /// pasted key, `keep`'s answer.
    #[must_use]
    pub fn remote(&self, kept_key: Option<&Path>) -> Remote {
        let key = match self.key {
            KeyChoice::Agent => None,
            KeyChoice::File => Some(self.value(FormField::Key).trim().to_owned()),
            KeyChoice::Paste => kept_key.map(|path| path.display().to_string()),
        };
        Remote {
            name: self.value(FormField::Name).trim().to_owned(),
            kind: self.kind,
            host: self.value(FormField::Host).trim().to_owned(),
            user: self.value(FormField::User).trim().to_owned(),
            port: self.value(FormField::Port).trim().parse().unwrap_or(0),
            key: key.filter(|key| !key.is_empty()),
        }
    }

    /// Text pasted into the focused field: a key keeps its lines; any
    /// other field takes the first line.
    pub fn paste(&mut self, text: &str) -> bool {
        let Some(field) = self.focus else {
            return false;
        };
        let text = text.replace("\r\n", "\n");
        let all = std::mem::take(&mut self.all);
        let Some(value) = self.value_mut(field) else {
            return false;
        };
        if all {
            value.clear();
        }
        if field == FormField::Paste {
            value.push_str(&text);
        } else {
            value.push_str(
                text.lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or("")
                    .trim(),
            );
        }
        self.changed(field);
        true
    }
}

/// Where a pasted key for machine `name` is kept:
/// `~/.config/ferrix/lode/keys/<name>`.
#[must_use]
pub fn key_path(name: &str) -> Option<PathBuf> {
    Some(
        crate::remote::config_path()?
            .parent()?
            .join("keys")
            .join(name),
    )
}

/// Keep pasted `text` as machine `name`'s key: its own file, mode 0600,
/// in a directory of mode 0700. The key is never said anywhere; an error
/// names only the file.
///
/// # Errors
///
/// What the file system said.
pub fn keep_key(name: &str, text: &str) -> Result<PathBuf, String> {
    use std::io::Write as _;
    use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
    let path = key_path(name).ok_or("no home directory to keep the key in")?;
    let dir = path.parent().ok_or("no directory for the key")?;
    std::fs::create_dir_all(dir).map_err(|why| format!("{}: {why}", dir.display()))?;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|why| format!("{}: {why}", path.display()))?;
    // A file that was there before keeps its old mode through open.
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|why| format!("{}: {why}", path.display()))?;
    let mut text = text.replace("\r\n", "\n").trim().to_owned();
    // ssh refuses a key whose last line has no newline.
    text.push('\n');
    file.write_all(text.as_bytes())
        .map_err(|why| format!("{}: {why}", path.display()))?;
    Ok(path)
}

/// The names and addresses a machine might reach this one by, the likeliest
/// first: the address of the route out, the host name, then the other
/// addresses this machine has.
#[must_use]
pub fn reach_choices() -> Vec<String> {
    let mut choices: Vec<String> = Vec::new();
    // Connecting a UDP socket sends nothing; it only picks the route.
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0")
        && socket.connect("192.0.2.1:9").is_ok()
        && let Ok(local) = socket.local_addr()
        && !local.ip().is_loopback()
        && !local.ip().is_unspecified()
    {
        choices.push(local.ip().to_string());
    }
    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| std::fs::read_to_string("/etc/hostname"))
        .unwrap_or_default();
    let hostname = hostname.trim();
    if !hostname.is_empty() && hostname != "localhost" {
        choices.push(hostname.to_owned());
    }
    if let Ok(trie) = std::fs::read_to_string("/proc/net/fib_trie") {
        for address in local_addresses(&trie) {
            if !choices.contains(&address) {
                choices.push(address);
            }
        }
    }
    choices.truncate(4);
    choices
}

/// This machine's own IPv4 addresses from `/proc/net/fib_trie`: each
/// `|-- A.B.C.D` followed by `/32 host LOCAL`, loopback left out.
#[must_use]
pub fn local_addresses(trie: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut last = None;
    for line in trie.lines() {
        let line = line.trim();
        if let Some(address) = line.strip_prefix("|-- ") {
            last = Some(address.trim().to_owned());
        } else if line.contains("/32 host LOCAL")
            && let Some(address) = last.take()
            && !address.starts_with("127.")
            && !out.contains(&address)
        {
            out.push(address);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        Edit, Form, FormField, KeyChoice, edit_line, local_addresses, name_from_host, name_problem,
        pasted_key_problem,
    };
    use crate::remote::Kind;

    #[test]
    fn the_name_follows_the_host_until_it_is_typed() {
        let mut form = Form::default();
        form.set(FormField::Host, "Box.lan");
        assert_eq!(form.value(FormField::Name), "box");
        form.set(FormField::Host, "192.168.9.193");
        assert_eq!(form.value(FormField::Name), "192-168-9-193");
        assert_eq!(form.problem(FormField::Name), None);
        form.set(FormField::Name, "n150");
        form.set(FormField::Host, "other");
        assert_eq!(form.value(FormField::Name), "n150");
        assert_eq!(name_from_host("fe80::1"), "fe80-1");
        assert_eq!(name_from_host("__x__"), "x");
    }

    #[test]
    fn each_field_says_what_is_wrong_as_it_is_typed() {
        let mut form = Form::default();
        assert_eq!(form.problem(FormField::Name), Some("a name is needed"));
        form.set(FormField::Name, "N150");
        assert_eq!(
            form.problem(FormField::Name),
            Some("a-z, 0-9 and dashes, at most 32")
        );
        assert_eq!(name_problem("n150"), None);
        form.set(FormField::Port, "70000");
        assert!(form.problem(FormField::Port).is_some());
        form.set(FormField::Port, "2222");
        assert_eq!(form.problem(FormField::Port), None);
        form.set(FormField::Host, "box lan");
        assert!(form.problem(FormField::Host).is_some());
    }

    #[test]
    fn the_fields_shown_follow_the_key_and_the_kind() {
        let mut form = Form::default();
        assert_eq!(form.shown().len(), 4);
        form.key = KeyChoice::Paste;
        form.kind = Kind::Tls;
        assert_eq!(
            form.shown(),
            vec![
                FormField::Name,
                FormField::Host,
                FormField::User,
                FormField::Port,
                FormField::Paste,
                FormField::Server
            ]
        );
    }

    #[test]
    fn control_keys_never_type_their_letter_and_edit_the_line() {
        let mut value = "abc".to_owned();
        let mut all = false;
        assert_eq!(
            edit_line(&mut value, &mut all, "v", "v", (true, false)),
            Edit::Unhandled
        );
        assert_eq!(
            edit_line(&mut value, &mut all, "x", "x", (false, true)),
            Edit::Unhandled
        );
        assert_eq!(value, "abc");
        assert_eq!(
            edit_line(&mut value, &mut all, "c", "", (true, false)),
            Edit::Copy("abc".to_owned())
        );
        let _ = edit_line(&mut value, &mut all, "a", "", (true, false));
        assert!(all);
        let _ = edit_line(&mut value, &mut all, "z", "z", (false, false));
        assert_eq!(value, "z");
        assert_eq!(
            edit_line(&mut value, &mut all, "x", "", (true, false)),
            Edit::Copy("z".to_owned())
        );
        assert_eq!(value, "");
    }

    #[test]
    fn a_paste_keeps_a_keys_lines_and_one_line_elsewhere() {
        let mut form = Form {
            focus: Some(FormField::Host),
            ..Form::default()
        };
        assert!(form.paste("  box.lan \r\nmore\r\n"));
        assert_eq!(form.value(FormField::Host), "box.lan");
        assert_eq!(form.value(FormField::Name), "box");
        form.focus = Some(FormField::Paste);
        let _ = form.paste("-----BEGIN X-----\r\nAAAA\r\n");
        assert_eq!(form.value(FormField::Paste), "-----BEGIN X-----\nAAAA\n");
        assert!(!format!("{form:?}").contains("AAAA"));
    }

    #[test]
    fn a_pasted_key_is_checked_before_it_is_kept() {
        assert!(pasted_key_problem("ssh-ed25519 AAAA me@x").is_some());
        assert!(pasted_key_problem("hello").is_some());
        // An OpenSSH key with no passphrase: cipher "none".
        let plain = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
                     b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQ==\n\
                     -----END OPENSSH PRIVATE KEY-----\n";
        assert_eq!(pasted_key_problem(plain), None);
        // One with aes256-ctr.
        let locked = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
                      b3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jdHI=\n\
                      -----END OPENSSH PRIVATE KEY-----\n";
        assert!(
            pasted_key_problem(locked).is_some_and(|why| why.contains("passphrase")),
            "{:?}",
            pasted_key_problem(locked)
        );
        assert!(pasted_key_problem("-----BEGIN RSA PRIVATE KEY-----\nAAAA").is_some());
    }

    #[test]
    fn a_kept_key_is_its_own_file_of_mode_0600() {
        use std::os::unix::fs::PermissionsExt as _;
        let home = std::env::temp_dir().join(format!("lode-form-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        // SAFETY of the test: no other test here reads XDG_CONFIG_HOME.
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &home) };
        let path =
            super::keep_key("n150", "-----BEGIN K-----\r\nAAAA\r\n-----END K-----").expect("kept");
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };
        assert_eq!(path, home.join("ferrix/lode/keys/n150"));
        let mode = std::fs::metadata(&path)
            .expect("there")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let dir_mode = std::fs::metadata(path.parent().expect("dir"))
            .expect("dir")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700);
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            "-----BEGIN K-----\nAAAA\n-----END K-----\n"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn this_machines_addresses_are_read_from_the_fib_trie() {
        let trie = "Main:\n  +-- 0.0.0.0/0 3 0 5\n     |-- 0.0.0.0\n        /0 universe UNICAST\n\
                    \x20    |-- 127.0.0.1\n        /32 host LOCAL\n\
                    \x20    |-- 192.168.9.10\n        /32 host LOCAL\n\
                    \x20    |-- 192.168.9.255\n        /32 link BROADCAST\n\
                    Local:\n     |-- 192.168.9.10\n        /32 host LOCAL\n";
        assert_eq!(local_addresses(trie), vec!["192.168.9.10".to_owned()]);
    }
}
