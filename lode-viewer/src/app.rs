//! What the window shows and what a press or a key does to it, with no
//! window: the page, the boot, the time cursor, the query and the table.
//! The window ([`crate::window`]) and `--render` both drive this.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::data::Log;
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
    /// The machines followed, and adding one.
    Machines,
    /// The crashes, grouped.
    Crashes,
}

impl Page {
    /// The page `--page` names.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "overview" => Some(Page::Overview),
            "logs" => Some(Page::Logs),
            "machines" => Some(Page::Machines),
            "crashes" => Some(Page::Crashes),
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
    /// The Crashes page.
    Crashes,
    /// Crashes of every boot (or of the boot shown).
    CrashScope(bool),
    /// Only this program's crashes: an index into the programs, 0 every
    /// program.
    CrashProgram(usize),
    /// Only this signal's: an index into the signals, 0 every one.
    CrashSignal(usize),
    /// Show this group's crashes.
    CrashGroup(usize),
    /// Show this crash of the group.
    CrashPick(usize),
    /// The crash shown, in Logs.
    CrashInLogs,
    /// Switch palette.
    Theme(Mode),
    /// Open or close the boot menu.
    BootMenu,
    /// Type into a field of the Add machine form.
    FormFocus(FormField),
    /// Choose how the machine's log comes.
    FormKind(crate::remote::Kind),
    /// Add the machine the form describes, and follow it.
    FormAdd,
    /// Choose how ssh logs in to the machine.
    FormKeyChoice(KeyChoice),
    /// Open the file dialog for the key.
    FormChooseKey,
    /// A field of the form typed into: its text now.
    FormText(FormField, String),
    /// Paste the clipboard into the key area.
    FormPaste,
    /// Take this of the ways the machine may reach this one.
    FormServer(usize),
    /// Stop following a machine and forget its settings (its store stays).
    RemoveRemote(usize),
    /// Show a followed machine's store.
    OpenRemote(usize),
    /// Show a kept machine's settings in the form, to change them.
    SelectRemote(usize),
    /// An empty form, to add a machine.
    FormNew,
    /// Pair the machine the form shows again (TLS).
    FormPairAgain,
    /// Do the TLS set-up shown, on the machine.
    TlsConfirm,
    /// Leave it undone.
    TlsCancel,
    /// Move the window by the pointer: a press on the header.
    WindowMove,
    /// Maximize the window, or give it its own size again.
    WindowMaximize,
    /// Minimize the window.
    WindowMinimize,
    /// Close the window.
    WindowClose,
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
    /// The track: the cursor goes where it is pressed, a fraction of the
    /// lanes' width.
    Track(f32),
    /// A saved query, by index.
    Saved(usize),
    /// Keep the query typed as a new saved one.
    NewSaved,
    /// Save the query over the saved one in use.
    Save,
    /// Type into the query.
    EditQuery,
    /// The query typed into: its text now.
    QueryText(String),
    /// Return in the query: take it.
    QueryAccept,
    /// Escape in the query: leave it as it was.
    QueryCancel,
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
    /// Copy one field's whole value of the selected record, by its place
    /// among [`App::record_fields`].
    CopyField(usize),
    /// Name the record pane's fields by their raw keys, or as a person
    /// reads them.
    RawNames,
    /// The window's records in Logs.
    WindowInLogs,
    /// Show the boot before beside this one.
    Compare,
    /// The table's overview strip pressed: scroll there, a fraction of
    /// its height.
    MinimapAt(f32),
}

pub use crate::form::{Form, FormField, KeyChoice};

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
    /// The machines the viewer follows itself, from machines.conf.
    pub remotes: Vec<crate::remote::Remote>,
    /// Their links, by the same index; none until started.
    pub links: Vec<Option<crate::remote::Link>>,
    /// Where machines.conf is.
    pub remotes_file: Option<PathBuf>,
    /// The Add machine form.
    pub form: Form,
    /// The TLS set-up waiting for the person's confirm: the machine and
    /// what pairing said it needs.
    pub tls_plan: Option<(crate::remote::Remote, crate::tls::Plan)>,
    /// What the TLS set-up has done, a line a step, shared with its thread.
    pub tls_progress: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    /// How the TLS set-up ended, for the machine's pairing: its name and
    /// whether it was done.
    pub tls_outcome: std::sync::Arc<std::sync::Mutex<Option<(String, bool)>>>,
    /// The machine of [`App::remotes`] the form shows.
    pub selected_remote: Option<usize>,
    /// What `lode-receive check` said last, shared with the thread that
    /// asks it.
    pub receiving: std::sync::Arc<std::sync::Mutex<Option<crate::tls::Check>>>,
    /// Whether that thread runs.
    checking: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Wakes the window when the file dialog answered.
    pub waker: Option<compositor_toolkit::Waker>,
    /// The ways a machine might reach this one, for TLS's server.
    pub reach: Vec<String>,
    /// A paste the window should ask the compositor for.
    pub want_paste: Option<crate::selection::Which>,
    /// The Machines page's scroll.
    pub machines_scroll: f32,
    /// The Crashes page: every boot's, the program and the signal
    /// chosen, the group shown (by its program, place and signal) and
    /// the crash of it shown, newest first.
    pub crash_all: bool,
    /// The program chosen.
    pub crash_program: Option<String>,
    /// The signal chosen.
    pub crash_signal: Option<String>,
    /// The group shown.
    pub crash_group: Option<(String, String, String)>,
    /// The crash of it shown, by its place in the group.
    pub crash_pick: usize,
    /// Whether the record pane names its fields by their raw keys.
    pub raw_names: bool,
    /// How tall the table's view is, for the strip's press and the keys.
    pub table_view: f32,
    /// How far each page can scroll: the Overview, the table, Machines.
    pub scroll_most: [f32; 3],
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
            remotes: Vec::new(),
            links: Vec::new(),
            remotes_file: None,
            form: Form::default(),
            tls_plan: None,
            tls_progress: std::sync::Arc::default(),
            tls_outcome: std::sync::Arc::default(),
            selected_remote: None,
            receiving: std::sync::Arc::default(),
            checking: std::sync::Arc::default(),
            waker: None,
            reach: Vec::new(),
            want_paste: None,
            machines_scroll: 0.0,
            raw_names: false,
            crash_all: false,
            crash_program: None,
            crash_signal: None,
            crash_group: None,
            crash_pick: 0,
            table_view: 0.0,
            scroll_most: [0.0; 3],
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

    /// The receiver's root, where followed machines' stores go.
    fn root(&mut self) -> Option<PathBuf> {
        if self.machines_root.is_none() {
            self.machines_root = crate::cli::receive_root();
        }
        self.machines_root.clone()
    }

    /// Read machines.conf from `path` and start following each machine
    /// that comes over ssh.
    pub fn load_remotes(&mut self, path: PathBuf) {
        self.remotes = std::fs::read_to_string(&path)
            .map(|text| crate::remote::parse(&text))
            .unwrap_or_default();
        self.remotes_file = Some(path);
        self.links = self.remotes.iter().map(|_| None).collect();
        let root = self.root();
        for (remote, link) in self.remotes.iter().zip(self.links.iter_mut()) {
            if let (crate::remote::Kind::Ssh, Some(root)) = (remote.kind, &root) {
                *link = Some(crate::remote::Link::start(remote.clone(), root.clone()));
            }
        }
        self.learn_ids();
        self.ask_receiver();
    }

    /// Ask lode-receive what it keeps, on a thread of its own, when a
    /// machine comes over TLS and no ask is running.
    pub fn ask_receiver(&mut self) {
        use std::sync::atomic::Ordering;
        if !self
            .remotes
            .iter()
            .any(|remote| remote.kind == crate::remote::Kind::Tls)
            || crate::tls::receiver().is_none()
            || self.checking.swap(true, Ordering::AcqRel)
        {
            return;
        }
        let root = self.root();
        let (answer, running) = (
            std::sync::Arc::clone(&self.receiving),
            std::sync::Arc::clone(&self.checking),
        );
        let waker = self.waker.clone();
        let spawned = std::thread::Builder::new()
            .name("receive-check".to_owned())
            .spawn(move || {
                let check = crate::tls::check(root.as_ref()).ok();
                if let Ok(mut answer) = answer.lock() {
                    *answer = check;
                }
                running.store(false, Ordering::Release);
                if let Some(waker) = waker {
                    waker.wake();
                }
            });
        if spawned.is_err() {
            self.checking.store(false, Ordering::Release);
        }
    }

    /// The store kept machine `index`'s records go to: a followed one's
    /// link says it; a TLS one's is lode-receive's store that says it was
    /// paired as its name (or is named by its machine id).
    #[must_use]
    pub fn remote_store(&self, index: usize) -> Option<crate::store::Received> {
        let remote = self.remotes.get(index)?;
        match remote.kind {
            crate::remote::Kind::Ssh => {
                let dir = self.links.get(index)?.as_ref()?.now().dir?;
                let root = dir.parent()?.to_path_buf();
                crate::store::received(&root)
                    .into_iter()
                    .find(|store| store.dir == dir)
                    .or(Some(crate::store::Received {
                        dir,
                        ..crate::store::Received::default()
                    }))
            }
            crate::remote::Kind::Tls => {
                let root = self
                    .machines_root
                    .clone()
                    .or_else(crate::cli::receive_root)?;
                crate::store::paired_store(&root, &remote.name, remote.machine_id.as_deref())
            }
        }
    }

    /// A TLS machine's id, once the store it sends to is named by it, kept
    /// with its settings; and a machine whose records arrive is paired.
    pub fn learn_ids(&mut self) {
        let mut changed = false;
        // The receiver's own name, as its certificate holds it: the
        // server of a machine paired before the server was kept.
        let server_name = self
            .machines_root
            .clone()
            .or_else(crate::cli::receive_root)
            .and_then(|root| std::fs::read_to_string(root.join(".pki/server.names")).ok())
            .and_then(|names| {
                names
                    .split([',', '\n', ' '])
                    .map(str::trim)
                    .find(|name| !name.is_empty())
                    .map(str::to_owned)
            });
        for index in 0..self.remotes.len() {
            if self
                .remotes
                .get(index)
                .is_none_or(|remote| remote.kind != crate::remote::Kind::Tls)
            {
                continue;
            }
            let Some(store) = self.remote_store(index) else {
                continue;
            };
            let Some(remote) = self.remotes.get_mut(index) else {
                continue;
            };
            if let Some(id) = store.machine_id()
                && remote.machine_id.as_deref() != Some(id.as_str())
            {
                remote.machine_id = Some(id);
                changed = true;
            }
            if store.paired.as_deref() == Some(remote.name.as_str())
                && remote.pairing != crate::remote::Pairing::Paired
                && remote.pairing != crate::remote::Pairing::Applying
            {
                remote.pairing = crate::remote::Pairing::Paired;
                changed = true;
            }
            if remote.server.is_none()
                && remote.pairing == crate::remote::Pairing::Paired
                && let Some(name) = &server_name
            {
                let mut with = remote.clone();
                with.server = Some(name.clone());
                if with.refused().is_none() {
                    *remote = with;
                    changed = true;
                }
            }
        }
        if changed {
            self.store_remotes();
            if let Some(index) = self.selected_remote
                && let Some(remote) = self.remotes.get(index)
                && self.form.editing.as_deref() == Some(remote.name.as_str())
                && self.form.focus.is_none()
            {
                self.form = Form::of(remote);
            }
        }
    }

    /// The machines' part of the window's timer, `tick` its count: the TLS
    /// set-up's end taken into the machine's pairing, and every ten the
    /// receiver asked again. Answers whether anything changed.
    pub fn poll_machines(&mut self, tick: u32) -> bool {
        let outcome = self.tls_outcome.lock().ok().and_then(|mut o| o.take());
        let mut changed = false;
        if let Some((name, done)) = outcome
            && let Some(remote) = self.remotes.iter_mut().find(|r| r.name == name)
        {
            remote.pairing = if done {
                crate::remote::Pairing::Paired
            } else {
                crate::remote::Pairing::Failed
            };
            self.store_remotes();
            self.ask_receiver();
            changed = true;
        }
        if tick.is_multiple_of(10) {
            self.learn_ids();
            self.ask_receiver();
        }
        changed
    }

    /// Keep the machines in their file.
    fn store_remotes(&mut self) {
        let Some(path) = self.remotes_file.clone() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(error) = std::fs::write(&path, crate::remote::write(&self.remotes)) {
            self.toast = Some(format!("{}: {error}", path.display()));
        }
    }

    /// The form's machine added and followed, or, for a kept machine, its
    /// settings saved.
    fn add_remote(&mut self) {
        self.form.error = None;
        if let Some((field, why)) = self.form.first_problem() {
            self.form.error = Some(format!("{}: {why}", field_label(field)));
            self.form.focus = Some(field);
            return;
        }
        if self.form.editing.is_some() {
            self.save_remote(false);
            return;
        }
        if self
            .remotes
            .iter()
            .any(|known| known.name == self.form.value(FormField::Name).trim())
        {
            self.form.error = Some(format!(
                "{} is a machine already",
                self.form.value(FormField::Name).trim()
            ));
            return;
        }
        let kept = if self.form.key == KeyChoice::Paste {
            match crate::form::keep_key(
                self.form.value(FormField::Name).trim(),
                self.form.value(FormField::Paste),
            ) {
                Ok(path) => Some(path),
                Err(why) => {
                    self.form.error = Some(format!("the key could not be kept: {why}"));
                    return;
                }
            }
        } else {
            None
        };
        let remote = self.form.remote(kept.as_deref());
        if remote.kind == crate::remote::Kind::Tls {
            self.pair_tls(remote);
            return;
        }
        if let Some(why) = remote.refused() {
            self.form.error = Some(format!("not yet: {why}"));
            return;
        }
        if self.remotes.iter().any(|known| known.name == remote.name) {
            self.form.error = Some(format!("{} is a machine already", remote.name));
            return;
        }
        let Some(root) = self.root() else {
            self.form.error = Some("no home directory to keep its log in".to_owned());
            return;
        };
        self.links
            .push(Some(crate::remote::Link::start(remote.clone(), root)));
        self.toast = Some(format!("following {} over ssh", remote.name));
        self.remotes.push(remote);
        self.store_remotes();
        self.select_remote(self.remotes.len() - 1);
    }

    /// Show kept machine `index` in the form, its settings in the fields.
    fn select_remote(&mut self, index: usize) {
        let Some(remote) = self.remotes.get(index) else {
            return;
        };
        self.form = Form::of(remote);
        if remote.kind == crate::remote::Kind::Tls && self.reach.is_empty() {
            self.reach = crate::form::reach_choices();
        }
        self.selected_remote = Some(index);
        self.page = Page::Machines;
        self.ask_receiver();
    }

    /// The form's changes to the kept machine it shows, saved; with
    /// `pair`, or when a TLS machine's way to be reached changed, paired
    /// again (shown first, as a new pairing is).
    fn save_remote(&mut self, pair: bool) {
        let Some(name) = self.form.editing.clone() else {
            return;
        };
        let Some(index) = self.remotes.iter().position(|r| r.name == name) else {
            self.form.error = Some(format!("{name} is no longer kept"));
            return;
        };
        let pasted = self.form.key == KeyChoice::Paste
            && !self.form.value(FormField::Paste).trim().is_empty();
        let kept = if pasted {
            match crate::form::keep_key(&name, self.form.value(FormField::Paste)) {
                Ok(path) => Some(path),
                Err(why) => {
                    self.form.error = Some(format!("the key could not be kept: {why}"));
                    return;
                }
            }
        } else {
            None
        };
        let Some(old) = self.remotes.get(index).cloned() else {
            return;
        };
        let mut remote = self.form.remote(kept.as_deref());
        if let Some(why) = remote.refused() {
            self.form.error = Some(format!("not yet: {why}"));
            return;
        }
        remote.pairing = old.pairing;
        remote.ca_fingerprint.clone_from(&old.ca_fingerprint);
        remote.machine_id.clone_from(&old.machine_id);
        let tls = remote.kind == crate::remote::Kind::Tls;
        let repair = tls
            && (pair
                || old.kind != crate::remote::Kind::Tls
                // A server not kept before is taken as typed.
                || (old.server.is_some() && old.server != remote.server)
                || old.pairing != crate::remote::Pairing::Paired);
        if tls && repair {
            self.pair_tls(remote);
            return;
        }
        if !tls && remote.pairing != crate::remote::Pairing::None {
            remote.pairing = crate::remote::Pairing::None;
        }
        // A followed machine's link starts again with what changed.
        if let Some(Some(link)) = self.links.get(index) {
            link.stop();
        }
        let link = match (remote.kind, self.root()) {
            (crate::remote::Kind::Ssh, Some(root)) => {
                Some(crate::remote::Link::start(remote.clone(), root))
            }
            _ => None,
        };
        if let Some(slot) = self.links.get_mut(index) {
            *slot = link;
        }
        if let Some(slot) = self.remotes.get_mut(index) {
            *slot = remote;
        }
        self.store_remotes();
        self.toast = Some(format!("{name} saved"));
        self.select_remote(index);
    }

    /// Pair the form's machine with the receiver here and show what it
    /// needs done on the machine, for the person to confirm.
    fn pair_tls(&mut self, remote: crate::remote::Remote) {
        self.form.error = None;
        if crate::tls::receiver().is_none() {
            self.form.error = Some("lode-receive is not installed on this machine".to_owned());
            return;
        }
        let name_ok = !remote.name.is_empty()
            && remote.name.len() <= 32
            && remote
                .name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !name_ok {
            self.form.error = Some("a name of a-z, 0-9 and dashes, at most 32".to_owned());
            return;
        }
        if self.form.editing.as_deref() != Some(remote.name.as_str())
            && self.remotes.iter().any(|known| known.name == remote.name)
        {
            self.form.error = Some(format!("{} is a machine already", remote.name));
            return;
        }
        if let Some(why) = remote.refused() {
            self.form.error = Some(format!("not yet: {why}"));
            return;
        }
        let server = self.form.value(FormField::Server).trim().to_owned();
        if server.is_empty() {
            self.form.error =
                Some("say how the machine reaches this one (its name or address)".to_owned());
            return;
        }
        let root = self.root();
        match crate::tls::pair(&remote.name, &server, root.as_ref()) {
            Ok(plan) => {
                if let Ok(mut progress) = self.tls_progress.lock() {
                    progress.clear();
                }
                self.tls_plan = Some((remote, plan));
            }
            Err(why) => self.form.error = Some(why),
        }
    }

    /// The confirmed TLS set-up, done on its own thread: the files and the
    /// line on the machine, the upload restarted, the receiver here made
    /// to run, and the machine kept in machines.conf.
    fn apply_tls(&mut self) {
        let Some((remote, plan)) = self.tls_plan.take() else {
            return;
        };
        let progress = std::sync::Arc::clone(&self.tls_progress);
        let outcome = std::sync::Arc::clone(&self.tls_outcome);
        let waker = self.waker.clone();
        let root = self.root();
        let said = move |line: String| {
            if let Ok(mut progress) = progress.lock() {
                progress.push(line);
            }
        };
        let thread_remote = remote.clone();
        let ca = plan.ca_fingerprint.clone();
        let _ = std::thread::Builder::new()
            .name("tls-setup".to_owned())
            .spawn(move || {
                let result = crate::tls::apply(&thread_remote, &plan, &said)
                    .and_then(|()| crate::tls::ensure_running(root.as_ref()));
                let done = result.is_ok();
                match result {
                    Ok(line) => {
                        said(line);
                        said(format!("{} sends its log here now", thread_remote.name));
                    }
                    Err(why) => said(format!("stopped: {why}")),
                }
                if let Ok(mut outcome) = outcome.lock() {
                    *outcome = Some((thread_remote.name.clone(), done));
                }
                if let Some(waker) = waker {
                    waker.wake();
                }
            });
        self.keep_pairing(remote, &ca);
    }

    /// TLS machine `remote` kept as it is being paired, every setting with
    /// it and the CA it is paired with, and shown selected; the set-up's
    /// outcome comes to [`App::poll_machines`].
    fn keep_pairing(&mut self, mut remote: crate::remote::Remote, ca: &str) {
        remote.pairing = crate::remote::Pairing::Applying;
        remote.ca_fingerprint = (!ca.is_empty()).then(|| ca.to_owned());
        if remote.refused().is_some() {
            remote.ca_fingerprint = None;
        }
        let index = match self.remotes.iter().position(|r| r.name == remote.name) {
            Some(index) => {
                if let Some(Some(link)) = self.links.get(index) {
                    link.stop();
                }
                if let Some(slot) = self.links.get_mut(index) {
                    *slot = None;
                }
                if let Some(slot) = self.remotes.get_mut(index) {
                    *slot = remote;
                }
                index
            }
            None => {
                self.remotes.push(remote);
                self.links.push(None);
                self.remotes.len() - 1
            }
        };
        self.store_remotes();
        self.select_remote(index);
    }

    /// Stop following machine `index` and forget it; its store stays.
    fn remove_remote(&mut self, index: usize) {
        if index >= self.remotes.len() {
            return;
        }
        let gone = self.remotes.remove(index);
        if index < self.links.len() {
            if let Some(Some(link)) = self.links.get(index) {
                link.stop();
            }
            let _ = self.links.remove(index);
        }
        self.store_remotes();
        self.selected_remote = match self.selected_remote {
            Some(at) if at == index => None,
            Some(at) if at > index => Some(at - 1),
            other => other,
        };
        if self.form.editing.as_deref() == Some(gone.name.as_str()) {
            self.form = Form::default();
        }
        self.toast = Some(format!(
            "{} is no longer followed; what came is kept",
            gone.name
        ));
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
    pub fn act(&mut self, action: &Action) {
        if !matches!(
            action,
            Action::EditQuery | Action::QueryText(_) | Action::QueryAccept | Action::QueryCancel
        ) && self.editing
        {
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
            Action::Crashes => self.page = Page::Crashes,
            Action::CrashScope(all) => {
                self.crash_all = all;
                self.crash_group = None;
            }
            Action::CrashProgram(index) => {
                let (programs, _) = crate::crashes::choices(&self.crash_list());
                self.crash_program = index
                    .checked_sub(1)
                    .and_then(|at| programs.get(at).cloned());
                self.crash_group = None;
            }
            Action::CrashSignal(index) => {
                let (_, signals) = crate::crashes::choices(&self.crash_list());
                self.crash_signal = index.checked_sub(1).and_then(|at| signals.get(at).cloned());
                self.crash_group = None;
            }
            Action::CrashGroup(index) => {
                self.crash_group = self.crash_groups().0.get(index).map(|group| {
                    (
                        group.program.clone(),
                        group.address.clone(),
                        group.signal.clone(),
                    )
                });
                self.crash_pick = 0;
            }
            Action::CrashPick(index) => self.crash_pick = index,
            Action::CrashInLogs => {
                if let Some(index) = self.crash_shown().map(|crash| crash.index) {
                    if let Some(boot) = self
                        .log
                        .boots
                        .iter()
                        .position(|boot| boot.range.contains(&index))
                        && boot != self.boot
                    {
                        self.go_to_boot(boot);
                    }
                    self.span = 60_000_000;
                    if let Some(at) = self.log.entries.get(index).map(crate::data::Entry::time) {
                        self.set_cursor(at);
                    }
                    self.page = Page::Logs;
                    self.selected = Some(index);
                    self.scroll_to(index);
                }
            }
            Action::Theme(mode) => self.mode = mode,
            Action::BootMenu => self.boot_menu = !self.boot_menu,
            Action::FormFocus(field) => {
                self.page = Page::Machines;
                if self.form.focus != Some(field) {
                    self.form.all = false;
                }
                self.form.focus = Some(field);
            }
            Action::FormKind(kind) => {
                if kind == crate::remote::Kind::Tls && crate::tls::receiver().is_none() {
                    self.form.error = Some(
                        "Forwarded over TLS needs lode-receive on this machine: install-linux.sh installs it beside the viewer".to_owned(),
                    );
                    return;
                }
                self.form.kind = kind;
                self.form.error = None;
                if kind == crate::remote::Kind::Tls {
                    if self.reach.is_empty() {
                        self.reach = crate::form::reach_choices();
                    }
                    if self.form.value(FormField::Server).is_empty()
                        && let Some(first) = self.reach.first().cloned()
                    {
                        self.form.set(FormField::Server, &first);
                    }
                }
                if self
                    .form
                    .focus
                    .is_some_and(|field| !self.form.shown().contains(&field))
                {
                    self.form.focus = None;
                }
            }
            Action::FormKeyChoice(choice) => {
                self.form.key = choice;
                self.form.error = None;
                self.form.focus = match choice {
                    KeyChoice::Agent => None,
                    KeyChoice::File => Some(FormField::Key),
                    KeyChoice::Paste => Some(FormField::Paste),
                };
            }
            Action::FormChooseKey => self.form.key = KeyChoice::File,
            Action::FormText(field, text) => {
                self.form.focus = Some(field);
                self.form.set(field, &text);
            }
            Action::FormPaste => {
                self.form.key = KeyChoice::Paste;
                self.form.focus = Some(FormField::Paste);
                self.want_paste = Some(crate::selection::Which::Clipboard);
            }
            Action::FormServer(index) => {
                if let Some(choice) = self.reach.get(index).cloned() {
                    self.form.set(FormField::Server, &choice);
                }
            }
            Action::FormAdd => self.add_remote(),
            Action::TlsConfirm => self.apply_tls(),
            Action::TlsCancel => {
                self.tls_plan = None;
                self.form.error = Some("nothing was changed on the machine".to_owned());
            }
            Action::RemoveRemote(index) => self.remove_remote(index),
            Action::SelectRemote(index) => self.select_remote(index),
            Action::FormNew => {
                self.form = Form::default();
                self.selected_remote = None;
            }
            Action::FormPairAgain => {
                self.form.error = None;
                if let Some((field, why)) = self.form.first_problem() {
                    self.form.error = Some(format!("{}: {why}", field_label(field)));
                    self.form.focus = Some(field);
                } else {
                    self.save_remote(true);
                }
            }
            Action::OpenRemote(index) => {
                let dir = self.remote_store(index).map(|store| store.dir);
                if let Some(root) = self.root() {
                    let _ = self.rescan_machines(&root);
                }
                match dir.and_then(|dir| self.machines.iter().position(|m| m.dir == dir)) {
                    Some(at) => {
                        self.open_machine(at);
                        self.live = true;
                        self.page = Page::Overview;
                    }
                    None => self.toast = Some("nothing has arrived from it yet".to_owned()),
                }
            }
            // The window's own: the window does them.
            Action::WindowMove
            | Action::WindowMaximize
            | Action::WindowMinimize
            | Action::WindowClose => {}
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
            Action::Track(fraction) => {
                let (start, span) = self.extent();
                let fraction = fraction.clamp(0.0, 1.0);
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
            Action::QueryText(text) => {
                self.editing = true;
                self.draft = text;
                self.query.set_text(&self.draft.clone());
            }
            Action::QueryAccept => {
                self.commit_draft();
                self.scroll = 0.0;
            }
            Action::QueryCancel => {
                self.editing = false;
                self.draft = self.query.text.clone();
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
            Action::CopyField(field) => {
                let fields = self
                    .selected
                    .map(|index| self.record_fields(index))
                    .unwrap_or_default();
                if let Some((key, value)) = fields.into_iter().nth(field) {
                    let name = ferrix_lode_core::fields::name_of(&key)
                        .map_or(key.clone(), |f| f.name.to_owned());
                    self.clipboard = Some(value);
                    self.toast = Some(format!("{name} copied"));
                }
            }
            Action::RawNames => self.raw_names = !self.raw_names,
            Action::WindowInLogs => {
                self.page = Page::Logs;
                self.refresh();
                self.scroll = 0.0;
            }
            Action::Compare => self.compare = !self.compare,
            Action::MinimapAt(fraction) => {
                let view = self.table_view;
                self.scroll = (fraction.clamp(0.0, 1.0) * self.table_height - view / 2.0)
                    .clamp(0.0, (self.table_height - view).max(0.0));
            }
        }
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

    /// Select the record `by` records on from the selected one, and keep
    /// it in view.
    pub fn move_selection(&mut self, by: isize) {
        let records: Vec<usize> = self
            .lines
            .iter()
            .filter_map(|line| match line {
                Line::Record(index) => Some(*index),
                _ => None,
            })
            .collect();
        if records.is_empty() {
            return;
        }
        let at = self
            .selected
            .and_then(|selected| records.iter().position(|&index| index == selected))
            .map_or(0, |at| at.saturating_add_signed(by).min(records.len() - 1));
        let Some(&chosen) = records.get(at) else {
            return;
        };
        self.page = Page::Logs;
        self.selected = Some(chosen);
        if let Some(top) = self
            .lines
            .iter()
            .zip(&self.tops)
            .find(|(line, _)| **line == Line::Record(chosen))
            .map(|(_, top)| *top)
        {
            let view = self.table_view;
            if top < self.scroll {
                self.scroll = top;
            } else if top + 28.0 > self.scroll + view {
                self.scroll = top + 28.0 - view;
            }
        }
    }

    /// Scroll the page shown by `by` pixels.
    pub fn scroll_by(&mut self, by: f32) {
        let (scroll, most) = match self.page {
            Page::Overview => (&mut self.overview_scroll, self.scroll_most[0]),
            Page::Logs => (&mut self.scroll, self.scroll_most[1]),
            Page::Machines => (&mut self.machines_scroll, self.scroll_most[2]),
            Page::Crashes => return,
        };
        *scroll = (*scroll + by).clamp(0.0, most);
    }

    /// Record `index`'s fields as the record pane lists them, by their
    /// raw keys: its level, what it carries (its message left to the
    /// pane's own box), then lode's own -- its time, host, boot, sequence
    /// and cursor -- each in its group's place
    /// (`ferrix_lode_core::fields`), whole.
    #[must_use]
    pub fn record_fields(&self, index: usize) -> Vec<(String, String)> {
        let Some(entry) = self.log.entries.get(index) else {
            return Vec::new();
        };
        let mut fields: Vec<(String, String)> = vec![(
            "PRIORITY".to_owned(),
            crate::ui::level_label(entry.stored_level()),
        )];
        for field in &entry.record.fields {
            if field.name == "MESSAGE" {
                continue;
            }
            let value = match std::str::from_utf8(&field.value) {
                Ok(text) => text.to_owned(),
                Err(_) => format!("[{} bytes of binary data]", field.value.len()),
            };
            fields.push((field.name.clone(), value));
        }
        let mut boot = String::new();
        ferrix_lode_core::segment::push_hex(&mut boot, &entry.record.boot_id);
        let civil = ferrix_lode_core::time::Civil::at(entry.time(), entry.record.utc_offset_s);
        fields.push((
            "__REALTIME_TIMESTAMP".to_owned(),
            format!(
                "{:04}-{:02}-{:02} {} {}",
                civil.year,
                civil.month,
                civil.day,
                clock_ms(entry.time(), entry.record.utc_offset_s),
                civil.offset()
            ),
        ));
        fields.push((
            "__MONOTONIC_TIMESTAMP".to_owned(),
            format!("{:.6} s", entry.record.monotonic_us as f64 / 1e6),
        ));
        fields.push(("_HOSTNAME".to_owned(), entry.record.host.to_string()));
        fields.push(("_BOOT_ID".to_owned(), boot));
        fields.push(("__SEQNUM".to_owned(), entry.record.seqnum.to_string()));
        fields.push((
            "__CURSOR".to_owned(),
            ferrix_lode_core::cursor::of(&entry.record),
        ));
        // By group, then the table's order; unknown keys last, by name.
        fields.sort_by(|a, b| {
            let key = |name: &str| {
                let known = ferrix_lode_core::fields::name_of(name);
                (
                    known.map_or(ferrix_lode_core::fields::FieldGroup::Other, |f| f.group),
                    ferrix_lode_core::fields::order_of(name),
                    name.to_owned(),
                )
            };
            key(&a.0).cmp(&key(&b.0))
        });
        fields
    }

    /// The crashes the Crashes page lists: the boot's, or every boot's.
    #[must_use]
    pub fn crash_list(&self) -> Vec<crate::crashes::Crash> {
        crate::crashes::crashes(&self.log, (!self.crash_all).then_some(self.boot))
    }

    /// The Crashes page's groups, filtered, and the crashes they are of.
    #[must_use]
    pub fn crash_groups(&self) -> (Vec<crate::crashes::Group>, Vec<crate::crashes::Crash>) {
        let crashes = self.crash_list();
        let groups = crate::crashes::groups(
            &crashes,
            self.crash_program.as_deref(),
            self.crash_signal.as_deref(),
        );
        (groups, crashes)
    }

    /// The crash the Crashes page shows, if one.
    #[must_use]
    pub fn crash_shown(&self) -> Option<crate::crashes::Crash> {
        let (groups, crashes) = self.crash_groups();
        let (program, address, signal) = self.crash_group.as_ref()?;
        let group = groups.iter().find(|group| {
            group.program == *program && group.address == *address && group.signal == *signal
        })?;
        let at = group
            .crashes
            .get(self.crash_pick)
            .or(group.crashes.first())?;
        crashes.get(*at).cloned()
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
        crate::crashes::crashes(&self.log, Some(self.boot)).len()
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

/// A field's label, for an error that names it.
#[must_use]
pub fn field_label(field: FormField) -> &'static str {
    match field {
        FormField::Name => "Name",
        FormField::Host => "Host",
        FormField::User => "User",
        FormField::Port => "Port",
        FormField::Key => "Key file",
        FormField::Paste => "Pasted key",
        FormField::Server => "This machine",
    }
}

/// What a kept machine's row says of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteLine {
    /// Its state and records, in words.
    pub text: String,
    /// Whether its records are coming.
    pub up: bool,
    /// What is wrong, if anything.
    pub error: Option<String>,
    /// The store its records go to.
    pub store: Option<PathBuf>,
}

/// `n` with its thousands grouped: `96,727`.
#[must_use]
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (at, c) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// An RFC 3339 time in UTC as the rows say it: `2026-10-23 20:49:24 UTC`.
#[must_use]
pub fn when(rfc3339: &str) -> String {
    match rfc3339.strip_suffix('Z') {
        Some(rest) => format!("{} UTC", rest.replacen('T', " ", 1)),
        None => rfc3339.replacen('T', " ", 1),
    }
}

impl App {
    /// What kept machine `index`'s row says: for one followed over ssh its
    /// link's state; for one over TLS what lode-receive keeps of it, or
    /// where its pairing stands.
    #[must_use]
    pub fn remote_line(&self, index: usize) -> RemoteLine {
        let Some(remote) = self.remotes.get(index) else {
            return RemoteLine::default();
        };
        match remote.kind {
            crate::remote::Kind::Ssh => {
                let state = self
                    .links
                    .get(index)
                    .and_then(|link| link.as_ref())
                    .map(crate::remote::Link::now)
                    .unwrap_or_default();
                let records = match state.last_seen {
                    Some(at) => format!(
                        "{} records this run, the last at {} UTC",
                        grouped(state.records),
                        clock(at * 1_000_000, 0)
                    ),
                    None => "no records yet".to_owned(),
                };
                RemoteLine {
                    text: format!("{} · {records}", state.state),
                    up: state.up,
                    error: state.error,
                    store: state.dir,
                }
            }
            crate::remote::Kind::Tls => {
                let check = self.receiving.lock().ok().and_then(|check| check.clone());
                let store = self.remote_store(index);
                let error = match (&check, remote.pairing) {
                    (_, crate::remote::Pairing::Failed) => {
                        Some("the set-up on the machine stopped: Pair again".to_owned())
                    }
                    (Some(check), _) if !check.running => {
                        Some("lode-receive is not running here".to_owned())
                    }
                    _ => None,
                };
                let Some(store) = store else {
                    let text = match remote.pairing {
                        crate::remote::Pairing::Paired => "paired · no records yet",
                        crate::remote::Pairing::Applying => "being set up on the machine",
                        crate::remote::Pairing::Failed => "not paired",
                        crate::remote::Pairing::None => "not paired yet",
                    };
                    return RemoteLine {
                        text: text.to_owned(),
                        up: false,
                        error,
                        store: None,
                    };
                };
                let counted = check.as_ref().and_then(|check| check.of(&store.dir));
                let seen = counted
                    .and_then(|line| line.last_seen.clone())
                    .or_else(|| store.last_seen.clone());
                let at = seen.map_or_else(String::new, |seen| format!(", last at {}", when(&seen)));
                let text = match counted {
                    Some(line) => format!("receiving: {} records{at}", grouped(line.records)),
                    None => format!("receiving{at}"),
                };
                RemoteLine {
                    text,
                    up: check.as_ref().is_none_or(|check| check.running),
                    error,
                    store: Some(store.dir),
                }
            }
        }
    }
}

/// A path with the home directory written `~`.
#[must_use]
pub fn home_short(path: &std::path::Path) -> String {
    if let Some(home) = std::env::var_os("HOME")
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

impl App {
    /// The UTC offset times are shown at: the boot's own, as lode stored
    /// it.
    #[must_use]
    pub fn zone(&self) -> i32 {
        self.log
            .of_boot(self.boot)
            .first()
            .map_or(0, |entry| entry.record.utc_offset_s)
    }

    /// The boot's offset as the picker says it.
    #[must_use]
    pub fn offset_of_boot_label(&self) -> i64 {
        self.log.offset_of(self.boot)
    }
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
        app.act(&Action::Step(-10));
        assert_eq!(app.window().1, 1_189_000_000);
        app.act(&Action::Zoom(0));
        assert!(app.whole_boot());
        assert_eq!(app.found.len(), 200);
        app.act(&Action::Zoom(30));
        app.act(&Action::PrevProblem);
        assert_eq!(app.cursor, 1_157_000_000);
        app.act(&Action::PrevProblem);
        assert_eq!(app.cursor, 1_107_000_000);
        app.act(&Action::NextProblem);
        assert_eq!(app.cursor, 1_157_000_000);
        app.act(&Action::Track(0.5));
        assert_eq!(app.cursor, 1_099_500_000);
    }

    #[test]
    fn typing_a_query_and_the_chips_narrow_the_table() {
        let mut app = app();
        app.act(&Action::Zoom(0));
        app.act(&Action::EditQuery);
        let mut typed = String::new();
        for character in "id = even".chars() {
            typed.push(character);
            app.act(&Action::QueryText(typed.clone()));
        }
        app.act(&Action::QueryAccept);
        assert_eq!(app.found.len(), 100);
        app.act(&Action::Level(6));
        assert_eq!(app.found.len(), 0);
        app.act(&Action::Level(6));
        let odd = app.log.sources.find("odd").expect("a source");
        app.act(&Action::Source(odd));
        assert_eq!(app.found.len(), 0);
        app.act(&Action::RemoveChip(0));
        app.act(&Action::Saved(0));
        assert_eq!(app.page, Page::Logs);
        assert_eq!(app.found.len(), 4);
        app.act(&Action::CopyCursor(app.found[0]));
        assert!(
            app.clipboard
                .as_deref()
                .is_some_and(|cursor| cursor.starts_with("i="))
        );
    }

    #[test]
    fn add_names_the_first_field_wrong_and_goes_to_it() {
        use super::FormField;
        let mut app = app();
        app.act(&Action::FormAdd);
        assert_eq!(app.form.focus, Some(FormField::Name));
        assert!(
            app.form
                .error
                .as_deref()
                .is_some_and(|e| e.starts_with("Name:"))
        );
        app.form.set(FormField::Host, "box.lan");
        app.act(&Action::FormKeyChoice(super::KeyChoice::Paste));
        assert_eq!(app.form.focus, Some(FormField::Paste));
        app.act(&Action::FormText(FormField::Paste, "x\n".to_owned()));
        assert_eq!(app.form.value(FormField::Paste), "x\n");
        app.act(&Action::FormAdd);
        assert_eq!(app.form.focus, Some(FormField::Paste));
        assert!(app.remotes.is_empty());
    }

    /// Run by hand against a Ferrix guest with sshd (`run-compositor
    /// --ssh PORT`): the form filled and pressed as a person would, a
    /// pasted key kept and followed live, then the TLS set-up planned and
    /// applied. `LODE_E2E_PORT`, `LODE_E2E_KEY` (a private key the guest
    /// takes), `LODE_E2E_DIR` (a scratch directory: the config, the stores
    /// and the receiver's root go there).
    #[test]
    #[ignore = "needs a guest with sshd"]
    fn the_form_adds_a_guest_live_and_pairs_it_over_tls() {
        use super::{FormField, KeyChoice};
        use crate::remote::Kind;
        let var = |name: &str| std::env::var(name).expect(name);
        let dir = std::path::PathBuf::from(var("LODE_E2E_DIR"));
        let key = std::fs::read_to_string(var("LODE_E2E_KEY")).expect("the key");
        let mut app = app();
        app.machines_root = Some(dir.join("root"));
        app.remotes_file = Some(dir.join("machines.conf"));
        // Live over SSH, the key pasted.
        app.act(&Action::FormText(FormField::Host, "127.0.0.1".to_owned()));
        app.form.set(FormField::Name, "guest");
        app.form.set(FormField::Port, &var("LODE_E2E_PORT"));
        app.act(&Action::FormKeyChoice(KeyChoice::Paste));
        app.act(&Action::FormText(FormField::Paste, key.clone()));
        app.act(&Action::FormAdd);
        assert_eq!(app.form.error, None);
        assert_eq!(app.remotes.len(), 1);
        let kept = app.remotes[0].key.clone().expect("a kept key");
        println!("key kept at {kept}");
        let started = std::time::Instant::now();
        let mut records = 0;
        while started.elapsed() < std::time::Duration::from_secs(40) {
            let state = app.links[0]
                .as_ref()
                .map(crate::remote::Link::now)
                .unwrap_or_default();
            records = state.records;
            if records > 0 && state.up {
                println!("live: {} records, {}", state.records, state.state);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        assert!(records > 0, "no records arrived");
        // Forwarded over TLS, the same key as a file.
        app.act(&Action::FormKind(Kind::Tls));
        assert_eq!(app.form.error, None, "TLS is offered");
        assert!(
            !app.form.value(FormField::Server).is_empty(),
            "the server is filled in"
        );
        app.form.set(FormField::Server, "10.0.2.2");
        app.form.set(FormField::Host, "127.0.0.1");
        app.form.set(FormField::Name, "guest-tls");
        app.form.set(FormField::Port, &var("LODE_E2E_PORT"));
        app.act(&Action::FormKeyChoice(KeyChoice::File));
        app.form.set(FormField::Key, &kept);
        app.act(&Action::FormAdd);
        assert_eq!(app.form.error, None);
        let (_, plan) = app.tls_plan.clone().expect("a plan to confirm");
        for command in crate::tls::remote_commands(&plan) {
            println!("plan: {command}");
        }
        app.act(&Action::TlsConfirm);
        let started = std::time::Instant::now();
        loop {
            let lines = app
                .tls_progress
                .lock()
                .map(|l| l.clone())
                .unwrap_or_default();
            if lines
                .iter()
                .any(|l| l.starts_with("stopped") || l.ends_with("sends its log here now"))
                || started.elapsed() > std::time::Duration::from_secs(60)
            {
                for line in &lines {
                    println!("tls: {line}");
                }
                assert!(
                    lines.iter().any(|l| l.ends_with("sends its log here now")),
                    "{lines:?}"
                );
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    /// A scratch directory of the test's own.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lode-viewer-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory");
        dir
    }

    /// Wait for a receiver check the app started to end, so what the test
    /// sets is not overwritten.
    fn checked(app: &App) {
        while app.checking.load(std::sync::atomic::Ordering::Acquire) {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// The customer's N150, 2026-10-23: paired over TLS from the form
    /// before the pairing was kept, sending as host `ferrix` with no
    /// machine id, so lode-receive keeps it in `host-ferrix/` with
    /// `paired=n150`. Its row shows that store's records, its settings
    /// show in the form, and Open shows the store.
    #[test]
    fn a_tls_machine_shows_the_store_it_was_paired_into() {
        use super::FormField;
        use crate::remote::{Kind, Pairing};
        let dir = scratch("tls-n150");
        let root = dir.join("lode-receive");
        let records: Vec<Record> = (1..=3)
            .map(|n| {
                let mut record = Record::new(6, n * 1_000_000, n);
                record.seqnum = n;
                record.push("MESSAGE", b"hello");
                record
            })
            .collect();
        crate::store::import(&records, &root.join("host-ferrix")).expect("a store");
        std::fs::write(
            root.join("host-ferrix/machine"),
            "hostname=ferrix\nlast_seen=2026-10-23T20:49:21Z\npaired=n150\n",
        )
        .expect("written");
        std::fs::create_dir_all(root.join(".pki")).expect("a directory");
        std::fs::write(root.join(".pki/server.names"), "192.0.2.74\n").expect("written");
        let conf = dir.join("machines.conf");
        std::fs::write(
            &conf,
            "[machine n150]\nkind = tls\nhost = 192.0.2.10\nuser = root\nport = 22\n",
        )
        .expect("written");
        let mut app = app();
        app.machines_root = Some(root.clone());
        app.load_remotes(conf.clone());
        checked(&app);
        // Its records found by the paired name, the pairing and the
        // server learned from the receiver and kept.
        assert_eq!(
            app.remote_store(0).map(|store| store.dir),
            Some(root.join("host-ferrix"))
        );
        assert_eq!(app.remotes[0].pairing, Pairing::Paired);
        assert_eq!(app.remotes[0].server.as_deref(), Some("192.0.2.74"));
        let kept = std::fs::read_to_string(&conf).expect("kept");
        assert!(kept.contains("server = 192.0.2.74\n"), "{kept}");
        assert!(kept.contains("pairing = paired\n"), "{kept}");
        // Without the receiver's count: when it was last heard.
        if let Ok(mut receiving) = app.receiving.lock() {
            *receiving = None;
        }
        let line = app.remote_line(0);
        assert_eq!(line.text, "receiving, last at 2026-10-23 20:49:21 UTC");
        assert!(!line.text.contains("nothing yet"));
        // With it: the records lode-receive holds.
        if let Ok(mut receiving) = app.receiving.lock() {
            *receiving = Some(crate::tls::Check {
                running: true,
                listen: "0.0.0.0:19532".to_owned(),
                machines: vec![crate::tls::Receiving {
                    dir: "host-ferrix".to_owned(),
                    records: 96_242,
                    last_seen: Some("2026-10-23T20:49:24Z".to_owned()),
                }],
            });
        }
        let line = app.remote_line(0);
        assert_eq!(
            line.text,
            "receiving: 96,242 records, last at 2026-10-23 20:49:24 UTC"
        );
        assert!(line.up && line.error.is_none());
        // Selected: every setting in the form, the name fixed.
        app.act(&Action::SelectRemote(0));
        assert_eq!(app.form.editing.as_deref(), Some("n150"));
        assert_eq!(app.form.kind, Kind::Tls);
        assert_eq!(app.form.value(FormField::Host), "192.0.2.10");
        assert_eq!(app.form.value(FormField::Server), "192.0.2.74");
        assert!(!app.form.shown().contains(&FormField::Name));
        // Changed and saved: kept, no new pairing for an unchanged server.
        app.form.set(FormField::User, "admin");
        app.act(&Action::FormAdd);
        assert_eq!(app.form.error, None);
        assert!(app.tls_plan.is_none());
        let kept = std::fs::read_to_string(&conf).expect("kept");
        assert!(kept.contains("user = admin\n"), "{kept}");
        assert!(kept.contains("pairing = paired\n"), "{kept}");
        assert_eq!(app.form.value(FormField::User), "admin");
        // Open shows its store, followed live.
        app.act(&Action::OpenRemote(0));
        assert_eq!(app.origin_name, "n150 · ferrix");
        assert_eq!(app.log.entries.len(), 3);
        assert!(app.live);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// After Pair: the machine kept with every setting, shown selected
    /// with them rather than an empty form, and the set-up's end taken
    /// into its pairing.
    #[test]
    fn a_paired_machine_is_kept_whole_and_shown_selected() {
        use super::{FormField, KeyChoice};
        use crate::remote::{KeySource, Kind, Pairing};
        let dir = scratch("tls-pair");
        let conf = dir.join("machines.conf");
        let mut app = app();
        app.machines_root = Some(dir.join("root"));
        app.remotes_file = Some(conf.clone());
        app.form.kind = Kind::Tls;
        app.form.set(FormField::Host, "192.0.2.10");
        app.form.set(FormField::Server, "192.0.2.74");
        app.form.key = KeyChoice::Paste;
        let key = dir.join("keys/box");
        let remote = app.form.remote(Some(&key));
        app.keep_pairing(remote, "sha256:14e393f5");
        assert_eq!(app.selected_remote, Some(0));
        assert_eq!(app.form.editing.as_deref(), Some("192-0-2-10"));
        assert_eq!(app.form.value(FormField::Host), "192.0.2.10");
        assert_eq!(app.form.value(FormField::Server), "192.0.2.74");
        assert_eq!(app.form.key, KeyChoice::Paste);
        // The pasted key stays kept with nothing pasted again.
        assert_eq!(app.form.problem(FormField::Paste), None);
        let kept = crate::remote::parse(&std::fs::read_to_string(&conf).expect("kept"));
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].key_source, KeySource::Pasted);
        assert_eq!(kept[0].key.as_deref(), Some(key.to_str().expect("a path")));
        assert_eq!(kept[0].server.as_deref(), Some("192.0.2.74"));
        assert_eq!(kept[0].pairing, Pairing::Applying);
        assert_eq!(kept[0].ca_fingerprint.as_deref(), Some("sha256:14e393f5"));
        assert_eq!(app.remote_line(0).text, "being set up on the machine");
        if let Ok(mut outcome) = app.tls_outcome.lock() {
            *outcome = Some(("192-0-2-10".to_owned(), true));
        }
        assert!(app.poll_machines(1));
        let kept = crate::remote::parse(&std::fs::read_to_string(&conf).expect("kept"));
        assert_eq!(kept[0].pairing, Pairing::Paired);
        assert_eq!(app.remote_line(0).text, "paired · no records yet");
        // Another machine: an empty form again.
        app.act(&Action::FormNew);
        assert_eq!(app.form.editing, None);
        assert_eq!(app.selected_remote, None);
        checked(&app);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn numbers_and_times_read_as_the_rows_say_them() {
        assert_eq!(super::grouped(96_242), "96,242");
        assert_eq!(super::grouped(7), "7");
        assert_eq!(super::grouped(1_000), "1,000");
        assert_eq!(
            super::when("2026-10-23T20:49:24Z"),
            "2026-10-23 20:49:24 UTC"
        );
    }

    #[test]
    fn a_clock_reads_the_time_of_day() {
        assert_eq!(clock(1_792_776_109_770_158, 0), "17:21:49");
        assert_eq!(clock(1_792_776_109_770_158, 7200), "19:21:49");
    }
}
