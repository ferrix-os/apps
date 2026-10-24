//! The crashes, with no window: every record of a program the kernel
//! ended (lode's crash records, and the kernel's own `ended by signal`
//! lines in a store from before lode read them), and every error a program
//! showed without a crash (`FERRIX_EVENT=app-error`), grouped by program,
//! where it failed and how, newest first; and the screenshot lode kept of
//! each, when it kept one (`docs/LOGGING.md` §19).
//!
//! Since lode keeps heavy parts as blobs (§21.4), a screenshot is a
//! capture record's `BLOB_SCREENSHOT` (`<id> <size> qoi|png`), read from
//! the store's `blobs/` packs only when the crash is shown; an older
//! store's `CAPTURE_FILE` is still read. A crash's tombstone (§21) names
//! it by its cursor (`TOMBSTONE_OF`), and with it the process's last
//! lines (`TOMBSTONE_TAIL_FIRST`, `_LAST`); a crash storm's repeat record
//! says how many times a signature crashed (`TOMBSTONE_REPEATS`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ferrix_lode_core::blob::{self, Reference};
use ferrix_lode_core::tombstone::MESSAGE_ID_REPEAT;

use crate::data::{Entry, Kind, Log};

/// What ended it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CrashKind {
    /// The kernel ended it on a fault, with a signal.
    Signal,
    /// It showed an error and went on (or not): no signal.
    AppError,
}

/// One crash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crash {
    /// Its record, by index into the log.
    pub index: usize,
    /// When.
    pub time: u64,
    /// The program, or `pid N` where nothing names it.
    pub program: String,
    /// The process, where said.
    pub pid: Option<String>,
    /// The signal's name, or what an app error says it is.
    pub signal: String,
    /// Where it failed: the program counter, else the faulting address.
    pub address: String,
    /// The trap, as the kernel names it.
    pub trap: Option<String>,
    /// Signal or app error.
    pub kind: CrashKind,
    /// The record's sequence number, which a capture names.
    pub seqnum: u64,
    /// Its signature, as its tombstone groups it: the program, the
    /// module and offset of its pc, the signal.
    pub signature: Option<String>,
}

/// Crashes alike: one program, one place, one signal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// The program.
    pub program: String,
    /// Where it failed.
    pub address: String,
    /// How.
    pub signal: String,
    /// Signal or app error.
    pub kind: CrashKind,
    /// Its crashes, by index into the crashes, newest first.
    pub crashes: Vec<usize>,
    /// When the newest was.
    pub last: u64,
    /// How many times it crashed, as a crash storm's repeat record counts
    /// them when that says more than the crashes kept.
    pub repeats: u64,
}

/// A field's text.
fn field(entry: &Entry, name: &str) -> Option<String> {
    entry
        .record
        .get_str(name)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// The crash `entry` is, if it is one.
#[must_use]
pub fn crash_of(index: usize, entry: &Entry) -> Option<Crash> {
    let time = entry.time();
    let seqnum = entry.record.seqnum;
    if entry.record.get_str("FERRIX_EVENT") == Some("app-error") {
        let program = field(entry, "_COMM")
            .or_else(|| field(entry, "SYSLOG_IDENTIFIER"))
            .or_else(|| field(entry, "_EXE").map(|exe| base_name(&exe)))
            .unwrap_or_else(|| "a program".to_owned());
        return Some(Crash {
            index,
            time,
            program,
            pid: field(entry, "_PID"),
            signal: "error shown".to_owned(),
            address: field(entry, "APP_ERROR_TITLE")
                .or_else(|| field(entry, "WINDOW_TITLE"))
                .unwrap_or_else(|| entry.message().into_owned()),
            trap: None,
            kind: CrashKind::AppError,
            seqnum,
            signature: None,
        });
    }
    if entry.row.kind == Kind::Crash {
        let pid = field(entry, "CRASH_PID");
        let program = field(entry, "CRASH_COMM")
            .or_else(|| field(entry, "CRASH_PROGRAM").map(|path| base_name(&path)))
            .or_else(|| pid.as_ref().map(|pid| format!("pid {pid}")))
            .unwrap_or_else(|| "a program".to_owned());
        return Some(Crash {
            index,
            time,
            program,
            pid,
            signal: field(entry, "CRASH_SIGNAL").unwrap_or_else(|| "a signal".to_owned()),
            // Where in what: the module and offset its tombstone groups
            // by, else the pc, else the faulting address.
            address: field(entry, "CRASH_MODULE")
                .or_else(|| field(entry, "CRASH_PC"))
                .or_else(|| field(entry, "CRASH_ADDRESS"))
                .unwrap_or_else(|| "?".to_owned()),
            trap: field(entry, "CRASH_TRAP"),
            kind: CrashKind::Signal,
            seqnum,
            signature: Some(ferrix_lode_core::tombstone::signature(&entry.record)),
        });
    }
    // The kernel's own line, in a store from before lode read it.
    let said = ferrix_lode_core::console::crash(entry.record.message())?;
    Some(Crash {
        index,
        time,
        program: format!("pid {}", said.pid),
        pid: Some(said.pid.to_string()),
        signal: said.signal,
        address: said.pc.or(said.address).unwrap_or_else(|| "?".to_owned()),
        trap: said.trap,
        kind: CrashKind::Signal,
        seqnum,
        signature: None,
    })
}

/// The last part of a path.
fn base_name(path: &str) -> String {
    Path::new(path).file_name().map_or_else(
        || path.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// The crashes of boot `boot` of `log`, or of every boot; oldest first.
#[must_use]
pub fn crashes(log: &Log, boot: Option<usize>) -> Vec<Crash> {
    let (base, entries) = match boot {
        Some(boot) => (
            log.boots.get(boot).map_or(0, |boot| boot.range.start),
            log.of_boot(boot),
        ),
        None => (0, log.entries.as_slice()),
    };
    entries
        .iter()
        .enumerate()
        .filter_map(|(at, entry)| crash_of(base + at, entry))
        .collect()
}

/// `crashes` grouped, those of `program` and `signal` only when given;
/// the group crashed in last first.
#[must_use]
pub fn groups(crashes: &[Crash], program: Option<&str>, signal: Option<&str>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    let mut at: HashMap<(String, String, String, CrashKind), usize> = HashMap::new();
    for (index, crash) in crashes.iter().enumerate().rev() {
        if program.is_some_and(|wanted| wanted != crash.program)
            || signal.is_some_and(|wanted| wanted != crash.signal)
        {
            continue;
        }
        let key = (
            crash.program.clone(),
            crash.address.clone(),
            crash.signal.clone(),
            crash.kind,
        );
        match at.get(&key) {
            Some(&group) => {
                if let Some(group) = groups.get_mut(group) {
                    group.crashes.push(index);
                }
            }
            None => {
                let _ = at.insert(key, groups.len());
                groups.push(Group {
                    program: crash.program.clone(),
                    address: crash.address.clone(),
                    signal: crash.signal.clone(),
                    kind: crash.kind,
                    crashes: vec![index],
                    last: crash.time,
                    repeats: 0,
                });
            }
        }
    }
    groups.sort_by_key(|group| std::cmp::Reverse(group.last));
    for group in &mut groups {
        group.repeats = group.crashes.len() as u64;
    }
    groups
}

/// The most a crash storm's repeat records say each signature crashed,
/// in boot `boot` of `log` or every boot.
#[must_use]
pub fn storm_counts(log: &Log, boot: Option<usize>) -> HashMap<String, u64> {
    let entries = match boot {
        Some(boot) => log.of_boot(boot),
        None => log.entries.as_slice(),
    };
    let mut out: HashMap<String, u64> = HashMap::new();
    for entry in entries {
        if entry.record.get_str("MESSAGE_ID") != Some(MESSAGE_ID_REPEAT) {
            continue;
        }
        if let (Some(signature), Some(count)) = (
            field(entry, "TOMBSTONE_SIGNATURE"),
            field(entry, "TOMBSTONE_REPEATS").and_then(|n| n.parse::<u64>().ok()),
        ) {
            let slot = out.entry(signature).or_insert(0);
            *slot = (*slot).max(count);
        }
    }
    out
}

/// A record's cursor, as a tombstone names one.
#[must_use]
pub fn cursor_of(entry: &Entry) -> String {
    ferrix_lode_core::tombstone::cursor(entry.record.seqnum, &entry.record.boot_id)
}

/// The record `cursor` names, by index into the log.
#[must_use]
pub fn by_cursor(log: &Log, cursor: &str) -> Option<usize> {
    log.entries
        .iter()
        .position(|entry| cursor_of(entry) == cursor)
}

/// The tombstone of the crash at `index`: the record whose `TOMBSTONE_OF`
/// is its cursor.
#[must_use]
pub fn tombstone_of(log: &Log, index: usize) -> Option<usize> {
    let cursor = cursor_of(log.entries.get(index)?);
    log.entries
        .iter()
        .position(|entry| entry.record.get_str("TOMBSTONE_OF") == Some(cursor.as_str()))
}

/// What a record links to, for the record pane: the crash a tombstone or
/// a capture is of, or a crash's tombstone. `None` for any other.
#[must_use]
pub fn linked(log: &Log, index: usize) -> Option<(usize, &'static str)> {
    let entry = log.entries.get(index)?;
    if let Some(of) = entry.record.get_str("TOMBSTONE_OF") {
        return by_cursor(log, of).map(|at| (at, "Its crash on the Crashes page"));
    }
    if entry.record.get_str("MESSAGE_ID") == Some(MESSAGE_ID_REPEAT) {
        let last = entry.record.get_str("TOMBSTONE_LAST")?;
        return by_cursor(log, last).map(|at| (at, "Its latest crash on the Crashes page"));
    }
    if crash_of(index, entry).is_some() {
        return Some((index, "On the Crashes page"));
    }
    None
}

/// The first and last of the crashed process's lines its tombstone names,
/// by index.
#[must_use]
pub fn tail_of(log: &Log, tombstone: usize) -> Option<(usize, usize)> {
    let entry = log.entries.get(tombstone)?;
    let first = by_cursor(log, entry.record.get_str("TOMBSTONE_TAIL_FIRST")?)?;
    let last = by_cursor(log, entry.record.get_str("TOMBSTONE_TAIL_LAST")?)?;
    Some((first, last))
}

/// The programs and the signals among `crashes`, each once, sorted.
#[must_use]
pub fn choices(crashes: &[Crash]) -> (Vec<String>, Vec<String>) {
    let mut programs: Vec<String> = crashes.iter().map(|c| c.program.clone()).collect();
    let mut signals: Vec<String> = crashes.iter().map(|c| c.signal.clone()).collect();
    programs.sort();
    programs.dedup();
    signals.sort();
    signals.dedup();
    (programs, signals)
}

/// Where the screenshot of a crash is, as its capture record says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screenshot {
    /// None was kept.
    None,
    /// lode could not write it, in its words (`BLOB_DROPPED`).
    Dropped(String),
    /// A blob in the store's packs.
    Blob(Reference),
    /// A file of an older store (`CAPTURE_FILE`), found.
    File(PathBuf),
}

/// The screenshot lode kept of the record numbered `seqnum`, as its
/// capture record names it: a blob (`BLOB_SCREENSHOT`), why there is none
/// (`BLOB_DROPPED`), or in an older store a file (`CAPTURE_FILE`,
/// relative to the journal's directory, found under one of `dirs` or
/// their parents). Nothing is read yet.
#[must_use]
pub fn screenshot_of(log: &Log, seqnum: u64, dirs: &[PathBuf]) -> Screenshot {
    let wanted = seqnum.to_string();
    let Some(capture) = log
        .entries
        .iter()
        .find(|entry| entry.record.get_str("CAPTURE_OF") == Some(wanted.as_str()))
    else {
        return Screenshot::None;
    };
    if let Some(reference) = capture
        .record
        .get("BLOB_SCREENSHOT")
        .and_then(Reference::parse)
    {
        return Screenshot::Blob(reference);
    }
    if let Some(why) = field(capture, "BLOB_DROPPED") {
        return Screenshot::Dropped(why);
    }
    let Some(file) = field(capture, "CAPTURE_FILE") else {
        return Screenshot::None;
    };
    roots(dirs)
        .map(|dir| dir.join(&file))
        .find(|path| path.is_file())
        .map_or(Screenshot::None, Screenshot::File)
}

/// `dirs`, then their parents and grandparents: where a store's files may
/// be from what the viewer was given.
fn roots(dirs: &[PathBuf]) -> impl Iterator<Item = &Path> {
    dirs.iter()
        .flat_map(|dir| {
            [
                Some(dir.as_path()),
                dir.parent(),
                dir.parent().and_then(Path::parent),
            ]
        })
        .flatten()
}

/// The blob `id`'s bytes and type, from the packs in a `blobs/` directory
/// beside one of `dirs`: through a sealed pack's index, else by reading
/// the pack's entries. Checked against the id.
#[must_use]
pub fn read_blob(dirs: &[PathBuf], id: &blob::BlobId) -> Option<(String, Vec<u8>)> {
    for root in roots(dirs) {
        let Ok(listing) = std::fs::read_dir(root.join("blobs")) else {
            continue;
        };
        let mut packs: Vec<PathBuf> = listing
            .flatten()
            .map(|item| item.path())
            .filter(|path| path.extension().is_some_and(|e| e == "fxb"))
            .collect();
        packs.sort();
        for pack in packs {
            let offset = std::fs::read(pack.with_extension("idx"))
                .ok()
                .and_then(|bytes| blob::decode_index(&bytes))
                .and_then(|index| index.into_iter().find(|(each, _)| each == id))
                .map(|(_, offset)| offset);
            let Ok(bytes) = std::fs::read(&pack) else {
                continue;
            };
            let offset = offset.or_else(|| {
                blob::scan(&bytes)
                    .into_iter()
                    .find(|entry| entry.id == *id)
                    .map(|entry| entry.offset)
            });
            if let Some(found) = offset.and_then(|at| blob::read(&bytes, at)) {
                return Some(found);
            }
        }
    }
    None
}

/// The pixels of a screenshot: its width, height and RGBA, read only now.
#[must_use]
pub fn read_screenshot(shot: &Screenshot, dirs: &[PathBuf]) -> Option<(u32, u32, Vec<u8>)> {
    match shot {
        Screenshot::Blob(reference) => {
            let (kind, bytes) = read_blob(dirs, &reference.id)?;
            match kind.as_str() {
                "qoi" => ferrix_lode_capture::qoi::decode(&bytes),
                "png" => {
                    let pixmap = compositor_image::png(&bytes).ok()?;
                    let rgba = pixmap
                        .pixels()
                        .iter()
                        .flat_map(|p| {
                            let c = p.demultiply();
                            [c.red(), c.green(), c.blue(), c.alpha()]
                        })
                        .collect();
                    Some((pixmap.width(), pixmap.height(), rgba))
                }
                _ => None,
            }
        }
        Screenshot::File(path) => {
            let bytes = std::fs::read(path).ok()?;
            ferrix_lode_capture::qoi::decode(&bytes)
        }
        Screenshot::None | Screenshot::Dropped(_) => None,
    }
}
