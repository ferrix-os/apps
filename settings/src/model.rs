//! What the window shows, and what a key or a click does to it.
//!
//! No window, no file and no pixel: the pages, the applications and which of
//! them are in game mode, the filter being typed, the row selected and the
//! first row in view. The window turns keys and clicks into the calls here
//! and draws what is left; a switch that turns answers a [`Change`], which
//! the window writes to the user's file ([`crate::store`]).

use compositor_desktop::game_mode::GameMode;

use crate::apps::App;

/// A page of Settings. One so far; a new page is a new variant, a line in
/// [`Page::ALL`] and its drawing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    /// Which applications are started in game mode.
    #[default]
    GameMode,
}

impl Page {
    /// Every page, in the order the sidebar lists them.
    pub const ALL: [Self; 1] = [Self::GameMode];

    /// What the sidebar and the page's heading call it.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::GameMode => "Game mode",
        }
    }

    /// The name the command line knows it by (`--page`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::GameMode => "game-mode",
        }
    }

    /// The page `name` names.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|page| page.name() == name)
    }
}

/// What game mode does, said on the page above the list. It is the truth
/// about this version, and it changes when what game mode does changes
/// (Ferrix's `docs/SETTINGS.md` §4).
pub const EXPLANATION: &str = "\
An application switched on here is started as a game when you start it from \
the launcher. Today that does one thing: the application, and every program \
it starts, runs with FERRIX_GAME_MODE=1 in its environment. Nothing runs \
faster for it yet, and nothing is given up.\n\
The mark is what later work will act on: a larger share of processor time \
and processors of their own while a game is in front, and a shorter path to \
the graphics driver. One planned option would let a game skip a protection \
between itself and the graphics driver in exchange for speed; it will be a \
switch of its own, off until you turn it on. An application started from a \
terminal is not marked.";

/// What the last thing done came to, said at the foot of the window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// Nothing done yet.
    #[default]
    Idle,
    /// A switch was turned and the file written.
    Saved {
        /// The application's name.
        name: String,
        /// On or off.
        on: bool,
    },
    /// The file could not be written; the switch is as it was.
    Failed(String),
}

/// A switch the user turned, to be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The application's desktop-file ID.
    pub id: String,
    /// Its name, for what is said about it.
    pub name: String,
    /// Into game mode, or out of it.
    pub on: bool,
}

/// Everything the window shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Model {
    /// The page in view.
    pub page: Page,
    /// Every application installed, sorted.
    pub apps: Vec<App>,
    /// The ones in game mode, as last read from or written to the file.
    pub games: GameMode,
    /// What has been typed to narrow the list.
    pub filter: String,
    /// The applications the filter leaves, as places in `apps`.
    shown: Vec<usize>,
    /// The row selected, a place in the shown ones.
    pub selected: usize,
    /// The first row in view, a place in the shown ones.
    pub scroll: usize,
    /// The row the pointer is over, if any.
    pub hovered: Option<usize>,
    /// What the last change came to.
    pub status: Status,
}

impl Model {
    /// The window as it opens: every application, the first selected.
    #[must_use]
    pub fn new(apps: Vec<App>, games: GameMode) -> Self {
        let shown = (0..apps.len()).collect();
        Self {
            apps,
            games,
            shown,
            ..Self::default()
        }
    }

    /// The applications the filter leaves, in order.
    pub fn shown(&self) -> impl Iterator<Item = &App> {
        self.shown.iter().filter_map(|&at| self.apps.get(at))
    }

    /// How many the filter leaves.
    #[must_use]
    pub fn shown_len(&self) -> usize {
        self.shown.len()
    }

    /// The application in shown row `row`.
    #[must_use]
    pub fn row(&self, row: usize) -> Option<&App> {
        self.shown.get(row).and_then(|&at| self.apps.get(at))
    }

    /// Where shown row `row`'s application is among all of them, which is
    /// how the painter finds its icon.
    #[must_use]
    pub fn app_index(&self, row: usize) -> Option<usize> {
        self.shown.get(row).copied()
    }

    /// Whether `app` is in game mode.
    #[must_use]
    pub fn is_game(&self, app: &App) -> bool {
        self.games.contains(&app.id)
    }

    /// How many of the installed applications are in game mode. An ID in
    /// the file that no installed application has is not counted here: the
    /// page cannot show it.
    #[must_use]
    pub fn games_installed(&self) -> usize {
        self.apps.iter().filter(|app| self.is_game(app)).count()
    }

    /// Work out again which applications the filter leaves, keeping the
    /// selection on the application it was on where that is still shown.
    fn refilter(&mut self) {
        let kept = self.shown.get(self.selected).copied();
        self.shown = self
            .apps
            .iter()
            .enumerate()
            .filter(|(_, app)| app.matches(&self.filter))
            .map(|(at, _)| at)
            .collect();
        self.selected = kept
            .and_then(|kept| self.shown.iter().position(|&at| at == kept))
            .unwrap_or(0);
        self.scroll = 0;
        self.hovered = None;
    }

    /// Type `text` onto the end of the filter.
    pub fn type_text(&mut self, text: &str) {
        self.filter.push_str(text);
        self.refilter();
    }

    /// Take the filter's last character away. Answers whether there was one.
    pub fn backspace(&mut self) -> bool {
        let had = self.filter.pop().is_some();
        if had {
            self.refilter();
        }
        had
    }

    /// Empty the filter. Answers whether there was anything in it.
    pub fn clear_filter(&mut self) -> bool {
        let had = !self.filter.is_empty();
        if had {
            self.filter.clear();
            self.refilter();
        }
        had
    }

    /// Move the selection by `rows` (negative is up), stopping at either
    /// end, and bring it into a view `visible` rows tall.
    pub fn move_by(&mut self, rows: isize, visible: usize) {
        let last = self.shown.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(rows).min(last);
        self.reveal(visible);
    }

    /// Select the first row, or the last.
    pub fn move_to_end(&mut self, last: bool, visible: usize) {
        self.selected = if last {
            self.shown.len().saturating_sub(1)
        } else {
            0
        };
        self.reveal(visible);
    }

    /// Select shown row `row`, if there is one.
    pub fn select(&mut self, row: usize) {
        if row < self.shown.len() {
            self.selected = row;
        }
    }

    /// Scroll so the selected row is among the `visible` in view.
    pub fn reveal(&mut self, visible: usize) {
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if visible > 0 && self.selected >= self.scroll.saturating_add(visible) {
            self.scroll = self.selected.saturating_add(1).saturating_sub(visible);
        }
        self.clamp_scroll(visible);
    }

    /// Scroll by `rows` (negative is up) without moving the selection.
    pub fn scroll_by(&mut self, rows: isize, visible: usize) {
        self.scroll = self.scroll.saturating_add_signed(rows);
        self.clamp_scroll(visible);
    }

    /// Keep the view from running past the end of the list.
    pub fn clamp_scroll(&mut self, visible: usize) {
        self.scroll = self.scroll.min(self.shown.len().saturating_sub(visible));
    }

    /// The user turned shown row `row`'s switch: what to write. The set is
    /// not changed here; [`Model::written`] changes it once the file has
    /// been, so the switch never shows what is not on disk.
    #[must_use]
    pub fn toggle(&self, row: usize) -> Option<Change> {
        let app = self.row(row)?;
        Some(Change {
            id: app.id.clone(),
            name: app.name.clone(),
            on: !self.is_game(app),
        })
    }

    /// The user turned the selected row's switch.
    #[must_use]
    pub fn toggle_selected(&self) -> Option<Change> {
        self.toggle(self.selected)
    }

    /// `change` was written, and the file now holds `games`.
    pub fn written(&mut self, change: &Change, games: GameMode) {
        self.games = games;
        self.status = Status::Saved {
            name: change.name.clone(),
            on: change.on,
        };
    }

    /// `change` could not be written, for the reason `why`.
    pub fn refused(&mut self, why: String) {
        self.status = Status::Failed(why);
    }
}

#[cfg(test)]
mod tests {
    use compositor_desktop::game_mode::GameMode;

    use super::{Model, Page, Status};
    use crate::apps::App;

    fn model() -> Model {
        let apps = ["Chrome", "Steam", "Terminal", "Test Pattern", "Top"]
            .into_iter()
            .map(|name| {
                let id = format!("{}.desktop", name.to_lowercase().replace(' ', "-"));
                App::new(&id, name, None, None)
            })
            .collect();
        let (games, _) = GameMode::parse("steam.desktop\nnot-installed.desktop\n");
        Model::new(apps, games)
    }

    fn names(model: &Model) -> Vec<&str> {
        model.shown().map(|app| app.name.as_str()).collect()
    }

    #[test]
    fn the_pages_have_names() {
        assert_eq!(Page::named("game-mode"), Some(Page::GameMode));
        assert_eq!(Page::named("display"), None);
        assert_eq!(Page::GameMode.title(), "Game mode");
        assert_eq!(Page::ALL.len(), 1);
    }

    #[test]
    fn it_opens_on_every_application_with_the_files_switches() {
        let model = model();
        assert_eq!(model.shown_len(), 5);
        assert_eq!(model.selected, 0);
        let on: Vec<&str> = model
            .shown()
            .filter(|app| model.is_game(app))
            .map(|app| app.name.as_str())
            .collect();
        assert_eq!(on, vec!["Steam"]);
        // The file's line for something not installed is not counted.
        assert_eq!(model.games_installed(), 1);
        assert_eq!(model.games.len(), 2);
    }

    #[test]
    fn typing_narrows_the_list_and_keeps_the_selection_where_it_can() {
        let mut model = model();
        model.move_by(3, 10);
        assert_eq!(
            model.row(model.selected).map(|a| a.name.as_str()),
            Some("Test Pattern")
        );
        model.type_text("t");
        assert_eq!(
            names(&model),
            vec!["Steam", "Terminal", "Test Pattern", "Top"]
        );
        assert_eq!(
            model.row(model.selected).map(|a| a.name.as_str()),
            Some("Test Pattern")
        );
        model.type_text("e");
        assert_eq!(names(&model), vec!["Steam", "Terminal", "Test Pattern"]);
        model.type_text("rm");
        // The selected application is gone from the list: the first is.
        assert_eq!(names(&model), vec!["Terminal"]);
        assert_eq!(model.selected, 0);
        assert!(model.backspace());
        assert_eq!(names(&model), vec!["Terminal", "Test Pattern"]);
        assert!(model.clear_filter());
        assert!(!model.clear_filter());
        assert!(!model.backspace());
        assert_eq!(model.shown_len(), 5);
        model.type_text("nothing has this");
        assert_eq!(model.shown_len(), 0);
        assert_eq!(model.toggle_selected(), None);
    }

    #[test]
    fn the_selection_stops_at_the_ends_and_stays_in_view() {
        let mut model = model();
        model.move_by(-1, 2);
        assert_eq!((model.selected, model.scroll), (0, 0));
        model.move_by(1, 2);
        assert_eq!((model.selected, model.scroll), (1, 0));
        model.move_by(1, 2);
        assert_eq!((model.selected, model.scroll), (2, 1));
        model.move_by(100, 2);
        assert_eq!((model.selected, model.scroll), (4, 3));
        model.move_to_end(false, 2);
        assert_eq!((model.selected, model.scroll), (0, 0));
        model.move_to_end(true, 2);
        assert_eq!((model.selected, model.scroll), (4, 3));
        // The wheel moves the view, never past either end, and not the row.
        model.scroll_by(-100, 2);
        assert_eq!((model.selected, model.scroll), (4, 0));
        model.scroll_by(100, 2);
        assert_eq!(model.scroll, 3);
        // A view taller than the list does not scroll.
        model.scroll_by(1, 10);
        assert_eq!(model.scroll, 0);
        model.select(2);
        assert_eq!(model.selected, 2);
        model.select(99);
        assert_eq!(model.selected, 2);
    }

    #[test]
    fn a_switch_shows_what_was_written_and_not_before() {
        let mut model = model();
        model.select(3);
        let change = model.toggle_selected().expect("a row is selected");
        assert_eq!(
            (change.id.as_str(), change.name.as_str(), change.on),
            ("test-pattern.desktop", "Test Pattern", true)
        );
        // Asked, not yet written: the switch has not moved.
        assert!(!model.games.contains("test-pattern.desktop"));

        let (written, _) = GameMode::parse("steam.desktop\ntest-pattern.desktop\n");
        model.written(&change, written);
        assert!(model.games.contains("test-pattern.desktop"));
        assert_eq!(
            model.status,
            Status::Saved {
                name: "Test Pattern".to_owned(),
                on: true
            }
        );
        // The same switch again asks for off.
        assert_eq!(model.toggle(3).map(|change| change.on), Some(false));
        // One already on asks for off, and a refusal leaves it on.
        let off = model.toggle(1).expect("Steam's row");
        assert_eq!((off.id.as_str(), off.on), ("steam.desktop", false));
        model.refused("the disk is full".to_owned());
        assert!(model.games.contains("steam.desktop"));
        assert_eq!(model.status, Status::Failed("the disk is full".to_owned()));
    }
}
