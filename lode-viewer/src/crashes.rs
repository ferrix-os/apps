//! The crashes, with no window: every record of a program the kernel
//! ended (lode's crash records, and the kernel's own `ended by signal`
//! lines in a store from before lode read them), and every error a program
//! showed without a crash (`FERRIX_EVENT=app-error`), grouped by program,
//! where it failed and how, newest first; and the screenshot lode kept of
//! each, when it kept one (`docs/LOGGING.md` §19).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
            address: field(entry, "CRASH_PC")
                .or_else(|| field(entry, "CRASH_ADDRESS"))
                .unwrap_or_else(|| "?".to_owned()),
            trap: field(entry, "CRASH_TRAP"),
            kind: CrashKind::Signal,
            seqnum,
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
                });
            }
        }
    }
    groups.sort_by_key(|group| std::cmp::Reverse(group.last));
    groups
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

/// The screenshot lode kept of the record numbered `seqnum`, as its
/// capture record names it (`CAPTURE_OF`, `CAPTURE_FILE`, relative to the
/// journal's directory), found under one of `dirs` or their parents.
#[must_use]
pub fn capture_of(log: &Log, seqnum: u64, dirs: &[PathBuf]) -> Option<PathBuf> {
    let wanted = seqnum.to_string();
    let file = log.entries.iter().find_map(|entry| {
        (entry.record.get_str("CAPTURE_OF") == Some(wanted.as_str()))
            .then(|| entry.record.get_str("CAPTURE_FILE").map(str::to_owned))
            .flatten()
    })?;
    dirs.iter()
        .flat_map(|dir| {
            [
                Some(dir.as_path()),
                dir.parent(),
                dir.parent().and_then(Path::parent),
            ]
        })
        .flatten()
        .map(|dir| dir.join(&file))
        .find(|path| path.is_file())
}

/// The pixels of a capture lode kept (QOI): its width, height and RGBA.
#[must_use]
pub fn read_capture(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let bytes = std::fs::read(path).ok()?;
    ferrix_lode_capture::qoi::decode(&bytes)
}
