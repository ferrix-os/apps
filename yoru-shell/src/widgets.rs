//! The widget sidebars' state, as the design keeps it: which widgets are
//! on which side and in what order, which one each side shows, whether a
//! side is pinned; the month a calendar shows; the to-do list. The
//! arrangement is the person's file `~/.config/yoru-shell/sidebars`, read
//! by both sides' programs, so a widget moved to the other side appears
//! there; the to-do list is `~/.local/share/yoru-shell/todo`, a line a
//! task, `[x] ` before a done one.

use std::path::{Path, PathBuf};

/// A widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The time, the date and the month.
    Clock,
    /// The weather, opt-in (`crate::weather`).
    Weather,
    /// Quick settings.
    Settings,
    /// The to-do list.
    Todo,
}

impl Kind {
    /// Every widget, in the order the design adds them.
    pub const ALL: [Kind; 4] = [Kind::Clock, Kind::Weather, Kind::Settings, Kind::Todo];

    /// Its name in the file.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Kind::Clock => "clock",
            Kind::Weather => "weather",
            Kind::Settings => "settings",
            Kind::Todo => "todo",
        }
    }

    /// Its name as shown.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Kind::Clock => "Clock",
            Kind::Weather => "Weather",
            Kind::Settings => "Quick settings",
            Kind::Todo => "To-do",
        }
    }

    /// Phosphor's icon for it.
    #[must_use]
    pub fn icon(self) -> &'static str {
        match self {
            Kind::Clock => "clock",
            Kind::Weather => "cloud-moon",
            Kind::Settings => "sliders-horizontal",
            Kind::Todo => "check-square",
        }
    }

    /// The widget named `id`.
    #[must_use]
    pub fn of(id: &str) -> Option<Self> {
        Kind::ALL.into_iter().find(|kind| kind.id() == id)
    }
}

/// A screen edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The left.
    Left,
    /// The right.
    Right,
}

impl Side {
    /// Its name in the file and on the command line.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
        }
    }

    /// The side named `id`.
    #[must_use]
    pub fn of(id: &str) -> Option<Self> {
        match id {
            "left" => Some(Side::Left),
            "right" => Some(Side::Right),
            _ => None,
        }
    }

    /// The other one.
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

/// Where every widget is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arrangement {
    /// The widgets on the left, in order.
    pub left: Vec<Kind>,
    /// The widgets on the right, in order.
    pub right: Vec<Kind>,
    /// The one each side shows.
    pub shown: (Option<Kind>, Option<Kind>),
    /// Which sides are pinned: always open, their room kept from windows.
    pub pinned: (bool, bool),
}

impl Default for Arrangement {
    /// The design's: clock and weather on the left, quick settings and
    /// the to-do list on the right.
    fn default() -> Self {
        Arrangement {
            left: vec![Kind::Clock, Kind::Weather],
            right: vec![Kind::Settings, Kind::Todo],
            shown: (Some(Kind::Clock), Some(Kind::Todo)),
            pinned: (false, false),
        }
    }
}

impl Arrangement {
    /// The arrangement in `text`, the file's lines `left clock weather`,
    /// `right settings todo`, `shown left clock`, `pinned right`; the
    /// design's where there is no file.
    #[must_use]
    pub fn read(text: &str) -> Self {
        let mut made = Arrangement {
            left: Vec::new(),
            right: Vec::new(),
            shown: (None, None),
            pinned: (false, false),
        };
        let mut any = false;
        for line in text.lines() {
            let mut words = line.split_whitespace();
            match (words.next(), words.next().and_then(Side::of)) {
                (Some("shown"), Some(side)) => {
                    let kind = words.next().and_then(Kind::of);
                    *made.shown_mut(side) = kind;
                }
                (Some("pinned"), Some(side)) => *made.pinned_mut(side) = true,
                (Some(side), _) if Side::of(side).is_some() => {
                    any = true;
                    let side = Side::of(side).unwrap_or(Side::Left);
                    let list: Vec<Kind> = line
                        .split_whitespace()
                        .skip(1)
                        .filter_map(Kind::of)
                        .filter(|kind| !made.left.contains(kind) && !made.right.contains(kind))
                        .collect();
                    made.on_mut(side).extend(list);
                }
                _ => {}
            }
        }
        if !any {
            return Arrangement {
                pinned: made.pinned,
                ..Arrangement::default()
            };
        }
        for side in [Side::Left, Side::Right] {
            let shown = made.shown(side);
            if shown.is_none_or(|kind| !made.on(side).contains(&kind)) {
                *made.shown_mut(side) = made.on(side).first().copied();
            }
        }
        made
    }

    /// The file's text.
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::new();
        for side in [Side::Left, Side::Right] {
            out.push_str(side.id());
            for kind in self.on(side) {
                out.push(' ');
                out.push_str(kind.id());
            }
            out.push('\n');
            if let Some(kind) = self.shown(side) {
                out.push_str(&format!("shown {} {}\n", side.id(), kind.id()));
            }
            if self.is_pinned(side) {
                out.push_str(&format!("pinned {}\n", side.id()));
            }
        }
        out
    }

    /// The widgets on `side`.
    #[must_use]
    pub fn on(&self, side: Side) -> &[Kind] {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }

    fn on_mut(&mut self, side: Side) -> &mut Vec<Kind> {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }

    /// The widget `side` shows.
    #[must_use]
    pub fn shown(&self, side: Side) -> Option<Kind> {
        match side {
            Side::Left => self.shown.0,
            Side::Right => self.shown.1,
        }
    }

    fn shown_mut(&mut self, side: Side) -> &mut Option<Kind> {
        match side {
            Side::Left => &mut self.shown.0,
            Side::Right => &mut self.shown.1,
        }
    }

    /// Whether `side` is pinned.
    #[must_use]
    pub fn is_pinned(&self, side: Side) -> bool {
        match side {
            Side::Left => self.pinned.0,
            Side::Right => self.pinned.1,
        }
    }

    fn pinned_mut(&mut self, side: Side) -> &mut bool {
        match side {
            Side::Left => &mut self.pinned.0,
            Side::Right => &mut self.pinned.1,
        }
    }

    /// Show `kind` on `side`, where it is.
    pub fn show(&mut self, side: Side, kind: Kind) {
        if self.on(side).contains(&kind) {
            *self.shown_mut(side) = Some(kind);
        }
    }

    /// Pin `side`, or unpin it.
    pub fn toggle_pin(&mut self, side: Side) {
        let pinned = self.pinned_mut(side);
        *pinned = !*pinned;
    }

    /// Move the widget `side` shows to the other side, shown there.
    pub fn move_shown(&mut self, side: Side) {
        let Some(kind) = self.shown(side) else {
            return;
        };
        self.on_mut(side).retain(|k| *k != kind);
        *self.shown_mut(side) = self.on(side).first().copied();
        let other = side.other();
        self.on_mut(other).push(kind);
        *self.shown_mut(other) = Some(kind);
    }

    /// Take the widget `side` shows away.
    pub fn remove_shown(&mut self, side: Side) {
        let Some(kind) = self.shown(side) else {
            return;
        };
        self.on_mut(side).retain(|k| *k != kind);
        *self.shown_mut(side) = self.on(side).first().copied();
    }

    /// Put `kind` on `side`, shown.
    pub fn add(&mut self, side: Side, kind: Kind) {
        if self.left.contains(&kind) || self.right.contains(&kind) {
            return;
        }
        self.on_mut(side).push(kind);
        *self.shown_mut(side) = Some(kind);
    }

    /// The widgets on neither side, which `side` offers to add.
    #[must_use]
    pub fn hidden(&self) -> Vec<Kind> {
        Kind::ALL
            .into_iter()
            .filter(|kind| !self.left.contains(kind) && !self.right.contains(kind))
            .collect()
    }
}

/// The person's configuration folder for the shell.
fn config_dir() -> Option<PathBuf> {
    std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .filter(|h| !h.is_empty())
                .map(|h| Path::new(&h).join(".config"))
        })
        .map(|d| d.join("yoru-shell"))
}

/// The shell's file `name` in the person's configuration folder.
#[must_use]
pub fn config_file(name: &str) -> Option<PathBuf> {
    config_dir().map(|d| d.join(name))
}

/// The person's data folder for the shell.
fn data_dir() -> Option<PathBuf> {
    std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .filter(|h| !h.is_empty())
                .map(|h| Path::new(&h).join(".local/share"))
        })
        .map(|d| d.join("yoru-shell"))
}

/// The arrangement's file.
#[must_use]
pub fn arrangement_file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("sidebars"))
}

/// The weather setting's file.
#[must_use]
pub fn weather_file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("weather"))
}

/// The to-do list's file.
#[must_use]
pub fn todo_file() -> Option<PathBuf> {
    data_dir().map(|d| d.join("todo"))
}

/// Write `text` to `path`, its folder made.
pub fn keep(path: &Path, text: &str) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, text);
}

/// A task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    /// What it says.
    pub text: String,
    /// Done.
    pub done: bool,
}

/// The tasks in `text`.
#[must_use]
pub fn tasks(text: &str) -> Vec<Task> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| match line.strip_prefix("[x] ") {
            Some(rest) => Task {
                text: rest.to_owned(),
                done: true,
            },
            None => Task {
                text: line.strip_prefix("[ ] ").unwrap_or(line).to_owned(),
                done: false,
            },
        })
        .collect()
}

/// The file's text for `tasks`.
#[must_use]
pub fn tasks_text(tasks: &[Task]) -> String {
    tasks
        .iter()
        .map(|task| format!("{} {}\n", if task.done { "[x]" } else { "[ ]" }, task.text))
        .collect()
}

/// "2 of 4 done".
#[must_use]
pub fn done_line(tasks: &[Task]) -> String {
    format!(
        "{} of {} done",
        tasks.iter().filter(|t| t.done).count(),
        tasks.len()
    )
}

/// The year, month (1-12) and day of `days` since 1970-01-01.
#[must_use]
pub fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// The days since 1970-01-01 of `year`-`month`-`day` (Hinnant's
/// `days_from_civil`).
#[must_use]
pub fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The month of local time `seconds` as a calendar: its title ("October
/// 2026") and six weeks of days from Monday, each `(day, in the month,
/// today)`.
#[must_use]
pub fn month(seconds: i64) -> (String, Vec<(i64, bool, bool)>) {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let today = seconds.div_euclid(86_400);
    let (year, month, _) = civil(today);
    let first = days_from_civil(year, month, 1);
    // 1970-01-01 was a Thursday: Monday is 0 at (days + 3) % 7.
    let weekday = (first + 3).rem_euclid(7);
    let start = first - weekday;
    let cells = (0..42)
        .map(|i| {
            let day = start + i;
            let (_, m, d) = civil(day);
            (d, m == month, day == today)
        })
        .collect();
    let name = MONTHS
        .get(usize::try_from(month - 1).unwrap_or(0))
        .copied()
        .unwrap_or("");
    (format!("{name} {year}"), cells)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_design_s_arrangement_with_no_file() {
        let made = Arrangement::read("");
        assert_eq!(made.left, vec![Kind::Clock, Kind::Weather]);
        assert_eq!(made.shown(Side::Right), Some(Kind::Todo));
        assert_eq!(Arrangement::read(&made.text()), made);
    }

    #[test]
    fn widgets_move_go_and_come_back() {
        let mut made = Arrangement::default();
        made.move_shown(Side::Left);
        assert_eq!(made.left, vec![Kind::Weather]);
        assert_eq!(made.right, vec![Kind::Settings, Kind::Todo, Kind::Clock]);
        assert_eq!(made.shown(Side::Right), Some(Kind::Clock));
        made.remove_shown(Side::Right);
        assert_eq!(made.hidden(), vec![Kind::Clock]);
        made.add(Side::Left, Kind::Clock);
        assert_eq!(made.shown(Side::Left), Some(Kind::Clock));
        made.toggle_pin(Side::Left);
        let back = Arrangement::read(&made.text());
        assert_eq!(back, made);
        assert!(back.is_pinned(Side::Left));
        // A side with nothing on it: its line is there, empty.
        let empty = Arrangement::read("left\nright todo\n");
        assert!(empty.left.is_empty());
        assert_eq!(empty.shown(Side::Right), Some(Kind::Todo));
    }

    #[test]
    fn tasks_read_back() {
        let list = tasks("[x] Water the plants\n[ ] Call the bank\nplain line\n\n");
        assert_eq!(list.len(), 3);
        assert!(list[0].done && !list[1].done && !list[2].done);
        assert_eq!(done_line(&list), "1 of 3 done");
        assert_eq!(tasks(&tasks_text(&list)), list);
    }

    #[test]
    fn october_2026_starts_on_a_thursday() {
        // 2026-10-25 12:00 UTC.
        let seconds = days_from_civil(2026, 10, 25) * 86_400 + 43_200;
        let (title, cells) = month(seconds);
        assert_eq!(title, "October 2026");
        assert_eq!(cells.len(), 42);
        // Monday 28 September, then the 1st on the Thursday.
        assert_eq!(cells[0], (28, false, false));
        assert_eq!(cells[3], (1, true, false));
        assert!(cells.contains(&(25, true, true)));
        assert_eq!(civil(days_from_civil(2024, 2, 29)), (2024, 2, 29));
    }
}
