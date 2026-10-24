//! `wlr/taskbar`: a button for every window (`modules/wlr/taskbar.cpp`),
//! over `zwlr_foreign_toplevel_management_v1` ([`crate::toplevels`]).
//!
//! As waybar draws it: `box#taskbar` holding a `button` a window, each
//! with the classes `active`, `minimized`, `maximized` and `fullscreen` as
//! the window is, and its application id; `empty` on the box while there
//! is no window. A button's label is `format` with `{title}`, `{app_id}`,
//! `{name}`, `{short_description}` and `{state}`; its tooltip
//! `tooltip-format`, `{title}` by default. `on-click`, `on-click-middle`
//! and `on-click-right` name what a button does: `activate`,
//! `minimize`, `minimize-raise` (a minimized window comes back, the active
//! one is minimized, any other is activated), `maximize`, `fullscreen` or
//! `close`. `ignore-list`, `sort-by-app-id` and `active-first` are carried
//! out.
//!
//! What differs from waybar's, and why:
//!
//! * `format` is `{title}` when the config names none, and `{icon}` is
//!   empty. waybar's default is `{icon}`, found through the icon theme by
//!   the window's desktop file; this waybar reads no icon theme, so that
//!   default would be a row of empty buttons.
//! * `on-click` is `minimize-raise` when the config names none, which is
//!   what a click on a taskbar does on every desktop a person has used:
//!   waybar's own default does nothing.
//! * `all-outputs` changes nothing: hyprix sends no `output_enter`, so
//!   every bar lists every window.
//! * `app_ids-mapping`, `rewrite`, `icon-size` and `icon-theme` have
//!   nothing here to act on.

use super::{Button, Common, Host, Module, Press};
use crate::fmt::{Args, format};
use crate::json::Value;
use crate::toplevels::{Request, Toplevel};
use crate::view::{ButtonView, ModuleView, Shape};

/// The `wlr/taskbar` module.
#[derive(Debug)]
pub struct Taskbar {
    common: Common,
    view: ModuleView,
    /// The windows the buttons are for, in the buttons' order.
    shown: Vec<Toplevel>,
}

/// What a click on a button names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Activate,
    Minimize,
    MinimizeRaise,
    Maximize,
    Fullscreen,
    Close,
}

impl Action {
    fn named(name: &str) -> Option<Self> {
        Some(match name {
            "activate" => Self::Activate,
            "minimize" => Self::Minimize,
            "minimize-raise" => Self::MinimizeRaise,
            "maximize" => Self::Maximize,
            "fullscreen" => Self::Fullscreen,
            "close" => Self::Close,
            _ => return None,
        })
    }

    /// The request this action makes of `window`, as waybar's
    /// `Task::handle_clicked` decides it.
    const fn request(self, window: &Toplevel) -> Request {
        match self {
            Self::Activate => Request::Activate,
            Self::Minimize => Request::Minimize(!window.minimized),
            Self::MinimizeRaise => {
                if window.minimized {
                    Request::Minimize(false)
                } else if window.active {
                    Request::Minimize(true)
                } else {
                    Request::Activate
                }
            }
            Self::Maximize => Request::Maximize(!window.maximized),
            Self::Fullscreen => Request::Fullscreen(!window.fullscreen),
            Self::Close => Request::Close,
        }
    }
}

impl Taskbar {
    /// Make it, empty until the compositor says what windows there are.
    pub fn new(name: &str, config: &Value, host: &mut dyn Host) -> Self {
        let common = Common::new(name, config, "{title}", 0);
        let mut view = common.view("taskbar", Shape::Box);
        view.classes.push("empty".to_owned());
        if common.format.contains("{icon}") {
            host.diag().once(
                "taskbar-icon",
                "wlr/taskbar: {icon} is drawn empty: this waybar reads no icon theme".to_owned(),
            );
        }
        if !config.get("all-outputs").as_bool() {
            host.diag().once(
                "taskbar-outputs",
                "wlr/taskbar: every window is listed on every bar: the compositor does not say which output a window is on".to_owned(),
            );
        }
        Self {
            common,
            view,
            shown: Vec::new(),
        }
    }

    /// Whether `window` is one `ignore-list` names, by application id or
    /// title.
    fn ignored(&self, window: &Toplevel) -> bool {
        self.common
            .config
            .get("ignore-list")
            .items()
            .iter()
            .filter_map(Value::as_str)
            .any(|name| name == window.app_id || name == window.title)
    }

    /// The `{state}` of a window: its states' names.
    fn state(window: &Toplevel) -> String {
        [
            (window.active, "active"),
            (window.minimized, "minimized"),
            (window.maximized, "maximized"),
            (window.fullscreen, "fullscreen"),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, name)| *name)
        .collect::<Vec<_>>()
        .join(" ")
    }

    fn args(&self, window: &Toplevel) -> Args {
        let markup = self.common.config.get("markup").as_bool();
        let text = |raw: &str| {
            if markup {
                raw.to_owned()
            } else {
                super::escape(raw)
            }
        };
        Args::new()
            .named("title", text(&window.title))
            .named("app_id", text(&window.app_id))
            .named("name", text(&window.app_id))
            .named("short_description", text(&window.app_id))
            .named("state", Self::state(window))
            .named("icon", "")
    }

    /// Take the list again and rebuild the buttons. Gives whether the view
    /// changed.
    fn show(&mut self, host: &mut dyn Host, windows: &[Toplevel]) -> bool {
        let mut shown: Vec<Toplevel> = windows
            .iter()
            .filter(|window| !self.ignored(window))
            .cloned()
            .collect();
        if self.common.config.get("sort-by-app-id").as_bool() {
            shown.sort_by(|a, b| a.app_id.cmp(&b.app_id));
        }
        if self.common.config.get("active-first").as_bool() {
            shown.sort_by_key(|window| !window.active);
        }
        let tooltip_format = self.common.tooltip_format("{title}", "");
        let mut buttons = Vec::with_capacity(shown.len());
        for window in &shown {
            let args = self.args(window);
            let markup = match format(&self.common.format, &args) {
                Ok(text) => text,
                Err(error) => {
                    host.diag().error(format!("wlr/taskbar: {error}"));
                    String::new()
                }
            };
            let mut classes = Vec::new();
            for (on, class) in [
                (window.active, "active"),
                (window.minimized, "minimized"),
                (window.maximized, "maximized"),
                (window.fullscreen, "fullscreen"),
            ] {
                if on {
                    classes.push(class.to_owned());
                }
            }
            if !window.app_id.is_empty() {
                classes.push(window.app_id.clone());
            }
            buttons.push(ButtonView {
                markup,
                classes,
                tooltip: if self.common.tooltip {
                    format(&tooltip_format, &args).ok()
                } else {
                    None
                },
            });
        }
        let empty = buttons.is_empty();
        let mut view = self.view.clone();
        view.buttons = buttons;
        view.classes.retain(|class| class != "empty");
        if empty {
            view.classes.push("empty".to_owned());
        }
        self.shown = shown;
        let changed = view != self.view;
        self.view = view;
        changed
    }

    /// The action `button` names, from the config or the default.
    fn action(&self, button: Button) -> Option<Action> {
        let key = match button {
            1 => "on-click",
            2 => "on-click-middle",
            3 => "on-click-right",
            _ => return None,
        };
        match self.common.config.get(key).as_str() {
            Some(name) => Action::named(name),
            None if button == 1 => Some(Action::MinimizeRaise),
            None => None,
        }
    }
}

impl Module for Taskbar {
    fn start(&mut self, _host: &mut dyn Host) {}

    fn view(&self) -> &ModuleView {
        &self.view
    }

    fn common(&mut self) -> &mut Common {
        &mut self.common
    }

    fn toplevels(&mut self, host: &mut dyn Host, windows: &[Toplevel]) -> bool {
        self.show(host, windows)
    }

    /// A click between buttons does nothing; `click_part` is where a
    /// button's click goes.
    fn click(
        &mut self,
        _host: &mut dyn Host,
        _button: Button,
        _press: Press,
        _at: (f32, f32),
    ) -> bool {
        false
    }

    fn click_part(
        &mut self,
        host: &mut dyn Host,
        button: Button,
        press: Press,
        part: usize,
    ) -> bool {
        // On the press, as a GtkButton's `button-press-event` is where
        // waybar's taskbar acts.
        if press != Press::Single {
            return false;
        }
        let (Some(window), Some(action)) = (self.shown.get(part), self.action(button)) else {
            return false;
        };
        host.toplevel(window.id, action.request(window));
        false
    }

    fn clickable(&mut self) -> bool {
        !self.shown.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::Taskbar;
    use crate::json;
    use crate::modules::fake::Fake;
    use crate::modules::{Module, Press};
    use crate::toplevels::{Request, Toplevel};

    fn window(id: u32, title: &str, active: bool, minimized: bool) -> Toplevel {
        Toplevel {
            id,
            app_id: "foot".to_owned(),
            title: title.to_owned(),
            active,
            minimized,
            ..Toplevel::default()
        }
    }

    fn taskbar(config: &str, host: &mut Fake) -> Taskbar {
        Taskbar::new("wlr/taskbar", &json::parse(config).unwrap(), host)
    }

    /// A button a window, titled, with its states as classes; `empty` only
    /// while there is none.
    #[test]
    fn a_button_for_every_window_with_its_states() {
        let mut host = Fake::default();
        let mut bar = taskbar("{}", &mut host);
        assert!(bar.view().classes.contains(&"empty".to_owned()));
        let windows = [
            window(4, "one <b>", false, true),
            window(5, "two", true, false),
        ];
        assert!(bar.toplevels(&mut host, &windows));
        let view = bar.view();
        assert!(!view.classes.contains(&"empty".to_owned()));
        assert_eq!(view.buttons.len(), 2);
        assert_eq!(view.buttons[0].markup, "one &lt;b&gt;");
        assert_eq!(view.buttons[0].classes, ["minimized", "foot"]);
        assert_eq!(view.buttons[1].classes, ["active", "foot"]);
        assert_eq!(view.buttons[1].tooltip.as_deref(), Some("two"));
        // The same list again changes nothing.
        assert!(!bar.toplevels(&mut host, &windows));
    }

    /// `minimize-raise`, the default: a minimized window comes back, the
    /// active one is minimized, another is activated.
    #[test]
    fn a_click_restores_minimizes_or_activates() {
        let mut host = Fake::default();
        let mut bar = taskbar("{}", &mut host);
        let windows = [
            window(4, "one", false, true),
            window(5, "two", true, false),
            window(6, "three", false, false),
        ];
        let _ = bar.toplevels(&mut host, &windows);
        for part in 0..3 {
            let _ = bar.click_part(&mut host, 1, Press::Single, part);
            // The release asks nothing.
            let _ = bar.click_part(&mut host, 1, Press::Release, part);
        }
        assert_eq!(
            host.toplevels,
            [
                (4, Request::Minimize(false)),
                (5, Request::Minimize(true)),
                (6, Request::Activate),
            ]
        );
    }

    /// The other buttons do what the config names, and nothing unnamed.
    #[test]
    fn the_middle_and_right_buttons_do_what_they_are_told() {
        let mut host = Fake::default();
        let mut bar = taskbar(r#"{"on-click-middle": "close"}"#, &mut host);
        let _ = bar.toplevels(&mut host, &[window(4, "one", true, false)]);
        let _ = bar.click_part(&mut host, 2, Press::Single, 0);
        let _ = bar.click_part(&mut host, 3, Press::Single, 0);
        assert_eq!(host.toplevels, [(4, Request::Close)]);
    }

    /// `ignore-list`, `active-first` and `format`.
    #[test]
    fn ignored_windows_go_and_the_active_one_comes_first() {
        let mut host = Fake::default();
        let mut bar = taskbar(
            r#"{"ignore-list": ["three"], "active-first": true, "format": "{app_id}: {title}"}"#,
            &mut host,
        );
        let _ = bar.toplevels(
            &mut host,
            &[
                window(4, "one", false, false),
                window(5, "two", true, false),
                window(6, "three", false, false),
            ],
        );
        let labels: Vec<&str> = bar
            .view()
            .buttons
            .iter()
            .map(|button| button.markup.as_str())
            .collect();
        assert_eq!(labels, ["foot: two", "foot: one"]);
    }
}
