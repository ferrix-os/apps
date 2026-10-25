//! What the dock shows, apart from how: the workspaces and the windows on
//! them, the launchers and whether each one's program is running, the
//! tools that are on, the pomodoro, and the time.

use crate::json::Json;

/// The workspaces the dock always shows, as the design names them.
pub const NAMED: [(i64, &str); 4] = [(1, "code"), (2, "docs"), (3, "misc"), (4, "free")];

/// One workspace's button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Space {
    /// Its number.
    pub id: i64,
    /// Its name, said when the pointer is over it.
    pub name: String,
    /// How many windows are on it.
    pub windows: i64,
    /// Whether it is the one shown.
    pub on: bool,
}

/// A program the dock starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launcher {
    /// Phosphor's name for its icon.
    pub icon: &'static str,
    /// What the pointer is told.
    pub title: String,
    /// The window classes that are this program.
    pub classes: Vec<String>,
    /// What starts it, as `exec` takes it.
    pub exec: String,
    /// Whether a window of it is on the workspace shown.
    pub running: bool,
}

/// What a tool does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// The palette: the launcher, fuzzel until Yoru's own.
    Search,
    /// The scratchpad terminal, dropped over everything.
    Scratchpad,
    /// Settings.
    Settings,
    /// The wallpaper switcher.
    Walls,
}

impl Tool {
    /// Every tool, in the dock's order, with its icon and words.
    pub const ALL: [(Tool, &'static str, &'static str); 4] = [
        (
            Tool::Search,
            "magnifying-glass",
            "Search or run · Super Space",
        ),
        (Tool::Scratchpad, "terminal", "Scratchpad · Super S"),
        (Tool::Walls, "image", "Wallpapers · Super I"),
        (Tool::Settings, "sliders-horizontal", "Settings"),
    ];
}

/// The pomodoro: 25 minutes of focus, then 5 of break.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pomodoro {
    /// The focus's length, in minutes.
    pub minutes: u32,
    /// Seconds left.
    pub left: u32,
    /// Whether it is counting.
    pub running: bool,
    /// Whether it is the break.
    pub resting: bool,
}

impl Pomodoro {
    /// The break's length, in seconds.
    pub const BREAK: u32 = 5 * 60;

    /// A timer of `minutes`, stopped at its start.
    #[must_use]
    pub fn new(minutes: u32) -> Self {
        Pomodoro {
            minutes,
            left: minutes * 60,
            running: false,
            resting: false,
        }
    }

    /// Start it, or pause it.
    pub fn toggle(&mut self) {
        self.running = !self.running;
    }

    /// A second gone: the focus ends in a running break, the break in a
    /// stopped focus. Whether a period ended.
    pub fn tick(&mut self) -> bool {
        if !self.running {
            return false;
        }
        self.left = self.left.saturating_sub(1);
        if self.left > 0 {
            return false;
        }
        if self.resting {
            *self = Pomodoro::new(self.minutes);
        } else {
            self.resting = true;
            self.left = Self::BREAK;
        }
        true
    }

    /// `mm:ss`.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{:02}:{:02}", self.left / 60, self.left % 60)
    }

    /// `focus` or `break`.
    #[must_use]
    pub fn mode(&self) -> &'static str {
        if self.resting { "break" } else { "focus" }
    }

    /// How much of the period has gone, 0 to 1.
    #[must_use]
    pub fn done(&self) -> f32 {
        let total = if self.resting {
            Self::BREAK
        } else {
            self.minutes * 60
        };
        #[expect(clippy::cast_precision_loss, reason = "seconds of an hour")]
        let share = 1.0 - self.left as f32 / total.max(1) as f32;
        share.clamp(0.0, 1.0)
    }
}

/// The time as the dock says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clock {
    /// `14:05`.
    pub time: String,
    /// `Sat 24 Oct`.
    pub date: String,
}

impl Clock {
    /// The clock at `seconds` since 1970, already moved into the local
    /// zone.
    #[must_use]
    pub fn at(seconds: i64) -> Self {
        const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let days = seconds.div_euclid(86_400);
        let of_day = seconds.rem_euclid(86_400);
        let (_, month, day) = civil(days);
        let weekday = usize::try_from(days.rem_euclid(7)).unwrap_or(0);
        Clock {
            time: format!("{:02}:{:02}", of_day / 3600, of_day / 60 % 60),
            date: format!(
                "{} {day} {}",
                DAYS.get(weekday).copied().unwrap_or(""),
                MONTHS
                    .get(usize::try_from(month - 1).unwrap_or(0))
                    .copied()
                    .unwrap_or("")
            ),
        }
    }
}

/// The year, month and day of `days` since 1970-01-01 (Howard Hinnant's
/// `civil_from_days`).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The local zone's offset from UTC in seconds: `TZ` written `UTC+2` or
/// `UTC-5:30`, else none (Ferrix keeps no zone database).
#[must_use]
pub fn offset(tz: Option<&str>) -> i64 {
    let Some(rest) = tz.and_then(|tz| tz.strip_prefix("UTC").or_else(|| tz.strip_prefix("GMT")))
    else {
        return 0;
    };
    let (sign, rest) = match rest.chars().next() {
        Some('+') => (1, &rest[1..]),
        Some('-') => (-1, &rest[1..]),
        _ => return 0,
    };
    let (hours, minutes) = rest.split_once(':').unwrap_or((rest, "0"));
    let hours: i64 = hours.parse().unwrap_or(0);
    let minutes: i64 = minutes.parse().unwrap_or(0);
    sign * (hours * 3600 + minutes * 60)
}

/// Everything the dock shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    /// The workspaces: the four named ones, and any other that exists.
    pub spaces: Vec<Space>,
    /// The launchers.
    pub launchers: Vec<Launcher>,
    /// Whether focus mode is on: every window but the focused one dimmed.
    pub focus: bool,
    /// The pomodoro.
    pub pomodoro: Pomodoro,
    /// The time.
    pub clock: Clock,
}

impl State {
    /// The dock with `launchers` and nothing yet known of the compositor.
    #[must_use]
    pub fn new(launchers: Vec<Launcher>, now: i64) -> Self {
        let mut state = State {
            spaces: Vec::new(),
            launchers,
            focus: false,
            pomodoro: Pomodoro::new(25),
            clock: Clock::at(now),
        };
        state.workspaces(&Json::Array(Vec::new()), 1);
        state
    }

    /// The workspaces from `j/workspaces`, `active` the one shown.
    pub fn workspaces(&mut self, list: &Json, active: i64) {
        let mut spaces: Vec<Space> = NAMED
            .iter()
            .map(|&(id, name)| Space {
                id,
                name: name.to_owned(),
                windows: 0,
                on: id == active,
            })
            .collect();
        for item in list.items() {
            let Some(id) = item.get("id").and_then(Json::int) else {
                continue;
            };
            // A special workspace (the scratchpad's) is not a button.
            if id <= 0 {
                continue;
            }
            let windows = item.get("windows").and_then(Json::int).unwrap_or(0);
            match spaces.iter_mut().find(|space| space.id == id) {
                Some(space) => space.windows = windows,
                None => spaces.push(Space {
                    id,
                    name: item
                        .get("name")
                        .and_then(Json::str)
                        .unwrap_or_default()
                        .to_owned(),
                    windows,
                    on: id == active,
                }),
            }
        }
        spaces.sort_by_key(|space| space.id);
        self.spaces = spaces;
    }

    /// Which launchers' programs have a window on the workspace shown,
    /// from `j/clients`.
    pub fn clients(&mut self, list: &Json) {
        let shown = self
            .spaces
            .iter()
            .find(|space| space.on)
            .map(|space| space.id);
        for launcher in &mut self.launchers {
            launcher.running = list.items().iter().any(|client| {
                let on = client
                    .get("workspace")
                    .and_then(|workspace| workspace.get("id"))
                    .and_then(Json::int);
                let class = client.get("class").and_then(Json::str).unwrap_or_default();
                on == shown
                    && launcher
                        .classes
                        .iter()
                        .any(|wanted| class.eq_ignore_ascii_case(wanted))
            });
        }
    }
}

/// The dock's launchers: the terminal always, and the browser, Files
/// (shelf) and Settings where the image has them, found as a launcher finds them.
#[must_use]
pub fn launchers(installed: &[(String, String, Option<String>)]) -> Vec<Launcher> {
    let mut out = vec![Launcher {
        icon: "terminal-window",
        title: "Terminal · Super ↵".to_owned(),
        classes: vec!["rocks.magical.term".to_owned(), "foot".to_owned()],
        exec: "/bin/term /bin/zinc".to_owned(),
        running: false,
    }];
    let find = |words: &[&str]| {
        installed.iter().find(|(id, _, _)| {
            let id = id.to_lowercase();
            words.iter().any(|word| id.contains(word))
        })
    };
    if let Some((_, exec, class)) = find(&["chrome", "chromium", "firefox"]) {
        out.push(Launcher {
            icon: "globe-simple",
            title: "Browser".to_owned(),
            classes: class
                .iter()
                .cloned()
                .chain(["google-chrome", "chromium", "firefox"].map(str::to_owned))
                .collect(),
            exec: exec.clone(),
            running: false,
        });
    }
    if let Some((_, exec, class)) = find(&["shelf"]) {
        out.push(Launcher {
            icon: "folder-simple",
            title: "Files".to_owned(),
            classes: class
                .iter()
                .cloned()
                .chain(std::iter::once("rocks.magical.shelf".to_owned()))
                .collect(),
            exec: exec.clone(),
            running: false,
        });
    }
    if let Some((_, exec, class)) = find(&["settings"]) {
        out.push(Launcher {
            icon: "gear-six",
            title: "Settings".to_owned(),
            classes: class
                .iter()
                .cloned()
                .chain(std::iter::once("rocks.magical.settings".to_owned()))
                .collect(),
            exec: exec.clone(),
            running: false,
        });
    }
    out
}

/// A desktop entry's `Exec` without its field codes (`%U`, `%f`).
#[must_use]
pub fn command(exec: &str) -> String {
    exec.split_whitespace()
        .filter(|word| !(word.len() == 2 && word.starts_with('%')))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{Clock, Pomodoro, State, command, launchers, offset};
    use crate::json::Json;

    /// The four named workspaces are always there; another that exists is
    /// added; the scratchpad's is not.
    #[test]
    fn workspaces_and_their_windows() {
        let mut state = State::new(launchers(&[]), 0);
        let list = Json::parse(
            r#"[{"id":1,"name":"1","windows":2},{"id":6,"name":"6","windows":1},{"id":-98,"name":"special:scratch","windows":1}]"#,
        )
        .expect("json");
        state.workspaces(&list, 6);
        let ids: Vec<i64> = state.spaces.iter().map(|space| space.id).collect();
        assert_eq!(ids, [1, 2, 3, 4, 6]);
        assert_eq!(state.spaces[0].windows, 2);
        assert_eq!(state.spaces[0].name, "code");
        assert!(state.spaces[4].on);
        let clients = Json::parse(
            r#"[{"class":"rocks.magical.term","workspace":{"id":6}},{"class":"foot","workspace":{"id":1}}]"#,
        )
        .expect("json");
        state.clients(&clients);
        assert!(
            state.launchers[0].running,
            "a terminal on the workspace shown"
        );
        state.workspaces(&list, 2);
        state.clients(&clients);
        assert!(!state.launchers[0].running, "none on workspace 2");
    }

    /// Twenty-five minutes of focus, then five of break, then a stopped
    /// focus again.
    #[test]
    fn a_pomodoro() {
        let mut timer = Pomodoro::new(1);
        assert!(!timer.tick(), "stopped");
        timer.toggle();
        assert_eq!(timer.label(), "01:00");
        for _ in 0..59 {
            assert!(!timer.tick());
        }
        assert!(timer.done() > 0.98);
        assert!(timer.tick(), "the focus ends");
        assert_eq!((timer.mode(), timer.label().as_str()), ("break", "05:00"));
        for _ in 0..299 {
            timer.tick();
        }
        assert!(timer.tick(), "the break ends");
        assert_eq!(timer, Pomodoro::new(1));
    }

    /// The design's clock and date.
    #[test]
    fn the_clock() {
        // 2026-10-24 14:05:00 UTC, a Saturday.
        let clock = Clock::at(1_792_850_700);
        assert_eq!(clock.time, "14:05");
        assert_eq!(clock.date, "Sat 24 Oct");
        assert_eq!(Clock::at(0).date, "Thu 1 Jan");
        assert_eq!(offset(Some("UTC+2")), 7200);
        assert_eq!(offset(Some("UTC-5:30")), -19800);
        assert_eq!(offset(Some("Europe/Berlin")), 0);
    }

    /// The browser and Settings when installed, field codes dropped.
    #[test]
    fn launchers_found() {
        let found = launchers(&[
            (
                "google-chrome.desktop".to_owned(),
                command("/bin/chrome --new-window %U"),
                None,
            ),
            ("settings.desktop".to_owned(), command("settings"), None),
            ("shelf.desktop".to_owned(), command("shelf %U"), None),
        ]);
        let titles: Vec<&str> = found.iter().map(|l| l.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Terminal · Super ↵", "Browser", "Files", "Settings"]
        );
        assert_eq!(found[1].exec, "/bin/chrome --new-window");
    }
}
