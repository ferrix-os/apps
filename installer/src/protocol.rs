//! The installer daemon's protocol, the client's side: a Unix stream
//! socket at `/run/ferrix-installer.sock` (`FERRIX_INSTALLER_SOCKET`
//! names another), one request a connection, answered in lines.
//!
//! ```text
//! disks                  min, live, then disk/part/free lines, then ok | error
//! install + key=value    step/progress/log lines, then done | error
//! reboot                 ok, and the machine restarts
//! ```
//!
//! Every line is UTF-8 and ends in `\n`; its fields are separated by one
//! space, and the last field of a `disk` or `part` line is the rest of
//! the line, spaces and all, or nothing.

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

/// Where the daemon listens.
pub const SOCKET: &str = "/run/ferrix-installer.sock";

/// The socket to ask: `FERRIX_INSTALLER_SOCKET`, else [`SOCKET`].
#[must_use]
pub fn socket(var: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    var("FERRIX_INSTALLER_SOCKET")
        .filter(|path| !path.is_empty())
        .map_or_else(|| PathBuf::from(SOCKET), PathBuf::from)
}

/// A sector, as the daemon counts them.
pub const SECTOR: u64 = 512;

/// A disk's partition table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Table {
    /// GUID partitions.
    Gpt,
    /// A DOS table.
    Mbr,
    /// None: the disk is blank, or holds a file system whole.
    None,
}

/// A partition on a disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    /// Its number on the disk.
    pub number: u32,
    /// Its first sector.
    pub first: u64,
    /// Its last sector.
    pub last: u64,
    /// Its size.
    pub bytes: u64,
    /// Its file system: fat, ntfs, ext4, btrfs, swap, unknown.
    pub fs: String,
    /// Its name or label; may be empty.
    pub name: String,
}

/// Unallocated space on a disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Free {
    /// Its first sector.
    pub first: u64,
    /// Its last sector.
    pub last: u64,
    /// Its size.
    pub bytes: u64,
}

/// A disk the daemon reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disk {
    /// Its device: `/dev/vde`.
    pub dev: String,
    /// Its size.
    pub bytes: u64,
    /// Its table.
    pub table: Table,
    /// Its model, as the device says it; may be empty.
    pub model: String,
    /// Its partitions, in the daemon's order.
    pub parts: Vec<Part>,
    /// Its free regions, in the daemon's order.
    pub free: Vec<Free>,
}

/// What `disks` answered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Disks {
    /// The smallest space Ferrix installs into.
    pub min: u64,
    /// The disk the live system runs from, never offered.
    pub live: Option<String>,
    /// Every disk, the live one too.
    pub disks: Vec<Disk>,
}

impl Disks {
    /// The disks a person may install on: every one but the live one.
    #[must_use]
    pub fn offered(&self) -> Vec<&Disk> {
        self.disks
            .iter()
            .filter(|disk| self.live.as_deref() != Some(disk.dev.as_str()))
            .collect()
    }
}

/// `text`'s first field and the rest after one space.
fn split(text: &str) -> (&str, &str) {
    text.split_once(' ').unwrap_or((text, ""))
}

/// A number field, or why not.
fn number(field: &str, line: &str) -> Result<u64, String> {
    field
        .parse()
        .map_err(|_| format!("the daemon said a number that is not one: {line}"))
}

/// Read a `disks` answer, given whole.
///
/// # Errors
///
/// The daemon's `error` line, or what could not be read, in words.
pub fn parse_disks(answer: &str) -> Result<Disks, String> {
    let mut disks = Disks::default();
    let mut ended = false;
    for line in answer.lines() {
        let (word, rest) = split(line);
        match word {
            "min" => disks.min = number(rest, line)?,
            "live" => disks.live = Some(rest.to_owned()).filter(|dev| !dev.is_empty()),
            "disk" => {
                let (dev, rest) = split(rest);
                let (bytes, rest) = split(rest);
                let (table, model) = split(rest);
                let table = match table {
                    "gpt" => Table::Gpt,
                    "mbr" => Table::Mbr,
                    "none" => Table::None,
                    _ => return Err(format!("the daemon said a table it has not: {line}")),
                };
                disks.disks.push(Disk {
                    dev: dev.to_owned(),
                    bytes: number(bytes, line)?,
                    table,
                    model: model.to_owned(),
                    parts: Vec::new(),
                    free: Vec::new(),
                });
            }
            "part" => {
                let (dev, rest) = split(rest);
                let (num, rest) = split(rest);
                let (first, rest) = split(rest);
                let (last, rest) = split(rest);
                let (bytes, rest) = split(rest);
                let (fs, name) = split(rest);
                let part = Part {
                    number: u32::try_from(number(num, line)?).map_err(|_| line.to_owned())?,
                    first: number(first, line)?,
                    last: number(last, line)?,
                    bytes: number(bytes, line)?,
                    fs: fs.to_owned(),
                    name: name.to_owned(),
                };
                disk_of(&mut disks, dev, line)?.parts.push(part);
            }
            "free" => {
                let fields: Vec<&str> = rest.split(' ').collect();
                let [dev, first, last, bytes] = fields.as_slice() else {
                    return Err(format!(
                        "the daemon said a free line of the wrong shape: {line}"
                    ));
                };
                let free = Free {
                    first: number(first, line)?,
                    last: number(last, line)?,
                    bytes: number(bytes, line)?,
                };
                disk_of(&mut disks, dev, line)?.free.push(free);
            }
            "ok" => {
                ended = true;
                break;
            }
            "error" => return Err(rest.to_owned()),
            "" => {}
            _ => return Err(format!("the daemon said a line it should not: {line}")),
        }
    }
    if !ended {
        return Err("the daemon stopped before it said ok".to_owned());
    }
    Ok(disks)
}

/// The disk `dev`, which a `part` or `free` line follows.
fn disk_of<'a>(disks: &'a mut Disks, dev: &str, line: &str) -> Result<&'a mut Disk, String> {
    disks
        .disks
        .iter_mut()
        .rev()
        .find(|disk| disk.dev == dev)
        .ok_or_else(|| format!("the daemon named a disk it had not listed: {line}"))
}

/// How the installer is to use the disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The whole disk, erased.
    Erase,
    /// A free region, its first and last sector.
    Free {
        /// The region's first sector.
        first: u64,
        /// The region's last sector.
        last: u64,
    },
    /// A partition, erased.
    Partition(u32),
}

/// An `install` request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Install {
    /// The disk.
    pub dev: String,
    /// Where on it.
    pub target: Target,
    /// The user's name to sign in with.
    pub user: String,
    /// Their full name.
    pub fullname: String,
    /// Their password.
    pub password: String,
    /// The computer's name.
    pub hostname: String,
    /// The xkb layout.
    pub keyboard: String,
    /// The xkb variant; may be empty.
    pub variant: String,
    /// The time zone: `Europe/Vienna`.
    pub timezone: String,
    /// The language: `en`.
    pub language: String,
}

impl Install {
    /// The request as it is written to the daemon: `install`, a
    /// `key=value` line each, an empty line. A value never holds a line's
    /// end: one is cut there.
    #[must_use]
    pub fn request(&self) -> String {
        let one = |value: &str| value.split('\n').next().unwrap_or("").to_owned();
        let mut pairs: Vec<(&str, String)> = vec![("target", self.dev.clone())];
        match self.target {
            Target::Erase => pairs.push(("mode", "erase".to_owned())),
            Target::Free { first, last } => {
                pairs.push(("mode", "free".to_owned()));
                pairs.push(("first", first.to_string()));
                pairs.push(("last", last.to_string()));
            }
            Target::Partition(number) => {
                pairs.push(("mode", "partition".to_owned()));
                pairs.push(("number", number.to_string()));
            }
        }
        for (key, value) in [
            ("user", &self.user),
            ("fullname", &self.fullname),
            ("password", &self.password),
            ("hostname", &self.hostname),
            ("keyboard", &self.keyboard),
            ("variant", &self.variant),
            ("timezone", &self.timezone),
            ("language", &self.language),
        ] {
            pairs.push((key, one(value)));
        }
        let mut text = "install\n".to_owned();
        for (key, value) in pairs {
            text.push_str(key);
            text.push('=');
            text.push_str(&value);
            text.push('\n');
        }
        text.push('\n');
        text
    }
}

/// One line of an install's stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Step `n` of `of` started, called so.
    Step(u32, u32, String),
    /// How far along the whole is, 0 to 100.
    Progress(u8),
    /// A line of the log.
    Log(String),
    /// It is installed: the last line.
    Done,
    /// It failed so: the last line.
    Error(String),
}

/// Read one line of an install's stream; `None` for a line it should not
/// say, which is kept in the log as it is.
#[must_use]
pub fn parse_event(line: &str) -> Option<Event> {
    let (word, rest) = split(line);
    match word {
        "step" => {
            let (n, rest) = split(rest);
            let (of, text) = split(rest);
            Some(Event::Step(
                n.parse().ok()?,
                of.parse().ok()?,
                text.to_owned(),
            ))
        }
        "progress" => Some(Event::Progress(rest.trim().parse::<u8>().ok()?.min(100))),
        "log" => Some(Event::Log(rest.to_owned())),
        "done" => Some(Event::Done),
        "error" => Some(Event::Error(rest.to_owned())),
        _ => None,
    }
}

/// Ask the daemon on `socket` for `request`, and read its whole answer.
fn ask(socket: &std::path::Path, request: &str) -> Result<String, String> {
    let mut stream = UnixStream::connect(socket).map_err(|e| {
        format!(
            "the installer service does not answer on {}: {e}",
            socket.display()
        )
    })?;
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("the request could not be sent: {e}"))?;
    let mut answer = String::new();
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => answer.push_str(&line),
            Err(e) => return Err(format!("the answer could not be read: {e}")),
        }
    }
    Ok(answer)
}

/// The disks, from the daemon on `socket`.
///
/// # Errors
///
/// The daemon could not be asked, or said an error, in words.
pub fn disks(socket: &std::path::Path) -> Result<Disks, String> {
    parse_disks(&ask(socket, "disks\n")?)
}

/// Ask the daemon on `socket` to restart the machine.
///
/// # Errors
///
/// The daemon could not be asked, or did not say `ok`.
pub fn reboot(socket: &std::path::Path) -> Result<(), String> {
    let answer = ask(socket, "reboot\n")?;
    match answer.lines().next() {
        Some("ok") => Ok(()),
        Some(line) => Err(split(line).1.to_owned()),
        None => Err("the service closed without an answer".to_owned()),
    }
}

/// Install as `install` says through the daemon on `socket`, handing
/// each line of its stream to `each` as it comes. A stream that ends
/// without `done` or `error` is an error.
pub fn install(socket: &std::path::Path, install: &Install, each: &mut dyn FnMut(Event)) {
    let mut stream = match UnixStream::connect(socket) {
        Ok(stream) => stream,
        Err(e) => {
            each(Event::Error(format!(
                "the installer service does not answer on {}: {e}",
                socket.display()
            )));
            return;
        }
    };
    if let Err(e) = stream.write_all(install.request().as_bytes()) {
        each(Event::Error(format!("the request could not be sent: {e}")));
        return;
    }
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {
                each(Event::Error(
                    "the installer service stopped before it finished".to_owned(),
                ));
                return;
            }
            Ok(_) => {
                let line = line.trim_end_matches('\n');
                let event = parse_event(line).unwrap_or_else(|| Event::Log(line.to_owned()));
                let last = matches!(event, Event::Done | Event::Error(_));
                each(event);
                if last {
                    return;
                }
            }
            Err(e) => {
                each(Event::Error(format!("the answer could not be read: {e}")));
                return;
            }
        }
    }
}

/// `bytes` as a person reads a size: `32 GiB`, `512 MiB`, `1.5 TiB`.
#[must_use]
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["bytes", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes < 1024 {
        return format!("{bytes} bytes");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    let name = UNITS.get(unit).copied().unwrap_or("bytes");
    if value >= 100.0 || (value - value.round()).abs() < 0.05 {
        format!("{} {name}", value.round())
    } else {
        format!("{value:.1} {name}")
    }
}

/// A file system's name as a person reads it.
#[must_use]
pub fn fs_name(fs: &str) -> &str {
    match fs {
        "fat" => "FAT",
        "ntfs" => "NTFS",
        "ext4" => "ext4",
        "btrfs" => "Btrfs",
        "swap" => "swap",
        _ => "unknown file system",
    }
}

#[cfg(test)]
mod tests {
    use super::{Event, Install, Table, Target, parse_disks, parse_event, size};

    const ANSWER: &str = "\
min 8589934592
live /dev/vda
disk /dev/vda 2147483648 none Ferrix live
disk /dev/vdb 34359738368 none
disk /dev/nvme0n1 274877906944 gpt Samsung SSD 970 EVO Plus
part /dev/nvme0n1 1 2048 1050623 536870912 fat EFI system partition
part /dev/nvme0n1 2 1050624 318767103 162671886336 ntfs
free /dev/nvme0n1 318767104 536868863 107373133824
ok
";

    #[test]
    fn a_disks_answer_reads_into_disks_with_their_parts_and_free_space() {
        let disks = parse_disks(ANSWER).expect("it reads");
        assert_eq!(disks.min, 8_589_934_592);
        assert_eq!(disks.live.as_deref(), Some("/dev/vda"));
        assert_eq!(disks.disks.len(), 3);
        let offered = disks.offered();
        assert_eq!(offered.len(), 2, "the live disk is not offered");
        let nvme = offered.get(1).expect("the nvme");
        assert_eq!(nvme.model, "Samsung SSD 970 EVO Plus");
        assert_eq!(nvme.table, Table::Gpt);
        assert_eq!(nvme.parts.len(), 2);
        assert_eq!(
            nvme.parts.first().map(|p| p.name.as_str()),
            Some("EFI system partition")
        );
        assert_eq!(
            nvme.parts.get(1).map(|p| p.name.as_str()),
            Some(""),
            "an empty name"
        );
        assert_eq!(nvme.free.first().map(|f| f.first), Some(318_767_104));
        assert_eq!(offered.first().map(|d| d.model.as_str()), Some(""));
    }

    #[test]
    fn an_error_or_a_cut_answer_is_an_error_in_words() {
        assert_eq!(
            parse_disks("min 1\nerror no disk can be read\n"),
            Err("no disk can be read".to_owned())
        );
        assert!(parse_disks("min 1\ndisk /dev/vdb 10 none\n").is_err());
        assert!(parse_disks("part /dev/vdz 1 1 2 3 fat x\nok\n").is_err());
        assert!(parse_disks("disk /dev/vdb ten none\nok\n").is_err());
    }

    #[test]
    fn an_install_request_names_the_target_and_ends_in_an_empty_line() {
        let install = Install {
            dev: "/dev/nvme0n1".to_owned(),
            target: Target::Free {
                first: 318_767_104,
                last: 536_868_863,
            },
            user: "ada".to_owned(),
            fullname: "Ada Lovelace".to_owned(),
            password: "pass word\nmode=erase".to_owned(),
            hostname: "ferrix".to_owned(),
            keyboard: "de".to_owned(),
            variant: String::new(),
            timezone: "Europe/Vienna".to_owned(),
            language: "en".to_owned(),
        };
        assert_eq!(
            install.request(),
            "install\ntarget=/dev/nvme0n1\nmode=free\nfirst=318767104\nlast=536868863\n\
             user=ada\nfullname=Ada Lovelace\npassword=pass word\nhostname=ferrix\n\
             keyboard=de\nvariant=\ntimezone=Europe/Vienna\nlanguage=en\n\n"
        );
        let erase = Install {
            target: Target::Erase,
            ..install.clone()
        };
        assert!(erase.request().contains("\nmode=erase\n"));
        let part = Install {
            target: Target::Partition(2),
            ..install
        };
        assert!(part.request().contains("\nmode=partition\nnumber=2\n"));
    }

    #[test]
    fn the_stream_reads_line_by_line() {
        assert_eq!(
            parse_event("step 2 of 7 Copying the system"),
            None,
            "of is a number"
        );
        assert_eq!(
            parse_event("step 2 7 Copying the system"),
            Some(Event::Step(2, 7, "Copying the system".to_owned()))
        );
        assert_eq!(parse_event("progress 140"), Some(Event::Progress(100)));
        assert_eq!(parse_event("log a b"), Some(Event::Log("a b".to_owned())));
        assert_eq!(parse_event("done"), Some(Event::Done));
        assert_eq!(
            parse_event("error disk full"),
            Some(Event::Error("disk full".to_owned()))
        );
    }

    #[test]
    fn sizes_read_as_a_person_says_them() {
        assert_eq!(size(34_359_738_368), "32 GiB");
        assert_eq!(size(536_870_912), "512 MiB");
        assert_eq!(size(107_373_133_824), "100 GiB");
        assert_eq!(size(1_610_612_736), "1.5 GiB");
        assert_eq!(size(512), "512 bytes");
    }
}
