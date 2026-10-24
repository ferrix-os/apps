//! The log as the viewer holds it: lode's records, each with what the
//! viewer reads off it once -- its level as shown, its source, its kind --
//! so a frame never parses a field.
//!
//! Records are kept in time order within each boot, boots oldest first, as
//! a person reads a log; lode's own order is its sequence number, which
//! the kernel's earlier lines, stored after lode started, do not follow.

use std::collections::HashMap;

use ferrix_lode_core::ingest::{
    MESSAGE_ID_CRASH, MESSAGE_ID_LOST, MESSAGE_ID_RESTARTED, MESSAGE_ID_STARTED, MESSAGE_ID_TORN,
    guess_level,
};
use ferrix_lode_core::limit::{MESSAGE_ID_REPEATED, MESSAGE_ID_RULE, MESSAGE_ID_SUPPRESSED};
use ferrix_lode_core::record::{LEVEL_SOURCE, Record, UNSPECIFIED};

/// How long the limiter's window is when a record does not say: lode's
/// default (`docs/LOGGING.md` §7.1).
pub const LIMIT_WINDOW_US: u64 = 30_000_000;

/// The level names, 0 to 7, then unspecified.
pub const LEVELS: [&str; 9] = [
    "emerg",
    "alert",
    "crit",
    "err",
    "warning",
    "notice",
    "info",
    "debug",
    "unspecified",
];

/// A level's name.
#[must_use]
pub fn level_name(level: u8) -> &'static str {
    LEVELS
        .get(usize::from(level))
        .copied()
        .unwrap_or("unspecified")
}

/// What a record is, beyond a line of text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A line someone wrote.
    Plain,
    /// lode's word that the limiter held lines back: how many.
    Held(u64),
    /// lode's fold of a repeated message: how many repeats.
    Folded(u64),
    /// lode's record of what an ingest rule dropped or sampled away.
    Rule(u64),
    /// lode started, restarted, lost kernel records or found a torn tail.
    Mark(Mark),
    /// A process ended by a signal.
    Crash,
}

/// The marks lode writes about itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Started for this boot.
    Started,
    /// Restarted within the boot.
    Restarted,
    /// Kernel records lost.
    Lost,
    /// A segment's torn tail.
    Torn,
}

/// What a display rule changed about a record's level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note {
    /// The level the store holds.
    pub stored: u8,
    /// The words that said otherwise.
    pub word: &'static str,
}

impl Entry {
    /// The level the store holds.
    #[must_use]
    pub fn stored_level(&self) -> u8 {
        self.row
            .note
            .map_or(self.record.priority, |note| note.stored)
    }

    /// The record as the store holds it.
    #[must_use]
    pub fn stored(&self) -> Record {
        let mut record = self.record.clone();
        record.priority = self.stored_level();
        record
    }
}

/// What the viewer reads off one record.
#[derive(Clone, Debug)]
pub struct Row {
    /// The level shown: the stored one, or a display rule's.
    pub level: u8,
    /// Set when a display rule changed the level.
    pub note: Option<Note>,
    /// The source's name, in [`Log::sources`].
    pub source: u16,
    /// `_TRANSPORT`, in [`Log::transports`].
    pub transport: u16,
    /// `SESSION_ID`, in [`Log::sessions`]; 0 for none.
    pub session: u16,
    /// What it is.
    pub kind: Kind,
    /// Its boot, in [`Log::boots`].
    pub boot: u16,
    /// The kernel's own sequence number, for a kernel record: read off
    /// its fields once, as it is added, not each time the boots are found.
    pub kernel_seq: Option<u64>,
}

/// One record and what is read off it.
#[derive(Clone, Debug)]
pub struct Entry {
    /// lode's record.
    pub record: Record,
    /// The viewer's reading.
    pub row: Row,
}

impl Entry {
    /// When it happened, in microseconds since 1970.
    #[must_use]
    pub fn time(&self) -> u64 {
        self.record.realtime_us
    }

    /// Its message, as text.
    #[must_use]
    pub fn message(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(self.record.message())
    }

    /// Whether it is an error or worse.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.row.level <= 3
    }

    /// Whether it is a warning.
    #[must_use]
    pub fn is_warning(&self) -> bool {
        self.row.level == 4
    }
}

/// A set of names, each kept once, looked up by number.
#[derive(Clone, Debug, Default)]
pub struct Names {
    /// The names, by number.
    pub names: Vec<String>,
    /// The numbers, by name.
    index: HashMap<String, u16>,
}

impl Names {
    /// The number of `name`, given one if it has none.
    pub fn intern(&mut self, name: &str) -> u16 {
        if let Some(&number) = self.index.get(name) {
            return number;
        }
        let number = u16::try_from(self.names.len()).unwrap_or(u16::MAX);
        if number == u16::MAX {
            return number;
        }
        self.names.push(name.to_owned());
        let _ = self.index.insert(name.to_owned(), number);
        number
    }

    /// The name numbered `number`.
    #[must_use]
    pub fn name(&self, number: u16) -> &str {
        self.names
            .get(usize::from(number))
            .map_or("?", String::as_str)
    }

    /// The number of `name`, if it is known.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<u16> {
        self.index.get(name).copied()
    }

    /// How many names there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// One boot.
#[derive(Clone, Debug)]
pub struct Boot {
    /// Its id.
    pub id: [u8; 16],
    /// Its machine's name.
    pub host: String,
    /// Its first and last record's time.
    pub first_us: u64,
    /// The last.
    pub last_us: u64,
    /// Where its records are in [`Log::entries`].
    pub range: std::ops::Range<usize>,
    /// Its lowest and highest kernel sequence numbers.
    pub kernel_seqs: Option<(u64, u64)>,
    /// How many records it holds in the store: its range's length, or, for
    /// a boot read a page at a time, the summaries' count.
    pub records: u64,
}

/// A boot the store holds whose records are not all read: what its
/// segments' headers and summaries say of it (`crate::paged`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnownBoot {
    /// Its id.
    pub id: [u8; 16],
    /// Its machine's name.
    pub host: String,
    /// Its first time.
    pub first_us: u64,
    /// Its last time.
    pub last_us: u64,
    /// How many records it holds.
    pub records: u64,
}

impl Boot {
    /// Its length in microseconds, at least one second.
    #[must_use]
    pub fn span(&self) -> u64 {
        self.last_us.saturating_sub(self.first_us).max(1_000_000)
    }

    /// Its id in hex.
    #[must_use]
    pub fn hex(&self) -> String {
        let mut text = String::new();
        ferrix_lode_core::segment::push_hex(&mut text, &self.id);
        text
    }
}

/// Everything the viewer holds.
#[derive(Clone, Debug, Default)]
pub struct Log {
    /// The records, boots oldest first, each boot's in time order.
    pub entries: Vec<Entry>,
    /// The boots, oldest first.
    pub boots: Vec<Boot>,
    /// The sources' names.
    pub sources: Names,
    /// The transports' names.
    pub transports: Names,
    /// The sessions' ids; the first is the empty one, for none.
    pub sessions: Names,
    /// The highest sequence number held, for following.
    pub last_seq: u64,
    /// Boots the store holds beyond the records read, for a log read a
    /// page at a time: each is a boot of [`Log::boots`] whether or not
    /// any of its records are, with its whole extent and count.
    pub known: Vec<KnownBoot>,
    /// Records let go and not yet freed: freeing a page's thousands of
    /// fields at once cost a frame, so they go a few at a time
    /// ([`Log::sweep`]).
    pub trash: Vec<Entry>,
}

/// The name a record's source goes by: who wrote it, as lode's own
/// `levels` and `top` name it.
#[must_use]
pub fn source_of(record: &Record) -> String {
    for name in [
        "SYSLOG_IDENTIFIER",
        "UNIT",
        "_SYSTEMD_UNIT",
        "_FERRIX_DRIVER",
        "FERRIX_SUBSYSTEM",
        "_TRANSPORT",
    ] {
        if let Some(value) = record.get_str(name)
            && !value.is_empty()
        {
            return value.to_owned();
        }
    }
    "?".to_owned()
}

/// The level shown for `record`, and the note when a display rule changed
/// it: a record the store holds as info or unspecified, whose level nobody
/// stated or derived, is shown at the level its words say, by lode's own
/// reading of them (`ferrix_lode_core::ingest::guess_level`). Records from
/// a lode that reads levels itself already say where theirs came from, and
/// are shown as stored.
#[must_use]
pub fn shown_level(record: &Record) -> (u8, Option<Note>) {
    let said = record.get_str(LEVEL_SOURCE);
    if matches!(said, Some("stated" | "kernel" | "derived")) {
        return (record.priority, None);
    }
    if record.priority != 6 && record.priority != UNSPECIFIED {
        return (record.priority, None);
    }
    // lode's own records say what they mean; a fold of repeats is the
    // repeated line's, and read as it is.
    if record
        .get_str("MESSAGE_ID")
        .is_some_and(|id| id != MESSAGE_ID_REPEATED)
    {
        return (record.priority, None);
    }
    let kernel = record.get_str("_TRANSPORT") == Some("kernel")
        && record
            .get_str("SYSLOG_IDENTIFIER")
            .is_none_or(|id| id == "kernel");
    match guess_level(record.message(), kernel) {
        Some((level, word)) if level < record.priority => (
            level,
            Some(Note {
                stored: record.priority,
                word,
            }),
        ),
        _ => (record.priority, None),
    }
}

/// What kind of record `record` is.
#[must_use]
pub fn kind_of(record: &Record) -> Kind {
    let count = |name: &str| {
        record
            .get_str(name)
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0)
    };
    match record.get_str("MESSAGE_ID") {
        Some(MESSAGE_ID_SUPPRESSED) => Kind::Held(count("N_SUPPRESSED")),
        Some(MESSAGE_ID_REPEATED) => Kind::Folded(count("REPEATED")),
        Some(MESSAGE_ID_RULE) => Kind::Rule(count("N_DROPPED")),
        Some(MESSAGE_ID_STARTED) => Kind::Mark(Mark::Started),
        Some(MESSAGE_ID_RESTARTED) => Kind::Mark(Mark::Restarted),
        Some(MESSAGE_ID_LOST) => Kind::Mark(Mark::Lost),
        Some(MESSAGE_ID_TORN) => Kind::Mark(Mark::Torn),
        Some(MESSAGE_ID_CRASH) => Kind::Crash,
        _ if record.get("CRASH_SIGNAL").is_some() => Kind::Crash,
        _ if record.get("N_SUPPRESSED").is_some() => Kind::Held(count("N_SUPPRESSED")),
        _ if record.get("REPEATED").is_some() => Kind::Folded(count("REPEATED")),
        _ => Kind::Plain,
    }
}

impl Log {
    /// A log of `records`.
    #[must_use]
    pub fn new(records: Vec<Record>) -> Self {
        let mut log = Log::default();
        let _ = log.sessions.intern("");
        log.add(records);
        log
    }

    /// Add `records`, which may be of any boot and in any order; a record
    /// whose sequence number and boot are held already is not added twice.
    /// Answers how many were new.
    pub fn add(&mut self, records: Vec<Record>) -> usize {
        if self.sessions.is_empty() {
            let _ = self.sessions.intern("");
        }
        // Only what is held within the new records' sequence numbers can
        // be one of them again: a page read before the window's oldest
        // meets none, and costs no set of the whole window.
        let low = records
            .iter()
            .map(|record| record.seqnum)
            .min()
            .unwrap_or(0);
        let high = records
            .iter()
            .map(|record| record.seqnum)
            .max()
            .unwrap_or(0);
        let known: std::collections::HashSet<(u64, [u8; 16])> = if low <= self.last_seq {
            self.entries
                .iter()
                .filter(|entry| (low..=high).contains(&entry.record.seqnum))
                .map(|entry| (entry.record.seqnum, entry.record.boot_id))
                .collect()
        } else {
            std::collections::HashSet::new()
        };
        let mut fresh: Vec<Entry> = Vec::with_capacity(records.len());
        for record in records {
            if known.contains(&(record.seqnum, record.boot_id)) {
                continue;
            }
            self.last_seq = self.last_seq.max(record.seqnum);
            let (level, note) = shown_level(&record);
            // The record is held at the level it is shown at, so lode's
            // filter language (`priority <= warning`) and the facets agree
            // with what the table shows; the stored level is the note's.
            let mut record = record;
            record.priority = level;
            let source = self.sources.intern(&source_of(&record));
            let transport = self
                .transports
                .intern(record.get_str("_TRANSPORT").unwrap_or("?"));
            let session = self
                .sessions
                .intern(record.get_str("SESSION_ID").unwrap_or(""));
            let kind = kind_of(&record);
            let kernel_seq = ferrix_lode_core::ingest::kernel_seq(&record);
            fresh.push(Entry {
                row: Row {
                    level,
                    note,
                    source,
                    transport,
                    session,
                    kind,
                    boot: 0,
                    kernel_seq,
                },
                record,
            });
        }
        let added = fresh.len();
        if added > 0 {
            self.place(fresh);
        }
        added
    }

    /// Put `fresh` among the entries in order. A page of a long boot lies
    /// wholly before or after the entries held: sorted alone and put at
    /// that end, which costs no sort of the window. Anything else is
    /// sorted with the rest.
    fn place(&mut self, mut fresh: Vec<Entry>) {
        let firsts = self.firsts(&fresh);
        let key = |entry: &Entry| {
            (
                firsts.get(&entry.record.boot_id).copied().unwrap_or(0),
                entry.record.boot_id,
                entry.record.realtime_us,
                entry.record.seqnum,
            )
        };
        fresh.sort_by_cached_key(key);
        let (Some(new_first), Some(new_last)) = (fresh.first(), fresh.last()) else {
            return;
        };
        match (self.entries.first(), self.entries.last()) {
            (Some(old_first), _) if key(new_last) < key(old_first) => {
                let _ = self.entries.splice(0..0, fresh);
                self.find_boots();
            }
            (_, Some(old_last)) if key(new_first) > key(old_last) => {
                self.entries.append(&mut fresh);
                self.find_boots();
            }
            _ => {
                self.entries.append(&mut fresh);
                self.order();
            }
        }
    }

    /// Each boot's first time, which orders the boots: the index's where
    /// it knows the boot, else the earliest of the entries held and
    /// `fresh`.
    fn firsts(&self, fresh: &[Entry]) -> HashMap<[u8; 16], u64> {
        let mut firsts: HashMap<[u8; 16], u64> = self
            .known
            .iter()
            .map(|known| (known.id, known.first_us))
            .collect();
        let known = firsts.len();
        for entry in self.entries.iter().chain(fresh) {
            if known > 0 && firsts.contains_key(&entry.record.boot_id) {
                continue;
            }
            let first = firsts.entry(entry.record.boot_id).or_insert(u64::MAX);
            *first = (*first).min(entry.record.realtime_us);
        }
        firsts
    }

    /// Put the entries in order and find the boots again.
    fn order(&mut self) {
        // Each boot's first time decides the boots' order.
        let firsts = self.firsts(&[]);
        // Each entry's key made once, not in every comparison: a window of
        // a long boot is sorted again each time a page comes.
        self.entries.sort_by_cached_key(|entry| {
            (
                firsts.get(&entry.record.boot_id).copied().unwrap_or(0),
                entry.record.boot_id,
                entry.record.realtime_us,
                entry.record.seqnum,
            )
        });
        self.find_boots();
    }

    /// Find the boots in the entries, which are in order.
    fn find_boots(&mut self) {
        self.boots.clear();
        // The entries are in boot order: each boot is one run of them.
        let mut boot = 0_u16;
        let mut index = 0;
        for chunk in self
            .entries
            .chunk_by_mut(|a, b| a.record.boot_id == b.record.boot_id)
        {
            let mut kernel_seqs: Option<(u64, u64)> = None;
            for entry in chunk.iter_mut() {
                entry.row.boot = boot;
                if let Some(seq) = entry.row.kernel_seq {
                    kernel_seqs = Some(match kernel_seqs {
                        Some((low, high)) => (low.min(seq), high.max(seq)),
                        None => (seq, seq),
                    });
                }
            }
            if let (Some(first), Some(last)) = (chunk.first(), chunk.last()) {
                self.boots.push(Boot {
                    id: first.record.boot_id,
                    host: first.record.host.to_string(),
                    first_us: first.record.realtime_us,
                    last_us: last.record.realtime_us,
                    range: index..index + chunk.len(),
                    kernel_seqs,
                    records: chunk.len() as u64,
                });
            }
            index += chunk.len();
            boot = boot.saturating_add(1);
        }
        if !self.known.is_empty() {
            self.complete_boots();
        }
    }

    /// Put the boots known from the store's index among those read: a
    /// boot read in part takes its whole extent and count from the index,
    /// a boot none of whose records are read is one with no entries.
    fn complete_boots(&mut self) {
        for known in &self.known {
            match self.boots.iter_mut().find(|boot| boot.id == known.id) {
                Some(boot) => {
                    boot.first_us = boot.first_us.min(known.first_us);
                    boot.last_us = boot.last_us.max(known.last_us);
                    boot.records = boot.records.max(known.records);
                }
                None => self.boots.push(Boot {
                    id: known.id,
                    host: known.host.clone(),
                    first_us: known.first_us,
                    last_us: known.last_us,
                    range: 0..0,
                    kernel_seqs: None,
                    records: known.records,
                }),
            }
        }
        self.boots.sort_by_key(|boot| boot.first_us);
        // A boot with no entries lies where the entries of the boots before
        // it end; the entries' boot numbers follow the new order.
        let mut end = 0;
        for (index, boot) in self.boots.iter_mut().enumerate() {
            if boot.range.is_empty() {
                boot.range = end..end;
            }
            end = boot.range.end;
            let number = u16::try_from(index).unwrap_or(u16::MAX);
            if let Some(entries) = self.entries.get_mut(boot.range.clone()) {
                for entry in entries {
                    entry.row.boot = number;
                }
            }
        }
    }

    /// Let go of boot `id`'s records with sequence numbers `low..=high`:
    /// a page of a log read a page at a time, out of the window.
    pub fn remove(&mut self, id: [u8; 16], (low, high): (u64, u64)) {
        let gone = self.entries.extract_if(.., |entry| {
            entry.record.boot_id == id && (low..=high).contains(&entry.record.seqnum)
        });
        self.trash.extend(gone);
        // What is left is in order still.
        self.find_boots();
    }

    /// Free at most `most` of the records let go.
    pub fn sweep(&mut self, most: usize) {
        let keep = self.trash.len().saturating_sub(most);
        self.trash.truncate(keep);
    }

    /// Let go of every record of boot `id`.
    pub fn clear_boot(&mut self, id: [u8; 16]) {
        self.entries.retain(|entry| entry.record.boot_id != id);
        self.find_boots();
    }

    /// The boot lode calls `offset`: 0 the last, -1 the one before.
    #[must_use]
    pub fn boot_at(&self, offset: i64) -> Option<usize> {
        let count = i64::try_from(self.boots.len()).ok()?;
        usize::try_from(count - 1 + offset).ok()
    }

    /// The offset lode calls boot `index` by.
    #[must_use]
    pub fn offset_of(&self, index: usize) -> i64 {
        i64::try_from(index).unwrap_or(0) - i64::try_from(self.boots.len()).unwrap_or(0) + 1
    }

    /// Boot `index`'s entries.
    #[must_use]
    pub fn of_boot(&self, index: usize) -> &[Entry] {
        self.boots
            .get(index)
            .and_then(|boot| self.entries.get(boot.range.clone()))
            .unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::{Kind, Log, shown_level};
    use ferrix_lode_core::record::Record;

    fn record(seq: u64, at_s: u64, id: &str, message: &str) -> Record {
        let mut record = Record::new(6, at_s * 1_000_000, at_s * 1_000_000);
        record.seqnum = seq;
        record.push("SYSLOG_IDENTIFIER", id.as_bytes());
        record.push("_TRANSPORT", b"console");
        record.push("MESSAGE", message.as_bytes());
        record
    }

    #[test]
    fn a_line_stored_as_info_whose_words_say_warning_is_shown_as_one() {
        let (level, note) =
            shown_level(&record(1, 1, "hyprix", "output WARNING: transmit refused"));
        assert_eq!(level, 4);
        assert_eq!(note.map(|note| note.stored), Some(6));
        let (level, note) = shown_level(&record(1, 1, "hyprix", "frame 2201 presented"));
        assert_eq!((level, note), (6, None));
        // A level lode stated is never second-guessed.
        let mut stated = record(1, 1, "hyprix", "could not open card0");
        stated.push("FERRIX_PRIORITY_SOURCE", b"stated");
        assert_eq!(shown_level(&stated), (6, None));
    }

    #[test]
    fn records_are_in_time_order_by_boot_and_a_repeat_is_kept_once() {
        let mut later = record(5, 100, "b", "late boot");
        later.boot_id = [2; 16];
        let mut log = Log::new(vec![
            later,
            record(2, 20, "a", "second"),
            record(1, 10, "a", "first"),
        ]);
        assert_eq!(log.boots.len(), 2);
        assert_eq!(log.entries[0].message(), "first");
        assert_eq!(log.boot_at(0), Some(1));
        assert_eq!(log.boot_at(-1), Some(0));
        assert_eq!(log.offset_of(0), -1);
        assert_eq!(log.add(vec![record(2, 20, "a", "second")]), 0);
        assert_eq!(log.add(vec![record(3, 30, "a", "third")]), 1);
        assert_eq!(log.of_boot(0).len(), 3);
    }

    #[test]
    fn lodes_own_records_have_their_kinds() {
        let mut held = record(9, 1, "lode", "Suppressed 6124 message(s)");
        held.push(
            "MESSAGE_ID",
            ferrix_lode_core::limit::MESSAGE_ID_SUPPRESSED.as_bytes(),
        );
        held.push("N_SUPPRESSED", b"6124");
        assert_eq!(super::kind_of(&held), Kind::Held(6124));
        let mut folded = record(9, 1, "hyprix", "message repeated 41 time(s): x");
        folded.push("REPEATED", b"41");
        assert_eq!(super::kind_of(&folded), Kind::Folded(41));
    }
}
