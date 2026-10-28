//! The window's tree, in tole (Ferrix's `docs/UI-FRAMEWORK.md`): the
//! markup under `ui/` is the header, the sidebar, the menus and the
//! pages; this module hands it what the [`App`] holds, after every change,
//! and turns what was pressed back into [`Action`]s. The charts are the
//! Rust painters of [`paint`]; the table's lines are a model over the log
//! ([`rows::LinesModel`]), made only for the rows in view.

mod crashes_show;
mod machines_show;
pub mod paint;
pub mod rows;
mod show;

pub use show::level_label;

use std::path::Path;
use std::rc::Rc;

use tole::{Element as _, Ui};

use crate::app::{Action, App};
pub use paint::Shared;

tole::include_ui!();

/// The header's height.
pub const HEADER: f32 = 38.0;
/// The sidebar's width.
pub const SIDEBAR: f32 = 220.0;
/// A retention class's budget when nothing says otherwise: lode's
/// `system` class, 1 GiB (`docs/LOGGING.md` §8.5).
pub const BUDGET: u64 = 1 << 30;

/// The fonts the viewer draws with: the design's own IBM Plex, from the
/// directory the package installs them in or, on a development host,
/// beside the sources, then the system's.
#[must_use]
pub fn fonts() -> tole::Fonts {
    let mut fonts = tole::Fonts::system();
    // Installed on a Linux desktop beside the program: `<prefix>/bin`
    // and `<prefix>/share/lode-viewer/fonts`.
    let beside = std::env::current_exe().ok().and_then(|exe| {
        let prefix = exe.parent()?.parent()?.to_path_buf();
        Some(
            prefix
                .join("share/lode-viewer/fonts")
                .to_string_lossy()
                .into_owned(),
        )
    });
    for dir in [
        std::env::var("LODE_VIEWER_FONTS").ok(),
        Some("/usr/share/fonts/ibm-plex".to_owned()),
        beside,
        Some(concat!(env!("CARGO_MANIFEST_DIR"), "/share/fonts").to_owned()),
    ]
    .into_iter()
    .flatten()
    {
        let path = Path::new(&dir);
        if path.is_dir()
            && !fonts
                .faces()
                .iter()
                .any(|face| face.families.iter().any(|name| name == "IBM Plex Sans"))
        {
            let _ = fonts.add_dir(path);
        }
    }
    fonts
}

/// `text` broken at spaces into lines no wider than `max` by `width`; a
/// word wider than `max` alone is broken where it must be. Explicit line
/// breaks stay.
pub fn wrap_with(text: &str, max: f32, mut width: impl FnMut(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ').filter(|word| !word.is_empty()) {
            let tried = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if width(&tried) <= max {
                line = tried;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // A word too wide alone is broken by characters.
            for c in word.chars() {
                line.push(c);
                if width(&line) > max && line.chars().count() > 1 {
                    let last = line.pop().unwrap_or(c);
                    lines.push(std::mem::take(&mut line));
                    line.push(last);
                }
            }
        }
        lines.push(line);
    }
    lines
}

/// The tree and what the program keeps of it.
pub struct View {
    /// The tree.
    pub ui: Ui,
    /// The window around the pages.
    pub shell: Shell,
    /// The Overview.
    pub overview: OverviewPage,
    /// Logs.
    pub logs: LogsPage,
    /// Machines.
    pub machines: MachinesPage,
    /// Crashes.
    pub crashes: CrashesPage,
    /// The crash screenshot last read.
    capture: crashes_show::Capture,
    /// The table's lines.
    pub lines: Rc<rows::LinesModel>,
    /// The state it shows.
    pub app: Shared,
    /// What [`View::show`] last set, so a frame is only made again for
    /// what changed.
    shown: show::Shown,
    /// The form's field last given the keys.
    shown_focus: Option<crate::app::FormField>,
    /// The saved query last being named.
    shown_saved_edit: Option<Option<usize>>,
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View").finish_non_exhaustive()
    }
}

impl View {
    /// The tree for `app`, drawn with `fonts`; `None` when the markup's
    /// items could not be made, a fault of this program.
    #[must_use]
    pub fn new(app: Shared, fonts: tole::Fonts) -> Option<Self> {
        let mut ui = Ui::new(fonts);
        let shell = Shell::new(&mut ui, None)?;
        let content = shell.content();
        let overview = OverviewPage::new(&mut ui, Some(content))?;
        let logs = LogsPage::new(&mut ui, Some(content))?;
        let machines = MachinesPage::new(&mut ui, Some(content))?;
        let crashes = CrashesPage::new(&mut ui, Some(content))?;
        for page in [
            overview.common(),
            logs.common(),
            machines.common(),
            crashes.common(),
        ] {
            let parent = shell.content();
            page.width.bind(&mut ui, move |ui| {
                ui.common(parent).map_or(0.0, |c| ui.get(c.width))
            });
            page.height.bind(&mut ui, move |ui| {
                ui.common(parent).map_or(0.0, |c| ui.get(c.height))
            });
        }
        let lines = Rc::new(rows::LinesModel::new(Rc::clone(&app)));
        logs.lines
            .set(&mut ui, tole::ModelRc::from_rc(Rc::clone(&lines) as _));
        overview.lanes.set(&mut ui, paint::lanes(&app));
        logs.lanes.set(&mut ui, paint::lanes(&app));
        overview.sparks.set(&mut ui, paint::sparks(&app));
        overview.activity.set(&mut ui, paint::activity(&app));
        overview.rate.set(&mut ui, paint::rate(&app));
        logs.minimap.set(&mut ui, paint::minimap(&app));
        let mut view = View {
            ui,
            shell,
            overview,
            logs,
            machines,
            crashes,
            capture: crashes_show::Capture::default(),
            lines,
            app,
            shown: show::Shown::default(),
            shown_focus: None,
            shown_saved_edit: None,
        };
        if let Some(dir) = crate::form::ssh_dir().filter(|dir| dir.is_dir()) {
            view.machines
                .key_folder
                .set(&mut view.ui, dir.to_string_lossy().as_ref().into());
        }
        view.show();
        Some(view)
    }

    /// The window's size in logical pixels.
    pub fn resize(&mut self, (width, height): (u32, u32)) {
        let common = self.shell.common();
        common.width.set(&mut self.ui, width as f32);
        common.height.set(&mut self.ui, height as f32);
    }

    /// Whether the window draws its own frame -- the compositor answered
    /// `client_side` or draws none -- and so its header shows minimize,
    /// maximize and close (`tole::window::Window::draws_own_frame`).
    pub fn set_own_frame(&mut self, on: bool) {
        self.shell.own_frame.set(&mut self.ui, on);
    }

    /// Take into the state what the tree changed by itself: how far the
    /// pages were scrolled. Call it before acting on the state.
    pub fn pull(&mut self) {
        let overview = self.overview.scroll.get(&mut self.ui);
        let logs = self.logs.scroll.get(&mut self.ui);
        let machines = self.machines.scroll.get(&mut self.ui);
        let mut most = [0.0_f32; 3];
        let mut table_view = 0.0;
        for (at, page) in [self.overview.id(), self.logs.id(), self.machines.id()]
            .into_iter()
            .enumerate()
        {
            if let Some(flick) = find(&self.ui, page, |kind| {
                matches!(kind, tole::Kind::Flickable(_) | tole::Kind::ListView(_))
            }) && let (Some(props), Some(common)) =
                (self.ui.flickable(flick), self.ui.common(flick))
            {
                let content = self.ui.get(props.content_height);
                let height = self.ui.get(common.height);
                if let Some(slot) = most.get_mut(at) {
                    *slot = (content - height).max(0.0);
                }
                if at == 1 {
                    table_view = height + 2.0;
                }
            }
        }
        if let Ok(mut app) = self.app.try_borrow_mut() {
            app.overview_scroll = overview;
            app.scroll = logs;
            app.machines_scroll = machines;
            app.scroll_most = most;
            app.table_view = table_view;
            self.shown.scrolls = (overview, logs, machines);
        }
    }

    /// Give the query the keys.
    pub fn focus_query(&mut self) {
        if let Some(input) = find(&self.ui, self.logs.id(), |kind| {
            matches!(kind, tole::Kind::TextInput(_))
        }) {
            self.ui.force_active_focus(input);
        }
    }

    /// Take the keys from the query.
    pub fn blur_query(&mut self) {
        if let Some(input) = find(&self.ui, self.logs.id(), |kind| {
            matches!(kind, tole::Kind::TextInput(_))
        }) {
            self.ui.set_focus(input, false);
        }
    }

    /// Whether a field has the keys.
    #[must_use]
    pub fn typing(&self) -> bool {
        self.ui
            .active_focus_item()
            .is_some_and(|item| self.ui.text_input(item).is_some())
    }

    /// What was pressed or typed since the last call, as the state's
    /// actions; the window's own (move, close) among them.
    pub fn actions(&mut self) -> Vec<Action> {
        let emitted = self.ui.take_signals();
        let mut out = Vec::new();
        for each in &emitted {
            if let Some(signal) = self.shell.signal(each) {
                out.extend(show::shell_action(&self.app.borrow(), signal));
            } else if let Some(signal) = self.overview.signal(each) {
                out.extend(show::overview_action(&self.app.borrow(), signal));
            } else if let Some(signal) = self.logs.signal(each) {
                out.extend(show::logs_action(&self.app.borrow(), signal));
            } else if let Some(signal) = self.machines.signal(each) {
                out.extend(show::machines_action(&self.app.borrow(), signal));
            } else if let Some(signal) = self.crashes.signal(each) {
                out.extend(crashes_show::action(&self.app.borrow(), signal));
            }
        }
        out
    }

    /// What the tree asked since the last call, done to the state, then
    /// the state shown again. Answers the actions done and, apart, the
    /// window's own (move, maximize, minimize, close), which the window
    /// does.
    pub fn step(&mut self) -> (Vec<Action>, Vec<Action>) {
        self.pull();
        let mut done = Vec::new();
        let mut window = Vec::new();
        for action in self.actions() {
            match action {
                Action::WindowMove
                | Action::WindowMaximize
                | Action::WindowMinimize
                | Action::WindowClose => window.push(action),
                action => {
                    let mut app = self.app.borrow_mut();
                    app.act(&action);
                    // A line pressed on the Overview opens in Logs.
                    if let Action::Select(index) = action
                        && app.page == crate::app::Page::Overview
                    {
                        app.page = crate::app::Page::Logs;
                        app.refresh();
                        app.selected = Some(index);
                        app.scroll_to(index);
                    }
                    drop(app);
                    // The query taken or left: the keys go back to the
                    // window, so `/` starts it again.
                    if matches!(action, Action::QueryAccept | Action::QueryCancel) {
                        self.blur_query();
                    }
                    // A name taken or left: the keys go back too.
                    if matches!(action, Action::SavedName(_) | Action::SavedEditCancel)
                        && self.app.borrow().saved_edit.is_none()
                        && let Some(item) = self.ui.active_focus_item()
                    {
                        self.ui.set_focus(item, false);
                    }
                    done.push(action);
                }
            }
        }
        // A long boot read a page at a time: the page the view nears.
        let _ = self.app.borrow_mut().page_in();
        self.show();
        self.ui.update();
        (done, window)
    }

    /// Hand the tree what the state holds now.
    pub fn show(&mut self) {
        let app = Rc::clone(&self.app);
        let app = app.borrow();
        show::show(self, &app);
        // A saved query's name being typed has the keys.
        if app.saved_edit != self.shown_saved_edit {
            self.shown_saved_edit = app.saved_edit;
            if app.saved_edit.is_some() {
                self.ui.update();
                self.focus_sidebar_field();
            }
        }
    }

    /// Give the keys to the sidebar's field shown, its text selected.
    fn focus_sidebar_field(&mut self) {
        let pages = self.shell.content();
        let mut stack = vec![self.shell.id()];
        while let Some(item) = stack.pop() {
            if item == pages {
                continue;
            }
            let shown = self
                .ui
                .common(item)
                .is_some_and(|common| self.ui.get(common.visible));
            if !shown {
                continue;
            }
            if self.ui.text_input(item).is_some() {
                self.ui.force_active_focus(item);
                self.ui.select_all(item);
                return;
            }
            stack.extend(self.ui.children(item).iter().rev());
        }
    }

    /// One frame of `size` logical pixels at `scale`, drawn whole.
    pub fn frame(&mut self, size: (u32, u32), scale: f32) -> Option<tole::tiny_skia::Pixmap> {
        self.ui.set_scale(scale);
        self.resize(size);
        self.show();
        let background = paint::ink(crate::theme::Theme::of(self.app.borrow().mode).bg);
        self.ui.update();
        self.ui.render(size, background)
    }
}

/// The first item under `root`, in tree order, whose kind `matches` says.
fn find(
    ui: &Ui,
    root: tole::ItemId,
    matches: impl Fn(&tole::Kind) -> bool,
) -> Option<tole::ItemId> {
    let mut stack = vec![root];
    while let Some(item) = stack.pop() {
        if ui.kind(item).is_some_and(|kind| matches(&kind)) {
            return Some(item);
        }
        stack.extend(ui.children(item).iter().rev());
    }
    None
}

/// `--render`: one frame of `app` into a PNG. Answers the exit status.
#[must_use]
pub fn render(app: App, out: &Path, size: (u32, u32), scale: u32) -> i32 {
    use crate::cli::say;
    let fonts = fonts();
    if fonts.faces().is_empty() {
        say("err: no font was found (LODE_VIEWER_FONTS, /usr/share/fonts)");
    }
    let shared = Rc::new(std::cell::RefCell::new(app));
    let Some(mut view) = View::new(shared, fonts) else {
        say("err: the window's tree could not be made");
        return 1;
    };
    let started = std::time::Instant::now();
    let Some(frame) = view.frame(size, scale as f32) else {
        say("err: a frame of that size cannot be made");
        return 1;
    };
    let drawn = started.elapsed();
    let png = tole::png::encode(frame.width(), frame.height(), frame.data());
    match std::fs::write(out, png) {
        Ok(()) => {
            say(&format!(
                "drew {}x{} at scale {scale} in {:.1} ms into {}",
                size.0,
                size.1,
                drawn.as_secs_f64() * 1e3,
                out.display()
            ));
            0
        }
        Err(error) => {
            say(&format!("err: {}: {error}", out.display()));
            1
        }
    }
}

/// `--bench`'s frames: each page drawn five times, the first a new tree;
/// answers the times in words.
#[must_use]
pub fn bench_frames(shared: &Shared, size: (u32, u32)) -> String {
    use crate::app::Page;
    let Some(mut view) = View::new(Rc::clone(shared), fonts()) else {
        return "the tree could not be made".to_owned();
    };
    let mut out = Vec::new();
    for page in [Page::Logs, Page::Overview] {
        shared.borrow_mut().page = page;
        let mut times = Vec::new();
        for n in 0..5 {
            if n > 0 {
                // A change that redraws the page: the cursor's place.
                let mut app = shared.borrow_mut();
                app.cursor = app.cursor.saturating_sub(1);
            }
            let started = std::time::Instant::now();
            let _ = view.frame(size, 1.0);
            times.push(started.elapsed().as_secs_f64() * 1e3);
        }
        let first = times.first().copied().unwrap_or(0.0);
        let rest: f64 = times.iter().skip(1).sum();
        out.push(format!(
            "{page:?} first {first:.0} ms then {:.0} ms",
            rest / 4.0
        ));
    }
    out.join(", ")
}
