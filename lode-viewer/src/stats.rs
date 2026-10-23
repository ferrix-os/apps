//! What the Overview counts: the activity in buckets of time, the rate,
//! the busiest sources, the swim lanes, the tiles and the problems a time
//! cursor steps between. Everything is over one boot's entries, by the
//! level each is shown at.

use crate::data::{Entry, Kind};

/// lode's limiter for chatter: 1,000 lines a source in 30 s
/// (`docs/LOGGING.md` §7.1), as lines a second.
pub const CHATTER_LIMIT_PER_S: f32 = 1000.0 / 30.0;

/// One bucket of time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bucket {
    /// Lines below warning.
    pub info: usize,
    /// Warnings.
    pub warnings: usize,
    /// Errors and worse.
    pub errors: usize,
    /// Lines the limiter held, by its records that end in the bucket.
    pub held: u64,
}

impl Bucket {
    /// Every line stored in it.
    #[must_use]
    pub fn total(&self) -> usize {
        self.info + self.warnings + self.errors
    }
}

/// The bucket width for a boot `span_us` long, at least `least_us`, so
/// that there are no more than `most` buckets: 10 s where that fits, then
/// 30 s, a minute, 5, 10, 30 and 60 minutes.
#[must_use]
pub fn bucket_width(span_us: u64, least_us: u64, most: usize) -> u64 {
    const STEPS_S: [u64; 8] = [10, 30, 60, 300, 600, 1800, 3600, 86_400];
    for step in STEPS_S {
        let width = (step * 1_000_000).max(least_us);
        if span_us.div_ceil(width) as usize <= most.max(1) {
            return width;
        }
    }
    86_400_000_000
}

/// `entries` counted in buckets `width_us` wide from `start_us`, `count`
/// of them.
#[must_use]
pub fn buckets(entries: &[Entry], start_us: u64, width_us: u64, count: usize) -> Vec<Bucket> {
    let mut out = vec![Bucket::default(); count];
    let width = width_us.max(1);
    for entry in entries {
        let Some(offset) = entry.time().checked_sub(start_us) else {
            continue;
        };
        let Some(bucket) = usize::try_from(offset / width)
            .ok()
            .and_then(|index| out.get_mut(index))
        else {
            continue;
        };
        if let Kind::Held(count) = entry.row.kind {
            bucket.held += count;
            continue;
        }
        match entry.row.level {
            0..=3 => bucket.errors += 1,
            4 => bucket.warnings += 1,
            _ => bucket.info += 1,
        }
    }
    out
}

/// One source's share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Share {
    /// The source.
    pub source: u16,
    /// Its lines.
    pub lines: usize,
    /// Its errors.
    pub errors: usize,
}

/// The `most` busiest sources of `entries` within `[from, to]`.
#[must_use]
pub fn top_sources(entries: &[Entry], (from, to): (u64, u64), most: usize) -> Vec<Share> {
    let mut shares: Vec<Share> = Vec::new();
    for entry in entries {
        if entry.time() < from || entry.time() > to {
            continue;
        }
        let source = entry.row.source;
        let errors = usize::from(entry.is_error());
        match shares.iter_mut().find(|share| share.source == source) {
            Some(share) => {
                share.lines += 1;
                share.errors += errors;
            }
            None => shares.push(Share {
                source,
                lines: 1,
                errors,
            }),
        }
    }
    shares.sort_by(|a, b| b.lines.cmp(&a.lines).then(a.source.cmp(&b.source)));
    shares.truncate(most);
    shares
}

/// The tiles' figures for a boot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tiles {
    /// Errors and worse.
    pub errors: usize,
    /// When the last was.
    pub last_error: Option<u64>,
    /// Warnings.
    pub warnings: usize,
    /// When the last was.
    pub last_warning: Option<u64>,
    /// Lines a second over the boot.
    pub rate: f32,
    /// The busiest second's lines, and when.
    pub peak: (usize, u64),
    /// Lines the limiter held.
    pub held: u64,
    /// Crashes.
    pub crashes: usize,
}

/// The tiles' figures for `entries`, a boot `span_us` long.
#[must_use]
pub fn tiles(entries: &[Entry], span_us: u64) -> Tiles {
    let mut tiles = Tiles::default();
    let mut second: Option<(u64, usize)> = None;
    for entry in entries {
        match entry.row.kind {
            Kind::Held(count) => tiles.held += count,
            Kind::Crash => tiles.crashes += 1,
            _ => {}
        }
        if entry.is_error() {
            tiles.errors += 1;
            tiles.last_error = Some(entry.time());
        } else if entry.is_warning() {
            tiles.warnings += 1;
            tiles.last_warning = Some(entry.time());
        }
        let at = entry.time() / 1_000_000;
        second = match second {
            Some((when, count)) if when == at => Some((when, count + 1)),
            Some((when, count)) => {
                if count > tiles.peak.0 {
                    tiles.peak = (count, when * 1_000_000);
                }
                Some((at, 1))
            }
            None => Some((at, 1)),
        };
    }
    if let Some((when, count)) = second
        && count > tiles.peak.0
    {
        tiles.peak = (count, when * 1_000_000);
    }
    tiles.rate = entries.len() as f32 / (span_us.max(1_000_000) as f32 / 1e6);
    tiles
}

/// The times of the errors in `entries`, which the cursor steps between.
#[must_use]
pub fn problems(entries: &[Entry]) -> Vec<u64> {
    let mut times: Vec<u64> = entries
        .iter()
        .filter(|entry| entry.is_error())
        .map(Entry::time)
        .collect();
    times.dedup();
    times
}

/// One swim lane: which sources it holds and what it shows across a boot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lane {
    /// Its name.
    pub name: String,
    /// The lines in each column.
    pub columns: Vec<usize>,
    /// Where its errors are, as fractions of the boot.
    pub errors: Vec<f32>,
    /// Where its warnings are.
    pub warnings: Vec<f32>,
    /// The spans the limiter held its lines, as fractions of the boot.
    pub held: Vec<(f32, f32)>,
}

/// The overview's lanes for `entries`, `columns` wide over the boot from
/// `start`, `span` long: the kernel's own lines, the two busiest other sources that
/// had errors or warnings (else the two busiest), and everything else.
#[must_use]
pub fn lanes(
    entries: &[Entry],
    names: &crate::data::Names,
    start: u64,
    span: u64,
    columns: usize,
) -> Vec<Lane> {
    let kernel = names.find("kernel");
    let mut shares = top_sources(entries, (0, u64::MAX), usize::MAX);
    let troubled = |source: u16| {
        entries
            .iter()
            .any(|entry| entry.row.source == source && entry.row.level <= 4)
    };
    shares.retain(|share| Some(share.source) != kernel && names.name(share.source) != "lode");
    let mut chosen: Vec<u16> = shares
        .iter()
        .filter(|share| troubled(share.source))
        .map(|share| share.source)
        .take(2)
        .collect();
    for share in &shares {
        if chosen.len() >= 2 {
            break;
        }
        if !chosen.contains(&share.source) {
            chosen.push(share.source);
        }
    }
    let mut lanes: Vec<(Option<Vec<u16>>, Lane)> = Vec::new();
    if let Some(kernel) = kernel {
        lanes.push((Some(vec![kernel]), lane("kernel", columns)));
    }
    for source in &chosen {
        lanes.push((Some(vec![*source]), lane(names.name(*source), columns)));
    }
    lanes.push((None, lane("other", columns)));
    let span = span.max(1) as f32;
    let fraction = |at: u64| (at.saturating_sub(start) as f32 / span).clamp(0.0, 1.0);
    for entry in entries {
        // A held record speaks for the source it held.
        let source = match entry.row.kind {
            Kind::Held(_) => entry
                .record
                .get_str("SUPPRESSED_SOURCE")
                .and_then(|name| names.find(name))
                .unwrap_or(entry.row.source),
            _ => entry.row.source,
        };
        let Some((_, lane)) = lanes.iter_mut().find(|(sources, _)| {
            sources
                .as_ref()
                .is_none_or(|sources| sources.contains(&source))
        }) else {
            continue;
        };
        let at = fraction(entry.time());
        if let Kind::Held(_) = entry.row.kind {
            let since = fraction(crate::lines::held_since(entry));
            lane.held.push((since, at));
            continue;
        }
        let column = ((at * columns as f32) as usize).min(columns.saturating_sub(1));
        if let Some(count) = lane.columns.get_mut(column) {
            *count += 1;
        }
        if entry.is_error() {
            lane.errors.push(at);
        } else if entry.is_warning() {
            lane.warnings.push(at);
        }
    }
    lanes.into_iter().map(|(_, lane)| lane).collect()
}

/// An empty lane.
fn lane(name: &str, columns: usize) -> Lane {
    Lane {
        name: name.to_owned(),
        columns: vec![0; columns],
        ..Lane::default()
    }
}

/// The indexes (into `entries`) within `[from, to]`, warnings and worse
/// first, each group in time order, and how many lines the window holds.
#[must_use]
pub fn worst_first(entries: &[Entry], (from, to): (u64, u64)) -> (Vec<usize>, usize) {
    let mut within: Vec<(usize, &Entry)> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.time() >= from && entry.time() <= to)
        .collect();
    let total = within.len();
    within.sort_by_key(|&(_, entry)| {
        let rank = match (entry.row.level, entry.row.kind) {
            (0..=3, _) => 0,
            (_, Kind::Held(_) | Kind::Mark(_) | Kind::Crash) | (4, _) => 1,
            _ => 2,
        };
        (rank, entry.time())
    });
    (within.into_iter().map(|(index, _)| index).collect(), total)
}

#[cfg(test)]
mod tests {
    use super::{bucket_width, buckets, lanes, problems, tiles, top_sources, worst_first};
    use crate::data::Log;
    use ferrix_lode_core::record::Record;

    fn record(seq: u64, at_s: u64, id: &str, level: u8) -> Record {
        let mut record = Record::new(level, at_s * 1_000_000, at_s);
        record.seqnum = seq;
        record.push("SYSLOG_IDENTIFIER", id.as_bytes());
        record.push("FERRIX_PRIORITY_SOURCE", b"stated");
        record.push("MESSAGE", b"m");
        record
    }

    fn log() -> Log {
        Log::new(vec![
            record(1, 0, "kernel", 6),
            record(2, 5, "kernel", 6),
            record(3, 12, "hyprix", 3),
            record(4, 15, "hyprix", 4),
            record(5, 15, "intel", 6),
            record(6, 25, "init", 6),
        ])
    }

    #[test]
    fn activity_is_counted_per_bucket_by_shown_level() {
        let log = log();
        let counted = buckets(&log.entries, 0, 10_000_000, 3);
        assert_eq!(counted[0].info, 2);
        assert_eq!(
            (counted[1].errors, counted[1].warnings, counted[1].info),
            (1, 1, 1)
        );
        assert_eq!(counted[2].total(), 1);
        assert_eq!(bucket_width(200_000_000, 0, 120), 10_000_000);
        assert_eq!(bucket_width(1_200_000_000, 0, 60), 30_000_000);
        assert_eq!(bucket_width(1_000_000, 30_000_000, 60), 30_000_000);
    }

    #[test]
    fn tiles_sources_lanes_and_problems() {
        let log = log();
        let figures = tiles(&log.entries, 25_000_000);
        assert_eq!((figures.errors, figures.warnings), (1, 1));
        assert_eq!(figures.last_error, Some(12_000_000));
        assert_eq!(figures.peak, (2, 15_000_000));
        let top = top_sources(&log.entries, (0, u64::MAX), 2);
        assert_eq!(log.sources.name(top[0].source), "kernel");
        assert_eq!(top[1].errors, 1);
        assert_eq!(problems(&log.entries), vec![12_000_000]);
        let lanes = lanes(&log.entries, &log.sources, 0, 25_000_000, 5);
        let names: Vec<&str> = lanes.iter().map(|lane| lane.name.as_str()).collect();
        assert_eq!(names, vec!["kernel", "hyprix", "intel", "other"]);
        assert_eq!(lanes[1].errors.len(), 1);
        assert_eq!(lanes[3].columns.iter().sum::<usize>(), 1);
        let (order, total) = worst_first(&log.entries, (10_000_000, 20_000_000));
        assert_eq!(total, 3);
        assert_eq!(log.entries[order[0]].row.level, 3);
        assert_eq!(log.entries[order[1]].row.level, 4);
    }
}
