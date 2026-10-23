//! The Logs table's lines: the records that matched, with what the viewer
//! says between them.
//!
//! * lode's word that the limiter held lines back is a line of its own,
//!   `⋯ 6,124 lines held from intel-xelp · 17:22:40 → 17:23:10`, which
//!   expands into what the record says of them -- the held lines
//!   themselves were never stored (`docs/LOGGING.md` §7.1).
//! * A run of lines from one source alike but for their numbers -- lode's
//!   own test for folding (`ferrix_lode_core::limit::similar`) -- shows its
//!   first line and one `⋯ N more like it` line, which expands into them.
//!   lode folds such runs as it stores them now; a store from before it did,
//!   or a run the limiter broke, is folded here for reading only.
//! * lode starting, restarting or losing records is a marker across the
//!   table.
//! * A record whose shown level a display rule set has a note under it.

use std::collections::HashSet;

use ferrix_lode_core::limit::similar;

use crate::data::{Entry, Kind, LIMIT_WINDOW_US, Log};

/// The fewest alike lines that fold.
pub const FOLD_MIN: usize = 4;

/// One line of the table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    /// A record, by its index in the log.
    Record(usize),
    /// The display rule's note under a record.
    Note(usize),
    /// lode's held-lines record, and whether it is expanded.
    Held(usize, bool),
    /// One line of what an expanded held record says.
    Detail(usize, String),
    /// A folded run: the index of its first record, how many more follow
    /// it, the last one's index, and whether it is expanded.
    Fold {
        /// The run's first record.
        first: usize,
        /// How many records after the first.
        more: usize,
        /// The run's last record.
        last: usize,
        /// How many lines the run stands for, lode's folded repeats
        /// counted.
        lines: u64,
        /// Whether its records are shown.
        expanded: bool,
    },
    /// lode's mark: started, restarted, records lost.
    Mark(usize),
}

impl Line {
    /// Its height in logical pixels.
    #[must_use]
    pub fn height(&self) -> f32 {
        match self {
            Line::Record(_) | Line::Fold { .. } => 28.0,
            Line::Note(_) => 20.0,
            Line::Held(..) => 30.0,
            Line::Detail(..) => 22.0,
            Line::Mark(_) => 40.0,
        }
    }

    /// The record it is about, if one.
    #[must_use]
    pub fn record(&self) -> Option<usize> {
        match self {
            Line::Record(index)
            | Line::Note(index)
            | Line::Held(index, _)
            | Line::Detail(index, _)
            | Line::Mark(index) => Some(*index),
            Line::Fold { first, .. } => Some(*first),
        }
    }
}

/// What a line says: a fold of lode's says its repeated line after the
/// count.
fn said(entry: &Entry) -> &[u8] {
    let message = entry.record.message();
    match entry.row.kind {
        Kind::Folded(_) => message
            .windows(2)
            .position(|pair| pair == b": ")
            .and_then(|at| message.get(at + 2..))
            .unwrap_or(message),
        _ => message,
    }
}

/// How many lines an entry stands for: a fold of lode's for its repeats
/// and the line they repeated.
fn stands_for(entry: &Entry) -> u64 {
    match entry.row.kind {
        Kind::Folded(count) => count,
        _ => 1,
    }
}

/// Whether two entries are alike enough to fold: one source, one level,
/// and the same words but for their numbers. lode's own folds of repeats
/// join a run of the line they repeated.
fn alike(a: &Entry, b: &Entry) -> bool {
    let plain = |entry: &Entry| matches!(entry.row.kind, Kind::Plain | Kind::Folded(_));
    plain(a)
        && plain(b)
        && a.row.source == b.row.source
        && a.row.level == b.row.level
        && similar(said(a), said(b))
}

/// The lines for `found`, records of `log` in order. `expanded` holds the
/// first record of each fold and each held record the person expanded.
#[must_use]
pub fn lines(log: &Log, found: &[usize], expanded: &HashSet<usize>) -> Vec<Line> {
    let rows: Vec<(usize, &Entry)> = found
        .iter()
        .filter_map(|&index| log.entries.get(index).map(|entry| (index, entry)))
        .collect();
    let mut out = Vec::with_capacity(rows.len() + 16);
    let mut noted = None;
    // Runs of alike lines; lode's own records are never alike, so each is a
    // run of one.
    for run in rows.chunk_by(|a, b| alike(a.1, b.1)) {
        let (Some(&(index, entry)), Some(&(last, _))) = (run.first(), run.last()) else {
            continue;
        };
        match entry.row.kind {
            Kind::Held(count) => {
                let open = expanded.contains(&index);
                out.push(Line::Held(index, open));
                if open {
                    for text in held_details(entry, count) {
                        out.push(Line::Detail(index, text));
                    }
                }
                continue;
            }
            Kind::Mark(_) => {
                out.push(Line::Mark(index));
                continue;
            }
            _ => {}
        }
        push_record(&mut out, index, entry, &mut noted);
        let folded = run.len() >= FOLD_MIN;
        let open = expanded.contains(&index);
        if folded {
            out.push(Line::Fold {
                first: index,
                more: run.len() - 1,
                last,
                lines: run.iter().map(|(_, inner)| stands_for(inner)).sum(),
                expanded: open,
            });
        }
        if !folded || open {
            for &(inner, entry) in run.iter().skip(1) {
                push_record(&mut out, inner, entry, &mut noted);
            }
        }
    }
    out
}

/// A record's line, and its note's when it has one that the record before
/// in the table did not: one source's run of lines read alike is noted
/// once, at its first.
fn push_record(
    out: &mut Vec<Line>,
    index: usize,
    entry: &Entry,
    noted: &mut Option<(u16, &'static str)>,
) {
    out.push(Line::Record(index));
    let key = entry.row.note.map(|note| (entry.row.source, note.word));
    if key.is_some() && key != *noted {
        out.push(Line::Note(index));
    }
    *noted = key;
}

/// When the window a held record speaks of began: the record says when
/// it ended, and lode's windows are 30 s unless a `limit` rule says
/// otherwise.
#[must_use]
pub fn held_since(entry: &Entry) -> u64 {
    entry.time().saturating_sub(LIMIT_WINDOW_US)
}

/// What an expanded held record says, a line each.
fn held_details(entry: &Entry, count: u64) -> Vec<String> {
    let mut lines = vec![format!(
        "{} lines held back by the limiter, never stored: {}",
        group(count),
        entry.message()
    )];
    if let (Some(first), Some(last)) = (
        entry.record.get_str("HELD_FIRST_SEQNUM"),
        entry.record.get_str("HELD_LAST_SEQNUM"),
    ) {
        lines.push(format!("kernel sequence {first} → {last} accounted for"));
    }
    if let Some(source) = entry.record.get_str("SUPPRESSED_SOURCE") {
        lines.push(format!(
            "lode -b --where 'id = \"{source}\"' shows the lines it kept around them"
        ));
    }
    lines
}

/// `n` with thousands grouped: 6,124.
#[must_use]
pub fn group(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(character);
    }
    out
}

/// Where each line begins, from the table's top, and the whole height.
#[must_use]
pub fn offsets(lines: &[Line]) -> (Vec<f32>, f32) {
    let mut tops = Vec::with_capacity(lines.len());
    let mut y = 0.0;
    for line in lines {
        tops.push(y);
        y += line.height();
    }
    (tops, y)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{Line, group, lines};
    use crate::data::Log;
    use ferrix_lode_core::record::Record;

    fn record(seq: u64, id: &str, message: &str) -> Record {
        let mut record = Record::new(6, seq * 1_000_000, seq);
        record.seqnum = seq;
        record.push("SYSLOG_IDENTIFIER", id.as_bytes());
        record.push("MESSAGE", message.as_bytes());
        record
    }

    #[test]
    fn a_run_alike_but_for_its_numbers_folds_and_expands() {
        let mut records = vec![record(1, "kernel", "booting")];
        for n in 0..6 {
            records.push(record(2 + n, "hyprix", &format!("frame {n} at 181,{n}")));
        }
        records.push(record(9, "hyprix", "something else"));
        let log = Log::new(records);
        let found: Vec<usize> = (0..log.entries.len()).collect();
        let folded = lines(&log, &found, &HashSet::new());
        assert_eq!(
            folded,
            vec![
                Line::Record(0),
                Line::Record(1),
                Line::Fold {
                    first: 1,
                    more: 5,
                    last: 6,
                    lines: 6,
                    expanded: false
                },
                Line::Record(7)
            ]
        );
        let open = lines(&log, &found, &HashSet::from([1]));
        assert_eq!(open.len(), 4 + 5);
        // Three alike lines are not a run worth folding.
        let short: Vec<usize> = vec![0, 1, 2, 3, 7];
        assert_eq!(lines(&log, &short, &HashSet::new()).len(), 5);
    }

    #[test]
    fn a_held_record_is_its_own_line_and_expands_into_what_it_says() {
        let mut held = record(2, "lode", "Suppressed 6124 message(s) from intel-xelp");
        held.push(
            "MESSAGE_ID",
            ferrix_lode_core::limit::MESSAGE_ID_SUPPRESSED.as_bytes(),
        );
        held.push("N_SUPPRESSED", b"6124");
        held.push("HELD_FIRST_SEQNUM", b"100");
        held.push("HELD_LAST_SEQNUM", b"6223");
        let mut warned = record(3, "kernel", "output WARNING: the transmit was refused");
        warned.push("_TRANSPORT", b"kernel");
        let log = Log::new(vec![record(1, "a", "x"), held, warned]);
        let found: Vec<usize> = (0..3).collect();
        let closed = lines(&log, &found, &HashSet::new());
        assert_eq!(
            closed,
            vec![
                Line::Record(0),
                Line::Held(1, false),
                Line::Record(2),
                Line::Note(2)
            ]
        );
        let open = lines(&log, &found, &HashSet::from([1]));
        assert!(matches!(&open[2], Line::Detail(1, text) if text.contains("6,124")));
        assert!(matches!(&open[3], Line::Detail(1, text) if text.contains("100 → 6223")));
    }

    #[test]
    fn lodes_folds_join_the_run_they_repeat_and_a_note_is_said_once_a_run() {
        let mut records = Vec::new();
        for n in 0..3 {
            records.push(record(
                1 + 2 * n,
                "intel",
                &format!("uapi refused EXECBUFFER2 {n}"),
            ));
            let mut folded = record(
                2 + 2 * n,
                "intel",
                "message repeated 6 time(s): uapi refused EXECBUFFER2 1",
            );
            folded.push(
                "MESSAGE_ID",
                ferrix_lode_core::limit::MESSAGE_ID_REPEATED.as_bytes(),
            );
            folded.push("REPEATED", b"6");
            records.push(folded);
        }
        let log = Log::new(records);
        let found: Vec<usize> = (0..6).collect();
        let lines = lines(&log, &found, &HashSet::new());
        assert_eq!(lines[0], Line::Record(0));
        assert_eq!(lines[1], Line::Note(0));
        assert!(matches!(
            lines[2],
            Line::Fold {
                more: 5,
                lines: 21,
                ..
            }
        ));
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn numbers_are_grouped_in_thousands() {
        assert_eq!(group(6124), "6,124");
        assert_eq!(group(999), "999");
        assert_eq!(group(1_234_567), "1,234,567");
    }
}
