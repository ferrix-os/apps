//! shelf's state, apart from how it is drawn: the tabs, each with one or
//! two panes, each pane where it is (a folder, inside an archive, a tag's
//! files, the trash), its history, its selection, its search and its
//! chips; the settings, the tags, the pins and the folder sizes; and the
//! dialog open, if any. [`App::act`] is everything a person can do, and
//! [`App::items`] what a pane shows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tole::FileKind;

use crate::archive::{self, Format, Index};
use crate::fsops::{self, Planned, RenameRule};
use crate::sizes::Sizes;
use crate::store::{Quick, Settings, Tags};
use crate::trash::{self, Trashed};

/// Where a pane is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Loc {
    /// A folder.
    Dir(PathBuf),
    /// Inside an archive: the file, and the folder in it (`""` its top).
    Archive {
        /// The archive.
        file: PathBuf,
        /// The folder inside, `/`-separated.
        inner: String,
    },
    /// Every file with a tag.
    Tag(String),
    /// The trash.
    Trash,
}

impl Loc {
    /// The folder on disk it is, or is in.
    #[must_use]
    pub fn dir(&self) -> Option<&Path> {
        match self {
            Loc::Dir(dir) => Some(dir),
            Loc::Archive { file, .. } => file.parent(),
            _ => None,
        }
    }
}

/// One thing a pane lists.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// Its name.
    pub name: String,
    /// Its path; inside an archive, the archive's path with the inner one
    /// after it, which is no file on disk.
    pub path: PathBuf,
    /// Inside an archive: its path there.
    pub inner: Option<String>,
    /// A folder.
    pub is_dir: bool,
    /// What it is.
    pub kind: FileKind,
    /// An archive, and which.
    pub archive: Option<Format>,
    /// Its bytes; a folder's, all it holds once added up.
    pub bytes: u64,
    /// Whether a folder's bytes are known yet.
    pub size_known: bool,
    /// A folder's entries.
    pub entries: usize,
    /// When it changed.
    pub mtime: i64,
    /// Its tags.
    pub tags: Vec<String>,
    /// In the trash: what the trash knows of it.
    pub trashed: Option<Trashed>,
}

impl Item {
    /// The kind's name, as the chips and the badge read it.
    #[must_use]
    pub fn kind_label(&self) -> &'static str {
        if self.archive.is_some() {
            "Archive"
        } else {
            self.kind.label()
        }
    }

    /// The badge's three letters.
    #[must_use]
    pub fn badge(&self) -> &'static str {
        if self.archive.is_some() {
            "ZIP"
        } else {
            self.kind.badge()
        }
    }

    /// The kind's name for the badge's colour.
    #[must_use]
    pub fn kind_name(&self) -> &'static str {
        if self.archive.is_some() {
            "archive"
        } else {
            self.kind.name()
        }
    }

    /// Whether it is a picture the preview draws.
    #[must_use]
    pub fn is_image(&self) -> bool {
        self.inner.is_none() && !self.is_dir && tole::files::is_drawable_image(&self.name)
    }
}

/// The chips' choices: each `None` for any.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chips {
    /// 0 folders, 1 images, 2 keys, 3 documents, 4 archives, 5 videos.
    pub kind: Option<usize>,
    /// 0 today, 1 the past week, 2 the past month, 3 older.
    pub date: Option<usize>,
    /// 0 under 100 KB, 1 to 10 MB, 2 over.
    pub size: Option<usize>,
    /// A tag.
    pub tag: Option<String>,
}

/// The kind chip's choices.
pub const KIND_CHIPS: [&str; 6] = [
    "Folders",
    "Images",
    "Keys",
    "Documents",
    "Archives",
    "Videos",
];
/// The modified chip's.
pub const DATE_CHIPS: [&str; 4] = ["Today", "Past week", "Past month", "Older"];
/// The size chip's.
pub const SIZE_CHIPS: [&str; 3] = ["Under 100 KB", "100 KB \u{2013} 10 MB", "Over 10 MB"];

impl Chips {
    fn takes(&self, item: &Item, now: i64) -> bool {
        let kind_ok = match self.kind {
            None => true,
            Some(0) => item.is_dir && item.archive.is_none(),
            Some(1) => item.kind == FileKind::Image,
            Some(2) => matches!(item.kind, FileKind::Private | FileKind::Public),
            Some(3) => matches!(item.kind, FileKind::Document | FileKind::Text),
            Some(4) => item.archive.is_some(),
            Some(_) => item.kind == FileKind::Video,
        };
        let age = (now - item.mtime).max(0) / 86_400;
        let date_ok = match self.date {
            None => true,
            Some(0) => age == 0,
            Some(1) => age <= 7,
            Some(2) => age <= 31,
            Some(_) => age > 31,
        };
        let size_ok = match self.size {
            None => true,
            Some(0) => item.bytes < 100_000,
            Some(1) => (100_000..10_000_000).contains(&item.bytes),
            Some(_) => item.bytes >= 10_000_000,
        };
        let tag_ok = self.tag.as_ref().is_none_or(|t| item.tags.contains(t));
        kind_ok && date_ok && size_ok && tag_ok
    }

    /// Whether any is set.
    #[must_use]
    pub fn any(&self) -> bool {
        self.kind.is_some() || self.date.is_some() || self.size.is_some() || self.tag.is_some()
    }
}

/// One pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pane {
    /// Where it is.
    pub loc: Loc,
    /// Where it was, and where it went back from.
    pub history: Vec<Loc>,
    /// Its place in the history.
    pub at: usize,
    /// The items chosen, by path.
    pub selected: Vec<PathBuf>,
    /// The one chosen last: what the preview shows and the keys move from.
    pub primary: Option<PathBuf>,
    /// Where a Shift click stretches from.
    pub anchor: Option<PathBuf>,
    /// The search.
    pub query: String,
    /// The chips.
    pub chips: Chips,
}

impl Pane {
    /// A pane at `loc`.
    #[must_use]
    pub fn new(loc: Loc) -> Self {
        Pane {
            history: vec![loc.clone()],
            loc,
            at: 0,
            selected: Vec::new(),
            primary: None,
            anchor: None,
            query: String::new(),
            chips: Chips::default(),
        }
    }

    fn go(&mut self, loc: Loc) {
        if loc == self.loc {
            return;
        }
        self.history.truncate(self.at + 1);
        self.history.push(loc.clone());
        self.at = self.history.len() - 1;
        self.loc = loc;
        self.clear();
        self.query.clear();
    }

    fn clear(&mut self) {
        self.selected.clear();
        self.primary = None;
        self.anchor = None;
    }

    /// Whether Back has somewhere to go.
    #[must_use]
    pub fn can_back(&self) -> bool {
        self.at > 0
    }

    /// Whether Forward has.
    #[must_use]
    pub fn can_forward(&self) -> bool {
        self.at + 1 < self.history.len()
    }

    /// The folder above, as a place.
    #[must_use]
    pub fn above(&self) -> Option<Loc> {
        match &self.loc {
            Loc::Dir(dir) => dir.parent().map(|p| Loc::Dir(p.to_path_buf())),
            Loc::Archive { file, inner } if inner.is_empty() => {
                file.parent().map(|p| Loc::Dir(p.to_path_buf()))
            }
            Loc::Archive { file, inner } => Some(Loc::Archive {
                file: file.clone(),
                inner: inner
                    .rsplit_once('/')
                    .map_or(String::new(), |(up, _)| up.to_owned()),
            }),
            Loc::Tag(_) | Loc::Trash => Some(Loc::Dir(tole::files::home())),
        }
    }
}

/// A tab: one pane, or two side by side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    /// Its panes; the second is shown when `dual`.
    pub panes: [Pane; 2],
    /// Two panes.
    pub dual: bool,
    /// The pane with the keys.
    pub focus: usize,
}

/// What a confirmation does when confirmed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pending {
    /// Copy these into a folder.
    Copy(Vec<PathBuf>, PathBuf),
    /// Move these into a folder.
    Move(Vec<PathBuf>, PathBuf),
    /// Put these in the trash.
    Trash(Vec<PathBuf>),
    /// Delete these from the trash for good.
    Delete(Vec<Trashed>),
    /// Empty the trash.
    Empty,
    /// Unpack an archive into a new folder beside it.
    Extract(PathBuf, PathBuf),
}

/// A dialog over the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    /// A question before something is done.
    Confirm {
        /// Its title.
        title: String,
        /// What will happen.
        body: String,
        /// The names it is done to (the first few).
        names: Vec<String>,
        /// The button's text.
        ok: String,
        /// Whether it cannot be undone.
        danger: bool,
        /// What it does.
        pending: Pending,
    },
    /// One file's new name.
    Rename {
        /// The file.
        path: PathBuf,
        /// The name typed.
        text: String,
    },
    /// Several files' new names.
    Bulk {
        /// The files.
        paths: Vec<PathBuf>,
        /// Replace (`false`) or number (`true`).
        format: bool,
        /// Replace's text to find.
        find: String,
        /// And what it becomes.
        with: String,
        /// Format's name before the number.
        base: String,
        /// Format's first number.
        start: u32,
    },
    /// A new folder's name.
    NewFolder {
        /// Where.
        dir: PathBuf,
        /// The name typed.
        text: String,
    },
}

/// Everything a person can do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Show tab `n`.
    Tab(usize),
    /// Close tab `n`.
    CloseTab(usize),
    /// A new tab where the pane with the keys is.
    NewTab,
    /// The list (`false`) or the grid.
    Grid(bool),
    /// One pane or two.
    ToggleDual,
    /// Close one of two panes.
    ClosePane(usize),
    /// Back, in the pane with the keys.
    Back,
    /// Forward.
    Forward,
    /// The folder above, in pane `n`.
    Up(usize),
    /// Pane `n` takes the keys.
    Focus(usize),
    /// The search typed.
    Query(String),
    /// The search's mode.
    SearchMode(i32),
    /// A chip: `kind`, `date`, `size` or `tag`, and the choice (`None` any).
    Chip(String, Option<usize>),
    /// Clear every chip.
    ClearChips,
    /// A setting turned over: `hidden`, `folders-first`, `preview`,
    /// `extensions`, `folder-sizes`, `compact`, `dark`.
    Setting(String),
    /// Sort by a column, or turn the sort round when it is that one.
    Sort(String),
    /// A click on a row: pane, index, Shift, Control.
    Click(usize, usize, bool, bool),
    /// A double click, or Return: open it.
    Activate(usize, usize),
    /// A breadcrumb of pane `n`: its folder.
    Crumb(usize, PathBuf),
    /// Go to a folder in the pane with the keys.
    Go(Loc),
    /// Every row of the pane with the keys chosen.
    SelectAll,
    /// Nothing chosen.
    ClearSelection,
    /// Move the choice up or down a row.
    Step(i32),
    /// Open the primary choice.
    OpenPrimary,
    /// Pin the primary choice to Quick access, or unpin it.
    Pin,
    /// Rename the choice: one, or several at once.
    Rename,
    /// Copy the choice to the other pane, after a question.
    CopyOver,
    /// Move it there, after a question.
    MoveOver,
    /// Put the choice in the trash, after a question.
    Trash,
    /// Unpack the archive chosen beside it, after a question.
    Extract,
    /// Put the choice back from the trash.
    Restore,
    /// Delete the choice from the trash for good, after a question.
    DeleteForever,
    /// Empty the trash, after a question.
    EmptyTrash,
    /// A new folder where the pane is.
    NewFolder,
    /// Give the choice a tag, or take it away.
    TagToggle(String),
    /// Give the choice a new tag.
    TagAdd(String),
    /// The dialog's text fields: which, and what they hold now.
    DialogText(String, String),
    /// The bulk rename's mode.
    BulkFormat(bool),
    /// The dialog's main button.
    DialogOk,
    /// Its Cancel, or Escape.
    DialogCancel,
    /// The window's own title bar, where it draws its own frame: move the
    /// window by the pointer, maximize it or give it back its size,
    /// minimize it, close it. The window does these; the state does
    /// nothing with them.
    WindowMove,
    /// See [`Action::WindowMove`].
    WindowMaximize,
    /// See [`Action::WindowMove`].
    WindowMinimize,
    /// See [`Action::WindowMove`].
    WindowClose,
}

/// The disk the home folder is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Disk {
    /// Its size in bytes.
    pub total: u64,
    /// What is used.
    pub used: u64,
}

/// The disk `path` is on, by `statvfs`.
#[must_use]
pub fn disk_of(path: &Path) -> Disk {
    use std::os::unix::ffi::OsStrExt as _;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return Disk::default();
    };
    // SAFETY: an all-zero `statvfs` is a valid value of the plain C struct
    // it is, and `statvfs` writes it whole before it is read.
    let mut out: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a NUL-terminated path that outlives the call, and
    // `out` is a writable `statvfs`.
    if unsafe { libc::statvfs(c.as_ptr(), &mut out) } != 0 {
        return Disk::default();
    }
    #[allow(
        clippy::useless_conversion,
        reason = "the field types differ by architecture"
    )]
    let (block, blocks, free) = (
        u64::from(out.f_frsize),
        u64::from(out.f_blocks),
        u64::from(out.f_bfree),
    );
    Disk {
        total: blocks * block,
        used: blocks.saturating_sub(free) * block,
    }
}

/// shelf's state.
#[derive(Debug)]
pub struct App {
    /// The tabs.
    pub tabs: Vec<Tab>,
    /// The tab shown.
    pub active: usize,
    /// How it looks and lists.
    pub settings: Settings,
    /// The tags.
    pub tags: Tags,
    /// Quick access.
    pub quick: Quick,
    /// Folder sizes.
    pub sizes: Sizes,
    /// The home folder.
    pub home: PathBuf,
    /// A line said at the foot for a moment.
    pub toast: Option<String>,
    /// The dialog open.
    pub dialog: Option<Dialog>,
    /// What each folder read gave, with the folder's change time then.
    listings: HashMap<PathBuf, (i64, Vec<Item>)>,
    /// Archives read, with their change time then.
    archives: HashMap<PathBuf, (i64, Result<Index, String>)>,
    /// The home folder's disk.
    pub disk: Disk,
    /// The trash's items.
    pub trash: Vec<Trashed>,
    /// A tab answered if `tabs` were ever empty, which it is not.
    spare: Tab,
}

impl App {
    /// shelf opening `dir` (else the home folder), with `settings`, `tags`
    /// and `quick` (kept or in memory) and folder sizes `threaded`.
    #[must_use]
    pub fn new(
        dir: Option<PathBuf>,
        settings: Settings,
        tags: Tags,
        quick: Quick,
        threaded: bool,
    ) -> Self {
        let home = tole::files::home();
        let start = dir.unwrap_or_else(|| home.clone());
        let tab = Tab {
            panes: [
                Pane::new(Loc::Dir(start.clone())),
                Pane::new(Loc::Dir(start)),
            ],
            dual: false,
            focus: 0,
        };
        App {
            spare: tab.clone(),
            tabs: vec![tab],
            active: 0,
            settings,
            tags,
            quick,
            sizes: Sizes::new(threaded),
            disk: disk_of(&home),
            home,
            toast: None,
            dialog: None,
            listings: HashMap::new(),
            archives: HashMap::new(),
            trash: trash::list(),
        }
    }

    /// The tab shown.
    #[must_use]
    pub fn tab(&self) -> &Tab {
        let last = self.tabs.len().saturating_sub(1);
        self.tabs.get(self.active.min(last)).unwrap_or(&self.spare)
    }

    fn tab_mut(&mut self) -> &mut Tab {
        let last = self.tabs.len().saturating_sub(1);
        let at = self.active.min(last);
        match self.tabs.get_mut(at) {
            Some(tab) => tab,
            None => &mut self.spare,
        }
    }

    /// The pane with the keys' index.
    #[must_use]
    pub fn focus(&self) -> usize {
        let tab = self.tab();
        if tab.dual { tab.focus.min(1) } else { 0 }
    }

    /// Pane `n` of the tab shown.
    #[must_use]
    pub fn pane(&self, n: usize) -> &Pane {
        let [first, second] = &self.tab().panes;
        if n == 0 { first } else { second }
    }

    fn pane_mut(&mut self, n: usize) -> &mut Pane {
        let [first, second] = &mut self.tab_mut().panes;
        if n == 0 { first } else { second }
    }

    /// The archive at `file`, read once while it does not change.
    pub fn archive(&mut self, file: &Path) -> Result<Index, String> {
        let mtime = std::fs::metadata(file).map_or(0, |m| tole::files::modified_secs(&m));
        if let Some((seen, index)) = self.archives.get(file)
            && *seen == mtime
        {
            return index.clone();
        }
        let index = archive::read(file);
        let _ = self
            .archives
            .insert(file.to_path_buf(), (mtime, index.clone()));
        index
    }

    /// One entry of a folder on disk.
    fn item_of(&mut self, path: &Path, name: &str) -> Item {
        let meta = std::fs::metadata(path).ok();
        let is_dir = meta.as_ref().is_some_and(std::fs::Metadata::is_dir);
        let len = if is_dir {
            0
        } else {
            meta.as_ref().map_or(0, std::fs::Metadata::len)
        };
        let archive = if is_dir {
            None
        } else {
            archive::format_of(name)
        };
        let kind = if archive.is_some() {
            FileKind::Archive
        } else {
            tole::files::classify(path, name, is_dir, len)
        };
        Item {
            name: name.to_owned(),
            path: path.to_path_buf(),
            inner: None,
            is_dir,
            kind,
            archive,
            bytes: len,
            size_known: !is_dir,
            entries: if is_dir {
                tole::files::item_count(path, self.settings.hidden)
            } else {
                0
            },
            mtime: meta.as_ref().map_or(0, tole::files::modified_secs),
            tags: Vec::new(),
            trashed: None,
        }
    }

    /// What `loc` holds, unfiltered: read again when the folder changed.
    fn listing(&mut self, loc: &Loc) -> Result<Vec<Item>, String> {
        let mut items = match loc {
            Loc::Dir(dir) => {
                let mtime =
                    std::fs::metadata(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                let mtime = tole::files::modified_secs(&mtime);
                match self.listings.get(dir) {
                    Some((seen, items)) if *seen == mtime => items.clone(),
                    _ => {
                        let read = std::fs::read_dir(dir)
                            .map_err(|e| format!("{}: {e}", dir.display()))?;
                        let entries: Vec<(PathBuf, String)> = read
                            .filter_map(Result::ok)
                            .map(|e| (e.path(), e.file_name().to_string_lossy().into_owned()))
                            .collect();
                        let items: Vec<Item> =
                            entries.iter().map(|(p, n)| self.item_of(p, n)).collect();
                        let _ = self.listings.insert(dir.clone(), (mtime, items.clone()));
                        items
                    }
                }
            }
            Loc::Archive { file, inner } => {
                let index = self.archive(file)?;
                index
                    .children(inner)
                    .into_iter()
                    .map(|entry| {
                        let name = entry.name().to_owned();
                        let kind = if entry.is_dir {
                            FileKind::Dir
                        } else {
                            tole::files::classify(Path::new(""), &name, false, 0)
                        };
                        Item {
                            path: file.join(&entry.path),
                            inner: Some(entry.path.clone()),
                            is_dir: entry.is_dir,
                            kind,
                            archive: None,
                            bytes: entry.size,
                            size_known: true,
                            entries: if entry.is_dir {
                                index.children(&entry.path).len()
                            } else {
                                0
                            },
                            mtime: entry.mtime,
                            tags: Vec::new(),
                            trashed: None,
                            name,
                        }
                    })
                    .collect()
            }
            Loc::Tag(tag) => {
                let files = self.tags.files_with(tag);
                files
                    .iter()
                    .map(|p| {
                        let name = p
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        self.item_of(p, &name)
                    })
                    .collect()
            }
            Loc::Trash => {
                let items = self.trash.clone();
                items
                    .iter()
                    .map(|t| {
                        let name = t
                            .original
                            .file_name()
                            .map_or_else(|| t.name.clone(), |n| n.to_string_lossy().into_owned());
                        let mut item = self.item_of(&t.path, &name);
                        item.trashed = Some(t.clone());
                        item
                    })
                    .collect()
            }
        };
        for item in &mut items {
            if item.inner.is_none() {
                item.tags = self.tags.of(&item.path).to_vec();
            }
            if item.is_dir && item.inner.is_none() && self.settings.folder_sizes {
                match self.sizes.get(&item.path) {
                    Some(total) => {
                        item.bytes = total.bytes;
                        item.size_known = true;
                    }
                    None => item.size_known = false,
                }
            }
        }
        Ok(items)
    }

    /// What pane `n` shows, filtered and sorted, or why it shows nothing.
    pub fn items(&mut self, n: usize) -> Result<Vec<Item>, String> {
        let pane = self.pane(n).clone();
        let mut items = self.listing(&pane.loc)?;
        let search = tole::Search::new(&pane.query, self.settings.search_mode)?;
        let now = tole::files::now();
        let hidden = self.settings.hidden;
        items.retain(|item| {
            (hidden || !item.name.starts_with('.'))
                && search.matches(&item.name, item.kind)
                && pane.chips.takes(item, now)
        });
        let ascending = self.settings.ascending;
        let folders_first = self.settings.folders_first;
        let sort = self.settings.sort.clone();
        items.sort_by(|a, b| {
            if folders_first && a.is_dir != b.is_dir {
                return b.is_dir.cmp(&a.is_dir);
            }
            let by_name = || {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then_with(|| a.name.cmp(&b.name))
            };
            let order = match sort.as_str() {
                "kind" => a.kind_label().cmp(b.kind_label()).then_with(by_name),
                "size" => a.bytes.cmp(&b.bytes).then_with(by_name),
                "modified" => b.mtime.cmp(&a.mtime).then_with(by_name),
                _ => by_name(),
            };
            if ascending { order } else { order.reverse() }
        });
        Ok(items)
    }

    /// The items chosen in the pane with the keys, in the pane's order.
    pub fn chosen(&mut self) -> Vec<Item> {
        let n = self.focus();
        let selected = self.pane(n).selected.clone();
        self.items(n)
            .unwrap_or_default()
            .into_iter()
            .filter(|i| selected.contains(&i.path))
            .collect()
    }

    /// The primary choice, if it is still shown.
    pub fn primary(&mut self) -> Option<Item> {
        let n = self.focus();
        let primary = self.pane(n).primary.clone()?;
        self.items(n).ok()?.into_iter().find(|i| i.path == primary)
    }

    fn say(&mut self, text: impl Into<String>) {
        self.toast = Some(text.into());
    }

    /// Forget what was read of `paths` and the folders around them.
    fn changed(&mut self, paths: &[&Path]) {
        for path in paths {
            self.sizes.forget(path);
            if let Some(parent) = path.parent() {
                let _ = self.listings.remove(parent);
            }
            let _ = self.listings.remove(*path);
        }
        self.trash = trash::list();
        self.disk = disk_of(&self.home);
    }

    /// Forget every folder read: a key's refresh, or a change elsewhere.
    pub fn reload(&mut self) {
        self.listings.clear();
        self.archives.clear();
        self.trash = trash::list();
        self.disk = disk_of(&self.home);
    }

    /// Do `action`.
    pub fn act(&mut self, action: Action) {
        match action {
            Action::Tab(n) if n < self.tabs.len() => self.active = n,
            Action::CloseTab(n) if self.tabs.len() > 1 && n < self.tabs.len() => {
                let _ = self.tabs.remove(n);
                if self.active >= n && self.active > 0 {
                    self.active -= 1;
                }
            }
            Action::NewTab => {
                let loc = self.pane(self.focus()).loc.clone();
                self.tabs.push(Tab {
                    panes: [Pane::new(loc.clone()), Pane::new(loc)],
                    dual: false,
                    focus: 0,
                });
                self.active = self.tabs.len() - 1;
            }
            Action::Grid(on) => {
                self.settings.grid = on;
                self.settings.save();
            }
            Action::ToggleDual => {
                let tab = self.tab_mut();
                tab.dual = !tab.dual;
                tab.focus = 0;
            }
            Action::ClosePane(n) => {
                let tab = self.tab_mut();
                if tab.dual {
                    if n == 0 {
                        tab.panes.swap(0, 1);
                    }
                    tab.dual = false;
                    tab.focus = 0;
                }
            }
            Action::Back => {
                let n = self.focus();
                let pane = self.pane_mut(n);
                if pane.can_back() {
                    pane.at -= 1;
                    if let Some(loc) = pane.history.get(pane.at) {
                        pane.loc = loc.clone();
                    }
                    pane.clear();
                }
            }
            Action::Forward => {
                let n = self.focus();
                let pane = self.pane_mut(n);
                if pane.can_forward() {
                    pane.at += 1;
                    if let Some(loc) = pane.history.get(pane.at) {
                        pane.loc = loc.clone();
                    }
                    pane.clear();
                }
            }
            Action::Up(n) => {
                self.tab_mut().focus = n;
                if let Some(up) = self.pane(n).above() {
                    self.pane_mut(n).go(up);
                }
            }
            Action::Focus(n) => self.tab_mut().focus = n.min(1),
            Action::Query(text) => {
                let n = self.focus();
                let pane = self.pane_mut(n);
                pane.query = text;
                pane.clear();
            }
            Action::SearchMode(mode) => {
                self.settings.search_mode = mode.clamp(0, 2);
                self.settings.save();
            }
            Action::Chip(which, choice) => {
                let n = self.focus();
                let tags: Vec<String> = self.tags.all().into_iter().map(|(t, _)| t).collect();
                let chips = &mut self.pane_mut(n).chips;
                match which.as_str() {
                    "kind" => chips.kind = choice,
                    "date" => chips.date = choice,
                    "size" => chips.size = choice,
                    _ => chips.tag = choice.and_then(|i| tags.get(i).cloned()),
                }
                self.pane_mut(n).clear();
            }
            Action::ClearChips => {
                let n = self.focus();
                self.pane_mut(n).chips = Chips::default();
            }
            Action::Setting(name) => {
                let s = &mut self.settings;
                match name.as_str() {
                    "hidden" => s.hidden = !s.hidden,
                    "folders-first" => s.folders_first = !s.folders_first,
                    "preview" => s.preview = !s.preview,
                    "extensions" => s.extensions = !s.extensions,
                    "folder-sizes" => s.folder_sizes = !s.folder_sizes,
                    "compact" => s.compact = !s.compact,
                    "dark" => s.dark = !s.dark,
                    _ => {}
                }
                s.save();
                self.listings.clear();
            }
            Action::Sort(column) => {
                if self.settings.sort == column {
                    self.settings.ascending = !self.settings.ascending;
                } else {
                    self.settings.sort = column;
                    self.settings.ascending = true;
                }
                self.settings.save();
            }
            Action::Click(n, index, shift, control) => self.click(n, index, shift, control),
            Action::Activate(n, index) => {
                self.tab_mut().focus = n;
                if let Some(item) = self.items(n).unwrap_or_default().get(index).cloned() {
                    self.open(n, &item);
                }
            }
            Action::Crumb(n, path) => {
                self.tab_mut().focus = n;
                let loc = match &self.pane(n).loc {
                    Loc::Archive { file, .. } if path.starts_with(file) && path != *file => {
                        Loc::Archive {
                            file: file.clone(),
                            inner: path
                                .strip_prefix(file)
                                .unwrap_or(Path::new(""))
                                .to_string_lossy()
                                .into_owned(),
                        }
                    }
                    Loc::Archive { file, .. } if path == *file => Loc::Archive {
                        file: file.clone(),
                        inner: String::new(),
                    },
                    _ => Loc::Dir(path),
                };
                self.pane_mut(n).go(loc);
            }
            Action::Go(loc) => {
                let n = self.focus();
                self.pane_mut(n).go(loc);
            }
            Action::SelectAll => {
                let n = self.focus();
                let all: Vec<PathBuf> = self
                    .items(n)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|i| i.path)
                    .collect();
                let pane = self.pane_mut(n);
                pane.primary = all.last().cloned();
                pane.anchor = all.first().cloned();
                pane.selected = all;
            }
            Action::ClearSelection => {
                let n = self.focus();
                self.pane_mut(n).clear();
            }
            Action::Step(by) => {
                let n = self.focus();
                let items = self.items(n).unwrap_or_default();
                let at = self
                    .pane(n)
                    .primary
                    .as_ref()
                    .and_then(|p| items.iter().position(|i| i.path == *p));
                let next = match at {
                    Some(at) => (at as i64 + i64::from(by)).clamp(0, items.len() as i64 - 1),
                    None if by > 0 => 0,
                    None => items.len() as i64 - 1,
                };
                if let Some(item) = usize::try_from(next).ok().and_then(|i| items.get(i)) {
                    let pane = self.pane_mut(n);
                    pane.selected = vec![item.path.clone()];
                    pane.primary = Some(item.path.clone());
                    pane.anchor = Some(item.path.clone());
                }
            }
            Action::OpenPrimary => {
                let n = self.focus();
                if let Some(item) = self.primary() {
                    self.open(n, &item);
                }
            }
            Action::Pin => {
                if let Some(item) = self
                    .primary()
                    .filter(|i| i.inner.is_none() && i.trashed.is_none())
                {
                    self.quick.toggle(&item.path);
                    let pinned = self.quick.has(&item.path);
                    self.say(if pinned {
                        format!("{} pinned to Quick access", item.name)
                    } else {
                        format!("{} unpinned", item.name)
                    });
                }
            }
            Action::Rename => self.ask_rename(),
            Action::CopyOver => self.ask_transfer(true),
            Action::MoveOver => self.ask_transfer(false),
            Action::Trash => self.ask_trash(),
            Action::Extract => self.ask_extract(),
            Action::Restore => self.restore(),
            Action::DeleteForever => {
                let items: Vec<Trashed> = self
                    .chosen()
                    .into_iter()
                    .filter_map(|i| i.trashed)
                    .collect();
                if !items.is_empty() {
                    let names = items.iter().map(|t| t.name.clone()).collect();
                    self.dialog = Some(Dialog::Confirm {
                        title: format!("Delete {} for good?", count_of(items.len())),
                        body: "They leave the trash and cannot be brought back.".to_owned(),
                        names,
                        ok: "Delete for good".to_owned(),
                        danger: true,
                        pending: Pending::Delete(items),
                    });
                }
            }
            Action::EmptyTrash => {
                if !self.trash.is_empty() {
                    let names = self.trash.iter().map(|t| t.name.clone()).collect();
                    self.dialog = Some(Dialog::Confirm {
                        title: format!("Empty the trash of {}?", count_of(self.trash.len())),
                        body: "Everything in it is deleted for good.".to_owned(),
                        names,
                        ok: "Empty trash".to_owned(),
                        danger: true,
                        pending: Pending::Empty,
                    });
                }
            }
            Action::NewFolder => {
                let n = self.focus();
                if let Loc::Dir(dir) = &self.pane(n).loc {
                    let text = fsops::unique_name(dir, "New folder");
                    self.dialog = Some(Dialog::NewFolder {
                        dir: dir.clone(),
                        text,
                    });
                }
            }
            Action::TagToggle(tag) => {
                let chosen = self.chosen();
                let targets: Vec<&Item> = chosen.iter().filter(|i| i.inner.is_none()).collect();
                let on = !targets.iter().all(|i| i.tags.contains(&tag));
                for item in targets {
                    self.tags.set(&item.path, &tag, on);
                }
            }
            Action::TagAdd(tag) => {
                let tag = tag.trim().to_lowercase();
                if !tag.is_empty() {
                    for item in self.chosen().into_iter().filter(|i| i.inner.is_none()) {
                        self.tags.set(&item.path, &tag, true);
                    }
                }
            }
            Action::DialogText(which, text) => self.dialog_text(&which, text),
            Action::BulkFormat(on) => {
                if let Some(Dialog::Bulk { format, .. }) = &mut self.dialog {
                    *format = on;
                }
            }
            Action::DialogOk => self.dialog_ok(),
            Action::DialogCancel => self.dialog = None,
            _ => {}
        }
    }

    fn click(&mut self, n: usize, index: usize, shift: bool, control: bool) {
        self.tab_mut().focus = n;
        let items = self.items(n).unwrap_or_default();
        let Some(item) = items.get(index) else {
            self.pane_mut(n).clear();
            return;
        };
        let path = item.path.clone();
        let pane = self.pane_mut(n);
        if control {
            if pane.selected.contains(&path) {
                pane.selected.retain(|p| *p != path);
            } else {
                pane.selected.push(path.clone());
            }
            pane.anchor = Some(path.clone());
        } else if shift && let Some(anchor) = pane.anchor.clone() {
            let from = items.iter().position(|i| i.path == anchor).unwrap_or(index);
            let (lo, hi) = (from.min(index), from.max(index));
            pane.selected = items
                .get(lo..=hi)
                .unwrap_or_default()
                .iter()
                .map(|i| i.path.clone())
                .collect();
        } else {
            pane.selected = vec![path.clone()];
            pane.anchor = Some(path.clone());
        }
        pane.primary = Some(path);
    }

    /// Open `item` from pane `n`: a folder or an archive is walked into, a
    /// file handed to the desktop's opener.
    fn open(&mut self, n: usize, item: &Item) {
        if let Some(inner) = &item.inner {
            if item.is_dir {
                let Loc::Archive { file, .. } = self.pane(n).loc.clone() else {
                    return;
                };
                self.pane_mut(n).go(Loc::Archive {
                    file,
                    inner: inner.clone(),
                });
            } else {
                self.say(format!(
                    "{} is inside an archive: extract it to open it",
                    item.name
                ));
            }
            return;
        }
        if item.trashed.is_some() {
            self.say("Restore it to open it");
            return;
        }
        if item.is_dir {
            self.pane_mut(n).go(Loc::Dir(item.path.clone()));
            return;
        }
        if let Some(format) = item.archive {
            if let Format::Other(kind) = format {
                self.say(format!(
                    "{} is a {kind} archive, which shelf does not read",
                    item.name
                ));
            } else {
                match self.archive(&item.path) {
                    Ok(_) => self.pane_mut(n).go(Loc::Archive {
                        file: item.path.clone(),
                        inner: String::new(),
                    }),
                    Err(why) => self.say(format!("{}: {why}", item.name)),
                }
            }
            return;
        }
        let opened = std::process::Command::new("xdg-open")
            .arg(&item.path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match opened {
            Ok(_) => self.say(format!("Opening {}", item.name)),
            Err(_) => self.say(format!("Nothing here opens {} (no xdg-open)", item.name)),
        }
    }

    fn ask_rename(&mut self) {
        let chosen: Vec<Item> = self
            .chosen()
            .into_iter()
            .filter(|i| i.inner.is_none() && i.trashed.is_none())
            .collect();
        match chosen.as_slice() {
            [] => {}
            [one] => {
                self.dialog = Some(Dialog::Rename {
                    path: one.path.clone(),
                    text: one.name.clone(),
                });
            }
            many => {
                let base = if many.iter().all(|i| i.kind == FileKind::Image) {
                    "photo"
                } else {
                    "file"
                };
                self.dialog = Some(Dialog::Bulk {
                    paths: many.iter().map(|i| i.path.clone()).collect(),
                    format: true,
                    find: String::new(),
                    with: String::new(),
                    base: base.to_owned(),
                    start: 1,
                });
            }
        }
    }

    /// The other pane's folder, when there are two and it is one.
    #[must_use]
    pub fn other_dir(&self) -> Option<PathBuf> {
        let tab = self.tab();
        if !tab.dual {
            return None;
        }
        match &self.pane(1 - self.focus()).loc {
            Loc::Dir(dir) => Some(dir.clone()),
            _ => None,
        }
    }

    fn ask_transfer(&mut self, copy: bool) {
        let Some(to) = self.other_dir() else {
            self.say("Open a folder in the other pane first");
            return;
        };
        let chosen: Vec<Item> = self
            .chosen()
            .into_iter()
            .filter(|i| i.inner.is_none() && i.trashed.is_none())
            .collect();
        let paths: Vec<PathBuf> = chosen
            .iter()
            .map(|i| i.path.clone())
            .filter(|p| !fsops::is_within(&to, p) && (copy || p.parent() != Some(to.as_path())))
            .collect();
        if paths.is_empty() {
            self.say("Nothing to put there");
            return;
        }
        let taken = paths
            .iter()
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| to.join(n).symlink_metadata().is_ok())
            })
            .count();
        let where_to = tole::files::tilde(&to.to_string_lossy());
        let body = if taken > 0 {
            format!(
                "To {where_to}. {} there already: the new ones get a number.",
                count_of(taken)
            )
        } else {
            format!("To {where_to}.")
        };
        let names = paths
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        self.dialog = Some(Dialog::Confirm {
            title: format!(
                "{} {}?",
                if copy { "Copy" } else { "Move" },
                count_of(paths.len())
            ),
            body,
            names,
            ok: if copy {
                "Copy".to_owned()
            } else {
                "Move".to_owned()
            },
            danger: false,
            pending: if copy {
                Pending::Copy(paths, to)
            } else {
                Pending::Move(paths, to)
            },
        });
    }

    fn ask_trash(&mut self) {
        let paths: Vec<PathBuf> = self
            .chosen()
            .into_iter()
            .filter(|i| i.inner.is_none() && i.trashed.is_none())
            .map(|i| i.path)
            .collect();
        if paths.is_empty() {
            return;
        }
        let names = paths
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        self.dialog = Some(Dialog::Confirm {
            title: format!("Move {} to the trash?", count_of(paths.len())),
            body: "They can be restored from Trash until it is emptied.".to_owned(),
            names,
            ok: "Move to Trash".to_owned(),
            danger: false,
            pending: Pending::Trash(paths),
        });
    }

    fn ask_extract(&mut self) {
        let Some(item) = self.primary() else {
            return;
        };
        if !matches!(
            item.archive,
            Some(Format::Zip | Format::Tar | Format::TarGz)
        ) {
            return;
        }
        let dir = item.path.parent().unwrap_or(Path::new("/")).to_path_buf();
        let ext = fsops::extension(&item.name);
        let stem = item
            .name
            .get(..item.name.len() - ext.len())
            .unwrap_or(&item.name);
        let dest = dir.join(fsops::unique_name(&dir, stem));
        let entries = self
            .archive(&item.path)
            .map(|i| i.entries.len())
            .unwrap_or(0);
        self.dialog = Some(Dialog::Confirm {
            title: format!("Extract {}?", item.name),
            body: format!(
                "Its {} go into a new folder, {}.",
                count_of(entries),
                tole::files::tilde(&dest.to_string_lossy())
            ),
            names: Vec::new(),
            ok: "Extract".to_owned(),
            danger: false,
            pending: Pending::Extract(item.path, dest),
        });
    }

    fn restore(&mut self) {
        let items: Vec<Trashed> = self
            .chosen()
            .into_iter()
            .filter_map(|i| i.trashed)
            .collect();
        let mut back = 0;
        for item in &items {
            match trash::restore(item) {
                Ok(to) => {
                    back += 1;
                    self.changed(&[&to]);
                }
                Err(error) => self.say(format!("{}: {error}", item.name)),
            }
        }
        if back > 0 {
            self.say(format!("Restored {}", count_of(back)));
            let n = self.focus();
            self.pane_mut(n).clear();
        }
    }

    fn dialog_text(&mut self, which: &str, text: String) {
        match (&mut self.dialog, which) {
            (Some(Dialog::Rename { text: t, .. } | Dialog::NewFolder { text: t, .. }), "name") => {
                *t = text
            }
            (Some(Dialog::Bulk { find, .. }), "find") => *find = text,
            (Some(Dialog::Bulk { with, .. }), "with") => *with = text,
            (Some(Dialog::Bulk { base, .. }), "base") => *base = text,
            (Some(Dialog::Bulk { start, .. }), "start") => {
                *start = text.trim().parse().unwrap_or(1)
            }
            _ => {}
        }
    }

    /// The bulk rename's plan, as it stands.
    #[must_use]
    pub fn bulk_plan(&self) -> Vec<Planned> {
        match &self.dialog {
            Some(Dialog::Bulk {
                paths,
                format,
                find,
                with,
                base,
                start,
            }) => {
                let rule = if *format {
                    RenameRule::Format {
                        base: base.clone(),
                        start: *start,
                    }
                } else {
                    RenameRule::Replace {
                        find: find.clone(),
                        with: with.clone(),
                    }
                };
                fsops::plan_rename(paths, &rule)
            }
            _ => Vec::new(),
        }
    }

    /// Why the dialog's button cannot be pressed now, if it cannot.
    #[must_use]
    pub fn dialog_problem(&self) -> Option<String> {
        match &self.dialog {
            Some(Dialog::Rename { path, text }) => {
                let dir = path.parent().unwrap_or(Path::new("/"));
                let old = path.file_name().map(|n| n.to_string_lossy().into_owned());
                fsops::name_problem(dir, text, old.as_deref()).map(str::to_owned)
            }
            Some(Dialog::NewFolder { dir, text }) => {
                fsops::name_problem(dir, text, None).map(str::to_owned)
            }
            Some(Dialog::Bulk { .. }) => {
                let plan = self.bulk_plan();
                if let Some(bad) = plan.iter().find(|p| p.problem.is_some()) {
                    Some(format!("{}: {}", bad.new, bad.problem.unwrap_or("")))
                } else if plan.iter().all(|p| p.new == p.old) {
                    Some("nothing would change".to_owned())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn dialog_ok(&mut self) {
        if self.dialog_problem().is_some() {
            return;
        }
        let Some(dialog) = self.dialog.take() else {
            return;
        };
        let n = self.focus();
        match dialog {
            Dialog::Rename { path, text } => match fsops::rename(&path, &text) {
                Ok(to) => {
                    self.tags.moved(&path, &to);
                    self.quick.moved(&path, &to);
                    self.changed(&[&path, &to]);
                    let pane = self.pane_mut(n);
                    pane.selected = vec![to.clone()];
                    pane.primary = Some(to);
                    self.say("Renamed 1 item");
                }
                Err(error) => self.say(format!("Not renamed: {error}")),
            },
            Dialog::NewFolder { dir, text } => match fsops::new_folder(&dir, &text) {
                Ok(to) => {
                    self.changed(&[&to]);
                    let pane = self.pane_mut(n);
                    pane.selected = vec![to.clone()];
                    pane.primary = Some(to);
                }
                Err(error) => self.say(format!("No folder made: {error}")),
            },
            Dialog::Bulk { .. } => {
                self.dialog = Some(dialog);
                let plan = self.bulk_plan();
                self.dialog = None;
                match fsops::apply_rename(&plan) {
                    Ok(paths) => {
                        for (p, to) in plan.iter().zip(&paths) {
                            self.tags.moved(&p.path, to);
                            self.quick.moved(&p.path, to);
                            self.changed(&[&p.path, to]);
                        }
                        let changed = plan.iter().filter(|p| p.new != p.old).count();
                        let pane = self.pane_mut(n);
                        pane.primary = paths.last().cloned();
                        pane.anchor = paths.first().cloned();
                        pane.selected = paths;
                        self.say(format!("Renamed {}", count_of(changed)));
                    }
                    Err(error) => self.say(format!("Not renamed: {error}")),
                }
            }
            Dialog::Confirm { pending, .. } => self.run(pending),
        }
    }

    fn run(&mut self, pending: Pending) {
        let n = self.focus();
        match pending {
            Pending::Copy(paths, to) | Pending::Move(paths, to) if paths.is_empty() => {
                let _ = to;
            }
            Pending::Copy(paths, to) => {
                let mut done = 0;
                for path in &paths {
                    match fsops::copy_into(path, &to) {
                        Ok(at) => {
                            self.tags.copied(path, &at);
                            self.changed(&[&at]);
                            done += 1;
                        }
                        Err(error) => self.say(format!("{}: {error}", path.display())),
                    }
                }
                if done == paths.len() {
                    self.say(format!(
                        "Copied {} to {}",
                        count_of(done),
                        tole::files::tilde(&to.to_string_lossy())
                    ));
                }
                self.pane_mut(n).clear();
            }
            Pending::Move(paths, to) => {
                let mut done = 0;
                for path in &paths {
                    match fsops::move_into(path, &to) {
                        Ok(at) => {
                            self.tags.moved(path, &at);
                            self.quick.moved(path, &at);
                            self.changed(&[path, &at]);
                            done += 1;
                        }
                        Err(error) => self.say(format!("{}: {error}", path.display())),
                    }
                }
                if done == paths.len() {
                    self.say(format!(
                        "Moved {} to {}",
                        count_of(done),
                        tole::files::tilde(&to.to_string_lossy())
                    ));
                }
                self.pane_mut(n).clear();
            }
            Pending::Trash(paths) => {
                let mut done = 0;
                for path in &paths {
                    match trash::trash(path) {
                        Ok(item) => {
                            self.tags.moved(path, &item.path);
                            self.changed(&[path]);
                            done += 1;
                        }
                        Err(error) => self.say(format!("{}: {error}", path.display())),
                    }
                }
                if done == paths.len() {
                    self.say(format!("Moved {} to the trash", count_of(done)));
                }
                self.pane_mut(n).clear();
            }
            Pending::Delete(items) => {
                let mut done = 0;
                for item in &items {
                    match trash::delete(item) {
                        Ok(()) => done += 1,
                        Err(error) => self.say(format!("{}: {error}", item.name)),
                    }
                }
                self.changed(&[]);
                if done == items.len() {
                    self.say(format!("Deleted {} for good", count_of(done)));
                }
                self.pane_mut(n).clear();
            }
            Pending::Empty => {
                match trash::empty_in(&trash::home_trash()) {
                    Ok(done) => self.say(format!("Emptied the trash: {}", count_of(done))),
                    Err(error) => self.say(format!("The trash: {error}")),
                }
                self.changed(&[]);
                self.pane_mut(n).clear();
            }
            Pending::Extract(file, dest) => match archive::extract(&file, &dest) {
                Ok(count) => {
                    self.changed(&[&dest]);
                    self.say(format!(
                        "Extracted {} to {}",
                        count_of(count),
                        dest.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    ));
                    let pane = self.pane_mut(n);
                    pane.selected = vec![dest.clone()];
                    pane.primary = Some(dest);
                }
                Err(why) => self.say(format!("Not extracted: {why}")),
            },
        }
    }
}

/// `1 item`, `3 items`.
#[must_use]
pub fn count_of(n: usize) -> String {
    format!("{n} item{}", if n == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_in(dir: &Path) -> App {
        App::new(
            Some(dir.to_path_buf()),
            Settings::in_memory(),
            Tags::in_memory(),
            Quick::in_memory(Vec::new()),
            false,
        )
    }

    fn names(app: &mut App, n: usize) -> Vec<String> {
        app.items(n)
            .expect("listed")
            .into_iter()
            .map(|i| i.name)
            .collect()
    }

    #[test]
    fn a_pane_lists_filters_sorts_and_walks() {
        let dir = std::env::temp_dir().join(format!("shelf-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("Pictures"));
        let _ = std::fs::write(dir.join("b.txt"), b"bbbb");
        let _ = std::fs::write(dir.join("a.png"), b"aa");
        let _ = std::fs::write(dir.join(".hidden"), b"");
        let mut app = app_in(&dir);
        assert_eq!(names(&mut app, 0), ["Pictures", "a.png", "b.txt"]);
        app.act(Action::Sort("size".into()));
        app.act(Action::Sort("size".into()));
        assert_eq!(
            names(&mut app, 0),
            ["Pictures", "b.txt", "a.png"],
            "folders first, then largest"
        );
        app.act(Action::Setting("hidden".into()));
        assert!(names(&mut app, 0).contains(&".hidden".to_owned()));
        app.act(Action::Query("[image]".into()));
        app.act(Action::SearchMode(1));
        assert_eq!(names(&mut app, 0), ["a.png"]);
        app.act(Action::Query(String::new()));
        app.act(Action::Chip("kind".into(), Some(0)));
        assert_eq!(names(&mut app, 0), ["Pictures"]);
        app.act(Action::ClearChips);
        // Open the folder, then Back and Forward.
        let at = app
            .items(0)
            .expect("listed")
            .iter()
            .position(|i| i.name == "Pictures")
            .expect("there");
        app.act(Action::Activate(0, at));
        assert_eq!(app.pane(0).loc, Loc::Dir(dir.join("Pictures")));
        app.act(Action::Back);
        assert_eq!(app.pane(0).loc, Loc::Dir(dir.clone()));
        app.act(Action::Forward);
        assert_eq!(app.pane(0).loc, Loc::Dir(dir.join("Pictures")));
        app.act(Action::Up(0));
        assert_eq!(app.pane(0).loc, Loc::Dir(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copies_moves_and_trash_ask_first_then_happen() {
        let dir = std::env::temp_dir().join(format!("shelf-app-ops-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("left"));
        let _ = std::fs::create_dir_all(dir.join("right"));
        let _ = std::fs::write(dir.join("left/one.txt"), b"1");
        let _ = std::fs::write(dir.join("left/two.txt"), b"2");
        let mut app = app_in(&dir.join("left"));
        app.act(Action::ToggleDual);
        app.act(Action::Crumb(1, dir.join("right")));
        app.act(Action::Focus(0));
        app.act(Action::Click(0, 0, false, false));
        app.act(Action::Click(0, 1, true, false));
        assert_eq!(app.pane(0).selected.len(), 2, "a Shift click stretches");
        app.act(Action::CopyOver);
        assert!(
            matches!(app.dialog, Some(Dialog::Confirm { .. })),
            "asked first"
        );
        assert!(!dir.join("right/one.txt").exists());
        app.act(Action::DialogOk);
        assert!(dir.join("right/one.txt").exists() && dir.join("left/one.txt").exists());
        app.act(Action::Click(0, 0, false, false));
        app.act(Action::MoveOver);
        app.act(Action::DialogOk);
        assert!(
            dir.join("right/one 2.txt").exists(),
            "the name taken got a number"
        );
        assert!(!dir.join("left/one.txt").exists());
        // Rename one.
        app.act(Action::Click(0, 0, false, false));
        app.act(Action::Rename);
        app.act(Action::DialogText("name".into(), "renamed.txt".into()));
        assert_eq!(app.dialog_problem(), None);
        app.act(Action::DialogOk);
        assert!(dir.join("left/renamed.txt").exists());
        // A new folder.
        app.act(Action::NewFolder);
        app.act(Action::DialogOk);
        assert!(dir.join("left/New folder").is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_archive_is_a_folder_to_walk_into() {
        let dir = std::env::temp_dir().join(format!("shelf-app-zip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            dir.join("backup.zip"),
            crate::demo::zip_of(&[("README.txt", b"hi"), ("keys/id", b"k")]),
        );
        let mut app = app_in(&dir);
        app.act(Action::Activate(0, 0));
        assert_eq!(names(&mut app, 0), ["keys", "README.txt"]);
        app.act(Action::Activate(0, 0));
        assert_eq!(names(&mut app, 0), ["id"]);
        app.act(Action::Up(0));
        app.act(Action::Up(0));
        assert_eq!(app.pane(0).loc, Loc::Dir(dir.clone()));
        app.act(Action::Click(0, 0, false, false));
        app.act(Action::Extract);
        app.act(Action::DialogOk);
        assert!(dir.join("backup/keys/id").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
