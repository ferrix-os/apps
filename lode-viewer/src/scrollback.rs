//! The viewer over a log read a page at a time (`docs/LOGGING.md`, "The
//! viewer's scrollback"): the window of [`crate::paged::Pager`] in the
//! [`Log`], the records not read as rows of their height above and below
//! the table's lines, pages read as the view nears them, the view kept on
//! the record it showed while pages come and go.

use ferrix_lode_core::record::Record;

use crate::app::App;
use crate::data::{KnownBoot, Log};
use crate::paged::{PAGE, Pager, WINDOW_PAGES};
use crate::store::Store;

/// A record's line's height, which a record not read stands for.
pub const ROW: f32 = 28.0;

/// How near, in pixels, the view comes to records not read before the next
/// page is read: two pages' rows, or two views when a query shows fewer.
fn margin(app: &App) -> f32 {
    if app.query_filters() {
        (app.table_view * 2.0).max(ROW * 40.0)
    } else {
        ROW * (2 * PAGE) as f32
    }
}

/// The boots the pager knows, as the log keeps them.
fn known(pager: &Pager) -> Vec<KnownBoot> {
    pager
        .boots()
        .iter()
        .map(|boot| KnownBoot {
            id: boot.id,
            host: boot.host.clone(),
            first_us: boot.first_us,
            last_us: boot.last_us,
            records: boot.records,
        })
        .collect()
}

/// `store` read whole, as always, when it is small enough; else its newest
/// boot's newest window, a page at a time from then on. Answers the log,
/// the pager when paged, and the directories refused.
#[must_use]
pub fn read_store(store: &mut Store) -> (Log, Option<Pager>, Vec<String>) {
    let (mut pager, refused) = Pager::open(store);
    // `LODE_VIEWER_WHOLE=1` reads every store whole, as before paging.
    let whole = std::env::var("LODE_VIEWER_WHOLE").is_ok_and(|v| v == "1");
    if whole || pager.records() <= crate::paged::WHOLE_UP_TO {
        let (records, refused) = store.load();
        return (Log::new(records), None, refused);
    }
    let newest = pager.boots().len().saturating_sub(1);
    let records = pager.start(newest);
    let mut log = Log {
        known: known(&pager),
        ..Log::default()
    };
    let _ = log.add(records);
    store.follow_from_now();
    (log, Some(pager), refused)
}

impl App {
    /// Whether the query keeps only some records: then a page holds an
    /// unknown number of lines.
    #[must_use]
    pub fn query_filters(&self) -> bool {
        !self.query.text.trim().is_empty()
            || !self.query.chips.is_empty()
            || self.query.levels.iter().any(|on| !on)
    }

    /// Whether the boot shown is read a page at a time.
    #[must_use]
    pub fn paged(&self) -> bool {
        self.pager.as_ref().is_some_and(|pager| {
            self.boot()
                .zip(pager.boots().get(pager.boot()))
                .is_some_and(|(shown, paged)| shown.id == paged.id)
        })
    }

    /// Show boot `index` from its newest records when the log is read a
    /// page at a time; nothing otherwise.
    pub(crate) fn page_boot(&mut self, index: usize) {
        let Some(pager) = self.pager.as_mut() else {
            return;
        };
        let Some(id) = self.log.boots.get(index).map(|boot| boot.id) else {
            return;
        };
        let Some(at) = pager.boots().iter().position(|boot| boot.id == id) else {
            return;
        };
        let shown = pager.boots().get(pager.boot()).map(|boot| boot.id);
        let records = pager.start(at);
        if let Some(shown) = shown {
            self.log.clear_boot(shown);
        }
        self.log.clear_boot(id);
        let _ = self.log.add(records);
        self.away = 0;
        self.whole_shown.1 = true;
    }

    /// The heights of the records not read, above and below the table's
    /// lines, put into its layout.
    pub(crate) fn add_gaps(&mut self) {
        self.gaps = (0.0, 0.0);
        let whole = self.whole_boot();
        let to_end = core::mem::take(&mut self.whole_shown.1) || (whole && !self.whole_shown.0);
        self.whole_shown.0 = whole;
        if !self.paged() || !whole {
            return;
        }
        let Some(pager) = &self.pager else {
            return;
        };
        let height = |records: usize| {
            if records == 0 {
                0.0
            } else if self.query_filters() {
                ROW
            } else {
                records as f32 * ROW
            }
        };
        self.gaps = (height(pager.older()), height(pager.newer()));
        for top in &mut self.tops {
            *top += self.gaps.0;
        }
        self.table_height += self.gaps.0 + self.gaps.1;
        if to_end {
            self.scroll = (self.table_height - self.table_view).max(0.0);
        }
    }

    /// The record at the view's top and how far the view is past its line,
    /// to put the view back on after the lines change.
    fn anchor(&self) -> Option<(u64, f32)> {
        let at = self.tops.partition_point(|top| *top <= self.scroll);
        let line = self.lines.get(at.checked_sub(1)?)?;
        let entry = self.log.entries.get(line.record()?)?;
        let top = self.tops.get(at - 1)?;
        Some((entry.record.seqnum, self.scroll - top))
    }

    /// Put the view back on the record `seq`, `past` pixels past its line.
    fn restore(&mut self, (seq, past): (u64, f32)) {
        let found = self
            .lines
            .iter()
            .zip(&self.tops)
            .find(|(line, _)| {
                line.record()
                    .and_then(|index| self.log.entries.get(index))
                    .is_some_and(|entry| entry.record.seqnum == seq)
            })
            .map(|(_, top)| *top);
        if let Some(top) = found {
            self.scroll = (top + past).max(0.0);
        }
    }

    /// Records into the window and records out, the selection and the open
    /// lines kept by sequence number, the view kept on its record.
    fn apply(&mut self, added: Vec<Record>, dropped: Option<(u64, u64)>) {
        let Some(id) = self
            .pager
            .as_ref()
            .and_then(|pager| pager.boots().get(pager.boot()))
            .map(|boot| boot.id)
        else {
            return;
        };
        let anchor = self.anchor();
        let seq_of = |log: &Log, index: usize| log.entries.get(index).map(|e| e.record.seqnum);
        // What a page let go before held is wanted back when it returns;
        // a selection made since wins.
        let selected = self
            .selected
            .and_then(|index| seq_of(&self.log, index))
            .or(self.kept.0.take());
        let mut expanded: Vec<u64> = self
            .expanded
            .iter()
            .filter_map(|&index| seq_of(&self.log, index))
            .collect();
        expanded.append(&mut self.kept.1);
        if let Some(range) = dropped {
            self.log.remove(id, range);
        }
        let _ = self.log.add(added);
        let index_of = |log: &Log, seq: u64| {
            log.entries
                .iter()
                .position(|entry| entry.record.boot_id == id && entry.record.seqnum == seq)
        };
        self.selected = selected.and_then(|seq| index_of(&self.log, seq));
        if self.selected.is_none() {
            self.kept.0 = selected;
        }
        let (held, gone): (Vec<u64>, Vec<u64>) = expanded
            .into_iter()
            .partition(|seq| index_of(&self.log, *seq).is_some());
        self.expanded = held
            .into_iter()
            .filter_map(|seq| index_of(&self.log, seq))
            .collect();
        self.kept.1 = gone;
        self.refresh();
        if let Some(anchor) = anchor {
            self.restore(anchor);
        }
    }

    /// Read the next page the view is nearing, if any: one page a call, so
    /// a frame reads at most one. Answers whether the table changed.
    pub fn page_in(&mut self) -> bool {
        // A page's records let go are freed a slice each step.
        self.log.sweep(1024);
        if self.page != crate::app::Page::Logs || !self.paged() || !self.whole_boot() {
            return false;
        }
        let margin = margin(self);
        // Dragged deep into the records not read: the window around where
        // the view is, at once, not page by page.
        if self.gaps.0 > 0.0 && self.scroll + self.table_view < self.gaps.0 - margin {
            self.jump_to_row(self.scroll / ROW);
            return true;
        }
        let bottom = self.table_height - self.gaps.1;
        if self.gaps.1 > 0.0 && self.scroll > bottom + margin {
            self.jump_to_row(self.scroll / ROW);
            return true;
        }
        let older = self.gaps.0 > 0.0 && self.scroll < self.gaps.0 + margin;
        let newer = self.gaps.1 > 0.0 && self.scroll + self.table_view > bottom - margin;
        let moved = match (older, newer, self.pager.as_mut()) {
            (true, _, Some(pager)) => pager.step_older(),
            (false, true, Some(pager)) => pager.step_newer(),
            _ => None,
        };
        let Some(moved) = moved else {
            return false;
        };
        self.apply(moved.added, moved.dropped);
        // Back at the newest with records followed meanwhile: read the
        // boot's end again, which holds them.
        if self.away > 0 && self.pager.as_ref().is_some_and(Pager::at_newest) {
            let anchor = self.anchor();
            self.page_boot(self.boot);
            self.refresh();
            if let Some(anchor) = anchor {
                self.restore(anchor);
            }
        }
        true
    }

    /// The window around the boot's record at `row` from its oldest, as a
    /// dragged scroll bar asks.
    fn jump_to_row(&mut self, row: f32) {
        let Some(pager) = self.pager.as_mut() else {
            return;
        };
        let Some(boot) = pager.boots().get(pager.boot()) else {
            return;
        };
        let (first, records) = (boot.first_seq, boot.records.max(1));
        let id = boot.id;
        // Sequence numbers of one boot run on with only a few gaps: the
        // row's share of the boot is its share of them.
        let share = (f64::from(row) / records as f64).clamp(0.0, 1.0);
        let last = pager
            .boots()
            .get(pager.boot() + 1)
            .map_or(first + records, |next| next.first_seq);
        let seq = first + (share * last.saturating_sub(first) as f64) as u64;
        let records = pager.jump(seq);
        self.log.clear_boot(id);
        let _ = self.log.add(records);
        self.refresh();
        let target = self
            .log
            .of_boot(self.boot)
            .iter()
            .position(|entry| entry.record.seqnum >= seq)
            .map(|at| at + self.log.boots.get(self.boot).map_or(0, |b| b.range.start));
        if let Some(index) = target {
            self.scroll_to(index);
        }
    }

    /// Before the query runs on a time window: when the window lies
    /// outside the records read, read the page around it.
    pub(crate) fn cover_window(&mut self) {
        if !self.paged() || self.whole_boot() {
            return;
        }
        let (from, to) = self.window();
        let entries = self.log.of_boot(self.boot);
        let (Some(first), Some(last)) = (entries.first(), entries.last()) else {
            return;
        };
        let Some(pager) = self.pager.as_mut() else {
            return;
        };
        let covered_from = pager.older() == 0 || first.record.realtime_us <= from;
        let covered_to = pager.at_newest() || last.record.realtime_us >= to;
        if covered_from && covered_to {
            return;
        }
        let id = first.record.boot_id;
        let records = pager.jump_to_time(from / 2 + to / 2);
        self.log.clear_boot(id);
        let _ = self.log.add(records);
    }

    /// Records followed into a log read a page at a time: kept when the
    /// window holds the boot's end, its oldest page let go as it grows;
    /// counted, and read when the view comes back, when it does not.
    /// Answers whether they were kept.
    pub(crate) fn follow_paged(&mut self, records: &[Record]) -> bool {
        let Some(pager) = self.pager.as_mut() else {
            return true;
        };
        pager.followed(records.len());
        if let Some(id) = pager.boots().get(pager.boot()).map(|boot| boot.id)
            && let Some(known) = self.log.known.iter_mut().find(|known| known.id == id)
        {
            known.records += records.len() as u64;
            known.last_us = records
                .iter()
                .map(|record| record.realtime_us)
                .fold(known.last_us, u64::max);
        }
        let Some(pager) = self.pager.as_mut() else {
            return true;
        };
        if !pager.at_newest() {
            self.away += records.len();
            return false;
        }
        let Some(id) = pager.boots().get(pager.boot()).map(|boot| boot.id) else {
            return true;
        };
        let held = self
            .log
            .entries
            .iter()
            .filter(|e| e.record.boot_id == id)
            .count();
        if held + records.len() > (WINDOW_PAGES + 1) * PAGE
            && let Some(range) = pager.trim_oldest()
        {
            self.log.remove(id, range);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{ROW, read_store};
    use crate::app::{Action, App, Origin, Page};
    use crate::paged::{PAGE, WINDOW_PAGES};
    use crate::store::Store;
    use ferrix_lode_core::record::Record;

    /// A store of one boot of `count` records in segments of 30,000.
    fn store(name: &str, count: u64) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lode-viewer-scrollback-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let records: Vec<Record> = (1..=count)
            .map(|seq| {
                let mut record = Record::new(6, 1_700_000_000_000_000 + seq * 1500, seq);
                record.seqnum = seq;
                record.boot_id = [7; 16];
                record.host = "box".into();
                record.push("MESSAGE", format!("line {seq}").as_bytes());
                // A source of its own for each of three in turn, so no run
                // of alike lines folds.
                let source = ["alpha", "beta", "gamma"][(seq % 3) as usize];
                record.push("SYSLOG_IDENTIFIER", source.as_bytes());
                record
            })
            .collect();
        crate::store::import_split(&records, &dir, 30_000).expect("written");
        dir
    }

    /// The viewer over `dir`, on Logs, the whole boot, a view 900 high.
    fn viewer(dir: &std::path::Path) -> App {
        let mut store = Store::at(&[dir.to_path_buf()]);
        let (log, pager, _) = read_store(&mut store);
        assert!(
            pager.is_some(),
            "a store this long is read a page at a time"
        );
        let mut app = App::new(log, Origin::Store(store), "test".to_owned());
        app.pager = pager;
        app.page = Page::Logs;
        app.act(&Action::Zoom(0));
        app.table_view = 900.0;
        app.refresh();
        app
    }

    /// The sequence number of the record at the view's top.
    fn at_top(app: &App) -> Option<u64> {
        let at = app.tops.partition_point(|top| *top <= app.scroll);
        let line = app.lines.get(at.checked_sub(1)?)?;
        Some(app.log.entries.get(line.record()?)?.record.seqnum)
    }

    #[test]
    fn scrolling_up_reads_older_pages_and_the_view_stays_on_its_record() {
        let dir = store("up", 300_000);
        let mut app = viewer(&dir);
        assert_eq!(app.log.entries.len(), PAGE * WINDOW_PAGES);
        assert!(app.gaps.0 > 0.0 && app.gaps.1 == 0.0);
        // It opens at the boot's newest records.
        assert!(app.scroll + app.table_view >= app.table_height - 1.0);
        assert_eq!(app.boot().map(|boot| boot.records), Some(300_000));
        // The newest record is the table's last line.
        let newest = app.lines.last().and_then(|line| line.record());
        assert_eq!(
            newest
                .and_then(|i| app.log.entries.get(i))
                .map(|e| e.record.seqnum),
            Some(300_000)
        );
        // Select a record, and scroll up to the window's oldest lines.
        let chosen = app.log.entries.len() - 10;
        app.selected = Some(chosen);
        let chosen_seq = app.log.entries[chosen].record.seqnum;
        let mut steps = 0;
        loop {
            app.scroll = app.gaps.0 + ROW * 5.0;
            let before = at_top(&app);
            if !app.page_in() {
                break;
            }
            steps += 1;
            assert_eq!(
                at_top(&app),
                before,
                "step {steps}: the view stayed on its record"
            );
            assert!(app.log.entries.len() <= PAGE * (WINDOW_PAGES + 1));
            let held = app.selected.map(|i| app.log.entries[i].record.seqnum);
            assert!(held.is_none() || held == Some(chosen_seq));
        }
        // At the boot's start: nothing older, its first record read.
        assert_eq!(app.gaps.0, 0.0);
        assert_eq!(app.log.entries.first().map(|e| e.record.seqnum), Some(1));
        assert!(app.gaps.1 > 0.0, "the newest records were let go");
        assert!(steps >= 50);
        // And back down to the newest, which hold the selection again.
        loop {
            app.scroll = (app.table_height - app.gaps.1 - app.table_view - ROW * 5.0).max(0.0);
            if !app.page_in() {
                break;
            }
        }
        assert_eq!(app.gaps.1, 0.0);
        assert_eq!(
            app.selected.map(|i| app.log.entries[i].record.seqnum),
            Some(chosen_seq),
            "the selection came back with its page"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn records_followed_at_the_end_are_kept_and_the_window_stays_bounded() {
        let dir = store("live", 250_000);
        let mut app = viewer(&dir);
        let record = |seq: u64| {
            let mut record = Record::new(6, 1_700_000_000_000_000 + seq * 1500, seq);
            record.seqnum = seq;
            record.boot_id = [7; 16];
            record.host = "box".into();
            record.push("MESSAGE", format!("new {seq}").as_bytes());
            record.push("SYSLOG_IDENTIFIER", b"live");
            record
        };
        // At the newest: two pages' worth arrive, the oldest page goes.
        let fresh: Vec<Record> = (250_001..=250_000 + 2 * PAGE as u64).map(record).collect();
        assert_eq!(app.arrived(fresh), 2 * PAGE);
        assert!(app.log.entries.len() <= PAGE * (WINDOW_PAGES + 1));
        assert_eq!(
            app.log.entries.iter().map(|e| e.record.seqnum).max(),
            Some(250_000 + 2 * PAGE as u64)
        );
        assert_eq!(
            app.boot().map(|boot| boot.records),
            Some(250_000 + 2 * PAGE as u64)
        );
        // Away from the end: counted, not added, until the view comes back.
        app.scroll = app.gaps.0 + ROW * 5.0;
        while app
            .pager
            .as_ref()
            .is_some_and(crate::paged::Pager::at_newest)
        {
            assert!(app.page_in());
            app.scroll = app.gaps.0 + ROW * 5.0;
        }
        let held = app.log.entries.len();
        assert_eq!(app.arrived(vec![record(400_000)]), 0);
        assert_eq!(app.log.entries.len(), held);
        assert_eq!(app.away, 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_drag_deep_into_what_is_not_read_reads_the_window_there() {
        let dir = store("drag", 300_000);
        let mut app = viewer(&dir);
        // A third of the way down the boot.
        app.scroll = 100_000.0 * ROW;
        assert!(app.page_in());
        let seqs: Vec<u64> = app.log.entries.iter().map(|e| e.record.seqnum).collect();
        assert!(
            seqs.contains(&100_000),
            "the window holds the record dragged to"
        );
        assert!(app.gaps.0 > 0.0 && app.gaps.1 > 0.0);
        let top = at_top(&app).expect("a record at the top");
        assert!(
            (95_000..=105_000).contains(&top),
            "the view is there: {top}"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
