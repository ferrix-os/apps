//! The Wallpaper page: what is behind the windows, on every display or
//! each its own or one picture across them, as hyprix's `wallpaper`
//! lines say it (Ferrix's `docs/RUNTIME-SETTINGS.md` §3.3, "The
//! wallpaper"), kept in `settings.conf` through `hyprctl setting` and
//! drawn at once by hyprix's wallpaper client.
//!
//! A line is the output (empty for every display without a line of its
//! own), then fields: `picture PATH`, `colour X`, `slideshow DIR`,
//! `video PATH`, `fit`, `every`, `shuffle`, `span 1`, or `off`.

use crate::pages::{Effect, Key, Kind, Line, Note, Page, State};

/// The pictures Settings ships, the design's six: their names and where
/// the package puts them.
pub const PRESETS: [&str; 6] = ["Tide", "Dune", "Fern", "Dusk", "Ember", "Graphite"];

/// Where the package installs the presets.
pub const PRESET_DIR: &str = "/usr/share/backgrounds/settings";

/// Where the presets are: installed, else (on a development host)
/// beside the sources.
#[must_use]
pub fn preset_dir() -> String {
    if std::path::Path::new(PRESET_DIR).is_dir() {
        PRESET_DIR.to_owned()
    } else {
        concat!(env!("CARGO_MANIFEST_DIR"), "/share/wallpapers").to_owned()
    }
}

/// The colours offered for a plain background, as hyprland writes them.
pub const COLOURS: [&str; 8] = [
    "1d2027", "0f1115", "2b3a55", "1f3d36", "3d2a45", "4a2c2a", "d8d4cc", "f2f0ea",
];

/// The fits, as the design words them and hyprix writes them.
const FITS: [(&str, &str); 5] = [
    ("Fill", "fill"),
    ("Fit", "fit"),
    ("Stretch", "stretch"),
    ("Tile", "tile"),
    ("Centre", "centre"),
];

/// How often a slideshow moves on.
const EVERY: [(&str, &str); 5] = [
    ("1 min", "1m"),
    ("10 min", "10m"),
    ("30 min", "30m"),
    ("1 hour", "1h"),
    ("1 day", "1d"),
];

/// What one line shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wall {
    /// `picture`, `colour`, `slideshow`, `video` or `off`.
    pub kind: String,
    /// The picture, the slideshow's folder or the video.
    pub path: String,
    /// The colour, `rrggbb` or as written.
    pub colour: String,
    /// The fit.
    pub fit: String,
    /// A slideshow's pace.
    pub every: String,
    /// A slideshow's order shuffled.
    pub shuffle: bool,
    /// One picture across every display.
    pub span: bool,
}

impl Wall {
    /// The value of a `wallpaper` line, its output first.
    #[must_use]
    pub fn read(value: &str) -> (String, Wall) {
        let mut fields = value.split(',').map(str::trim);
        let output = fields.next().unwrap_or("").to_owned();
        let mut wall = Wall {
            kind: "colour".to_owned(),
            colour: "1d2027".to_owned(),
            fit: "fill".to_owned(),
            every: "30m".to_owned(),
            shuffle: true,
            ..Wall::default()
        };
        for field in fields {
            let (word, rest) = field.split_once(' ').unwrap_or((field, ""));
            let rest = rest.trim();
            match word {
                "picture" | "slideshow" | "video" => {
                    word.clone_into(&mut wall.kind);
                    rest.clone_into(&mut wall.path);
                }
                "colour" | "color" => rest.clone_into(&mut wall.colour),
                "fit" => rest.clone_into(&mut wall.fit),
                "every" => rest.clone_into(&mut wall.every),
                "shuffle" => wall.shuffle = rest != "0",
                "span" => wall.span = rest != "0",
                "off" => "off".clone_into(&mut wall.kind),
                _ => {}
            }
        }
        (output, wall)
    }

    /// As a `wallpaper` line for `output` (empty: every display).
    #[must_use]
    pub fn line(&self, output: &str) -> String {
        let mut fields = vec![output.to_owned()];
        match self.kind.as_str() {
            "off" => fields.push("off".to_owned()),
            "picture" => {
                fields.push(format!("picture {}", self.path));
                fields.push(format!("fit {}", self.fit));
            }
            "slideshow" => {
                fields.push(format!("slideshow {}", self.path));
                fields.push(format!("every {}", self.every));
                fields.push(format!("shuffle {}", u8::from(self.shuffle)));
                fields.push(format!("fit {}", self.fit));
            }
            "video" => fields.push(format!("video {}", self.path)),
            _ => {}
        }
        if self.kind != "off" {
            fields.push(format!("colour {}", self.colour));
        }
        if self.span && output.is_empty() {
            fields.push("span 1".to_owned());
        }
        fields.join(", ")
    }
}

/// A row of choices.
fn choice(title: &str, key: Key, options: &[(&str, &str)], now: &str) -> Line {
    let mut row = Line::new(Kind::Choice, title).key(key);
    row.options = options.iter().map(|(w, _)| (*w).to_owned()).collect();
    row.raw = options.iter().map(|(_, v)| (*v).to_owned()).collect();
    row.current = row
        .raw
        .iter()
        .position(|v| v == now)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(-1);
    row
}

impl State {
    /// The wallpaper lines `settings.conf` holds: every display's (key
    /// `wallpaper`) and each display's own (`wallpaper NAME`).
    fn walls(&self) -> (Wall, Vec<(String, Wall)>) {
        let all = self.values.get("wallpaper").map_or_else(
            || Wall::read(&format!(", picture {}/tide.jpg, fit fill", preset_dir())).1,
            |v| Wall::read(v).1,
        );
        let own = self
            .values
            .iter()
            .filter_map(|(k, v)| {
                k.strip_prefix("wallpaper ")
                    .map(|name| (name.to_owned(), Wall::read(v).1))
            })
            .collect();
        (all, own)
    }

    /// Same on all, different on each, or span across.
    fn wall_mode(&self) -> &'static str {
        let (all, own) = self.walls();
        if all.span {
            "span"
        } else if !own.is_empty() {
            "each"
        } else {
            "same"
        }
    }

    /// The display the page changes, and its line's key: with one line
    /// for every display, that line.
    fn wall_target(&self) -> (String, Wall) {
        let (all, own) = self.walls();
        if self.wall_mode() != "each" {
            return ("wallpaper".to_owned(), all);
        }
        let name = self
            .monitors
            .get(self.display)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        let wall = own
            .into_iter()
            .find(|(n, _)| *n == name)
            .map_or(all, |(_, w)| w);
        (format!("wallpaper {name}"), wall)
    }

    /// The Wallpaper page's rows.
    #[must_use]
    pub fn wallpaper_rows(&self) -> Vec<Line> {
        let mut rows = Vec::new();
        let (all, own) = self.walls();
        // The displays, each showing its picture or colour.
        if !self.monitors.is_empty() {
            let mut screens = Line::new(Kind::Screens, "").key(Key::Display);
            let fit = self.arrangement(&mut screens);
            for m in &self.monitors {
                let wall = own
                    .iter()
                    .find(|(n, _)| *n == m.name)
                    .map_or(&all, |(_, w)| w);
                screens.values.push(if wall.kind == "picture" {
                    wall.path.clone()
                } else {
                    String::new()
                });
                screens
                    .colors
                    .push(wall.colour.trim_start_matches('#').to_owned());
            }
            let _ = fit;
            screens.current = if self.wall_mode() == "each" {
                i32::try_from(self.display).unwrap_or(0)
            } else {
                -1
            };
            rows.push(screens);
        }
        if self.monitors.len() > 1 {
            rows.push(Line::new(Kind::Section, "Displays"));
            rows.push(choice(
                "Wallpaper on your displays",
                Key::WallMode,
                &[
                    ("Same on all", "same"),
                    ("Different on each", "each"),
                    ("Span across", "span"),
                ],
                self.wall_mode(),
            ));
        }
        let (_, mut wall) = self.wall_target();
        if self.wall_video {
            "video".clone_into(&mut wall.kind);
        }
        rows.push(Line::new(Kind::Section, "Background"));
        rows.push(choice(
            "Type",
            Key::WallKind,
            &[
                ("Picture", "picture"),
                ("Colour", "colour"),
                ("Slideshow", "slideshow"),
                ("Video", "video"),
            ],
            &wall.kind,
        ));
        match wall.kind.as_str() {
            "picture" => {
                let mut pictures = Line::new(Kind::Pictures, "").key(Key::WallPicture);
                for name in PRESETS {
                    pictures.options.push(name.to_owned());
                    pictures
                        .values
                        .push(format!("{}/{}.jpg", preset_dir(), name.to_lowercase()));
                }
                // A picture of the person's own, chosen before.
                if !pictures.values.contains(&wall.path) && !wall.path.is_empty() {
                    pictures.options.push(
                        std::path::Path::new(&wall.path)
                            .file_stem()
                            .map_or("Your picture".to_owned(), |s| {
                                s.to_string_lossy().into_owned()
                            }),
                    );
                    pictures.values.push(wall.path.clone());
                }
                pictures.current = pictures
                    .values
                    .iter()
                    .position(|p| *p == wall.path)
                    .and_then(|i| i32::try_from(i).ok())
                    .unwrap_or(-1);
                rows.push(pictures);
                rows.push(choice("Fit", Key::WallFit, &FITS, &wall.fit));
            }
            "colour" => {
                let mut colours = Line::new(Kind::Accents, "").key(Key::WallColour);
                colours.colors = COLOURS.iter().map(|c| (*c).to_owned()).collect();
                colours.current = COLOURS
                    .iter()
                    .position(|c| wall.colour.trim_start_matches('#').eq_ignore_ascii_case(c))
                    .and_then(|i| i32::try_from(i).ok())
                    .unwrap_or(-1);
                rows.push(colours);
            }
            "slideshow" => {
                let mut folder = Line::new(Kind::Action, &wall.path)
                    .desc("Pictures from")
                    .key(Key::WallFolder);
                "Choose…".clone_into(&mut folder.tag);
                rows.push(folder);
                rows.push(choice(
                    "Next picture every",
                    Key::WallEvery,
                    &EVERY,
                    &wall.every,
                ));
                let mut shuffle = Line::new(Kind::Switch, "Shuffle")
                    .desc("Show them in no order.")
                    .key(Key::WallShuffle);
                shuffle.on = wall.shuffle;
                rows.push(shuffle);
                rows.push(choice("Fit", Key::WallFit, &FITS, &wall.fit));
            }
            _ => {
                rows.push(
                    Line::new(
                        Kind::Empty,
                        "A video plays on one display, an AV1 file, without fitting or pausing yet: choosing one comes later.",
                    ),
                );
            }
        }
        rows.push(Line::new(Kind::Section, "Lock screen"));
        let mut lock = Line::new(Kind::Switch, "Show the wallpaper on the lock screen")
            .desc("Otherwise the lock screen is plain dark.")
            .key(Key::Setting("hyprix:wallpaper_lock"));
        lock.on = self.value("hyprix:wallpaper_lock").trim() != "0";
        rows.push(lock.note(Note::Now));
        rows
    }

    /// The displays as boxes in a 688 × 240 card, into `row`'s `options`
    /// and `nums`.
    fn arrangement(&self, row: &mut Line) -> f32 {
        let logical = |m: &crate::backend::Monitor| {
            let (w, h) = (m.width as f32 / m.scale, m.height as f32 / m.scale);
            if m.transform % 2 == 1 { (h, w) } else { (w, h) }
        };
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for m in &self.monitors {
            let (w, h) = logical(m);
            x0 = x0.min(m.x as f32);
            y0 = y0.min(m.y as f32);
            x1 = x1.max(m.x as f32 + w);
            y1 = y1.max(m.y as f32 + h);
        }
        let fit = (620.0 / (x1 - x0).max(1.0)).min(200.0 / (y1 - y0).max(1.0));
        let (ox, oy) = (
            (688.0 - (x1 - x0) * fit) / 2.0,
            (240.0 - (y1 - y0) * fit) / 2.0,
        );
        for m in &self.monitors {
            let (w, h) = logical(m);
            row.options
                .push(crate::desktop::display_title(m, self.monitors.len()));
            row.nums.extend([
                ox + (m.x as f32 - x0) * fit + 3.0,
                oy + (m.y as f32 - y0) * fit,
                w * fit - 6.0,
                h * fit,
            ]);
        }
        fit
    }

    /// A Wallpaper row's choice: the line it writes.
    #[must_use]
    pub fn wall_pick(&mut self, row: &Line, option: usize) -> Option<Effect> {
        let raw = row.raw.get(option).cloned();
        let (key, mut wall) = self.wall_target();
        match row.key {
            Key::Display => {
                self.display = option.min(self.monitors.len().saturating_sub(1));
                return None;
            }
            Key::WallMode => {
                let (all, own) = self.walls();
                let mode = raw?;
                let mut set = Vec::new();
                let mut reset = Vec::new();
                let mut all = all;
                all.span = mode == "span";
                set.push(("wallpaper".to_owned(), all.line("")));
                if mode == "each" {
                    // Each display starts from what it shows now.
                    let missing = self
                        .monitors
                        .iter()
                        .filter(|m| !own.iter().any(|(n, _)| *n == m.name));
                    set.extend(
                        missing.map(|m| (format!("wallpaper {}", m.name), all.line(&m.name))),
                    );
                } else {
                    reset.extend(own.into_iter().map(|(n, _)| format!("wallpaper {n}")));
                }
                return Some(Effect::Wall { set, reset });
            }
            Key::WallKind => {
                let kind = raw?;
                if kind == "video" {
                    // Shown, not chosen: the page says why.
                    "video".clone_into(&mut wall.kind);
                    self.wall_video = true;
                    return None;
                }
                self.wall_video = false;
                if kind == "picture" && !wall.path.ends_with(".jpg") && !wall.path.ends_with(".png")
                {
                    wall.path = format!("{}/tide.jpg", preset_dir());
                }
                if kind == "slideshow" {
                    wall.path = self.pictures_dir();
                }
                wall.kind = kind;
            }
            Key::WallPicture => {
                wall.path = row.values.get(option)?.clone();
                "picture".clone_into(&mut wall.kind);
            }
            Key::WallFit => wall.fit = raw?,
            Key::WallEvery => wall.every = raw?,
            Key::WallColour => {
                (*COLOURS.get(option)?).clone_into(&mut wall.colour);
                "colour".clone_into(&mut wall.kind);
            }
            _ => return None,
        }
        let output = key
            .strip_prefix("wallpaper")
            .unwrap_or("")
            .trim()
            .to_owned();
        Some(Effect::Wall {
            set: vec![(key, wall.line(&output))],
            reset: Vec::new(),
        })
    }

    /// The shuffle turned.
    #[must_use]
    pub fn wall_flip(&self, row: &Line) -> Option<Effect> {
        if row.key != Key::WallShuffle {
            return None;
        }
        let (key, mut wall) = self.wall_target();
        wall.shuffle = !wall.shuffle;
        let output = key
            .strip_prefix("wallpaper")
            .unwrap_or("")
            .trim()
            .to_owned();
        Some(Effect::Wall {
            set: vec![(key, wall.line(&output))],
            reset: Vec::new(),
        })
    }

    /// A file chosen in the chooser: a picture for a picture, its folder
    /// for a slideshow.
    #[must_use]
    pub fn wall_chosen(&self, path: &str) -> Option<Effect> {
        let (key, mut wall) = self.wall_target();
        if wall.kind == "slideshow" {
            wall.path = std::path::Path::new(path)
                .parent()?
                .to_string_lossy()
                .into_owned();
        } else {
            path.clone_into(&mut wall.path);
            "picture".clone_into(&mut wall.kind);
        }
        let output = key
            .strip_prefix("wallpaper")
            .unwrap_or("")
            .trim()
            .to_owned();
        Some(Effect::Wall {
            set: vec![(key, wall.line(&output))],
            reset: Vec::new(),
        })
    }

    /// Where a slideshow starts: the person's wallpapers folder, else the
    /// presets'.
    fn pictures_dir(&self) -> String {
        "~/Pictures/Wallpapers".to_owned()
    }

    /// Whether the page is on Wallpaper.
    #[must_use]
    pub fn on_wallpaper(&self) -> bool {
        self.page == Page::Wallpaper
    }
}

#[cfg(test)]
mod tests {
    use super::{Wall, preset_dir};
    use crate::backend::Monitor;
    use crate::pages::{Effect, Key, Kind, Page, State};

    #[test]
    fn a_wallpaper_line_reads_back_as_it_was_written() {
        let (output, wall) = Wall::read(
            "DP-1, slideshow ~/Pictures/Wallpapers, every 1h, shuffle 0, fit fit, colour 101010",
        );
        assert_eq!(output, "DP-1");
        assert_eq!(
            (wall.kind.as_str(), wall.every.as_str(), wall.shuffle),
            ("slideshow", "1h", false)
        );
        assert_eq!(Wall::read(&wall.line("DP-1")).1, wall);
        let (_, off) = Wall::read("HDMI-A-1, off");
        assert_eq!(off.kind, "off");
        assert_eq!(off.line("HDMI-A-1"), "HDMI-A-1, off");
    }

    fn two_displays() -> State {
        let mut state = State::new(Page::Wallpaper);
        state.monitors = vec![
            Monitor {
                name: "eDP-1".to_owned(),
                width: 2560,
                height: 1600,
                scale: 1.5,
                ..Monitor::default()
            },
            Monitor {
                name: "DP-1".to_owned(),
                width: 3840,
                height: 2160,
                scale: 1.5,
                x: 1707,
                ..Monitor::default()
            },
        ];
        state
    }

    #[test]
    fn a_preset_picked_is_one_line_for_every_display() {
        let mut state = two_displays();
        let pictures = state
            .rows()
            .into_iter()
            .find(|r| r.kind == Kind::Pictures)
            .expect("pictures");
        assert_eq!(pictures.options.first().map(String::as_str), Some("Tide"));
        assert_eq!(pictures.current, 0, "Tide until something is chosen");
        let effect = state.wall_pick(&pictures, 3).expect("Dusk");
        assert_eq!(
            effect,
            Effect::Wall {
                set: vec![(
                    "wallpaper".to_owned(),
                    format!(
                        ", picture {}/dusk.jpg, fit fill, colour 1d2027",
                        preset_dir()
                    )
                )],
                reset: Vec::new()
            }
        );
    }

    #[test]
    fn different_on_each_gives_each_display_its_line_and_same_takes_them_back() {
        let mut state = two_displays();
        let mode = state
            .rows()
            .into_iter()
            .find(|r| r.key == Key::WallMode)
            .expect("mode");
        let Some(Effect::Wall { set, .. }) = state.wall_pick(&mode, 1) else {
            panic!("no lines");
        };
        assert_eq!(set.len(), 3);
        for (k, v) in &set {
            let _ = state.values.insert(k.clone(), v.clone());
        }
        // The second display picked, a colour for it alone.
        let screens = state
            .rows()
            .into_iter()
            .find(|r| r.kind == Kind::Screens)
            .expect("screens");
        assert_eq!(state.wall_pick(&screens, 1), None);
        let kind = state
            .rows()
            .into_iter()
            .find(|r| r.key == Key::WallKind)
            .expect("type");
        let Some(Effect::Wall { set, .. }) = state.wall_pick(&kind, 1) else {
            panic!("no line");
        };
        assert_eq!(set[0].0, "wallpaper DP-1");
        assert!(set[0].1.starts_with("DP-1, colour"));
        // Same on all again: the displays' own lines go.
        let mode = state
            .rows()
            .into_iter()
            .find(|r| r.key == Key::WallMode)
            .expect("mode");
        let Some(Effect::Wall { reset, .. }) = state.wall_pick(&mode, 0) else {
            panic!("no reset");
        };
        assert_eq!(reset, ["wallpaper DP-1", "wallpaper eDP-1"]);
    }

    #[test]
    fn a_file_chosen_is_the_picture_and_video_is_said_not_chosen() {
        let mut state = two_displays();
        let Some(Effect::Wall { set, .. }) = state.wall_chosen("/home/ada/Pictures/sea.png") else {
            panic!("no line");
        };
        assert!(set[0].1.contains("picture /home/ada/Pictures/sea.png"));
        let kind = state
            .rows()
            .into_iter()
            .find(|r| r.key == Key::WallKind)
            .expect("type");
        assert_eq!(state.wall_pick(&kind, 3), None);
        assert!(state.rows().iter().any(|r| r.kind == Kind::Empty));
    }
}
