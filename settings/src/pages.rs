//! What the window shows, with no window: the pages and their groups, the
//! search, the page's rows, and what a press on each does.
//!
//! The pages are the customer's design (`Ferrix Settings.dc.html`): four
//! groups -- Desktop, Session, System, Advanced -- of thirteen pages. A
//! page is a list of rows ([`Line`]), each a kind the markup draws
//! (`ui/shell.tole`'s `RowKind`) and what pressing it does ([`Key`]).
//! The search narrows the sidebar to the pages whose name or words match;
//! nothing matching says so.
//!
//! A change says when it takes effect: at once, after signing in again,
//! after a restart, or not kept yet -- one line under the setting, a
//! [`Note`].

use std::path::PathBuf;

use compositor_desktop::game_mode::GameMode;

use crate::apps::App;

/// The sidebar's groups, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// The screen, the windows, the keyboard.
    Desktop,
    /// What a session does: idle, game mode.
    Session,
    /// The computer: power, users, name, remote login, logs.
    System,
    /// How it starts.
    Advanced,
}

impl Group {
    /// Every group, in the sidebar's order.
    pub const ALL: [Group; 4] = [
        Group::Desktop,
        Group::Session,
        Group::System,
        Group::Advanced,
    ];

    /// Its name.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Group::Desktop => "Desktop",
            Group::Session => "Session",
            Group::System => "System",
            Group::Advanced => "Advanced",
        }
    }

    /// Its place in [`Group::ALL`], which is also its mark's number.
    #[must_use]
    pub fn index(self) -> usize {
        Group::ALL.iter().position(|g| *g == self).unwrap_or(0)
    }
}

/// The pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    /// Monitors: resolution, rate, scale, rotation, arrangement.
    Displays,
    /// Accent, spacing, corners, see-through windows, animations.
    #[default]
    Appearance,
    /// Layout, title bars, focus.
    Windows,
    /// The background.
    Wallpaper,
    /// Layouts and key repeat.
    Keyboard,
    /// Locking and the screen going off.
    Idle,
    /// Which applications start in game mode.
    Game,
    /// The power mode.
    Power,
    /// Accounts and passwords.
    Users,
    /// The computer's name, the clock, the time zone, and what it runs.
    Computer,
    /// Signing in from another computer.
    Remote,
    /// What the system log keeps.
    Logs,
    /// How the computer starts.
    Boot,
}

impl Page {
    /// Every page, in the sidebar's order.
    pub const ALL: [Page; 13] = [
        Page::Displays,
        Page::Appearance,
        Page::Windows,
        Page::Wallpaper,
        Page::Keyboard,
        Page::Idle,
        Page::Game,
        Page::Power,
        Page::Users,
        Page::Computer,
        Page::Remote,
        Page::Logs,
        Page::Boot,
    ];

    /// Its name on the command line: `--page game-mode`.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Page::Displays => "displays",
            Page::Appearance => "appearance",
            Page::Windows => "windows",
            Page::Wallpaper => "wallpaper",
            Page::Keyboard => "keyboard",
            Page::Idle => "idle",
            Page::Game => "game-mode",
            Page::Power => "power",
            Page::Users => "users",
            Page::Computer => "name-and-time",
            Page::Remote => "remote-login",
            Page::Logs => "logs",
            Page::Boot => "boot",
        }
    }

    /// The page named `name` on the command line.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Page::ALL
            .into_iter()
            .find(|page| page.id() == name || (name == "about" && *page == Page::Computer))
    }

    /// Its title.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Page::Displays => "Displays",
            Page::Appearance => "Appearance",
            Page::Windows => "Windows",
            Page::Wallpaper => "Wallpaper",
            Page::Keyboard => "Keyboard",
            Page::Idle => "Idle and lock",
            Page::Game => "Game mode",
            Page::Power => "Power",
            Page::Users => "Users",
            Page::Computer => "Name and time",
            Page::Remote => "Remote login",
            Page::Logs => "Logs",
            Page::Boot => "Boot options",
        }
    }

    /// The line under its title.
    #[must_use]
    pub fn subtitle(self) -> &'static str {
        match self {
            Page::Displays => "Select a display to change it.",
            Page::Appearance => "Changes take effect as you make them.",
            Page::Windows => "How new windows are placed on the screen.",
            Page::Wallpaper => "The picture behind your windows.",
            Page::Keyboard => "Layouts you type in, and how held keys repeat.",
            Page::Idle => "What happens when you step away.",
            Page::Game => "Apps you mark start in game mode from the launcher.",
            Page::Power => "How fast the processor runs, and how much power it uses.",
            Page::Users => "Who can sign in to this computer.",
            Page::Computer => "This computer's name, clock and time zone.",
            Page::Remote => "Sign in to this computer from another one.",
            Page::Logs => "What the system log keeps, and for how long.",
            Page::Boot => "How this computer starts, and what it does after a crash.",
        }
    }

    /// Its group.
    #[must_use]
    pub fn group(self) -> Group {
        match self {
            Page::Displays
            | Page::Appearance
            | Page::Windows
            | Page::Wallpaper
            | Page::Keyboard => Group::Desktop,
            Page::Idle | Page::Game => Group::Session,
            Page::Power | Page::Users | Page::Computer | Page::Remote | Page::Logs => Group::System,
            Page::Boot => Group::Advanced,
        }
    }

    /// The words the search finds it by, besides its name: the design's.
    #[must_use]
    pub fn keywords(self) -> &'static str {
        match self {
            Page::Displays => "monitor screen resolution scale refresh rotate",
            Page::Appearance => {
                "accent colour color gaps border rounding opacity blur shadow animation motion theme"
            }
            Page::Windows => "layout tiling title bars focus mouse",
            Page::Wallpaper => "background picture",
            Page::Keyboard => "layout repeat delay rate language",
            Page::Idle => "lock screen off sleep grace",
            Page::Game => "games apps launcher",
            Page::Power => "battery performance energy",
            Page::Users => "password account",
            Page::Computer => "hostname computer name clock date time zone about",
            Page::Remote => "ssh network",
            Page::Logs => "retention journal capture screenshot",
            Page::Boot => "boot cmdline checks panic target console certified",
        }
    }

    /// Whether the search `query` finds it: every word of it in the
    /// page's name or words, whatever the case.
    #[must_use]
    pub fn matches(self, query: &str) -> bool {
        let haystack = format!("{} {}", self.label(), self.keywords()).to_lowercase();
        query
            .split_whitespace()
            .all(|word| haystack.contains(&word.to_lowercase()))
    }
}

/// When a change takes effect, said under the setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Note {
    /// At once: nothing is said.
    Now,
    /// At the next sign-in.
    SignIn,
    /// At the next start.
    Restart,
    /// Not kept: gone at the next start.
    NotKept,
}

impl Note {
    /// The line under the setting, if any.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Note::Now => "",
            Note::SignIn => "Takes effect after you sign in again",
            Note::Restart => "Takes effect after a restart",
            Note::NotKept => "Not kept yet",
        }
    }
}

/// What a row of the page does when pressed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    /// Nothing: a heading, words.
    None,
    /// Game mode's filter.
    Filter,
    /// Application `id`'s game-mode switch.
    App(String),
}

/// A row's kind: the markup's `RowKind`, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Section,
    Choice,
    Switch,
    Slider,
    Check,
    Info,
    App,
    Accents,
    Empty,
    Field,
    Action,
    Layouts,
    Preview,
    Monitors,
    Name,
}

/// One row of a page, as the markup's `Row` holds it, and what it does.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub kind: Kind,
    pub title: String,
    pub desc: String,
    pub note: Note,
    pub options: Vec<String>,
    pub values: Vec<String>,
    /// Colours as `rrggbb`.
    pub colors: Vec<String>,
    pub nums: Vec<f32>,
    pub current: i32,
    pub on: bool,
    pub value: f32,
    pub from: f32,
    pub to: f32,
    pub step: f32,
    pub shown: String,
    pub tag: String,
    pub enabled: bool,
    /// What pressing it does.
    pub key: Key,
}

impl Line {
    /// A row of `kind` titled `title`, doing nothing yet.
    #[must_use]
    pub fn new(kind: Kind, title: &str) -> Self {
        Line {
            kind,
            title: title.to_owned(),
            desc: String::new(),
            note: Note::Now,
            options: Vec::new(),
            values: Vec::new(),
            colors: Vec::new(),
            nums: Vec::new(),
            current: -1,
            on: false,
            value: 0.0,
            from: 0.0,
            to: 1.0,
            step: 1.0,
            shown: String::new(),
            tag: String::new(),
            enabled: true,
            key: Key::None,
        }
    }

    /// With the line under its title.
    #[must_use]
    pub fn desc(mut self, desc: &str) -> Self {
        self.desc = desc.to_owned();
        self
    }

    /// With its note.
    #[must_use]
    pub fn note(mut self, note: Note) -> Self {
        self.note = note;
        self
    }

    /// Doing `key` when pressed.
    #[must_use]
    pub fn key(mut self, key: Key) -> Self {
        self.key = key;
        self
    }
}

/// A row of the sidebar: a group's, or one of its pages'.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    /// A group: pressing it opens or closes it.
    Group(Group),
    /// A page: pressing it shows it.
    Page(Page),
}

/// Game mode's page: the applications, the file, the filter.
#[derive(Clone, Debug, Default)]
pub struct Games {
    /// Every application a launcher lists.
    pub apps: Vec<App>,
    /// What the file holds.
    pub set: GameMode,
    /// The user's file, if there is a user.
    pub file: Option<PathBuf>,
    /// The filter typed.
    pub filter: String,
}

impl Games {
    /// The applications the filter leaves.
    pub fn shown(&self) -> impl Iterator<Item = &App> {
        self.apps.iter().filter(|app| app.matches(&self.filter))
    }
}

/// What a change asks of the system, for the window to do and say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Turn application `id`'s game-mode switch.
    GameMode {
        /// The application.
        id: String,
        /// On or off.
        on: bool,
    },
}

/// The whole state of the window.
#[derive(Clone, Debug)]
pub struct State {
    /// The page shown.
    pub page: Page,
    /// Which groups are open, by [`Group::index`].
    pub open: [bool; 4],
    /// The search typed.
    pub query: String,
    /// Whether the window is dark.
    pub dark: bool,
    /// The accent, an index into [`crate::look::ACCENTS`].
    pub accent: usize,
    /// Who is signed in, and where: the header's right.
    pub user: String,
    /// The computer's name.
    pub hostname: String,
    /// Game mode's page.
    pub games: Games,
    /// The last thing said at the window's foot, if anything.
    pub status: Option<String>,
}

impl State {
    /// The window on `page`, its group open.
    #[must_use]
    pub fn new(page: Page) -> Self {
        let mut open = [false; 4];
        if let Some(slot) = open.get_mut(page.group().index()) {
            *slot = true;
        }
        State {
            page,
            open,
            query: String::new(),
            dark: true,
            accent: 0,
            user: String::new(),
            hostname: String::new(),
            games: Games::default(),
            status: None,
        }
    }

    /// The sidebar: each group, and under an open one its pages. While
    /// searching, only the groups with a page found, open, and only those
    /// pages.
    #[must_use]
    pub fn nav(&self) -> Vec<Nav> {
        let searching = !self.query.trim().is_empty();
        let mut out = Vec::new();
        for group in Group::ALL {
            let pages: Vec<Page> = Page::ALL
                .into_iter()
                .filter(|p| p.group() == group && (!searching || p.matches(&self.query)))
                .collect();
            if pages.is_empty() {
                continue;
            }
            out.push(Nav::Group(group));
            if searching || self.open.get(group.index()).copied().unwrap_or(false) {
                out.extend(pages.into_iter().map(Nav::Page));
            }
        }
        out
    }

    /// Whether group `group` shows its pages.
    #[must_use]
    pub fn is_open(&self, group: Group) -> bool {
        !self.query.trim().is_empty() || self.open.get(group.index()).copied().unwrap_or(false)
    }

    /// Press sidebar row `nav`.
    pub fn navigate(&mut self, nav: Nav) {
        match nav {
            Nav::Group(group) => {
                if let Some(open) = self.open.get_mut(group.index()) {
                    *open = !*open;
                }
            }
            Nav::Page(page) => self.go(page),
        }
    }

    /// Show `page`, its group open.
    pub fn go(&mut self, page: Page) {
        self.page = page;
        if let Some(open) = self.open.get_mut(page.group().index()) {
            *open = true;
        }
    }

    /// The search typed: the sidebar narrows to it, and the first page it
    /// finds is shown if the one shown is not among them.
    pub fn search(&mut self, query: &str) {
        query.clone_into(&mut self.query);
        if !self.query.trim().is_empty()
            && !self.page.matches(&self.query)
            && let Some(first) = Page::ALL.into_iter().find(|p| p.matches(&self.query))
        {
            self.page = first;
        }
    }

    /// Whether the search finds nothing.
    #[must_use]
    pub fn no_results(&self) -> bool {
        !self.query.trim().is_empty() && !Page::ALL.into_iter().any(|p| p.matches(&self.query))
    }

    /// The rows of the page shown.
    #[must_use]
    pub fn rows(&self) -> Vec<Line> {
        match self.page {
            Page::Game => self.game_rows(),
            page => vec![Line::new(
                Kind::Empty,
                &format!("{} comes with the next landing of Settings.", page.label()),
            )],
        }
    }

    /// Game mode's rows: the filter, then each application it leaves.
    fn game_rows(&self) -> Vec<Line> {
        let mut rows = vec![{
            let mut filter = Line::new(Kind::Field, "")
                .desc("Filter apps")
                .key(Key::Filter);
            filter.shown.clone_from(&self.games.filter);
            filter
        }];
        if self.games.file.is_none() {
            rows.push(Line::new(
                Kind::Empty,
                "There is no home folder to keep the list in, so switches cannot be turned.",
            ));
        }
        let mut any = false;
        for app in self.games.shown() {
            any = true;
            let mut row = Line::new(Kind::App, &app.name).key(Key::App(app.id.clone()));
            row.desc = app.about.clone().unwrap_or_default();
            row.on = self.games.set.contains(&app.id);
            row.tag = app
                .name
                .chars()
                .next()
                .map(|c| c.to_uppercase().collect())
                .unwrap_or_default();
            row.enabled = self.games.file.is_some();
            rows.push(row);
        }
        if !any {
            rows.push(Line::new(
                Kind::Empty,
                if self.games.apps.is_empty() {
                    "No applications are installed."
                } else {
                    "No application matches the filter."
                },
            ));
        }
        rows
    }

    /// Text typed into row `row`'s field.
    pub fn edited(&mut self, row: &Line, text: &str) {
        if row.key == Key::Filter {
            text.clone_into(&mut self.games.filter);
        }
    }

    /// Row `row` turned: answers what that asks of the system.
    #[must_use]
    pub fn flip(&self, row: &Line) -> Option<Effect> {
        match &row.key {
            Key::App(id) if self.games.file.is_some() => Some(Effect::GameMode {
                id: id.clone(),
                on: !self.games.set.contains(id),
            }),
            _ => None,
        }
    }

    /// Return in row `row`: in game mode's filter, the first application
    /// it leaves is turned, as Return turned the selected one before.
    #[must_use]
    pub fn accept(&self, row: &Line) -> Option<Effect> {
        if row.key != Key::Filter {
            return None;
        }
        let app = self.games.shown().next()?;
        self.games.file.as_ref()?;
        Some(Effect::GameMode {
            id: app.id.clone(),
            on: !self.games.set.contains(&app.id),
        })
    }

    /// The badge on page `page`: changes made that wait for a sign-in or
    /// a restart.
    #[must_use]
    pub fn badge(&self, _page: Page) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::{Group, Kind, Nav, Page, State};
    use crate::apps::App;

    #[test]
    fn the_sidebar_opens_the_shown_pages_group_and_a_press_on_a_group_turns_it() {
        let mut state = State::new(Page::Appearance);
        let nav = state.nav();
        assert_eq!(nav.first(), Some(&Nav::Group(Group::Desktop)));
        assert!(nav.contains(&Nav::Page(Page::Keyboard)));
        assert!(!nav.contains(&Nav::Page(Page::Game)));
        state.navigate(Nav::Group(Group::Session));
        assert!(state.nav().contains(&Nav::Page(Page::Game)));
        state.navigate(Nav::Page(Page::Game));
        assert_eq!(state.page, Page::Game);
        state.navigate(Nav::Group(Group::Desktop));
        assert!(!state.nav().contains(&Nav::Page(Page::Keyboard)));
    }

    #[test]
    fn the_search_narrows_the_sidebar_and_says_when_nothing_matches() {
        let mut state = State::new(Page::Appearance);
        state.search("ssh");
        assert_eq!(
            state.nav(),
            vec![Nav::Group(Group::System), Nav::Page(Page::Remote)]
        );
        assert_eq!(state.page, Page::Remote, "the first found is shown");
        assert!(!state.no_results());
        state.search("blur");
        assert!(state.nav().contains(&Nav::Page(Page::Appearance)));
        state.search("zzz");
        assert!(state.nav().is_empty());
        assert!(state.no_results());
        state.search("");
        assert!(!state.no_results());
    }

    #[test]
    fn game_mode_lists_filters_and_return_turns_the_first_left() {
        let mut state = State::new(Page::Game);
        state.games.apps = vec![
            App::new("probe.desktop", "Game Probe", None, None),
            App::new("foot.desktop", "Terminal", Some("Terminal emulator"), None),
        ];
        state.games.file = Some("/nowhere/game-mode.conf".into());
        let rows = state.rows();
        assert_eq!(rows.first().map(|r| r.kind), Some(Kind::Field));
        assert_eq!(rows.iter().filter(|r| r.kind == Kind::App).count(), 2);
        let filter = rows.first().cloned().unwrap();
        state.edited(&filter, "probe");
        let rows = state.rows();
        let apps: Vec<&str> = rows
            .iter()
            .filter(|r| r.kind == Kind::App)
            .map(|r| r.title.as_str())
            .collect();
        assert_eq!(apps, ["Game Probe"]);
        assert_eq!(
            state.accept(&filter),
            Some(super::Effect::GameMode {
                id: "probe.desktop".to_owned(),
                on: true
            })
        );
        state.edited(&filter, "nothing like it");
        assert!(state.rows().iter().any(|r| r.kind == Kind::Empty));
        assert_eq!(state.accept(&filter), None);
    }
}
