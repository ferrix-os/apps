//! Machines the viewer reaches itself: their settings, kept in
//! `~/.config/ferrix/lode/machines.conf`, and the link that follows one.
//!
//! **Live over SSH** works as `cargo xtask box-log` does: `ssh` runs
//! `lode -f -o json` on the machine, from `--after-cursor` the last record
//! kept when there is one and from the boot's start when not, and a
//! dropped connection is made again every few seconds from the last record
//! kept, so nothing is lost or repeated across a reboot. What arrives is
//! written into a store of the machine's own under the lode-receive root,
//! `<root>/<machine-id>/` (or `host-<hostname>/`), the layout lode-receive
//! keeps, so the history stays when the link drops and the viewer reads it
//! as any other machine's. The cursor to resume from is the store's own
//! last record: nothing else is kept.
//!
//! Only keys: `ssh` runs with `BatchMode=yes`, so it never asks for a
//! password, and with `IdentitiesOnly=yes` when a key is named. An agent's
//! keys are used as `ssh` uses them. No secret is ever written here.
//!
//! **Forwarded over TLS** -- lode's own upload to a lode-receive here -- is
//! a kind the file can name and the window shows, not yet one it sets up.

use std::fs;
use std::io::{BufRead, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use ferrix_lode_core::record::Record;
use ferrix_lode_core::segment::{self, Header};

/// How a machine's log comes here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// `lode -f` over ssh, followed by the viewer.
    #[default]
    Ssh,
    /// lode's upload over TLS to a lode-receive here.
    Tls,
}

impl Kind {
    /// Its word in the file.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Kind::Ssh => "ssh",
            Kind::Tls => "tls",
        }
    }
}

/// One machine's settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Remote {
    /// What the person calls it.
    pub name: String,
    /// How its log comes.
    pub kind: Kind,
    /// The host ssh reaches.
    pub host: String,
    /// The user to log in as.
    pub user: String,
    /// The port.
    pub port: u16,
    /// A private key to log in with; none for ssh's own choice and the
    /// agent's.
    pub key: Option<String>,
}

impl Remote {
    /// Why these settings cannot be used, if they cannot.
    #[must_use]
    pub fn refused(&self) -> Option<String> {
        let plain = |text: &str| {
            !text.is_empty()
                && text
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'))
        };
        if !plain(&self.name) {
            return Some("a name of letters, digits, dots, dashes and underscores".to_owned());
        }
        if !plain(&self.host) {
            return Some("a host name or address".to_owned());
        }
        if !self.user.is_empty() && !plain(&self.user) {
            return Some("a user name of letters and digits".to_owned());
        }
        if self.port == 0 {
            return Some("a port from 1 to 65535".to_owned());
        }
        if self.key.as_deref().is_some_and(|key| key.contains('\n')) {
            return Some("a key file's path".to_owned());
        }
        None
    }

    /// The `ssh` command that follows the machine's lode from `cursor`.
    #[must_use]
    pub fn ssh(&self, cursor: Option<&str>) -> Command {
        self.ssh_command(&follow_command(cursor))
    }

    /// The `ssh` command that runs `script` on the machine: keys only,
    /// never a password prompt.
    #[must_use]
    pub fn ssh_command(&self, script: &str) -> Command {
        let mut command = Command::new("ssh");
        command.args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=5",
            "-o",
            "ServerAliveInterval=5",
            "-o",
            "ServerAliveCountMax=3",
            // A machine met for the first time is trusted; one whose key
            // changed is refused, as ssh refuses it.
            "-o",
            "StrictHostKeyChecking=accept-new",
        ]);
        if let Some(file) = std::env::var_os("LODE_VIEWER_KNOWN_HOSTS") {
            let mut option = std::ffi::OsString::from("UserKnownHostsFile=");
            option.push(file);
            command.arg("-o").arg(option);
        }
        if let Some(key) = &self.key {
            command
                .args(["-o", "IdentitiesOnly=yes", "-i"])
                .arg(expand_home(key));
        }
        command.args(["-p", &self.port.to_string()]);
        let destination = if self.user.is_empty() {
            self.host.clone()
        } else {
            format!("{}@{}", self.user, self.host)
        };
        command.arg(destination).arg(script);
        command
    }
}

/// `~/` at a path's start, the home directory.
fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

/// The command run on the machine: lode following, from `cursor` when
/// there is one and from the boot's start when not -- box-log's.
#[must_use]
pub fn follow_command(cursor: Option<&str>) -> String {
    let quote = |word: &str| format!("'{}'", word.replace('\'', "'\\''"));
    let mut command = String::from("/bin/lode -f -o json -n 100000000");
    match cursor {
        Some(cursor) => command.push_str(&format!(" --after-cursor={}", quote(cursor))),
        None => command.push_str(" -b"),
    }
    command
}

/// The machines file's path: `~/.config/ferrix/lode/machines.conf`.
#[must_use]
pub fn config_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("ferrix/lode/machines.conf"))
}

/// The machines file read: `[machine NAME]` sections of `key = value`.
#[must_use]
pub fn parse(text: &str) -> Vec<Remote> {
    let mut out: Vec<Remote> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix("[machine ")
            .and_then(|rest| rest.strip_suffix(']'))
        {
            out.push(Remote {
                name: name.trim().to_owned(),
                port: 22,
                ..Remote::default()
            });
            continue;
        }
        let (Some(remote), Some((key, value))) = (out.last_mut(), line.split_once('=')) else {
            continue;
        };
        let value = value.trim().to_owned();
        match key.trim() {
            "kind" => remote.kind = if value == "tls" { Kind::Tls } else { Kind::Ssh },
            "host" => remote.host = value,
            "user" => remote.user = value,
            "port" => remote.port = value.parse().unwrap_or(0),
            "key" => remote.key = (!value.is_empty()).then_some(value),
            _ => {}
        }
    }
    out.retain(|remote| remote.refused().is_none());
    out
}

/// The machines file written.
#[must_use]
pub fn write(remotes: &[Remote]) -> String {
    let mut text =
        String::from("# lode-viewer's machines. Keys only: no password is ever kept here.\n");
    for remote in remotes {
        text.push_str(&format!(
            "\n[machine {}]\nkind = {}\nhost = {}\nuser = {}\nport = {}\n",
            remote.name,
            remote.kind.word(),
            remote.host,
            remote.user,
            remote.port
        ));
        if let Some(key) = &remote.key {
            text.push_str(&format!("key = {key}\n"));
        }
    }
    text
}

/// Where a link stands, for the window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkState {
    /// In words: connecting, following, waiting to try again.
    pub state: String,
    /// Whether records are coming.
    pub up: bool,
    /// Records kept since the viewer started.
    pub records: u64,
    /// When the last record came, seconds since 1970.
    pub last_seen: Option<u64>,
    /// The last error ssh or the store gave.
    pub error: Option<String>,
    /// The store the records go to, once the first has said which.
    pub dir: Option<PathBuf>,
}

/// A running link, and what it says.
#[derive(Debug)]
pub struct Link {
    /// The machine.
    pub remote: Remote,
    /// Its state, shared with the thread that follows it.
    pub state: Arc<Mutex<LinkState>>,
    /// Set to stop it.
    stop: Arc<Mutex<bool>>,
}

impl Link {
    /// Start following `remote` into `root`.
    #[must_use]
    pub fn start(remote: Remote, root: PathBuf) -> Self {
        let state = Arc::new(Mutex::new(LinkState {
            state: "connecting".to_owned(),
            ..LinkState::default()
        }));
        let stop = Arc::new(Mutex::new(false));
        let (thread_state, thread_stop, thread_remote) =
            (Arc::clone(&state), Arc::clone(&stop), remote.clone());
        let _ = std::thread::Builder::new()
            .name(format!("link-{}", remote.name))
            .spawn(move || follow(&thread_remote, &root, &thread_state, &thread_stop));
        Link {
            remote,
            state,
            stop,
        }
    }

    /// What it says now.
    #[must_use]
    pub fn now(&self) -> LinkState {
        self.state
            .lock()
            .map(|state| state.clone())
            .unwrap_or_default()
    }

    /// Stop it; the ssh running is ended within a few seconds.
    pub fn stop(&self) {
        if let Ok(mut stop) = self.stop.lock() {
            *stop = true;
        }
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Seconds since 1970.
fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Change the shared state.
fn set(state: &Arc<Mutex<LinkState>>, change: impl FnOnce(&mut LinkState)) {
    if let Ok(mut state) = state.lock() {
        change(&mut state);
    }
}

/// The link's loop: connect, keep what comes, and connect again from the
/// last record kept, until stopped.
fn follow(remote: &Remote, root: &Path, state: &Arc<Mutex<LinkState>>, stop: &Arc<Mutex<bool>>) {
    let stopped = || stop.lock().map_or(true, |stop| *stop);
    let mut writer = StoreWriter::new(root, &remote.name);
    while !stopped() {
        let cursor = writer.cursor();
        set(state, |state| state.state = "connecting".to_owned());
        let child = remote
            .ssh(cursor.as_deref())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(error) => {
                set(state, |state| {
                    state.error = Some(format!("ssh: {error}"));
                    state.up = false;
                    "ssh could not be started".clone_into(&mut state.state);
                });
                wait(&stopped, 5);
                continue;
            }
        };
        let stderr = child.stderr.take().map(spawn_stderr);
        if let Some(stdout) = child.stdout.take() {
            let watcher = watch_stop(&child, Arc::clone(stop));
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if stopped() {
                    break;
                }
                let Some(record) = crate::json::record_line(&line) else {
                    continue;
                };
                match writer.keep(record) {
                    Ok(()) => set(state, |state| {
                        state.records += 1;
                        state.last_seen = Some(now_s());
                        state.up = true;
                        state.error = None;
                        "following".clone_into(&mut state.state);
                        state.dir = writer.dir.clone();
                    }),
                    Err(error) => set(state, |state| {
                        state.error = Some(format!("the store: {error}"));
                    }),
                }
            }
            drop(watcher);
        }
        let _ = child.kill();
        let status = child.wait();
        let said = stderr
            .and_then(|thread| thread.join().ok())
            .unwrap_or_default();
        set(state, |state| {
            state.up = false;
            "waiting to connect again".clone_into(&mut state.state);
            let why = said
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .map(str::to_owned)
                .or_else(|| {
                    status
                        .ok()
                        .filter(|status| !status.success())
                        .map(|status| format!("ssh ended: {status}"))
                });
            if why.is_some() {
                state.error = why;
            }
        });
        wait(&stopped, 3);
    }
    set(state, |state| {
        state.up = false;
        "stopped".clone_into(&mut state.state);
    });
}

/// Wait `seconds`, or less once stopped.
fn wait(stopped: &impl Fn() -> bool, seconds: u64) {
    for _ in 0..seconds * 4 {
        if stopped() {
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Read what ssh says on its standard error, to show the last line.
fn spawn_stderr(stderr: std::process::ChildStderr) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut said = String::new();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            said.push_str(&line);
            said.push('\n');
            if said.len() > 16 * 1024 {
                said.drain(..8 * 1024);
            }
        }
        said
    })
}

/// End `child` once stopped, so a link that hears nothing still ends.
fn watch_stop(child: &Child, stop: Arc<Mutex<bool>>) -> std::thread::JoinHandle<()> {
    let pid = child.id();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(500));
            if stop.lock().map_or(true, |stop| *stop) {
                // The ssh is ours: its pid, not a pattern.
                let _ = Command::new("kill").arg(pid.to_string()).status();
                return;
            }
            if !Path::new(&format!("/proc/{pid}")).exists() {
                return;
            }
        }
    })
}

/// Records kept in a machine's store as lode writes one: a segment a boot,
/// appended to; the machine's `machine` file says its name and when it was
/// last heard.
#[derive(Debug)]
pub struct StoreWriter {
    /// The receiver's root.
    root: PathBuf,
    /// The machine's name in the settings, for `host-<name>` when its
    /// records name no host.
    name: String,
    /// The machine's store, once known.
    pub dir: Option<PathBuf>,
    /// The segment being written: its boot and path.
    open: Option<([u8; 16], PathBuf)>,
    /// The last record kept, to resume after.
    last: Option<(u64, [u8; 16])>,
}

impl StoreWriter {
    /// A writer into `root` for the machine `name`, which finds the store
    /// a link made before, to resume where it stopped.
    #[must_use]
    pub fn new(root: &Path, name: &str) -> Self {
        let mut writer = StoreWriter {
            root: root.to_path_buf(),
            name: name.to_owned(),
            dir: None,
            open: None,
            last: None,
        };
        let marker = root.join(format!(".viewer/{name}"));
        if let Ok(dir) = fs::read_to_string(&marker) {
            let dir = PathBuf::from(dir.trim());
            let (records, _) = crate::store::Store::at(std::slice::from_ref(&dir)).load();
            writer.last = records
                .iter()
                .max_by_key(|record| (record.realtime_us, record.seqnum))
                .map(|record| (record.seqnum, record.boot_id));
            writer.dir = Some(dir);
        }
        writer
    }

    /// Where to resume: the cursor of the last record kept.
    #[must_use]
    pub fn cursor(&self) -> Option<String> {
        self.last.map(|(seqnum, boot)| {
            let mut record = Record::new(6, 0, 0);
            record.seqnum = seqnum;
            record.boot_id = boot;
            ferrix_lode_core::cursor::of(&record)
        })
    }

    /// Keep `record`.
    ///
    /// # Errors
    ///
    /// A store that cannot be written.
    pub fn keep(&mut self, record: Record) -> std::io::Result<()> {
        if self.last == Some((record.seqnum, record.boot_id)) {
            return Ok(());
        }
        let dir = match &self.dir {
            Some(dir) => dir.clone(),
            None => {
                let dir = self.root.join(machine_dir(&record, &self.name));
                fs::create_dir_all(&dir)?;
                let marker = self.root.join(".viewer");
                fs::create_dir_all(&marker)?;
                fs::write(marker.join(&self.name), dir.to_string_lossy().as_bytes())?;
                self.dir = Some(dir.clone());
                dir
            }
        };
        let path = match &self.open {
            Some((boot, path)) if *boot == record.boot_id => path.clone(),
            _ => {
                let name = segment::file_name("system", &record.boot_id, record.seqnum);
                let path = dir.join(name);
                if !path.exists() {
                    let header = Header {
                        boot_id: record.boot_id,
                        machine_id: [0; 16],
                        first_seqnum: record.seqnum,
                        created_us: record.realtime_us,
                        host: record.host.to_string(),
                    };
                    fs::write(&path, header.encode())?;
                }
                self.open = Some((record.boot_id, path.clone()));
                path
            }
        };
        let mut bytes = Vec::new();
        segment::encode_record(&record, &mut bytes);
        fs::OpenOptions::new()
            .append(true)
            .open(&path)?
            .write_all(&bytes)?;
        self.last = Some((record.seqnum, record.boot_id));
        let host = if record.host.is_empty() {
            self.name.clone()
        } else {
            record.host.to_string()
        };
        fs::write(
            dir.join("machine"),
            format!("hostname={host}\nlast_seen={}\n", rfc3339(now_s())),
        )?;
        Ok(())
    }
}

/// The store's directory for the machine `record` came from: its machine
/// id, else `host-<hostname>`, else `host-<name>`.
fn machine_dir(record: &Record, name: &str) -> String {
    if let Some(id) = record
        .get_str("_MACHINE_ID")
        .filter(|id| id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return id.to_ascii_lowercase();
    }
    let host: String = record
        .host
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
        .collect();
    if host.is_empty() {
        format!("host-{name}")
    } else {
        format!("host-{host}")
    }
}

/// `seconds` since 1970 as RFC 3339, UTC.
fn rfc3339(seconds: u64) -> String {
    let civil = ferrix_lode_core::time::Civil::at(seconds * 1_000_000, 0);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        civil.year, civil.month, civil.day, civil.hour, civil.minute, civil.second
    )
}

#[cfg(test)]
mod tests {
    use super::{Kind, Remote, StoreWriter, follow_command, parse, write};
    use ferrix_lode_core::record::Record;
    use std::rc::Rc;

    fn n150() -> Remote {
        Remote {
            name: "n150".to_owned(),
            kind: Kind::Ssh,
            host: "192.0.2.10".to_owned(),
            user: "root".to_owned(),
            port: 2222,
            key: Some("~/.ssh/id_ed25519".to_owned()),
        }
    }

    #[test]
    fn the_machines_file_reads_back_and_refuses_what_is_unsafe() {
        let remotes = vec![n150()];
        assert_eq!(parse(&write(&remotes)), remotes);
        let bad = "[machine x]\nhost = a;rm -rf /\nport = 22\n[machine ok]\nhost = box\n";
        let read = parse(bad);
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].name, "ok");
        assert_eq!(read[0].port, 22);
        assert!(!write(&remotes).to_ascii_lowercase().contains("password ="));
    }

    #[test]
    fn ssh_follows_from_the_cursor_with_keys_only() {
        let command = n150().ssh(Some("i=2a;b=00"));
        let words: Vec<String> = command
            .get_args()
            .map(|word| word.to_string_lossy().into_owned())
            .collect();
        assert!(words.contains(&"BatchMode=yes".to_owned()));
        assert!(words.contains(&"IdentitiesOnly=yes".to_owned()));
        assert!(words.contains(&"root@192.0.2.10".to_owned()));
        assert!(words.iter().any(|word| word.ends_with("/.ssh/id_ed25519")));
        assert_eq!(
            words.last().map(String::as_str),
            Some("/bin/lode -f -o json -n 100000000 --after-cursor='i=2a;b=00'")
        );
        assert!(follow_command(None).ends_with(" -b"));
    }

    #[test]
    fn records_kept_make_a_store_that_a_new_link_resumes_from() {
        let root = std::env::temp_dir().join(format!("lode-viewer-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut writer = StoreWriter::new(&root, "n150");
        assert_eq!(writer.cursor(), None);
        for seq in 1..=3_u64 {
            let mut record = Record::new(6, seq * 1_000_000, seq);
            record.seqnum = seq;
            record.boot_id = [7; 16];
            record.host = Rc::from("ferrix");
            record.push("MESSAGE", format!("line {seq}").as_bytes());
            writer.keep(record.clone()).expect("kept");
            // A record sent twice across a reconnection is kept once.
            writer.keep(record).expect("kept");
        }
        let dir = root.join("host-ferrix");
        let (records, _) = crate::store::Store::at(std::slice::from_ref(&dir)).load();
        assert_eq!(records.len(), 3);
        let machines = crate::store::machines(&root);
        assert_eq!(machines.len(), 1);
        assert_eq!(machines[0].name, "ferrix");
        let again = StoreWriter::new(&root, "n150");
        assert_eq!(
            again.cursor().as_deref(),
            Some("i=3;b=07070707070707070707070707070707")
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
