//! The Logs table's lines as a model of the markup's `LogLine`s: a row is
//! made from the log only when the table shows it, so a hundred thousand
//! records cost nothing until scrolled to. The last row is the table's
//! end, saying whether the window goes on.

use tole::{Model, ModelTracker};

use super::paint::Shared;
use super::{LineKind, LogLine};
use crate::app::{App, clock, clock_ms};
use crate::data::{Kind, Mark, level_name};
use crate::lines::{Line, group, held_since};

/// The end row's height.
const END: f32 = 30.0;

/// The table's lines.
pub struct LinesModel {
    /// The state they are read from.
    app: Shared,
    /// Says when they changed.
    tracker: ModelTracker,
}

impl std::fmt::Debug for LinesModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinesModel").finish_non_exhaustive()
    }
}

impl LinesModel {
    /// The lines of `app`.
    #[must_use]
    pub fn new(app: Shared) -> Self {
        LinesModel {
            app,
            tracker: ModelTracker::default(),
        }
    }

    /// Say that every line may have changed.
    pub fn reset(&self) {
        self.tracker.reset();
    }
}

impl Model for LinesModel {
    type Row = LogLine;

    fn row_count(&self) -> usize {
        self.app.try_borrow().map_or(0, |app| {
            app.lines.len() + 1 + usize::from(app.gaps.0 > 0.0) + usize::from(app.gaps.1 > 0.0)
        })
    }

    fn row(&self, index: usize) -> Option<LogLine> {
        let app = self.app.try_borrow().ok()?;
        match place(&app, index)? {
            Place::Line(line) => app.lines.get(line).map(|line| row(&app, line)),
            Place::Older => Some(gap(&app, true)),
            Place::Newer => Some(gap(&app, false)),
            Place::End => Some(LogLine {
                kind: LineKind::End,
                record: -1,
                text: if app.live {
                    "— end of window · following —"
                } else if app.lines.is_empty() {
                    "— nothing matches —"
                } else {
                    "— end of window —"
                }
                .into(),
                ..LogLine::default()
            }),
        }
    }

    fn row_height(&self, index: usize) -> Option<f32> {
        let app = self.app.try_borrow().ok()?;
        Some(match place(&app, index)? {
            Place::Line(line) => app.lines.get(line).map_or(END, Line::height),
            Place::Older => app.gaps.0,
            Place::Newer => app.gaps.1,
            Place::End => END,
        })
    }

    fn tracker(&self) -> &ModelTracker {
        &self.tracker
    }
}

/// What row `index` of the table is.
enum Place {
    /// The records not read, older than the window.
    Older,
    /// Line `.0` of the window's.
    Line(usize),
    /// The records not read, newer than the window.
    Newer,
    /// The table's end.
    End,
}

/// Row `index`'s place: the older records not read first, when there are
/// any, then the lines, the newer records not read, and the end.
fn place(app: &App, index: usize) -> Option<Place> {
    let older = usize::from(app.gaps.0 > 0.0);
    if index < older {
        return Some(Place::Older);
    }
    let line = index - older;
    if line < app.lines.len() {
        return Some(Place::Line(line));
    }
    let after = line - app.lines.len();
    match (after, app.gaps.1 > 0.0) {
        (0, true) => Some(Place::Newer),
        (0, false) | (1, true) => Some(Place::End),
        _ => None,
    }
}

/// The row standing for the records not read, older or newer.
fn gap(app: &App, older: bool) -> LogLine {
    let count = app
        .pager
        .as_ref()
        .map_or(0, |pager| if older { pager.older() } else { pager.newer() });
    let text = if older {
        format!(
            "↑ {} older records, read as the view comes to them",
            group(count as u64)
        )
    } else {
        format!(
            "↓ {} newer records, read as the view comes to them",
            group(count as u64)
        )
    };
    LogLine {
        kind: LineKind::Gap,
        record: -1,
        text: text.into(),
        ..LogLine::default()
    }
}

/// One line of the table, as its delegate shows it.
#[must_use]
pub fn row(app: &App, line: &Line) -> LogLine {
    let zone = app.zone();
    let Some(index) = line.record() else {
        return LogLine::default();
    };
    let Some(entry) = app.log.entries.get(index) else {
        return LogLine::default();
    };
    let record = i32::try_from(index).unwrap_or(-1);
    let level = i32::from(entry.row.level);
    match line {
        Line::Record(_) => {
            let message = match entry.row.kind {
                Kind::Folded(count) => {
                    let text = entry.message();
                    let rest = text
                        .split_once(": ")
                        .map_or("", |(_, rest)| rest)
                        .to_owned();
                    format!("message repeated {} times  {rest}", group(count))
                }
                _ => entry.message().into_owned(),
            };
            LogLine {
                kind: LineKind::Record,
                record,
                level,
                crash: entry.row.kind == Kind::Crash,
                time: clock_ms(entry.time(), zone).into(),
                source: app.log.sources.name(entry.row.source).into(),
                text: message.replace('\n', " \\n ").into(),
                quiet: matches!(entry.row.kind, Kind::Folded(_)),
                action: "".into(),
            }
        }
        Line::Note(_) => {
            let text = entry.row.note.map_or_else(String::new, |note| {
                format!(
                    "Stored as {}; text says “{}”. Shown as {} by a display rule.",
                    level_name(note.stored),
                    note.word,
                    level_name(entry.row.level)
                )
            });
            LogLine {
                kind: LineKind::Note,
                record,
                level,
                text: text.into(),
                ..LogLine::default()
            }
        }
        Line::Held(_, open) => {
            let count = match entry.row.kind {
                Kind::Held(count) => count,
                _ => 0,
            };
            let source = entry
                .record
                .get_str("SUPPRESSED_SOURCE")
                .unwrap_or("a source")
                .to_owned();
            LogLine {
                kind: LineKind::Held,
                record,
                level,
                text: format!(
                    "… {} lines held from {source} · {} → {}",
                    group(count),
                    clock(held_since(entry), zone),
                    clock(entry.time(), zone)
                )
                .into(),
                action: if *open { "Fold" } else { "Expand" }.into(),
                ..LogLine::default()
            }
        }
        Line::Detail(_, text) => LogLine {
            kind: LineKind::Detail,
            record,
            level,
            text: text.as_str().into(),
            ..LogLine::default()
        },
        Line::Fold {
            first,
            more,
            last,
            lines,
            expanded,
        } => {
            let b = app.log.entries.get(*last).unwrap_or(entry);
            let source = app.log.sources.name(entry.row.source);
            let folded = if *lines > *more as u64 + 1 {
                format!(", {} lines with lode's folds", group(*lines))
            } else {
                String::new()
            };
            let text = if *expanded {
                format!(
                    "{} records alike from {source}{folded}, shown",
                    group(*more as u64 + 1)
                )
            } else {
                format!(
                    "… {} more like it from {source}{folded} · {} → {}",
                    group(*more as u64),
                    clock(entry.time(), zone),
                    clock(b.time(), zone)
                )
            };
            LogLine {
                kind: LineKind::Fold,
                record: i32::try_from(*first).unwrap_or(-1),
                level,
                text: text.into(),
                action: if *expanded { "Fold" } else { "Expand" }.into(),
                ..LogLine::default()
            }
        }
        Line::Mark(_) => {
            let what = match entry.row.kind {
                Kind::Mark(Mark::Started) => "lode started",
                Kind::Mark(Mark::Restarted) => "lode restarted",
                Kind::Mark(Mark::Lost) => "records lost",
                _ => "a segment's torn tail",
            };
            LogLine {
                kind: LineKind::Mark,
                record,
                level,
                text: format!(
                    "— {what} {} · {}",
                    clock(entry.time(), zone),
                    entry.message()
                )
                .into(),
                ..LogLine::default()
            }
        }
    }
}
