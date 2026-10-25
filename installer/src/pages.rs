//! What the window shows and what a press does to it, with no window:
//! the pages in order, each page's rows, and the choices made so far.
//!
//! A press changes the [`State`]; one that asks something of the system
//! answers an [`Effect`], which the window does (`src/window.rs`).

use crate::account::{self, Password};
use crate::keyboard::{self, LAYOUTS};
use crate::protocol::{self, Disk, Disks, Free, Install, Part, Target};

/// The pages, in the order they come.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    /// The mark, the language, Try or Install.
    #[default]
    Welcome,
    /// The keyboard layout, and a line to try it.
    Keyboard,
    /// Which disk, and how.
    Disk,
    /// The account and the computer's name.
    You,
    /// The time zone.
    Zone,
    /// Every choice, and Install.
    Summary,
    /// The install, as it goes.
    Installing,
    /// Installed: restart.
    Done,
    /// It failed: why, and the log.
    Failed,
}

impl Page {
    /// Every page, in order, for `--screenshot`.
    pub const ALL: [Page; 9] = [
        Page::Welcome,
        Page::Keyboard,
        Page::Disk,
        Page::You,
        Page::Zone,
        Page::Summary,
        Page::Installing,
        Page::Done,
        Page::Failed,
    ];

    /// Its name on the command line and in a screenshot's file name.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Page::Welcome => "welcome",
            Page::Keyboard => "keyboard",
            Page::Disk => "disk",
            Page::You => "you",
            Page::Zone => "time-zone",
            Page::Summary => "summary",
            Page::Installing => "installing",
            Page::Done => "done",
            Page::Failed => "error",
        }
    }

    /// Its title over the rows.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Page::Welcome => "",
            Page::Keyboard => "Keyboard layout",
            Page::Disk => "Where to install Ferrix",
            Page::You => "Who are you?",
            Page::Zone => "Where are you?",
            Page::Summary => "Ready to install",
            Page::Installing => "Installing Ferrix",
            Page::Done => "",
            Page::Failed => "The installation failed",
        }
    }

    /// The line under its title.
    #[must_use]
    pub fn subtitle(self) -> &'static str {
        match self {
            Page::Welcome | Page::Done => "",
            Page::Keyboard => "Choose how your keyboard types. The live session follows at once.",
            Page::Disk => "Choose a disk, then whether Ferrix takes all of it or only part.",
            Page::You => "The account you sign in with, and the computer's name.",
            Page::Zone => "Choose your time zone. The clock follows it.",
            Page::Summary => "Check your choices. Nothing is written until you press Install.",
            Page::Installing => "This takes a few minutes. Keep the computer on.",
            Page::Failed => "Nothing more is written. Go back to change a choice and try again.",
        }
    }

    /// The step the header's track marks: 0 Welcome to 6 Install.
    #[must_use]
    pub fn step(self) -> usize {
        match self {
            Page::Welcome => 0,
            Page::Keyboard => 1,
            Page::Disk => 2,
            Page::You => 3,
            Page::Zone => 4,
            Page::Summary => 5,
            Page::Installing | Page::Done | Page::Failed => 6,
        }
    }
}

/// The header track's names, one a step.
pub const STEPS: [&str; 7] = [
    "Welcome",
    "Keyboard",
    "Disk",
    "You",
    "Time zone",
    "Summary",
    "Install",
];

/// The kinds of row the markup draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The mark (or a tick, or a cross), a big title and a line.
    Hero,
    /// A heading over cards.
    Section,
    /// A card to choose, a dot before it: a disk, how, where.
    Radio,
    /// A short row in a list to choose from: a layout, a zone.
    Pick,
    /// A labelled line to type in, and what is wrong with it.
    Field,
    /// A card of warning: red when something is erased.
    Warning,
    /// Lines of label and value.
    Info,
    /// A card with a segmented choice: the variant.
    Choice,
    /// The progress bar and the step.
    Progress,
    /// A line of the log.
    Log,
    /// Big buttons: Try Ferrix, Install Ferrix.
    Buttons,
    /// A quiet button: Show details.
    Link,
    /// Words alone, faint.
    Empty,
}

/// What a row is for, so a press on it can be told.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    /// Nothing: words.
    None,
    /// The one language.
    Language,
    /// Welcome's two buttons.
    Start,
    /// The line to try the keyboard in.
    TryKeys,
    /// The variant's choice.
    Variant,
    /// A layout, by its xkb name.
    Layout(&'static str),
    /// An offered disk, by its device.
    Disk(String),
    /// Ask the daemon for the disks again.
    Rescan,
    /// Erase the whole disk.
    Erase,
    /// Into free space or a partition.
    Alongside,
    /// A region of the disk, by its first sector.
    Region(u64),
    /// The full name.
    FullName,
    /// The user name.
    User,
    /// The computer's name.
    Host,
    /// The password.
    Password,
    /// The password again.
    Confirm,
    /// The time zone search.
    ZoneSearch,
    /// A zone.
    Zone(String),
    /// The log shown or hidden.
    Details,
    /// Restart now.
    Restart,
}

/// Warning levels.
pub mod level {
    /// Plain.
    pub const PLAIN: i32 = 0;
    /// Amber: take care.
    pub const WARN: i32 = 1;
    /// Red: something is erased, or failed.
    pub const DANGER: i32 = 2;
}

/// One row, as the markup draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// How it is drawn.
    pub kind: Kind,
    /// What it is for.
    pub key: Key,
    /// Its words.
    pub title: String,
    /// Words under them, or a field's placeholder.
    pub desc: String,
    /// A word at its right, or a field's sort (`password`, `big`).
    pub tag: String,
    /// A field's text.
    pub shown: String,
    /// What is wrong, under a field; a warning's further lines.
    pub note: String,
    /// The note's level.
    pub level: i32,
    /// Chosen.
    pub on: bool,
    /// May be chosen.
    pub enabled: bool,
    /// Options: a choice's, a button row's, an info card's labels.
    pub options: Vec<String>,
    /// An info card's values; a warning's lines.
    pub values: Vec<String>,
    /// The option chosen; the primary button.
    pub current: i32,
    /// A progress bar's value, 0 to 1.
    pub value: f32,
}

impl Line {
    /// A row of `kind` for `key`, called `title`.
    #[must_use]
    pub fn new(kind: Kind, key: Key, title: impl Into<String>) -> Self {
        Line {
            kind,
            key,
            title: title.into(),
            desc: String::new(),
            tag: String::new(),
            shown: String::new(),
            note: String::new(),
            level: level::PLAIN,
            on: false,
            enabled: true,
            options: Vec::new(),
            values: Vec::new(),
            current: -1,
            value: 0.0,
        }
    }

    fn desc(mut self, desc: impl Into<String>) -> Self {
        self.desc = desc.into();
        self
    }

    fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    fn on(mut self, on: bool) -> Self {
        self.on = on;
        self
    }

    fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    fn level(mut self, level: i32) -> Self {
        self.level = level;
        self
    }
}

/// How the disk is used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// All of it, erased.
    #[default]
    Erase,
    /// A free region or a partition.
    Alongside,
}

/// A part of a disk Ferrix may go into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Region {
    /// Unallocated space.
    Free(Free),
    /// A partition, erased.
    Part(Part),
}

impl Region {
    /// Its first sector, which names it.
    #[must_use]
    pub fn first(&self) -> u64 {
        match self {
            Region::Free(free) => free.first,
            Region::Part(part) => part.first,
        }
    }

    /// Its size.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        match self {
            Region::Free(free) => free.bytes,
            Region::Part(part) => part.bytes,
        }
    }

    /// It in words: `Partition 2 · NTFS · “Windows”`, `Free space`.
    #[must_use]
    pub fn words(&self) -> String {
        match self {
            Region::Free(_) => "Free space".to_owned(),
            Region::Part(part) => {
                let mut words = format!(
                    "Partition {} · {}",
                    part.number,
                    protocol::fs_name(&part.fs)
                );
                if !part.name.is_empty() {
                    words.push_str(&format!(" · “{}”", part.name));
                }
                words
            }
        }
    }
}

/// A disk's regions, free and partitions, in the order they lie on it.
#[must_use]
pub fn regions(disk: &Disk) -> Vec<Region> {
    let mut all: Vec<Region> = disk
        .free
        .iter()
        .copied()
        .map(Region::Free)
        .chain(disk.parts.iter().cloned().map(Region::Part))
        .collect();
    // A disk with no table is offered whole, as Erase: its one free
    // region is the disk.
    if disk.table == protocol::Table::None {
        all.retain(|region| matches!(region, Region::Part(_)));
    }
    all.sort_by_key(Region::first);
    all
}

/// A disk in words: `Samsung SSD 970 EVO Plus (256 GiB, /dev/nvme0n1)`.
#[must_use]
pub fn disk_words(disk: &Disk) -> String {
    let model = if disk.model.is_empty() {
        "Disk"
    } else {
        disk.model.as_str()
    };
    format!("{model} ({}, {})", protocol::size(disk.bytes), disk.dev)
}

/// An install's progress, as the stream has told it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// The step now and of how many.
    pub step: (u32, u32),
    /// What the step is.
    pub text: String,
    /// 0 to 100.
    pub percent: u8,
    /// Every log line.
    pub log: Vec<String>,
    /// Why it failed, once it has.
    pub error: Option<String>,
}

/// What a press asks of the system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Close the window: Try Ferrix.
    Close,
    /// Make the live session type in this layout and variant.
    Keyboard(String, String),
    /// Ask the daemon for the disks.
    Disks,
    /// Start the install.
    Install(Install),
    /// Restart the machine.
    Reboot,
}

/// Everything the window shows.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    /// The page shown.
    pub page: Page,
    /// The keyboard: an xkb layout and variant.
    pub layout: String,
    /// The variant; empty is the layout's default.
    pub variant: String,
    /// What was typed into the try line.
    pub tried: String,
    /// The daemon's disks, or why there are none.
    pub disks: Option<Result<Disks, String>>,
    /// The disk chosen, by device.
    pub disk: Option<String>,
    /// How.
    pub mode: Mode,
    /// The region chosen, by its first sector.
    pub region: Option<u64>,
    /// The full name.
    pub fullname: String,
    /// The user name.
    pub user: String,
    /// Whether the user name was typed, not derived.
    pub user_typed: bool,
    /// The computer's name.
    pub hostname: String,
    /// The password.
    pub password: String,
    /// The password again.
    pub confirm: String,
    /// Whether Next was pressed on You, so every problem shows.
    pub you_tried: bool,
    /// Every zone offered.
    pub zones: Vec<String>,
    /// The zone search.
    pub zone_query: String,
    /// The zone chosen.
    pub zone: String,
    /// The install.
    pub progress: Progress,
    /// Whether the install's log is shown.
    pub details: bool,
    /// What the window last could not do, said at its foot.
    pub status: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        State {
            page: Page::Welcome,
            layout: "us".to_owned(),
            variant: String::new(),
            tried: String::new(),
            disks: None,
            disk: None,
            mode: Mode::Erase,
            region: None,
            fullname: String::new(),
            user: String::new(),
            user_typed: false,
            hostname: "ferrix".to_owned(),
            password: String::new(),
            confirm: String::new(),
            you_tried: false,
            zones: vec![crate::zones::DEFAULT.to_owned()],
            zone_query: String::new(),
            zone: crate::zones::DEFAULT.to_owned(),
            progress: Progress::default(),
            details: false,
            status: None,
        }
    }
}

/// Why `disk` cannot take Ferrix at all, or `None`.
fn too_small(disk: &Disk, min: u64) -> Option<String> {
    (disk.bytes < min).then(|| format!("Too small: Ferrix needs at least {}.", protocol::size(min)))
}

impl State {
    /// A state with the daemon's disks, the zones, and the first disk
    /// that can take Ferrix chosen.
    pub fn set_disks(&mut self, disks: Result<Disks, String>) {
        self.disks = Some(disks);
        let keep = self.disk.clone().filter(|dev| {
            self.offered()
                .iter()
                .any(|d| &d.dev == dev && self.usable(d))
        });
        if keep.is_none() {
            let first = self
                .offered()
                .into_iter()
                .find(|disk| self.usable(disk))
                .map(|disk| disk.dev.clone());
            self.choose_disk(first);
        }
    }

    /// The smallest space Ferrix installs into.
    fn min(&self) -> u64 {
        match &self.disks {
            Some(Ok(disks)) => disks.min,
            _ => 0,
        }
    }

    /// The disks offered.
    #[must_use]
    pub fn offered(&self) -> Vec<&Disk> {
        match &self.disks {
            Some(Ok(disks)) => disks.offered(),
            _ => Vec::new(),
        }
    }

    /// Whether `disk` can take Ferrix.
    fn usable(&self, disk: &Disk) -> bool {
        too_small(disk, self.min()).is_none()
    }

    /// The disk chosen.
    #[must_use]
    pub fn chosen_disk(&self) -> Option<&Disk> {
        let dev = self.disk.as_deref()?;
        self.offered().into_iter().find(|disk| disk.dev == dev)
    }

    /// The regions of the chosen disk Ferrix fits in.
    fn fitting(&self) -> Vec<Region> {
        let min = self.min();
        self.chosen_disk()
            .map(|disk| {
                regions(disk)
                    .into_iter()
                    .filter(|region| region.bytes() >= min)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The region chosen.
    #[must_use]
    pub fn chosen_region(&self) -> Option<Region> {
        let first = self.region?;
        self.fitting().into_iter().find(|r| r.first() == first)
    }

    /// Choose the disk `dev`, and how: all of a blank one, else the
    /// first free region it has room in, else all of it.
    fn choose_disk(&mut self, dev: Option<String>) {
        self.disk = dev;
        self.region = None;
        self.mode = Mode::Erase;
        let Some(disk) = self.chosen_disk() else {
            return;
        };
        if disk.parts.is_empty() {
            return;
        }
        let free = self
            .fitting()
            .into_iter()
            .find(|region| matches!(region, Region::Free(_)));
        if let Some(free) = free {
            self.mode = Mode::Alongside;
            self.region = Some(free.first());
        }
    }

    /// What will be erased, in words; `None` when nothing is.
    #[must_use]
    pub fn erased(&self) -> Option<String> {
        let disk = self.chosen_disk()?;
        match self.mode {
            Mode::Erase => Some(format!(
                "Everything on {} is erased: every partition, file and system on it.",
                disk_words(disk)
            )),
            Mode::Alongside => match self.chosen_region()? {
                Region::Free(_) => None,
                Region::Part(part) => Some(format!(
                    "Partition {} on {} ({}, {}{}) is erased, and every file on it.",
                    part.number,
                    disk.dev,
                    protocol::fs_name(&part.fs),
                    protocol::size(part.bytes),
                    if part.name.is_empty() {
                        String::new()
                    } else {
                        format!(", “{}”", part.name)
                    }
                )),
            },
        }
    }

    /// Where on the disk Ferrix goes, in words.
    #[must_use]
    pub fn target_words(&self) -> String {
        if self.chosen_disk().is_none() {
            return "No disk chosen".to_owned();
        }
        match (self.mode, self.chosen_region()) {
            (Mode::Erase, _) => "The whole disk, erased".to_owned(),
            (Mode::Alongside, Some(region @ Region::Free(_))) => {
                format!("{} of free space", protocol::size(region.bytes()))
            }
            (Mode::Alongside, Some(region)) => {
                format!(
                    "{} ({}), erased",
                    region.words(),
                    protocol::size(region.bytes())
                )
            }
            (Mode::Alongside, None) => "None chosen".to_owned(),
        }
    }

    /// The install request the choices make, or `None` while one is
    /// missing.
    #[must_use]
    pub fn request(&self) -> Option<Install> {
        let disk = self.chosen_disk()?;
        if !self.usable(disk) || !self.you_ok() {
            return None;
        }
        let target = match self.mode {
            Mode::Erase => Target::Erase,
            Mode::Alongside => match self.chosen_region()? {
                Region::Free(free) => Target::Free {
                    first: free.first,
                    last: free.last,
                },
                Region::Part(part) => Target::Partition(part.number),
            },
        };
        Some(Install {
            dev: disk.dev.clone(),
            target,
            user: self.user.clone(),
            fullname: self.fullname.trim().to_owned(),
            password: self.password.clone(),
            hostname: self.hostname.clone(),
            keyboard: self.layout.clone(),
            variant: self.variant.clone(),
            timezone: self.zone.clone(),
            language: "en".to_owned(),
        })
    }

    /// Whether the You page's answers can be used.
    fn you_ok(&self) -> bool {
        !self.fullname.trim().is_empty()
            && account::user_problem(&self.user).is_none()
            && account::hostname_problem(&self.hostname).is_none()
            && !matches!(
                account::password(&self.password, &self.confirm),
                Password::Refused(_)
            )
    }

    /// Whether Next may be pressed.
    #[must_use]
    pub fn can_next(&self) -> bool {
        match self.page {
            Page::Welcome | Page::Keyboard | Page::Zone => true,
            // You says what is wrong when Next is pressed.
            Page::You => true,
            Page::Disk => {
                self.chosen_disk().is_some_and(|d| self.usable(d))
                    && (self.mode == Mode::Erase || self.chosen_region().is_some())
            }
            Page::Summary => self.request().is_some(),
            Page::Installing | Page::Done | Page::Failed => false,
        }
    }

    /// Next's words.
    #[must_use]
    pub fn next_label(&self) -> &'static str {
        if self.page == Page::Summary {
            "Install"
        } else {
            "Continue"
        }
    }

    /// Back's words.
    #[must_use]
    pub fn back_label(&self) -> &'static str {
        if self.page == Page::Failed {
            "Back to the summary"
        } else {
            "Back"
        }
    }

    /// Whether the foot, Back and Next, is shown.
    #[must_use]
    pub fn has_foot(&self) -> bool {
        !matches!(self.page, Page::Welcome | Page::Installing | Page::Done)
    }

    /// Whether Next is shown.
    #[must_use]
    pub fn has_next(&self) -> bool {
        self.page != Page::Failed
    }

    /// Back: the page before.
    pub fn back(&mut self) {
        self.page = match self.page {
            Page::Welcome | Page::Keyboard => Page::Welcome,
            Page::Disk => Page::Keyboard,
            Page::You => Page::Disk,
            Page::Zone => Page::You,
            Page::Summary | Page::Failed => Page::Summary,
            Page::Installing => Page::Installing,
            Page::Done => Page::Done,
        };
    }

    /// Next: the page after, when this one is answered.
    pub fn go_next(&mut self) -> Option<Effect> {
        if !self.can_next() {
            return None;
        }
        match self.page {
            Page::Welcome => self.page = Page::Keyboard,
            Page::Keyboard => {
                self.page = Page::Disk;
                if self.disks.is_none() {
                    return Some(Effect::Disks);
                }
            }
            Page::Disk => self.page = Page::You,
            Page::You => {
                if self.you_ok() {
                    self.page = Page::Zone;
                } else {
                    self.you_tried = true;
                }
            }
            Page::Zone => self.page = Page::Summary,
            Page::Summary => {
                let request = self.request()?;
                self.page = Page::Installing;
                self.progress = Progress::default();
                self.details = false;
                return Some(Effect::Install(request));
            }
            Page::Installing | Page::Done | Page::Failed => {}
        }
        None
    }

    /// A line of the install's stream.
    pub fn event(&mut self, event: protocol::Event) {
        match event {
            protocol::Event::Step(n, of, text) => {
                self.progress.step = (n, of);
                self.progress.text = text;
            }
            protocol::Event::Progress(percent) => self.progress.percent = percent,
            protocol::Event::Log(line) => self.progress.log.push(line),
            protocol::Event::Done => {
                self.progress.percent = 100;
                self.page = Page::Done;
            }
            protocol::Event::Error(why) => {
                self.progress.error = Some(why);
                self.page = Page::Failed;
            }
        }
    }

    /// A press on a row's button or card.
    pub fn press(&mut self, key: &Key, option: usize) -> Option<Effect> {
        match key {
            Key::Start => {
                if option == 0 {
                    return Some(Effect::Close);
                }
                return self.go_next();
            }
            Key::Layout(id) => {
                self.layout = (*id).to_owned();
                self.variant.clear();
                return Some(Effect::Keyboard(self.layout.clone(), String::new()));
            }
            Key::Variant => {
                let variants = keyboard::layout(&self.layout).map_or(&[][..], |l| l.variants);
                self.variant = match option.checked_sub(1) {
                    None => String::new(),
                    Some(at) => variants.get(at).map(|v| v.0.to_owned())?,
                };
                return Some(Effect::Keyboard(self.layout.clone(), self.variant.clone()));
            }
            Key::Disk(dev) => {
                let usable = self
                    .offered()
                    .into_iter()
                    .find(|disk| &disk.dev == dev)
                    .is_some_and(|disk| self.usable(disk));
                if usable && self.disk.as_ref() != Some(dev) {
                    self.choose_disk(Some(dev.clone()));
                }
            }
            Key::Rescan => return Some(Effect::Disks),
            Key::Erase => {
                self.mode = Mode::Erase;
            }
            Key::Alongside => {
                if !self.fitting().is_empty() {
                    self.mode = Mode::Alongside;
                    if self.chosen_region().is_none() {
                        self.region = self.fitting().first().map(Region::first);
                    }
                }
            }
            Key::Region(first) => {
                if self.fitting().iter().any(|r| r.first() == *first) {
                    self.mode = Mode::Alongside;
                    self.region = Some(*first);
                }
            }
            Key::Zone(zone) => self.zone.clone_from(zone),
            Key::Details => self.details = !self.details,
            Key::Restart => return Some(Effect::Reboot),
            Key::None
            | Key::Language
            | Key::TryKeys
            | Key::FullName
            | Key::User
            | Key::Host
            | Key::Password
            | Key::Confirm
            | Key::ZoneSearch => {}
        }
        None
    }

    /// Text typed into a row's field.
    pub fn edited(&mut self, key: &Key, text: &str) {
        match key {
            Key::TryKeys => self.tried = text.to_owned(),
            Key::FullName => {
                self.fullname = text.to_owned();
                if !self.user_typed {
                    self.user = account::derive_user(text);
                }
            }
            Key::User => {
                self.user = text.to_owned();
                self.user_typed = !text.is_empty();
            }
            Key::Host => self.hostname = text.to_owned(),
            Key::Password => self.password = text.to_owned(),
            Key::Confirm => self.confirm = text.to_owned(),
            Key::ZoneSearch => self.zone_query = text.to_owned(),
            _ => {}
        }
    }

    /// Return in a row's field: the next page, or in the zone search the
    /// first zone it leaves.
    pub fn accept(&mut self, key: &Key) -> Option<Effect> {
        if *key == Key::ZoneSearch {
            if let Some(zone) = crate::zones::filter(&self.zones, &self.zone_query).first() {
                self.zone = (*zone).clone();
            }
            return None;
        }
        if *key == Key::TryKeys {
            return None;
        }
        self.go_next()
    }

    /// The rows of the page shown.
    #[must_use]
    pub fn rows(&self) -> Vec<Line> {
        match self.page {
            Page::Welcome => self.welcome(),
            Page::Keyboard => self.keyboard(),
            Page::Disk => self.disk_rows(),
            Page::You => self.you(),
            Page::Zone => self.zone_rows(),
            Page::Summary => self.summary(),
            Page::Installing => self.installing(),
            Page::Done => vec![
                Line::new(Kind::Hero, Key::None, "Ferrix is installed")
                    .desc("Remove the installation medium, then restart.")
                    .tag("done"),
                Line {
                    options: vec!["Restart now".to_owned()],
                    current: 0,
                    ..Line::new(Kind::Buttons, Key::Restart, "")
                },
            ],
            Page::Failed => self.failed(),
        }
    }

    fn welcome(&self) -> Vec<Line> {
        vec![
            Line::new(Kind::Hero, Key::None, "Welcome to Ferrix")
                .desc("Try it from this medium, or install it on this computer.")
                .tag("mark"),
            Line::new(Kind::Section, Key::None, "Language"),
            Line::new(Kind::Pick, Key::Language, "English").on(true),
            Line::new(
                Kind::Empty,
                Key::None,
                "More languages come in a later release.",
            ),
            Line {
                options: vec!["Try Ferrix".to_owned(), "Install Ferrix".to_owned()],
                current: 1,
                desc: "Try Ferrix changes nothing on this computer.".to_owned(),
                ..Line::new(Kind::Buttons, Key::Start, "")
            },
        ]
    }

    fn keyboard(&self) -> Vec<Line> {
        let mut rows = vec![Line {
            shown: self.tried.clone(),
            ..Line::new(Kind::Field, Key::TryKeys, "")
                .desc("Type here to try your keyboard")
                .tag("big")
        }];
        let layout = keyboard::layout(&self.layout);
        let variants = layout.map_or(&[][..], |l| l.variants);
        let mut options = vec!["Default".to_owned()];
        options.extend(variants.iter().map(|(_, name)| (*name).to_owned()));
        let current = variants
            .iter()
            .position(|(id, _)| *id == self.variant)
            .map_or(0, |at| at + 1);
        rows.push(Line {
            options,
            current: i32::try_from(current).unwrap_or(0),
            ..Line::new(Kind::Choice, Key::Variant, "Variant")
                .desc(layout.map_or(self.layout.as_str(), |l| l.name))
        });
        rows.push(Line::new(Kind::Section, Key::None, "Layout"));
        let mut sorted: Vec<_> = LAYOUTS.iter().collect();
        sorted.sort_by_key(|layout| (layout.id != "us", layout.name));
        for each in sorted {
            rows.push(
                Line::new(Kind::Pick, Key::Layout(each.id), each.name)
                    .tag(each.id)
                    .on(each.id == self.layout),
            );
        }
        rows
    }

    fn disk_rows(&self) -> Vec<Line> {
        let mut rows = Vec::new();
        let disks = match &self.disks {
            None => {
                rows.push(Line::new(Kind::Empty, Key::None, "Looking for disks…"));
                return rows;
            }
            Some(Err(why)) => {
                rows.push(
                    Line::new(Kind::Warning, Key::None, "The disks could not be read")
                        .desc(why.clone())
                        .level(level::DANGER),
                );
                rows.push(Line {
                    options: vec!["Look again".to_owned()],
                    current: -1,
                    ..Line::new(Kind::Buttons, Key::Rescan, "")
                });
                return rows;
            }
            Some(Ok(disks)) => disks,
        };
        let offered = disks.offered();
        if offered.is_empty() {
            rows.push(Line::new(
                Kind::Warning,
                Key::None,
                "No disk to install on",
            )
            .desc("This computer has no disk but the one Ferrix is running from. Connect one, then look again.")
            .level(level::WARN));
            rows.push(Line {
                options: vec!["Look again".to_owned()],
                current: -1,
                ..Line::new(Kind::Buttons, Key::Rescan, "")
            });
            return rows;
        }
        rows.push(Line::new(Kind::Section, Key::None, "Disk"));
        for disk in &offered {
            let mut desc = match disk.table {
                protocol::Table::None => "Empty: no partitions".to_owned(),
                _ if disk.parts.is_empty() => "No partitions".to_owned(),
                _ => {
                    let n = disk.parts.len();
                    let names: Vec<String> = disk
                        .parts
                        .iter()
                        .map(|p| protocol::fs_name(&p.fs).to_owned())
                        .collect();
                    format!(
                        "{n} partition{}: {}",
                        if n == 1 { "" } else { "s" },
                        names.join(", ")
                    )
                }
            };
            let small = too_small(disk, disks.min);
            if let Some(why) = &small {
                desc = why.clone();
            }
            let model = if disk.model.is_empty() {
                "Disk".to_owned()
            } else {
                disk.model.clone()
            };
            rows.push(
                Line::new(Kind::Radio, Key::Disk(disk.dev.clone()), model)
                    .desc(format!("{} · {desc}", disk.dev))
                    .tag(protocol::size(disk.bytes))
                    .on(self.disk.as_deref() == Some(disk.dev.as_str()))
                    .enabled(small.is_none()),
            );
        }
        let Some(disk) = self.chosen_disk() else {
            return rows;
        };
        rows.push(Line::new(Kind::Section, Key::None, "How"));
        let mut erase = Line::new(Kind::Radio, Key::Erase, "Erase disk and install Ferrix")
            .desc("Ferrix takes the whole disk.")
            .on(self.mode == Mode::Erase);
        if self.mode == Mode::Erase
            && let Some(erased) = self.erased()
        {
            erase.note = erased;
            erase.level = level::DANGER;
        }
        rows.push(erase);
        let fitting = self.fitting();
        let alongside_desc = if fitting.is_empty() {
            format!(
                "No free space or partition of at least {} on this disk.",
                protocol::size(disks.min)
            )
        } else {
            "Keep what else is on the disk.".to_owned()
        };
        rows.push(
            Line::new(
                Kind::Radio,
                Key::Alongside,
                "Install into free space or a partition",
            )
            .desc(alongside_desc)
            .on(self.mode == Mode::Alongside)
            .enabled(!fitting.is_empty()),
        );
        if self.mode == Mode::Alongside {
            rows.push(Line::new(Kind::Section, Key::None, "Where"));
            for region in regions(disk) {
                let fits = region.bytes() >= disks.min;
                let on = fits && self.region == Some(region.first());
                let desc = match (&region, fits) {
                    (_, false) => format!(
                        "Too small: Ferrix needs at least {}.",
                        protocol::size(disks.min)
                    ),
                    (Region::Free(_), true) => "Nothing is erased.".to_owned(),
                    (Region::Part(_), true) => "Its contents are erased.".to_owned(),
                };
                let mut line = Line::new(Kind::Radio, Key::Region(region.first()), region.words())
                    .desc(desc)
                    .tag(protocol::size(region.bytes()))
                    .on(on)
                    .enabled(fits);
                if on && let Some(erased) = self.erased() {
                    line.note = erased;
                    line.level = level::DANGER;
                }
                rows.push(line);
            }
        }
        rows
    }

    fn you(&self) -> Vec<Line> {
        let shown = |problem: Option<String>, typed: bool| {
            if typed || self.you_tried {
                problem.unwrap_or_default()
            } else {
                String::new()
            }
        };
        let field = |key: Key, title: &str, value: &str, hint: &str| Line {
            shown: value.to_owned(),
            ..Line::new(Kind::Field, key, title).desc(hint)
        };
        let name_problem = self
            .fullname
            .trim()
            .is_empty()
            .then(|| "Type your name.".to_owned());
        let mut name = field(Key::FullName, "Your name", &self.fullname, "Ada Lovelace");
        name.note = shown(name_problem, false);
        name.level = level::DANGER;
        let mut user = field(Key::User, "User name", &self.user, "ada");
        user.note = shown(account::user_problem(&self.user), !self.user.is_empty());
        user.level = level::DANGER;
        if user.note.is_empty() {
            user.note = "You sign in with it; your home folder is named after it.".to_owned();
            user.level = level::PLAIN;
        }
        let mut host = field(Key::Host, "Computer name", &self.hostname, "ferrix");
        host.note = shown(account::hostname_problem(&self.hostname), true);
        host.level = level::DANGER;
        if host.note.is_empty() {
            host.note = "The name other computers see it by.".to_owned();
            host.level = level::PLAIN;
        }
        let mut password = field(Key::Password, "Password", &self.password, "");
        password.tag = "password".to_owned();
        let mut confirm = field(Key::Confirm, "Confirm password", &self.confirm, "");
        confirm.tag = "password".to_owned();
        match account::password(&self.password, &self.confirm) {
            Password::Fine => {}
            Password::Weak(why) => {
                password.note = why;
                password.level = level::WARN;
            }
            Password::Refused(why) => {
                let typed = !self.confirm.is_empty() || self.you_tried;
                confirm.note = shown(Some(why), typed);
                confirm.level = level::DANGER;
            }
        }
        vec![name, user, host, password, confirm]
    }

    fn zone_rows(&self) -> Vec<Line> {
        let mut rows = vec![Line {
            shown: self.zone_query.clone(),
            ..Line::new(Kind::Field, Key::ZoneSearch, "")
                .desc("Search: a city or a region")
                .tag("search")
        }];
        rows.push(Line::new(Kind::Section, Key::None, "Time zone").tag(zone_words(&self.zone)));
        let found = crate::zones::filter(&self.zones, &self.zone_query);
        if found.is_empty() {
            rows.push(Line::new(
                Kind::Empty,
                Key::None,
                format!("No time zone matches “{}”.", self.zone_query.trim()),
            ));
        }
        for zone in found {
            rows.push(
                Line::new(Kind::Pick, Key::Zone(zone.clone()), zone_words(zone))
                    .on(*zone == self.zone),
            );
        }
        rows
    }

    fn summary(&self) -> Vec<Line> {
        let mut rows = vec![Line {
            options: [
                "Language",
                "Keyboard",
                "Disk",
                "Install into",
                "Your name",
                "User name",
                "Computer name",
                "Time zone",
            ]
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
            values: vec![
                "English".to_owned(),
                keyboard::describe(&self.layout, &self.variant),
                self.chosen_disk()
                    .map_or_else(|| "None chosen".to_owned(), disk_words),
                self.target_words(),
                self.fullname.trim().to_owned(),
                self.user.clone(),
                self.hostname.clone(),
                zone_words(&self.zone),
            ],
            ..Line::new(Kind::Info, Key::None, "")
        }];
        match self.erased() {
            Some(erased) => rows.push(
                Line::new(
                    Kind::Warning,
                    Key::None,
                    "This is erased when you press Install",
                )
                .desc(erased)
                .level(level::DANGER),
            ),
            None => rows.push(
                Line::new(Kind::Warning, Key::None, "Nothing is erased")
                    .desc("Ferrix goes into free space. What else is on the disk stays as it is.")
                    .level(level::PLAIN),
            ),
        }
        rows
    }

    fn installing(&self) -> Vec<Line> {
        let (n, of) = self.progress.step;
        let title = if of == 0 {
            "Starting…".to_owned()
        } else {
            format!("Step {n} of {of}: {}", self.progress.text)
        };
        let mut rows = vec![Line {
            value: f32::from(self.progress.percent) / 100.0,
            tag: format!("{} %", self.progress.percent),
            ..Line::new(Kind::Progress, Key::None, title)
        }];
        rows.push(
            Line::new(
                Kind::Link,
                Key::Details,
                if self.details {
                    "Hide details"
                } else {
                    "Show details"
                },
            )
            .on(self.details),
        );
        if self.details {
            rows.extend(self.log_rows());
        }
        rows
    }

    fn log_rows(&self) -> Vec<Line> {
        if self.progress.log.is_empty() {
            return vec![Line::new(Kind::Log, Key::None, "(nothing logged yet)")];
        }
        self.progress
            .log
            .iter()
            .map(|line| Line::new(Kind::Log, Key::None, line.clone()))
            .collect()
    }

    fn failed(&self) -> Vec<Line> {
        let why = self
            .progress
            .error
            .clone()
            .unwrap_or_else(|| "The installer stopped.".to_owned());
        let mut rows = vec![
            Line::new(Kind::Warning, Key::None, "What went wrong")
                .desc(why)
                .level(level::DANGER),
            Line::new(Kind::Section, Key::None, "Log"),
        ];
        rows.extend(self.log_rows());
        rows
    }
}

/// A zone as a person reads it: `America/New York`.
#[must_use]
pub fn zone_words(zone: &str) -> String {
    zone.replace('_', " ")
}

/// `text` broken into lines of about `width` characters, between words.
#[must_use]
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::{Effect, Key, Mode, Page, State, wrap};
    use crate::protocol::{Target, parse_disks};

    fn with_disks() -> State {
        let mut state = State::default();
        state.set_disks(parse_disks(crate::fake::DISKS));
        state
    }

    #[test]
    fn the_live_disk_is_not_offered_and_a_small_one_is_disabled() {
        let state = with_disks();
        let disks: Vec<_> = state.offered().iter().map(|d| d.dev.clone()).collect();
        assert_eq!(disks, ["/dev/vdb", "/dev/nvme0n1", "/dev/sdb"]);
        let mut state = state;
        state.page = Page::Disk;
        let rows = state.rows();
        let small = rows
            .iter()
            .find(|r| r.key == Key::Disk("/dev/sdb".to_owned()))
            .expect("the stick");
        assert!(!small.enabled);
        assert!(small.desc.contains("Too small"));
        // Pressing it changes nothing.
        let _ = state.press(&Key::Disk("/dev/sdb".to_owned()), 0);
        assert_eq!(state.disk.as_deref(), Some("/dev/vdb"));
    }

    #[test]
    fn a_blank_disk_is_erased_and_a_used_one_takes_its_free_space() {
        let mut state = with_disks();
        state.page = Page::Disk;
        assert_eq!(state.mode, Mode::Erase);
        assert!(
            state
                .erased()
                .is_some_and(|e| e.contains("Virtio Block Device (32 GiB, /dev/vdb)"))
        );
        let _ = state.press(&Key::Disk("/dev/nvme0n1".to_owned()), 0);
        assert_eq!(state.mode, Mode::Alongside);
        assert_eq!(state.region, Some(327_155_712));
        assert_eq!(state.erased(), None, "free space erases nothing");
        // The NTFS partition: its contents are erased, and said so.
        let _ = state.press(&Key::Region(1_050_624), 0);
        assert!(
            state
                .erased()
                .is_some_and(|e| e.contains("Partition 2") && e.contains("NTFS"))
        );
        // The EFI partition is too small to choose.
        let _ = state.press(&Key::Region(2048), 0);
        assert_eq!(state.region, Some(1_050_624));
        let rows = state.rows();
        assert!(
            rows.iter()
                .any(|r| r.on && r.level == super::level::DANGER && r.note.contains("Partition 2"))
        );
    }

    #[test]
    fn the_pages_go_in_order_and_install_asks_for_what_was_chosen() {
        let mut state = State::default();
        assert_eq!(state.press(&Key::Start, 1), None);
        assert_eq!(state.page, Page::Keyboard);
        assert_eq!(
            state.press(&Key::Layout("de"), 0),
            Some(Effect::Keyboard("de".to_owned(), String::new()))
        );
        assert_eq!(
            state.press(&Key::Variant, 1),
            Some(Effect::Keyboard("de".to_owned(), "nodeadkeys".to_owned()))
        );
        assert_eq!(state.go_next(), Some(Effect::Disks));
        assert_eq!(state.page, Page::Disk);
        assert!(!state.can_next(), "no disks yet");
        state.set_disks(parse_disks(crate::fake::DISKS));
        assert_eq!(state.go_next(), None);
        assert_eq!(state.page, Page::You);
        // Next with nothing typed stays, and says what is missing.
        assert_eq!(state.go_next(), None);
        assert_eq!(state.page, Page::You);
        assert!(state.rows().iter().any(|r| r.note == "Type your name."));
        state.edited(&Key::FullName, "Ada Lovelace");
        assert_eq!(state.user, "ada");
        state.edited(&Key::Password, "correct horse");
        state.edited(&Key::Confirm, "correct horse");
        assert_eq!(state.go_next(), None);
        assert_eq!(state.page, Page::Zone);
        state.edited(&Key::ZoneSearch, "vienna");
        state.zones = crate::zones::sample();
        assert_eq!(state.accept(&Key::ZoneSearch), None);
        assert_eq!(state.zone, "Europe/Vienna");
        let _ = state.go_next();
        assert_eq!(state.page, Page::Summary);
        let Some(Effect::Install(install)) = state.go_next() else {
            panic!("no install");
        };
        assert_eq!(state.page, Page::Installing);
        assert_eq!(install.dev, "/dev/vdb");
        assert_eq!(install.target, Target::Erase);
        assert_eq!(install.keyboard, "de");
        assert_eq!(install.variant, "nodeadkeys");
        assert_eq!(install.timezone, "Europe/Vienna");
        assert_eq!(install.hostname, "ferrix");
    }

    #[test]
    fn a_typed_user_name_is_kept_and_a_bad_one_said() {
        let mut state = State {
            page: Page::You,
            ..State::default()
        };
        state.edited(&Key::FullName, "Ada");
        state.edited(&Key::User, "Root");
        state.edited(&Key::FullName, "Ada Lovelace");
        assert_eq!(state.user, "Root");
        let rows = state.rows();
        let user = rows.iter().find(|r| r.key == Key::User).expect("user");
        assert!(user.note.contains("small letter"));
        state.edited(&Key::Password, "short");
        state.edited(&Key::Confirm, "short");
        let rows = state.rows();
        let password = rows.iter().find(|r| r.key == Key::Password).expect("pw");
        assert!(password.note.contains("8 characters"));
    }

    #[test]
    fn the_stream_moves_the_page_to_done_or_failed() {
        let mut state = State {
            page: Page::Installing,
            ..State::default()
        };
        state.event(crate::protocol::Event::Step(3, 7, "Copying".to_owned()));
        state.event(crate::protocol::Event::Progress(42));
        assert!(
            state
                .rows()
                .iter()
                .any(|r| r.title == "Step 3 of 7: Copying")
        );
        state.event(crate::protocol::Event::Error("disk full".to_owned()));
        assert_eq!(state.page, Page::Failed);
        state.back();
        assert_eq!(state.page, Page::Summary);
    }

    #[test]
    fn try_ferrix_closes() {
        let mut state = State::default();
        assert_eq!(state.press(&Key::Start, 0), Some(Effect::Close));
    }

    #[test]
    fn words_wrap_between_words() {
        assert_eq!(wrap("one two three four", 9), ["one two", "three", "four"]);
    }
}
