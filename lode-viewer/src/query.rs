//! What the Logs page asks of the log: lode's filter language
//! (`docs/LOGGING.md` §6, `ferrix_lode_core::filter`) typed after `where`,
//! the chips beside it, the facets' ticks and the time window. Facet counts
//! are taken over what everything but the facets lets through, so ticking
//! one does not empty the others.

use ferrix_lode_core::filter::{self, Expr};

use crate::data::{Entry, Log};

/// A saved query: a name and the expression it stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    /// Its name, without the `@`.
    pub name: String,
    /// The expression.
    pub text: String,
}

/// The queries every viewer starts with.
#[must_use]
pub fn builtin() -> Vec<Saved> {
    vec![
        Saved {
            name: "triage".to_owned(),
            text: "priority <= warning or crash = \"*\"".to_owned(),
        },
        Saved {
            name: "gpu".to_owned(),
            text: "id = \"intel*\" or driver = \"intel*\" or subsystem = \"drm*\" or message ~ \"card0\""
                .to_owned(),
        },
    ]
}

/// The saved queries' file: `name = expression` a line, `#` a comment.
#[must_use]
pub fn parse_saved(text: &str) -> Vec<Saved> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (name, expr) = line.split_once('=')?;
            let name = name.trim().trim_start_matches('@');
            (!name.is_empty() && filter::parse(expr.trim()).is_ok()).then(|| Saved {
                name: name.to_owned(),
                text: expr.trim().to_owned(),
            })
        })
        .collect()
}

/// The saved queries as their file holds them.
#[must_use]
pub fn write_saved(saved: &[Saved]) -> String {
    let mut text = String::from("# lode-viewer's saved queries: name = lode filter expression\n");
    for each in saved {
        text.push_str(&format!("{} = {}\n", each.name, each.text));
    }
    text
}

/// A chip: one more condition, shown beside the query and removed with its
/// cross.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chip {
    /// Only this source.
    Source(u16),
    /// Not this source.
    NotSource(u16),
    /// Only this transport.
    Transport(u16),
    /// Only this session.
    Session(u16),
    /// Only crashes.
    Crashes,
}

/// The whole query.
#[derive(Clone, Debug)]
pub struct Query {
    /// The expression as typed.
    pub text: String,
    /// It parsed; `None` while empty.
    pub expr: Option<Expr>,
    /// Why it did not parse.
    pub error: Option<String>,
    /// The levels ticked, 0 to 7 and unspecified.
    pub levels: [bool; 9],
    /// The chips.
    pub chips: Vec<Chip>,
    /// The boot, by index into the log's boots.
    pub boot: usize,
    /// The time window, microseconds since 1970, both ends included.
    pub window: Option<(u64, u64)>,
}

impl Default for Query {
    fn default() -> Self {
        Query {
            text: String::new(),
            expr: None,
            error: None,
            levels: [true; 9],
            chips: Vec::new(),
            boot: 0,
            window: None,
        }
    }
}

/// How many records of each kind pass everything but the facets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facets {
    /// By level, 0 to 7 and unspecified.
    pub levels: [usize; 9],
    /// By source, most first.
    pub sources: Vec<(u16, usize)>,
    /// By transport, most first.
    pub transports: Vec<(u16, usize)>,
    /// By session, most first; the empty session left out.
    pub sessions: Vec<(u16, usize)>,
}

impl Query {
    /// Set the expression; an empty one is every record.
    pub fn set_text(&mut self, text: &str) {
        text.clone_into(&mut self.text);
        let trimmed = text.trim();
        let trimmed = trimmed.strip_prefix("where ").unwrap_or(trimmed).trim();
        if trimmed.is_empty() {
            self.expr = None;
            self.error = None;
            return;
        }
        match filter::parse(trimmed) {
            Ok(expr) => {
                self.expr = Some(expr);
                self.error = None;
            }
            Err(error) => {
                self.error = Some(format!("{error:?}"));
            }
        }
    }

    /// Whether `entry` passes all but the facets: the window, the
    /// expression and the chips.
    #[must_use]
    pub fn base(&self, entry: &Entry) -> bool {
        if let Some((from, to)) = self.window {
            let at = entry.time();
            if at < from || at > to {
                return false;
            }
        }
        if let Some(expr) = &self.expr
            && !expr.matches(&entry.record)
        {
            return false;
        }
        self.chips.iter().all(|chip| match *chip {
            Chip::Source(source) => entry.row.source == source,
            Chip::NotSource(source) => entry.row.source != source,
            Chip::Transport(transport) => entry.row.transport == transport,
            Chip::Session(session) => entry.row.session == session,
            Chip::Crashes => entry.row.kind == crate::data::Kind::Crash,
        })
    }

    /// Whether `entry` passes the facets.
    #[must_use]
    pub fn facets(&self, entry: &Entry) -> bool {
        self.levels
            .get(usize::from(entry.row.level))
            .copied()
            .unwrap_or(true)
    }

    /// Whether `entry` passes.
    #[must_use]
    pub fn matches(&self, entry: &Entry) -> bool {
        self.base(entry) && self.facets(entry)
    }

    /// The indexes into the log's entries of the boot's records that pass,
    /// and the facet counts.
    #[must_use]
    pub fn run(&self, log: &Log) -> (Vec<usize>, Facets) {
        let mut found = Vec::new();
        let mut facets = Facets::default();
        let Some(boot) = log.boots.get(self.boot) else {
            return (found, facets);
        };
        let mut sources = vec![0_usize; log.sources.len()];
        let mut transports = vec![0_usize; log.transports.len()];
        let mut sessions = vec![0_usize; log.sessions.len()];
        let entries = log.entries.get(boot.range.clone()).unwrap_or(&[]);
        for (entry, index) in entries.iter().zip(boot.range.clone()) {
            if !self.base(entry) {
                continue;
            }
            if let Some(count) = facets.levels.get_mut(usize::from(entry.row.level)) {
                *count += 1;
            }
            if let Some(count) = sources.get_mut(usize::from(entry.row.source)) {
                *count += 1;
            }
            if let Some(count) = transports.get_mut(usize::from(entry.row.transport)) {
                *count += 1;
            }
            if let Some(count) = sessions.get_mut(usize::from(entry.row.session)) {
                *count += 1;
            }
            if self.facets(entry) {
                found.push(index);
            }
        }
        let ranked = |counts: Vec<usize>, skip_first: bool| {
            let mut list: Vec<(u16, usize)> = counts
                .into_iter()
                .enumerate()
                .filter(|&(index, count)| count > 0 && !(skip_first && index == 0))
                .map(|(index, count)| (u16::try_from(index).unwrap_or(u16::MAX), count))
                .collect();
            list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            list
        };
        facets.sources = ranked(sources, false);
        facets.transports = ranked(transports, false);
        facets.sessions = ranked(sessions, true);
        (found, facets)
    }
}

#[cfg(test)]
mod tests {
    use super::{Chip, Query, builtin, parse_saved, write_saved};
    use crate::data::Log;
    use ferrix_lode_core::record::Record;

    fn record(seq: u64, id: &str, level: u8, message: &str) -> Record {
        let mut record = Record::new(level, seq * 1_000_000, seq);
        record.seqnum = seq;
        record.push("SYSLOG_IDENTIFIER", id.as_bytes());
        record.push("_TRANSPORT", b"stdout");
        record.push("FERRIX_PRIORITY_SOURCE", b"stated");
        record.push("MESSAGE", message.as_bytes());
        record
    }

    fn log() -> Log {
        Log::new(vec![
            record(1, "kernel", 6, "booting"),
            record(2, "hyprix", 3, "page flip failed"),
            record(3, "hyprix", 4, "slow frame"),
            record(4, "sessiond", 8, "seat0"),
            record(5, "hyprix", 6, "frame presented"),
        ])
    }

    #[test]
    fn lodes_language_chips_levels_and_the_window_all_narrow() {
        let log = log();
        let mut query = Query::default();
        assert_eq!(query.run(&log).0.len(), 5);
        query.set_text("where id = \"hypr*\"");
        let (found, facets) = query.run(&log);
        assert_eq!(found.len(), 3);
        assert_eq!(facets.levels[3], 1);
        assert_eq!(facets.levels[6], 1);
        // Unticking info leaves the counts as they were.
        query.levels[6] = false;
        let (found, after) = query.run(&log);
        assert_eq!(found.len(), 2);
        assert_eq!(after.levels, facets.levels);
        query.set_text("");
        query.levels = [true; 9];
        let hyprix = log.sources.find("hyprix").expect("a source");
        query.chips.push(Chip::NotSource(hyprix));
        assert_eq!(query.run(&log).0.len(), 2);
        query.chips.clear();
        query.window = Some((2_000_000, 3_000_000));
        assert_eq!(query.run(&log).0.len(), 2);
        // Unspecified is outside every range, as lode's `-p` has it.
        query.window = None;
        query.set_text("priority <= warning");
        assert_eq!(query.run(&log).0.len(), 2);
        query.set_text("level = unspecified");
        assert_eq!(query.run(&log).0.len(), 1);
        query.set_text("priority <= (");
        assert!(query.error.is_some());
    }

    #[test]
    fn the_language_sees_the_level_a_display_rule_shows() {
        let mut quiet = Record::new(6, 1_000_000, 1);
        quiet.seqnum = 1;
        quiet.push("SYSLOG_IDENTIFIER", b"intel");
        quiet.push("MESSAGE", b"uapi refused EXECBUFFER2");
        let log = Log::new(vec![quiet]);
        let mut query = Query::default();
        query.set_text("priority <= warning");
        assert_eq!(query.run(&log).0.len(), 1);
        assert_eq!(log.entries[0].stored_level(), 6);
        assert_eq!(log.entries[0].stored().priority, 6);
    }

    #[test]
    fn saved_queries_read_back_what_was_written() {
        let saved = builtin();
        assert_eq!(parse_saved(&write_saved(&saved)), saved);
        assert!(parse_saved("bad = priority <= (\n# note\n").is_empty());
    }

    #[test]
    fn facets_rank_most_first() {
        let log = log();
        let (_, facets) = Query::default().run(&log);
        let first = facets.sources[0];
        assert_eq!(log.sources.name(first.0), "hyprix");
        assert_eq!(first.1, 3);
        assert!(facets.sessions.is_empty());
    }
}
