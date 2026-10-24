//! A large store read a page at a time (`docs/LOGGING.md`, "The viewer's
//! scrollback"): only the segments' headers and summaries on opening; then
//! the boot shown, from its newest record back, in pages of [`PAGE`]
//! records, at most [`WINDOW_PAGES`] of them held at once.
//!
//! A boot is one run of segments per class (`system`, a user's), each read
//! backwards with lode-core's `segment::entries_before`; pages of several
//! classes are merged by sequence number. A page found is remembered by
//! where its entries lie -- a few spans of byte offsets -- so a page let go
//! is read again exactly, forwards, when it is wanted back. Only the bytes
//! of the last few segments touched are kept.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use ferrix_lode_core::compress;
use ferrix_lode_core::record::Record;
use ferrix_lode_core::segment::{self, Found, Header};

use crate::store::{Segment, Store};

/// Records a page holds at most.
pub const PAGE: usize = 4096;

/// Pages held at once at most: 65,536 records.
pub const WINDOW_PAGES: usize = 16;

/// A store with at most this many records is read whole, as it always was:
/// paging is for logs too long to hold.
pub const WHOLE_UP_TO: u64 = 200_000;

/// How many segments' bytes are kept.
const KEPT_SEGMENTS: usize = 4;

/// A boot of the store, from the segments' headers and summaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootInfo {
    /// Its id.
    pub id: [u8; 16],
    /// The host that wrote it.
    pub host: String,
    /// Its segments, oldest first, as indexes into the pager's.
    pub segments: Vec<usize>,
    /// Its first time, in microseconds.
    pub first_us: u64,
    /// Its last time: the newest sealed segment's, or the newest header's
    /// for a boot still being written, until its records are read.
    pub last_us: u64,
    /// How many records it holds: the summaries' counts, and an estimate
    /// from its size for a segment with none.
    pub records: u64,
    /// Its lowest sequence number.
    pub first_seq: u64,
}

/// Where some of a page's entries lie in one segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    /// The segment.
    segment: usize,
    /// The first entry's first byte.
    start: usize,
    /// One past the last entry's last byte.
    end: usize,
}

/// A page found.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Page {
    /// Where its entries lie.
    spans: Vec<Span>,
    /// How many records.
    count: usize,
    /// Its lowest and highest sequence numbers.
    seqs: (u64, u64),
    /// Its earliest and latest times.
    times: (u64, u64),
}

/// One class's segments of the boot, read backwards.
#[derive(Clone, Debug, Default)]
struct Chain {
    /// The segments, oldest first.
    segments: Vec<usize>,
    /// The segment being read (an index into `segments`), and where the
    /// next read back ends; `None` when it is read to its start.
    at: Option<(usize, usize)>,
    /// Entries read but not yet put in a page, oldest first.
    buffer: Vec<(usize, Found)>,
}

/// What a step of the window changed.
#[derive(Debug, Default)]
pub struct Moved {
    /// The records that came in.
    pub added: Vec<Record>,
    /// The sequence numbers of the records let go, lowest and highest, of
    /// the boot shown.
    pub dropped: Option<(u64, u64)>,
}

/// The store read a page at a time.
#[derive(Debug)]
pub struct Pager {
    /// Every segment, oldest first.
    segments: Vec<Segment>,
    /// The boots, oldest first.
    boots: Vec<BootInfo>,
    /// The boot shown.
    boot: usize,
    /// Its classes' chains.
    chains: Vec<Chain>,
    /// Its pages found so far, newest first.
    pages: Vec<Page>,
    /// The pages held: `pages[held.0..held.1]`.
    held: (usize, usize),
    /// The bytes of the segments last touched, most recent last.
    bytes: Vec<(usize, Rc<Vec<u8>>)>,
}

impl Pager {
    /// The store's segments and boots, read from headers and summaries
    /// only. Answers the pager and the directories refused.
    #[must_use]
    pub fn open(store: &Store) -> (Self, Vec<String>) {
        let (segments, refused) = store.segments();
        let mut boots: Vec<BootInfo> = Vec::new();
        for (index, each) in segments.iter().enumerate() {
            let (first_us, last_us, records) = match &each.summary {
                Some(summary) => (summary.times.0, summary.times.1, summary.records),
                // A segment being written: its header's time, and its size
                // over a record's usual size, until it is read.
                None => (
                    each.header.created_us,
                    each.header.created_us,
                    each.size / 160,
                ),
            };
            match boots.iter_mut().find(|boot| boot.id == each.header.boot_id) {
                Some(boot) => {
                    boot.first_seq = boot.first_seq.min(each.first);
                    boot.segments.push(index);
                    boot.first_us = boot.first_us.min(first_us);
                    boot.last_us = boot.last_us.max(last_us);
                    boot.records += records;
                }
                None => boots.push(BootInfo {
                    id: each.header.boot_id,
                    host: each.header.host.clone(),
                    segments: vec![index],
                    first_us,
                    last_us,
                    records,
                    first_seq: each.first,
                }),
            }
        }
        boots.sort_by_key(|boot| boot.first_us);
        let pager = Pager {
            segments,
            boots,
            boot: 0,
            chains: Vec::new(),
            pages: Vec::new(),
            held: (0, 0),
            bytes: Vec::new(),
        };
        (pager, refused)
    }

    /// The boots, oldest first.
    #[must_use]
    pub fn boots(&self) -> &[BootInfo] {
        &self.boots
    }

    /// How many records the store holds, as its summaries count them.
    #[must_use]
    pub fn records(&self) -> u64 {
        self.boots.iter().map(|boot| boot.records).sum()
    }

    /// The boot shown.
    #[must_use]
    pub fn boot(&self) -> usize {
        self.boot
    }

    /// Show boot `index` from its newest records: the newest pages, up to
    /// a window's. Answers its records.
    pub fn start(&mut self, index: usize) -> Vec<Record> {
        self.boot = index.min(self.boots.len().saturating_sub(1));
        self.pages.clear();
        self.held = (0, 0);
        self.chains = self.chains_of(self.boot);
        let mut records = Vec::new();
        while self.held.1 < WINDOW_PAGES {
            let Some(page) = self.find_older() else {
                break;
            };
            records.extend(page);
            self.held.1 += 1;
        }
        records
    }

    /// Show the page around sequence number `seq` of the boot shown, half
    /// a window either side: what a jump to a record loads. The pages on
    /// the way there are found, not kept.
    pub fn jump(&mut self, seq: u64) -> Vec<Record> {
        self.pages.clear();
        self.held = (0, 0);
        self.chains = self.chains_of(self.boot);
        let mut target = None;
        while target.is_none() {
            if self.find_older().is_none() {
                break;
            }
            let at = self.pages.len() - 1;
            if self.pages.get(at).is_some_and(|page| page.seqs.0 <= seq) {
                target = Some(at);
            }
        }
        let target = target.unwrap_or(self.pages.len().saturating_sub(1));
        let from = target.saturating_sub(WINDOW_PAGES / 2);
        // Older pages past the target, to fill the window's other half.
        while self.pages.len() < target + WINDOW_PAGES / 2 && self.find_older().is_some() {}
        let to = (from + WINDOW_PAGES).min(self.pages.len());
        // Near the boot's start the window still holds a window's worth.
        let from = from.min(to.saturating_sub(WINDOW_PAGES));
        self.held = (from, to);
        (from..to).flat_map(|page| self.read_page(page)).collect()
    }

    /// Show the page holding the boot's records around time `at`, as
    /// [`Pager::jump`] does for a sequence number.
    pub fn jump_to_time(&mut self, at: u64) -> Vec<Record> {
        self.pages.clear();
        self.held = (0, 0);
        self.chains = self.chains_of(self.boot);
        while self.find_older().is_some() {
            if self.pages.last().is_some_and(|page| page.times.0 <= at) {
                break;
            }
        }
        let seq = self.pages.last().map_or(0, |page| page.seqs.0);
        self.jump(seq)
    }

    /// Whether the window holds the boot's newest records.
    #[must_use]
    pub fn at_newest(&self) -> bool {
        self.held.0 == 0
    }

    /// How many records of the boot are newer than the window.
    #[must_use]
    pub fn newer(&self) -> usize {
        self.pages
            .get(..self.held.0)
            .map_or(0, |pages| pages.iter().map(|page| page.count).sum())
    }

    /// How many records of the boot are older than the window: those of
    /// pages found past it, and the rest as the summaries count them.
    #[must_use]
    pub fn older(&self) -> usize {
        let found: usize = self.pages.iter().map(|page| page.count).sum();
        let past: usize = self
            .pages
            .get(self.held.1..)
            .map_or(0, |pages| pages.iter().map(|page| page.count).sum());
        if self.exhausted() {
            return past;
        }
        let total = self.boots.get(self.boot).map_or(0, |boot| {
            usize::try_from(boot.records).unwrap_or(usize::MAX)
        });
        past + total.saturating_sub(found)
    }

    /// Whether every record of the boot has been found.
    fn exhausted(&self) -> bool {
        self.chains
            .iter()
            .all(|chain| chain.at.is_none() && chain.buffer.is_empty())
    }

    /// The next older page into the window, letting the newest go when the
    /// window is full. `None` when there is none.
    pub fn step_older(&mut self) -> Option<Moved> {
        let added = if self.held.1 < self.pages.len() {
            self.read_page(self.held.1)
        } else {
            self.find_older()?
        };
        self.held.1 += 1;
        let mut moved = Moved {
            added,
            dropped: None,
        };
        if self.held.1 - self.held.0 > WINDOW_PAGES {
            moved.dropped = self.pages.get(self.held.0).map(|page| page.seqs);
            self.held.0 += 1;
        }
        Some(moved)
    }

    /// The next newer page into the window, letting the oldest go when the
    /// window is full. `None` at the newest.
    pub fn step_newer(&mut self) -> Option<Moved> {
        if self.held.0 == 0 {
            return None;
        }
        self.held.0 -= 1;
        let added = self.read_page(self.held.0);
        let mut moved = Moved {
            added,
            dropped: None,
        };
        if self.held.1 - self.held.0 > WINDOW_PAGES {
            self.held.1 -= 1;
            moved.dropped = self.pages.get(self.held.1).map(|page| page.seqs);
        }
        Some(moved)
    }

    /// Let the window's oldest page go, when it holds more than one: the
    /// window grows at its newest end while following. Answers the
    /// sequence numbers let go.
    pub fn trim_oldest(&mut self) -> Option<(u64, u64)> {
        if self.held.1 - self.held.0 <= 1 {
            return None;
        }
        self.held.1 -= 1;
        self.pages.get(self.held.1).map(|page| page.seqs)
    }

    /// Records came while following, at the boot's end, which the window
    /// holds: they are part of its newest page from now on, for counting.
    pub fn followed(&mut self, count: usize) {
        if let Some(page) = self.pages.first_mut() {
            page.count += count;
        }
        if let Some(boot) = self.boots.get_mut(self.boot) {
            boot.records += count as u64;
        }
    }

    /// The chains of boot `index`: its segments by class, each read from
    /// its newest segment's end.
    fn chains_of(&self, index: usize) -> Vec<Chain> {
        let mut chains: Vec<(String, Chain)> = Vec::new();
        let Some(boot) = self.boots.get(index) else {
            return Vec::new();
        };
        for &segment in &boot.segments {
            let Some(each) = self.segments.get(segment) else {
                continue;
            };
            match chains.iter_mut().find(|(class, _)| *class == each.class) {
                Some((_, chain)) => chain.segments.push(segment),
                None => chains.push((
                    each.class.clone(),
                    Chain {
                        segments: vec![segment],
                        ..Chain::default()
                    },
                )),
            }
        }
        chains
            .into_iter()
            .map(|(_, mut chain)| {
                chain.segments.sort_by_key(|&segment| {
                    self.segments.get(segment).map_or(0, |each| each.first)
                });
                chain.at = chain
                    .segments
                    .len()
                    .checked_sub(1)
                    .map(|last| (last, usize::MAX));
                chain
            })
            .collect()
    }

    /// The bytes of segment `index`, unpacked: kept for the last few.
    fn bytes(&mut self, index: usize) -> Option<Rc<Vec<u8>>> {
        if let Some(at) = self.bytes.iter().position(|(segment, _)| *segment == index) {
            let kept = self.bytes.remove(at);
            let bytes = Rc::clone(&kept.1);
            self.bytes.push(kept);
            return Some(bytes);
        }
        let path: PathBuf = self.segments.get(index)?.path.clone();
        let bytes = Rc::new(compress::expand(std::fs::read(path).ok()?));
        if self.bytes.len() >= KEPT_SEGMENTS {
            let _ = self.bytes.remove(0);
        }
        self.bytes.push((index, Rc::clone(&bytes)));
        Some(bytes)
    }

    /// Fill `chain`'s buffer with the entries before where it stands, a
    /// page's at most, going to the segment before when one is read to its
    /// start. A segment that cannot be read backwards -- a torn tail, a
    /// damaged entry -- is read forwards instead, its good part whole.
    fn refill(&mut self, which: usize) {
        loop {
            let Some(chain) = self.chains.get(which) else {
                return;
            };
            if !chain.buffer.is_empty() {
                return;
            }
            let Some((position, end)) = chain.at else {
                return;
            };
            let Some(&segment) = chain.segments.get(position) else {
                return;
            };
            let Some(bytes) = self.bytes(segment) else {
                self.next_segment(which);
                continue;
            };
            let header_len = Header::decode(&bytes).map_or(bytes.len(), |(_, len)| len);
            let end = end.min(bytes.len());
            let (found, start) = segment::entries_before(&bytes, end, PAGE);
            let chain = match self.chains.get_mut(which) {
                Some(chain) => chain,
                None => return,
            };
            if !found.is_empty() {
                chain.buffer = found.into_iter().map(|each| (segment, each)).collect();
                chain.at = Some((position, start));
                return;
            }
            if start > header_len {
                // Not readable backwards from here: read forwards from the
                // start what lies before `end`.
                let before: Vec<(usize, Found)> = segment::scan(&bytes, 0)
                    .map(|scanned| scanned.records)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|each| each.offset + each.length <= end)
                    .map(|each| (segment, each))
                    .collect();
                chain.buffer = before;
                self.next_segment(which);
                return;
            }
            self.next_segment(which);
        }
    }

    /// Move `chain` to the end of its segment before, or mark it read.
    fn next_segment(&mut self, which: usize) {
        if let Some(chain) = self.chains.get_mut(which) {
            chain.at = match chain.at {
                Some((position, _)) if position > 0 => Some((position - 1, usize::MAX)),
                _ => None,
            };
        }
    }

    /// Find the next older page, merging the classes by sequence number,
    /// and answer its records.
    fn find_older(&mut self) -> Option<Vec<Record>> {
        let mut taken: Vec<(usize, Found)> = Vec::new();
        while taken.len() < PAGE {
            for which in 0..self.chains.len() {
                self.refill(which);
            }
            let newest = self
                .chains
                .iter()
                .enumerate()
                .filter_map(|(which, chain)| {
                    chain
                        .buffer
                        .last()
                        .map(|(_, found)| (which, found.record.seqnum))
                })
                .max_by_key(|(_, seq)| *seq);
            let Some((which, _)) = newest else {
                break;
            };
            if let Some(entry) = self.chains.get_mut(which).and_then(|c| c.buffer.pop()) {
                taken.push(entry);
            }
        }
        if taken.is_empty() {
            return None;
        }
        let mut spans: Vec<Span> = Vec::new();
        let mut seqs = (u64::MAX, 0);
        let mut times = (u64::MAX, 0);
        // Taken newest first: each entry lies just before the last span of
        // its segment, or starts a new span.
        for (segment, found) in &taken {
            seqs = (
                seqs.0.min(found.record.seqnum),
                seqs.1.max(found.record.seqnum),
            );
            times = (
                times.0.min(found.record.realtime_us),
                times.1.max(found.record.realtime_us),
            );
            match spans.iter_mut().rev().find(|span| span.segment == *segment) {
                Some(span) if found.offset + found.length == span.start => {
                    span.start = found.offset;
                }
                _ => spans.push(Span {
                    segment: *segment,
                    start: found.offset,
                    end: found.offset + found.length,
                }),
            }
        }
        self.pages.push(Page {
            spans,
            count: taken.len(),
            seqs,
            times,
        });
        let mut records: Vec<Record> = taken.into_iter().map(|(_, found)| found.record).collect();
        records.reverse();
        Some(records)
    }

    /// The records of page `index`, read again from where they lie.
    fn read_page(&mut self, index: usize) -> Vec<Record> {
        let Some(page) = self.pages.get(index).cloned() else {
            return Vec::new();
        };
        let mut records = Vec::with_capacity(page.count);
        let mut hosts: HashMap<usize, (Header, Rc<str>)> = HashMap::new();
        for span in page.spans {
            let Some(bytes) = self.bytes(span.segment) else {
                continue;
            };
            let (header, host) = match hosts.get(&span.segment) {
                Some(known) => known.clone(),
                None => {
                    let Some((header, _)) = Header::decode(&bytes) else {
                        continue;
                    };
                    let host: Rc<str> = Rc::from(header.host.as_str());
                    let _ = hosts.insert(span.segment, (header.clone(), Rc::clone(&host)));
                    (header, host)
                }
            };
            let mut at = span.start;
            while at < span.end {
                let Some((mut record, length)) = segment::entry_at(&bytes, at) else {
                    break;
                };
                record.boot_id = header.boot_id;
                record.host = Rc::clone(&host);
                records.push(record);
                at += length;
            }
        }
        records.sort_by_key(|record| record.seqnum);
        records
    }
}

#[cfg(test)]
mod tests {
    use super::{PAGE, Pager, WINDOW_PAGES};
    use crate::store::Store;
    use ferrix_lode_core::record::Record;

    /// A store of `boots` boots of `each` records, in segments of at most
    /// `per` records, in a directory of its own.
    fn store(name: &str, boots: u8, each: u64, per: u64) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lode-viewer-paged-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("made");
        let mut seq = 1;
        for boot in 0..boots {
            let mut records = Vec::new();
            for n in 0..each {
                let mut record = Record::new(6, 1_700_000_000_000_000 + seq * 1000, n);
                record.seqnum = seq;
                record.boot_id = [boot + 1; 16];
                record.host = "box".into();
                record.push("MESSAGE", format!("line {seq}").as_bytes());
                records.push(record);
                seq += 1;
            }
            crate::store::import_split(&records, &dir, per).expect("written");
        }
        dir
    }

    fn seqs(records: &[Record]) -> Vec<u64> {
        records.iter().map(|record| record.seqnum).collect()
    }

    #[test]
    fn a_boot_opens_at_its_newest_window_and_counts_the_rest() {
        let dir = store("newest", 2, 100_000, 30_000);
        let (mut pager, refused) = Pager::open(&Store::at(std::slice::from_ref(&dir)));
        assert!(refused.is_empty());
        assert_eq!(pager.boots().len(), 2);
        assert_eq!(pager.records(), 200_000);
        let records = pager.start(1);
        assert_eq!(records.len(), PAGE * WINDOW_PAGES);
        let got = seqs(&records);
        let want: Vec<u64> = (200_001 - (PAGE * WINDOW_PAGES) as u64..=200_000).collect();
        let mut sorted = got.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, want, "the newest records, every one once");
        assert_eq!(pager.newer(), 0);
        assert_eq!(pager.older(), 100_000 - PAGE * WINDOW_PAGES);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stepping_older_and_back_keeps_the_window_and_every_record() {
        let dir = store("steps", 1, 120_000, 25_000);
        let (mut pager, _) = Pager::open(&Store::at(std::slice::from_ref(&dir)));
        let mut held: Vec<u64> = seqs(&pager.start(0));
        // Older, page by page, to the boot's first record.
        while let Some(moved) = pager.step_older() {
            held.extend(seqs(&moved.added));
            if let Some((low, high)) = moved.dropped {
                held.retain(|seq| *seq < low || *seq > high);
            }
            assert!(held.len() <= PAGE * WINDOW_PAGES);
        }
        assert_eq!(pager.older(), 0);
        assert_eq!(held.iter().min(), Some(&1));
        assert_eq!(pager.newer(), 120_000 - held.len());
        // And back to the newest.
        while let Some(moved) = pager.step_newer() {
            held.extend(seqs(&moved.added));
            if let Some((low, high)) = moved.dropped {
                held.retain(|seq| *seq < low || *seq > high);
            }
        }
        held.sort_unstable();
        held.dedup();
        assert_eq!(held.len(), PAGE * WINDOW_PAGES);
        assert_eq!(held.last(), Some(&120_000));
        assert!(pager.at_newest());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_jump_loads_the_window_around_its_record() {
        let dir = store("jump", 1, 150_000, 40_000);
        let (mut pager, _) = Pager::open(&Store::at(std::slice::from_ref(&dir)));
        let _ = pager.start(0);
        // A record in the middle: the window around it, pages either side.
        let records = pager.jump(70_000);
        let got = seqs(&records);
        assert!(got.contains(&70_000));
        assert_eq!(got.len(), PAGE * WINDOW_PAGES);
        assert!(pager.newer() > 0 && pager.older() > 0);
        assert_eq!(pager.newer() + got.len() + pager.older(), 150_000);
        // Near the start: still a whole window, nothing older.
        let records = pager.jump(10);
        assert!(seqs(&records).contains(&10));
        // Fifteen whole pages and the boot's oldest, a part of one.
        assert_eq!(records.len(), PAGE * (WINDOW_PAGES - 1) + 150_000 % PAGE);
        assert_eq!(pager.older(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
