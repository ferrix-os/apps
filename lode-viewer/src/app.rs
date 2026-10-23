//! What the window shows and what a press or a key does to it, with no
//! window: the page, the boot, the time cursor, the query and the table.
//! The window ([`crate::window`]) and `--render` both drive this.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::data::{Kind, Log};
use crate::lines::{Line, lines, offsets};
use crate::query::{Chip, Facets, Query, Saved};
use crate::store::{Status, Store};
use crate::theme::Mode;

/// The pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    /// A boot at a glance.
    #[default]
    Overview,
    /// The records.
    Logs,
}

impl Page {
    /// The page `--page` names.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "overview" => Some(Page::Overview),
            "logs" => Some(Page::Logs),
            _ => None,
        }
    }
}

/// Where the records come from.
#[derive(Debug)]
pub enum Origin {
    /// lode's store, which Live follows.
    Store(Store),
    /// A JSONL export, read once.
    File(PathBuf),
}

/// The zooms the track offers, in seconds; 0 is the whole boot.
pub const ZOOMS: [(&str, u64); 4] = [("30s", 30), ("1m", 60), ("5m", 300), ("boot", 0)];

/// Everything a press can do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Show a page.
    Page(Page),
    /// The crashes, in Logs.
    Crashes,
    /// Switch palette.
    Theme(Mode),
    /// Open or close the boot menu.
    BootMenu,
    /// Open or close the machine menu.
    MachineMenu,
    /// Show machine `index`.
    Machine(usize),
    /// Show boot `index`.
    Boot(usize),
    /// Turn Live on.
    Live,
    /// Move the window by seconds.
    Step(i64),
    /// The problem before the cursor.
    PrevProblem,
    /// The one after.
    NextProblem,
    /// A zoom, in seconds; 0 the whole boot.
    Zoom(u64),
    /// The track: the cursor goes where it is pressed, its left edge and
    /// width given.
    Track(f32, f32),
    /// A saved query, by index.
    Saved(usize),
    /// Keep the query typed as a new saved one.
    NewSaved,
    /// Save the query over the saved one in use.
    Save,
    /// Type into the query.
    EditQuery,
    /// Take a chip away.
    RemoveChip(usize),
    /// Add a chip.
    AddChip(Chip),
    /// Tick or untick a level.
    Level(u8),
    /// Only this source.
    Source(u16),
    /// Only this transport.
    Transport(u16),
    /// Only this session.
    Session(u16),
    /// Select a record.
    Select(usize),
    /// Expand or fold a fold or a held record.
    Expand(usize),
    /// Close the record's pane.
    CloseDetail,
    /// The 30 s around a record.
    Around(usize),
    /// Only a record's source.
    OnlySource(usize),
    /// Copy a record as JSON.
    CopyJson(usize),
    /// Copy a record's cursor.
    CopyCursor(usize),
    /// The window's records in Logs.
    WindowInLogs,
    /// Show the boot before beside this one.
    Compare,
    /// The table's overview strip: scroll to where it is pressed, its top
    /// and height given.
    Minimap(f32, f32),
}

/// The state.
#[derive(Debug)]
pub struct App {
    /// The records.
    pub log: Log,
    /// Where they came from.
    pub origin: Origin,
    /// What the origin is called in the header.
    pub origin_name: String,
    /// The store's state.
    pub status: Status,
    /// Things the person should know: a directory refused, a file unread.
    pub notices: Vec<String>,
    /// The page shown.
    pub page: Page,
    /// The palette.
    pub mode: Mode,
    /// The boot shown, an index into the log's boots.
    pub boot: usize,
    /// Whether the window follows the store's newest records.
    pub live: bool,
    /// The middle of the time window, microseconds since 1970.
    pub cursor: u64,
    /// Its width; 0 the whole boot.
    pub span: u64,
    /// The query.
    pub query: Query,
    /// The saved queries.
    pub saved: Vec<Saved>,
    /// The saved query in use.
    pub saved_in_use: Option<usize>,
    /// Where saved queries are kept.
    pub saved_file: Option<PathBuf>,
    /// Whether the query is being typed.
    pub editing: bool,
    /// What is typed.
    pub draft: String,
    /// The records the query lets through, by index into the log.
    pub found: Vec<usize>,
    /// The facet counts.
    pub facets: Facets,
    /// The table's lines.
    pub lines: Vec<Line>,
    /// Each line's top.
    pub tops: Vec<f32>,
    /// The table's height.
    pub table_height: f32,
    /// The folds and held records expanded.
    pub expanded: HashSet<usize>,
    /// The record selected.
    pub selected: Option<usize>,
    /// The table's scroll, in logical pixels.
    pub scroll: f32,
    /// The Overview's scroll.
    pub overview_scroll: f32,
    /// Whether the boot menu is open.
    pub boot_menu: bool,
    /// The machines whose stores are kept side by side (lode-receive's),
    /// when the viewer was opened on several.
    pub machines: Vec<crate::store::Machine>,
    /// The machine shown, in [`App::machines`].
    pub machine: usize,
    /// Whether the machine menu is open.
    pub machine_menu: bool,
    /// The receiver's root the machines were found in.
    pub machines_root: Option<std::path::PathBuf>,
    /// Whether the boot before is drawn beside this one.
    pub compare: bool,
    /// What the last copy put on the clipboard, for the window to offer.
    pub clipboard: Option<String>,
    /// A line said at the foot for a while.
    pub toast: Option<String>,
}

impl App {
    /// The viewer over `log`, from `origin`.
    #[must_use]
    pub fn new(log: Log, origin: Origin, origin_name: String) -> Self {
        let boot = log.boots.len().saturating_sub(1);
        let status = match &origin {
            Origin::Store(store) => store.status(),
            Origin::File(_) => Status::default(),
        };
        let mut app = App {
            log,
            origin,
            origin_name,
            status,
            notices: Vec::new(),
            page: Page::Overview,
            mode: Mode::Light,
            boot,
            live: false,
            cursor: 0,
            span: 60_000_000,
            query: Query::default(),
            saved: crate::query::builtin(),
            saved_in_use: None,
            saved_file: None,
            editing: false,
            draft: String::new(),
            found: Vec::new(),
            facets: Facets::default(),
            lines: Vec::new(),
            tops: Vec::new(),
            table_height: 0.0,
            expanded: HashSet::new(),
            selected: None,
            scroll: 0.0,
            overview_scroll: 0.0,
            boot_menu: false,
            machines: Vec::new(),
            machine: 0,
            machine_menu: false,
            machines_root: None,
            compare: false,
            clipboard: None,
            toast: None,
        };
        app.go_to_boot(boot);
        app
    }

    /// Show machine `index` of [`App::machines`]: its store read, its last
    /// boot in the window, Live kept as it was.
    pub fn open_machine(&mut self, index: usize) {
        let Some(machine) = self.machines.get(index).cloned() else {
            return;
        };
        let mut store = Store::at(std::slice::from_ref(&machine.dir));
        let (records, refused) = store.load();
        self.notices = refused;
        self.status = store.status();
        self.log = Log::new(records);
        self.origin = Origin::Store(store);
        self.origin_name.clone_from(&machine.name);
        self.machine = index;
        self.expanded.clear();
        self.selected = None;
        self.scroll = 0.0;
        self.go_to_boot(self.log.boots.len().saturating_sub(1));
    }

    /// Read the saved queries from `path`, keeping the built-in ones that
    /// it does not name again.
    pub fn load_saved(&mut self, path: PathBuf) {
        if let Ok(text) = std::fs::read_to_string(&path) {
            for each in crate::query::parse_saved(&text) {
                match self.saved.iter_mut().find(|known| known.name == each.name) {
                    Some(known) => *known = each,
                    None => self.saved.push(each),
                }
            }
        }
        self.saved_file = Some(path);
    }

    /// Keep the saved queries in their file.
    fn store_saved(&mut self) {
        let Some(path) = &self.saved_file else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::fs::write(path, crate::query::write_saved(&self.saved)) {
            Ok(()) => self.toast = Some(format!("saved to {}", path.display())),
            Err(error) => self.toast = Some(format!("{}: {error}", path.display())),
        }
    }

    /// The boot shown.
    #[must_use]
    pub fn boot(&self) -> Option<&crate::data::Boot> {
        self.log.boots.get(self.boot)
    }

    /// The boot's first time and its length.
    #[must_use]
    pub fn extent(&self) -> (u64, u64) {
        self.boot()
            .map_or((0, 1_000_000), |boot| (boot.first_us, boot.span()))
    }

    /// The time window: its first and last microsecond.
    #[must_use]
    pub fn window(&self) -> (u64, u64) {
        let (start, span) = self.extent();
        if self.span == 0 || self.span >= span {
            return (start, start + span);
        }
        let half = self.span / 2;
        let from = self
            .cursor
            .saturating_sub(half)
            .clamp(start, start + span - self.span);
        (from, from + self.span)
    }

    /// Whether the window is the whole boot.
    #[must_use]
    pub fn whole_boot(&self) -> bool {
        self.span == 0 || self.span >= self.extent().1
    }

    /// Show boot `index`, its end in the window.
    pub fn go_to_boot(&mut self, index: usize) {
        self.boot = index.min(self.log.boots.len().saturating_sub(1));
        let (start, span) = self.extent();
        self.cursor = start + span;
        self.query.boot = self.boot;
        self.selected = None;
        self.expanded.clear();
        self.boot_menu = false;
        self.refresh();
    }

    /// The time the cursor stands at, kept inside the boot.
    fn set_cursor(&mut self, at: u64) {
        let (start, span) = self.extent();
        self.cursor = at.clamp(start, start + span);
        self.live = false;
        self.refresh();
    }

    /// Keep the cursor where the whole window fits in the boot: the
    /// window is always as wide as its zoom says.
    fn clamp_cursor(&mut self) {
        let (start, span) = self.extent();
        if self.whole_boot() {
            return;
        }
        let half = self.span / 2;
        self.cursor = self.cursor.clamp(start + half, start + span - half);
    }

    /// Run the query again and lay the table out.
    pub fn refresh(&mut self) {
        self.clamp_cursor();
        self.query.window = if self.whole_boot() {
            None
        } else {
            Some(self.window())
        };
        let (found, facets) = self.query.run(&self.log);
        self.found = found;
        self.facets = facets;
        self.relayout();
    }

    /// Lay the table out again, the query unchanged.
    pub fn relayout(&mut self) {
        self.lines = lines(&self.log, &self.found, &self.expanded);
        let (tops, height) = offsets(&self.lines);
        self.tops = tops;
        self.table_height = height;
    }

    /// Records came: add them, and when Live keep the window at the end.
    pub fn arrived(&mut self, records: Vec<ferrix_lode_core::record::Record>) -> usize {
        if records.is_empty() {
            return 0;
        }
        let selected = self
            .selected
            .and_then(|index| self.log.entries.get(index))
            .map(|entry| (entry.record.seqnum, entry.record.boot_id));
        let last_boot = self.boot + 1 == self.log.boots.len();
        let added = self.log.add(records);
        if added == 0 {
            return 0;
        }
        // Indexes moved: find the selected record again.
        self.selected = selected.and_then(|key| {
            self.log
                .entries
                .iter()
                .position(|entry| (entry.record.seqnum, entry.record.boot_id) == key)
        });
        self.expanded.clear();
        if last_boot {
            self.boot = self.log.boots.len().saturating_sub(1);
            self.query.boot = self.boot;
        }
        if self.live {
            let (start, span) = self.extent();
            self.cursor = start + span;
        }
        self.refresh();
        added
    }

    /// Look again for the machines under the receiver's root: one that
    /// began sending since appears, and the one shown stays shown.
    pub fn rescan_machines(&mut self, root: &std::path::Path) -> bool {
        let found = crate::store::machines(root);
        if found.len() == self.machines.len() {
            self.machines = found;
            return false;
        }
        let shown = self
            .machines
            .get(self.machine)
            .map(|machine| machine.dir.clone());
        self.machine = shown
            .and_then(|dir| found.iter().position(|machine| machine.dir == dir))
            .unwrap_or(0);
        self.machines = found;
        true
    }

    /// Follow the store once: what it has that is new.
    pub fn follow(&mut self) -> usize {
        let Origin::Store(store) = &mut self.origin else {
            return 0;
        };
        let fresh = store.follow(self.log.last_seq);
        self.status = store.status();
        self.arrived(fresh)
    }

    /// Do `action`. `x` is where the pointer pressed, across.
    pub fn act(&mut self, action: &Action, x: f32) {
        if !matches!(action, Action::EditQuery) && self.editing {
            self.commit_draft();
        }
        if !matches!(action, Action::BootMenu | Action::Boot(_)) {
            self.boot_menu = false;
        }
        if !matches!(action, Action::MachineMenu | Action::Machine(_)) {
            self.machine_menu = false;
        }
        match action.clone() {
            Action::Page(page) => self.page = page,
            Action::Crashes => {
                self.query.chips.retain(|chip| *chip != Chip::Crashes);
                self.query.chips.push(Chip::Crashes);
                self.span = 0;
                self.page = Page::Logs;
                self.refresh();
            }
            Action::Theme(mode) => self.mode = mode,
            Action::BootMenu => self.boot_menu = !self.boot_menu,
            Action::MachineMenu => self.machine_menu = !self.machine_menu,
            Action::Machine(index) => {
                self.machine_menu = false;
                self.open_machine(index);
            }
            Action::Boot(index) => self.go_to_boot(index),
            Action::Live => {
                if matches!(self.origin, Origin::Store(_)) {
                    self.go_to_boot(self.log.boots.len().saturating_sub(1));
                    self.live = true;
                    let _ = self.follow();
                } else {
                    self.toast = Some("Live follows lode's store; this is a file".to_owned());
                }
            }
            Action::Step(seconds) => {
                let at = if seconds < 0 {
                    self.cursor
                        .saturating_sub(seconds.unsigned_abs() * 1_000_000)
                } else {
                    self.cursor + seconds.unsigned_abs() * 1_000_000
                };
                self.set_cursor(at);
            }
            Action::PrevProblem | Action::NextProblem => {
                let problems = crate::stats::problems(self.log.of_boot(self.boot));
                let found = if *action == Action::PrevProblem {
                    problems
                        .iter()
                        .rev()
                        .find(|&&at| at + 500_000 < self.cursor)
                } else {
                    problems.iter().find(|&&at| at > self.cursor + 500_000)
                };
                if let Some(&at) = found {
                    self.set_cursor(at);
                }
            }
            Action::Zoom(seconds) => {
                self.span = seconds * 1_000_000;
                self.refresh();
            }
            Action::Track(left, width) => {
                let (start, span) = self.extent();
                let fraction = ((x - left) / width.max(1.0)).clamp(0.0, 1.0);
                self.set_cursor(start + (f64::from(fraction) * span as f64) as u64);
            }
            Action::Saved(index) => {
                if let Some(saved) = self.saved.get(index).cloned() {
                    self.query.set_text(&saved.text);
                    self.draft = saved.text;
                    self.saved_in_use = Some(index);
                    self.page = Page::Logs;
                    self.refresh();
                }
            }
            Action::NewSaved => {
                let text = self.query.text.trim().to_owned();
                if text.is_empty() || self.query.error.is_some() {
                    self.toast = Some("type a query first, then + new keeps it".to_owned());
                } else {
                    let mut n = 1;
                    while self.saved.iter().any(|each| each.name == format!("q{n}")) {
                        n += 1;
                    }
                    self.saved.push(Saved {
                        name: format!("q{n}"),
                        text,
                    });
                    self.saved_in_use = Some(self.saved.len() - 1);
                    self.store_saved();
                }
            }
            Action::Save => {
                if let Some(index) = self.saved_in_use
                    && self.query.error.is_none()
                    && let Some(saved) = self.saved.get_mut(index)
                {
                    saved.text = self.query.text.trim().to_owned();
                    self.store_saved();
                }
            }
            Action::EditQuery => {
                if !self.editing {
                    self.editing = true;
                    self.draft = self.query.text.clone();
                }
            }
            Action::RemoveChip(index) => {
                if index < self.query.chips.len() {
                    let _ = self.query.chips.remove(index);
                    self.refresh();
                }
            }
            Action::AddChip(chip) => {
                if !self.query.chips.contains(&chip) {
                    self.query.chips.push(chip);
                    self.refresh();
                }
            }
            Action::Level(level) => {
                if let Some(ticked) = self.query.levels.get_mut(usize::from(level)) {
                    *ticked = !*ticked;
                    self.refresh();
                }
            }
            Action::Source(source) => self.toggle_chip(Chip::Source(source)),
            Action::Transport(transport) => self.toggle_chip(Chip::Transport(transport)),
            Action::Session(session) => self.toggle_chip(Chip::Session(session)),
            Action::Select(index) => self.selected = Some(index),
            Action::Expand(index) => {
                if !self.expanded.remove(&index) {
                    let _ = self.expanded.insert(index);
                }
                self.relayout();
            }
            Action::CloseDetail => self.selected = None,
            Action::Around(index) => {
                if let Some(at) = self.log.entries.get(index).map(crate::data::Entry::time) {
                    self.span = 60_000_000;
                    self.set_cursor(at);
                    self.scroll_to(index);
                }
            }
            Action::OnlySource(index) => {
                if let Some(source) = self.log.entries.get(index).map(|entry| entry.row.source) {
                    self.toggle_chip(Chip::Source(source));
                }
            }
            Action::CopyJson(index) => {
                if let Some(entry) = self.log.entries.get(index) {
                    let mut out = Vec::new();
                    ferrix_lode_core::format::write(
                        &entry.stored(),
                        ferrix_lode_core::format::Output::Json,
                        &ferrix_lode_core::time::Zone::utc(),
                        &mut out,
                    );
                    self.clipboard = Some(String::from_utf8_lossy(&out).trim_end().to_owned());
                    self.toast = Some("record copied as JSON".to_owned());
                }
            }
            Action::CopyCursor(index) => {
                if let Some(entry) = self.log.entries.get(index) {
                    self.clipboard = Some(ferrix_lode_core::cursor::of(&entry.record));
                    self.toast = Some("cursor copied".to_owned());
                }
            }
            Action::WindowInLogs => {
                self.page = Page::Logs;
                self.refresh();
                self.scroll = 0.0;
            }
            Action::Compare => self.compare = !self.compare,
            Action::Minimap(top, height) => {
                let _ = (top, height);
            }
        }
    }

    /// The minimap pressed at `y`: scroll there.
    pub fn minimap(&mut self, top: f32, height: f32, y: f32, view: f32) {
        let fraction = ((y - top) / height.max(1.0)).clamp(0.0, 1.0);
        self.scroll = (fraction * self.table_height - view / 2.0)
            .clamp(0.0, (self.table_height - view).max(0.0));
    }

    /// Put a chip on, or take it off when it is on.
    fn toggle_chip(&mut self, chip: Chip) {
        if let Some(at) = self.query.chips.iter().position(|each| *each == chip) {
            let _ = self.query.chips.remove(at);
        } else {
            self.query.chips.push(chip);
        }
        self.page = Page::Logs;
        self.refresh();
    }

    /// Scroll the table so the line of record `index` is near the top.
    pub fn scroll_to(&mut self, index: usize) {
        if let Some(top) = self
            .lines
            .iter()
            .zip(&self.tops)
            .find(|(line, _)| line.record() == Some(index))
            .map(|(_, top)| *top)
        {
            self.scroll = (top - 80.0).max(0.0);
        }
    }

    /// Take what was typed as the query.
    pub fn commit_draft(&mut self) {
        self.editing = false;
        let draft = self.draft.clone();
        self.query.set_text(&draft);
        if self.query.error.is_none() {
            self.refresh();
        }
    }

    /// A key, while the query is typed into: answers whether anything
    /// changed.
    pub fn type_key(&mut self, keysym: &str, text: &str) -> bool {
        match keysym {
            "Return" | "KP_Enter" => {
                self.commit_draft();
                self.scroll = 0.0;
            }
            "Escape" => {
                self.editing = false;
                self.draft = self.query.text.clone();
            }
            "BackSpace" => {
                let _ = self.draft.pop();
                self.query.set_text(&self.draft.clone());
            }
            _ if !text.is_empty() => {
                self.draft.push_str(text);
                self.query.set_text(&self.draft.clone());
            }
            _ => return false,
        }
        true
    }

    /// Whether the query differs from the saved one in use.
    #[must_use]
    pub fn query_edited(&self) -> bool {
        self.saved_in_use
            .and_then(|index| self.saved.get(index))
            .is_some_and(|saved| saved.text.trim() != self.query.text.trim())
    }

    /// How many crashes the boot has.
    #[must_use]
    pub fn crashes(&self) -> usize {
        self.log
            .of_boot(self.boot)
            .iter()
            .filter(|entry| entry.row.kind == Kind::Crash)
            .count()
    }
}

/// Time of day of `us` at `offset_s` from UTC: `17:22:43`.
#[must_use]
pub fn clock(us: u64, offset_s: i32) -> String {
    let civil = ferrix_lode_core::time::Civil::at(us, offset_s);
    format!("{:02}:{:02}:{:02}", civil.hour, civil.minute, civil.second)
}

/// The same with milliseconds: `17:21:37.256`.
#[must_use]
pub fn clock_ms(us: u64, offset_s: i32) -> String {
    let civil = ferrix_lode_core::time::Civil::at(us, offset_s);
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        civil.hour,
        civil.minute,
        civil.second,
        civil.micros / 1000
    )
}

/// Hours and minutes: `17:22`.
#[must_use]
pub fn clock_hm(us: u64, offset_s: i32) -> String {
    let civil = ferrix_lode_core::time::Civil::at(us, offset_s);
    format!("{:02}:{:02}", civil.hour, civil.minute)
}

#[cfg(test)]
mod tests {
    use super::{Action, App, Origin, Page, clock};
    use crate::data::Log;
    use ferrix_lode_core::record::Record;

    fn app() -> App {
        let records = (0..200)
            .map(|n| {
                let mut record = Record::new(
                    if n % 50 == 7 { 3 } else { 6 },
                    1_000_000_000 + n * 1_000_000,
                    n,
                );
                record.seqnum = n + 1;
                record.push(
                    "SYSLOG_IDENTIFIER",
                    if n % 2 == 0 { b"even" } else { b"odd" },
                );
                record.push("FERRIX_PRIORITY_SOURCE", b"stated");
                record.push("MESSAGE", format!("line {n}").as_bytes());
                record
            })
            .collect();
        App::new(
            Log::new(records),
            Origin::File("x.jsonl".into()),
            "x".to_owned(),
        )
    }

    #[test]
    fn the_window_ends_at_the_boots_end_and_steps_and_zooms() {
        let mut app = app();
        assert_eq!(
            app.window(),
            (1_000_000_000 + 139_000_000, 1_000_000_000 + 199_000_000)
        );
        assert_eq!(app.found.len(), 61);
        app.act(&Action::Step(-10), 0.0);
        assert_eq!(app.window().1, 1_189_000_000);
        app.act(&Action::Zoom(0), 0.0);
        assert!(app.whole_boot());
        assert_eq!(app.found.len(), 200);
        app.act(&Action::Zoom(30), 0.0);
        app.act(&Action::PrevProblem, 0.0);
        assert_eq!(app.cursor, 1_157_000_000);
        app.act(&Action::PrevProblem, 0.0);
        assert_eq!(app.cursor, 1_107_000_000);
        app.act(&Action::NextProblem, 0.0);
        assert_eq!(app.cursor, 1_157_000_000);
        app.act(&Action::Track(100.0, 200.0), 200.0);
        assert_eq!(app.cursor, 1_099_500_000);
    }

    #[test]
    fn typing_a_query_and_the_chips_narrow_the_table() {
        let mut app = app();
        app.act(&Action::Zoom(0), 0.0);
        app.act(&Action::EditQuery, 0.0);
        for character in "id = even".chars() {
            let _ = app.type_key("x", &character.to_string());
        }
        let _ = app.type_key("Return", "");
        assert_eq!(app.found.len(), 100);
        app.act(&Action::Level(6), 0.0);
        assert_eq!(app.found.len(), 0);
        app.act(&Action::Level(6), 0.0);
        let odd = app.log.sources.find("odd").expect("a source");
        app.act(&Action::Source(odd), 0.0);
        assert_eq!(app.found.len(), 0);
        app.act(&Action::RemoveChip(0), 0.0);
        app.act(&Action::Saved(0), 0.0);
        assert_eq!(app.page, Page::Logs);
        assert_eq!(app.found.len(), 4);
        app.act(&Action::CopyCursor(app.found[0]), 0.0);
        assert!(
            app.clipboard
                .as_deref()
                .is_some_and(|cursor| cursor.starts_with("i="))
        );
    }

    #[test]
    fn a_clock_reads_the_time_of_day() {
        assert_eq!(clock(1_792_776_109_770_158, 0), "17:21:49");
        assert_eq!(clock(1_792_776_109_770_158, 7200), "19:21:49");
    }
}
