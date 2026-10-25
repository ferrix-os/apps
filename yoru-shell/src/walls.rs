//! The wallpapers, as the switcher (`yoru-shell --walls`, Alt+I) holds
//! them: the pictures and videos under the system's and the person's
//! wallpaper folders, each with the colour the design's picker reads off
//! it, filtered by folder and by colour, sorted, and one of them in hand.
//!
//! What is found: `/usr/share/backgrounds/<folder>/` and
//! `~/Pictures/Wallpapers/<folder>/` (a picture straight in
//! `~/Pictures/Wallpapers` is in the folder "Mine"). A new folder is made
//! under the person's. Setting one writes `wallpaper = , picture PATH,
//! fit fill` (a video: `video PATH`) through `hyprctl setting`, so it is
//! kept in `~/.config/hypr/settings.conf` and drawn at once.

use std::path::{Path, PathBuf};

/// Where the image puts the wallpapers it ships.
pub const SYSTEM: &str = "/usr/share/backgrounds";

/// The person's wallpapers, under `$HOME`.
pub const OWN: &str = "Pictures/Wallpapers";

/// The folder a picture straight in the person's folder is in.
pub const LOOSE: &str = "Mine";

/// Yoru's own wallpaper, the one in force when nothing else was set.
pub const DEFAULT: &str = "/usr/share/backgrounds/yoru/yoru-nocturne.svg";

/// The colour dots, as the design names them, with the hue each stands
/// for (OKLCH degrees).
pub const HUES: [(&str, u16); 8] = [
    ("Red", 25),
    ("Orange", 55),
    ("Yellow", 95),
    ("Green", 145),
    ("Teal", 190),
    ("Blue", 250),
    ("Purple", 300),
    ("Pink", 345),
];

/// What a picture's colour is, as the design measures it on a 48 by 27
/// sample: its hue where it has one, its mean lightness and chroma.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// The weighted mean hue in OKLCH degrees; `None` for a grey picture.
    pub hue: Option<f32>,
    /// The mean relative luminance, 0 to 1.
    pub lum: f32,
    /// The mean chroma (max - min of the channels), 0 to 1.
    pub chroma: f32,
}

impl Look {
    /// What a video is taken as, and a picture that could not be read:
    /// Nocturne's own night.
    pub const NIGHT: Look = Look {
        hue: Some(289.0),
        lum: 0.12,
        chroma: 0.14,
    };

    /// Whether the picture has a hue to sort and filter by.
    #[must_use]
    pub fn hued(&self) -> Option<f32> {
        self.hue.filter(|_| self.chroma >= 0.08)
    }
}

/// The look of `pixels`, RGBA rows of a sample (any size): the design's
/// `analyze`. A pixel's hue counts by chroma² × value; the HSL hue found
/// is moved onto OKLCH's by the design's table.
#[must_use]
pub fn look(pixels: &[[u8; 4]]) -> Look {
    let (mut sx, mut sy, mut weight, mut lum, mut chroma) = (0.0_f32, 0.0_f32, 0.0, 0.0, 0.0);
    let mut n = 0.0_f32;
    for [r, g, b, _] in pixels {
        let (r, g, b) = (
            f32::from(*r) / 255.0,
            f32::from(*g) / 255.0,
            f32::from(*b) / 255.0,
        );
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let ch = max - min;
        lum += 0.2126 * r + 0.7152 * g + 0.0722 * b;
        chroma += ch;
        n += 1.0;
        if ch < 0.14 {
            continue;
        }
        let sector = if (max - r).abs() < f32::EPSILON {
            ((g - b) / ch).rem_euclid(6.0)
        } else if (max - g).abs() < f32::EPSILON {
            (b - r) / ch + 2.0
        } else {
            (r - g) / ch + 4.0
        };
        let hue = (sector * 60.0).rem_euclid(360.0).to_radians();
        let w = ch * ch * max;
        sx += hue.cos() * w;
        sy += hue.sin() * w;
        weight += w;
    }
    if n == 0.0 {
        return Look::NIGHT;
    }
    let hue = (weight > 0.5).then(|| oklch_hue(sy.atan2(sx).to_degrees().rem_euclid(360.0)));
    Look {
        hue,
        lum: lum / n,
        chroma: chroma / n,
    }
}

/// An HSL hue as OKLCH's, by the design's table of the six primaries.
fn oklch_hue(hsl: f32) -> f32 {
    const TABLE: [(f32, f32); 7] = [
        (0.0, 29.0),
        (60.0, 110.0),
        (120.0, 142.0),
        (180.0, 195.0),
        (240.0, 264.0),
        (300.0, 328.0),
        (360.0, 389.0),
    ];
    for pair in TABLE.windows(2) {
        let [(a, oa), (b, ob)] = pair else {
            continue;
        };
        let ((a, oa), (b, ob)) = ((*a, *oa), (*b, *ob));
        if hsl >= a && hsl <= b {
            let t = (hsl - a) / (b - a);
            return (oa + t * (ob - oa)).rem_euclid(360.0);
        }
    }
    hsl
}

/// The look of the picture at `path`: decoded, sampled on a 48 by 27
/// grid; `None` when it cannot be read.
#[must_use]
pub fn look_of(path: &Path) -> Option<Look> {
    let pixmap = compositor_image::load(path, compositor_image::Fit::Within(96, 54)).ok()?;
    let (width, height) = (pixmap.width(), pixmap.height());
    if width == 0 || height == 0 {
        return None;
    }
    let all = pixmap.pixels();
    let mut sample = Vec::with_capacity(48 * 27);
    for row in 0..27_u32 {
        for column in 0..48_u32 {
            let x = (column * 2 + 1) * width / 96;
            let y = (row * 2 + 1) * height / 54;
            let at = usize::try_from(y * width + x).ok()?;
            let pixel = all.get(at)?.demultiply();
            sample.push([pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]);
        }
    }
    Some(look(&sample))
}

/// One wallpaper.
#[derive(Clone, Debug, PartialEq)]
pub struct Wall {
    /// Where it is.
    pub path: String,
    /// Its name as shown: the file's, without the extension, `-` and `_`
    /// as spaces, the first letter up.
    pub name: String,
    /// The folder it is in, as shown.
    pub folder: String,
    /// A video: a live wallpaper.
    pub video: bool,
    /// Its colour; `None` until it was read.
    pub look: Option<Look>,
}

impl Wall {
    /// The line under its name.
    #[must_use]
    pub fn meta(&self) -> String {
        if self.video {
            "Live wallpaper · video".to_owned()
        } else {
            format!("Image · {}", self.folder)
        }
    }

    /// The value of hyprix's `wallpaper` setting that shows it everywhere.
    #[must_use]
    pub fn setting(&self) -> String {
        if self.video {
            format!(", video {}", self.path)
        } else {
            format!(", picture {}, fit fill", self.path)
        }
    }
}

/// Whether `name` is a picture the compositor draws.
fn picture(name: &str) -> bool {
    matches!(extension(name).as_str(), "png" | "jpg" | "jpeg" | "svg")
}

/// Whether `name` is a video `pattern --video` plays: AV1 in IVF, as
/// `cargo xtask wallpapers` converts a person's MP4 or WebM.
fn video(name: &str) -> bool {
    extension(name) == "ivf"
}

/// `name`'s extension, lower case.
fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

/// A file's name as the switcher shows it.
#[must_use]
pub fn shown_name(file: &str) -> String {
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let spaced: String = stem
        .chars()
        .map(|c| if c == '-' || c == '_' { ' ' } else { c })
        .collect();
    let mut chars = spaced.trim().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A folder's name as shown: the system's `settings` (the Settings app's
/// presets) is Ferrix's, the rest as named.
#[must_use]
pub fn folder_name(dir: &str) -> String {
    match dir {
        "settings" => "Ferrix".to_owned(),
        "yoru" => "Yoru".to_owned(),
        other => shown_name(other),
    }
}

/// The files in `dir` that are wallpapers, sorted by name, as
/// `(file name, path)`.
fn files(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            (!name.starts_with('.') && (picture(&name) || video(&name)))
                .then(|| (name, entry.path()))
        })
        .collect();
    found.sort();
    found
}

/// The folders in `dir`, sorted, as `(name, path)`.
fn folders(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            (!name.starts_with('.')).then(|| (name, entry.path()))
        })
        .collect();
    found.sort();
    found
}

/// Every wallpaper under the system's folder `system` and the person's
/// `own`, the system's first; and every folder, empty ones too, in the
/// order shown.
#[must_use]
pub fn find(system: &Path, own: Option<&Path>) -> (Vec<Wall>, Vec<String>) {
    let mut walls = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut add = |folder: String, dir: &Path, walls: &mut Vec<Wall>| {
        for (file, path) in files(dir) {
            walls.push(Wall {
                path: path.to_string_lossy().into_owned(),
                name: shown_name(&file),
                folder: folder.clone(),
                video: video(&file),
                look: None,
            });
        }
        if !names.contains(&folder) {
            names.push(folder);
        }
    };
    for (dir, path) in folders(system) {
        add(folder_name(&dir), &path, &mut walls);
    }
    if let Some(own) = own {
        if !files(own).is_empty() {
            add(LOOSE.to_owned(), own, &mut walls);
        }
        for (dir, path) in folders(own) {
            add(shown_name(&dir), &path, &mut walls);
        }
    }
    (walls, names)
}

/// The picture or video `hyprctl settings` says is the wallpaper of
/// every screen, `None` when no line sets one.
#[must_use]
pub fn in_force(settings: &str) -> Option<String> {
    let mut found = None;
    for line in settings.lines() {
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != "wallpaper" {
            continue;
        }
        let mut fields = value.split(',').map(str::trim);
        if !fields.next().is_some_and(str::is_empty) {
            continue;
        }
        found = fields.find_map(|field| {
            field
                .strip_prefix("picture ")
                .or_else(|| field.strip_prefix("video "))
                .map(|path| path.trim().to_owned())
        });
    }
    found
}

/// Which wallpapers the colour filter keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Colour {
    /// Every one.
    Any,
    /// Those near the hue (within 30°).
    Hue(u16),
    /// The light ones.
    Light,
    /// The dark ones.
    Dark,
}

impl Colour {
    /// Whether a wallpaper of look `look` passes.
    #[must_use]
    pub fn keeps(self, look: Option<Look>) -> bool {
        match (self, look) {
            (Colour::Any, _) => true,
            (_, None) => false,
            (Colour::Light, Some(look)) => look.lum > 0.5,
            (Colour::Dark, Some(look)) => look.lum < 0.3,
            (Colour::Hue(hue), Some(look)) => look.hued().is_some_and(|own| {
                ((own - f32::from(hue) + 540.0).rem_euclid(360.0) - 180.0).abs() < 30.0
            }),
        }
    }
}

/// The order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    /// As found.
    Found,
    /// By hue, grey ones last.
    Hue,
    /// Lightest first.
    Light,
}

/// How the wallpapers are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// One in the middle, the rest to its sides.
    Cover,
    /// Four to a row.
    Grid,
}

/// The switcher's state.
#[derive(Clone, Debug, PartialEq)]
pub struct Picker {
    /// Every wallpaper found.
    pub walls: Vec<Wall>,
    /// Every folder, in the order shown.
    pub folders: Vec<String>,
    /// The folder shown; `None` for all of them.
    pub folder: Option<String>,
    /// The colour filter.
    pub colour: Colour,
    /// The order.
    pub sort: Sort,
    /// The layout.
    pub layout: Layout,
    /// The place in hand in the list shown.
    pub pos: usize,
    /// The path of the wallpaper in force.
    pub current: String,
    /// A new folder's name is being typed.
    pub naming: bool,
}

/// The columns of the grid.
pub const COLUMNS: usize = 4;

impl Picker {
    /// The switcher over `walls` in `folders`, the one at `current` in
    /// hand.
    #[must_use]
    pub fn new(walls: Vec<Wall>, folders: Vec<String>, current: &str) -> Self {
        let mut picker = Picker {
            walls,
            folders,
            folder: None,
            colour: Colour::Any,
            sort: Sort::Found,
            layout: Layout::Cover,
            pos: 0,
            current: current.to_owned(),
            naming: false,
        };
        picker.hold_current();
        picker
    }

    /// The wallpapers shown, as indices into `walls`, in their order.
    #[must_use]
    pub fn list(&self) -> Vec<usize> {
        let mut list: Vec<usize> = self
            .walls
            .iter()
            .enumerate()
            .filter(|(_, wall)| {
                self.folder.as_ref().is_none_or(|f| *f == wall.folder)
                    && self.colour.keeps(wall.look)
            })
            .map(|(i, _)| i)
            .collect();
        let look = |i: &usize| self.walls.get(*i).and_then(|wall| wall.look);
        match self.sort {
            Sort::Found => {}
            Sort::Hue => list.sort_by(|a, b| {
                let key = |i: &usize| look(i).and_then(|l| l.hued()).unwrap_or(999.0);
                key(a).total_cmp(&key(b))
            }),
            Sort::Light => list.sort_by(|a, b| {
                let key = |i: &usize| look(i).map_or(0.0, |l| l.lum);
                key(b).total_cmp(&key(a))
            }),
        }
        list
    }

    /// The wallpaper in hand, an index into `walls`.
    #[must_use]
    pub fn held(&self) -> Option<usize> {
        self.list().get(self.pos).copied()
    }

    /// Put the one in force in hand, or the first where it is not shown.
    fn hold_current(&mut self) {
        let list = self.list();
        self.pos = list
            .iter()
            .position(|&i| {
                self.walls
                    .get(i)
                    .is_some_and(|wall| wall.path == self.current)
            })
            .unwrap_or(0);
    }

    /// Move the hand `by` places, staying in the list.
    pub fn step(&mut self, by: isize) {
        let len = self.list().len();
        if len == 0 {
            self.pos = 0;
            return;
        }
        let pos = isize::try_from(self.pos).unwrap_or(0).saturating_add(by);
        self.pos = usize::try_from(pos.max(0)).unwrap_or(0).min(len - 1);
    }

    /// Up or down a row of the grid; left or right in the cover.
    pub fn row(&mut self, down: bool) {
        let by = if self.layout == Layout::Grid {
            COLUMNS as isize
        } else {
            1
        };
        self.step(if down { by } else { -by });
    }

    /// Show folder `folder` (`None`: all).
    pub fn show_folder(&mut self, folder: Option<String>) {
        self.folder = folder;
        self.naming = false;
        self.hold_current();
    }

    /// Filter by `colour`; the dot pressed again takes the filter off.
    pub fn filter(&mut self, colour: Colour) {
        self.colour = if self.colour == colour {
            Colour::Any
        } else {
            colour
        };
        self.hold_current();
    }

    /// Sort by `sort`.
    pub fn sort_by(&mut self, sort: Sort) {
        self.sort = sort;
        self.hold_current();
    }

    /// The other layout.
    pub fn flip(&mut self) {
        self.layout = match self.layout {
            Layout::Cover => Layout::Grid,
            Layout::Grid => Layout::Cover,
        };
    }

    /// A folder named `name` was made: shown, and in the list of folders.
    /// False for a name that is empty, "All" or one there is.
    pub fn named(&mut self, name: &str) -> bool {
        let name = name.trim();
        self.naming = false;
        if name.is_empty() || name == "All" || self.folders.iter().any(|f| f == name) {
            return false;
        }
        self.folders.push(name.to_owned());
        self.show_folder(Some(name.to_owned()));
        true
    }

    /// How many wallpapers folder `folder` has (`None`: all).
    #[must_use]
    pub fn count(&self, folder: Option<&str>) -> usize {
        self.walls
            .iter()
            .filter(|wall| folder.is_none_or(|f| f == wall.folder))
            .count()
    }

    /// The line under the title: "6 in All".
    #[must_use]
    pub fn count_line(&self) -> String {
        format!(
            "{} in {}",
            self.list().len(),
            self.folder.as_deref().unwrap_or("All")
        )
    }
}

/// Where a cover sits, relative to the middle of the stage: the design's
/// `translateX`, `translateZ` and `rotateY` flattened to a box. `offset`
/// is the place from the one in hand.
#[must_use]
pub fn cover_box(offset: isize, centre: f32) -> (f32, f32, f32, f32) {
    const WIDTH: f32 = 400.0;
    const HEIGHT: f32 = 250.0;
    if offset == 0 {
        return (centre - WIDTH / 2.0, 20.0, WIDTH, HEIGHT);
    }
    let away = offset.unsigned_abs() as f32;
    let side = offset.signum() as f32;
    // Pushed back 140 under a perspective of 1100, turned 62°.
    let depth = 1100.0 / (1100.0 + 140.0 + 40.0);
    let (width, height) = (WIDTH * depth * 0.62, HEIGHT * depth);
    let middle = centre + side * (240.0 + (away - 1.0) * 62.0);
    (
        middle - width / 2.0,
        20.0 + (HEIGHT - height) / 2.0,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(name: &str, folder: &str, look: Option<Look>) -> Wall {
        Wall {
            path: format!("/w/{name}.jpg"),
            name: name.to_owned(),
            folder: folder.to_owned(),
            video: false,
            look,
        }
    }

    fn hued(hue: f32, lum: f32) -> Option<Look> {
        Some(Look {
            hue: Some(hue),
            lum,
            chroma: 0.3,
        })
    }

    #[test]
    fn a_red_picture_reads_red_and_a_grey_one_has_no_hue() {
        let red = look(&[[220, 30, 30, 255]; 64]);
        let hue = red.hue.expect("a hue");
        assert!((hue - 29.0).abs() < 2.0, "red is OKLCH's 29°: {hue}");
        assert!(Colour::Hue(25).keeps(Some(red)));
        assert!(!Colour::Hue(250).keeps(Some(red)));
        let grey = look(&[[128, 128, 128, 255]; 64]);
        assert_eq!(grey.hue, None);
        assert!(!Colour::Hue(25).keeps(Some(grey)));
        assert!(Colour::Dark.keeps(Some(look(&[[10, 10, 20, 255]; 4]))));
        assert!(Colour::Light.keeps(Some(look(&[[240, 240, 235, 255]; 4]))));
    }

    #[test]
    fn names_read_as_words() {
        assert_eq!(shown_name("pink-dusk.jpg"), "Pink dusk");
        assert_eq!(shown_name("late_night.ivf"), "Late night");
        assert_eq!(folder_name("settings"), "Ferrix");
    }

    #[test]
    fn the_setting_in_force_is_read_back() {
        let text = "general:gaps_in = 4\nwallpaper = , picture /a/b.jpg, fit fill\n";
        assert_eq!(in_force(text).as_deref(), Some("/a/b.jpg"));
        assert_eq!(in_force("wallpaper = DP-1, picture /x.png\n"), None);
        assert_eq!(
            in_force("wallpaper = , video /v.ivf\n").as_deref(),
            Some("/v.ivf")
        );
        let w = wall("b", "Art", None);
        assert_eq!(w.setting(), ", picture /w/b.jpg, fit fill");
    }

    #[test]
    fn the_one_in_force_is_in_hand_and_the_hand_stays_in_the_list() {
        let walls = vec![
            wall("a", "Art", hued(29.0, 0.4)),
            wall("b", "Art", hued(250.0, 0.2)),
            wall("c", "Gradients", hued(145.0, 0.7)),
        ];
        let mut picker = Picker::new(walls, vec!["Art".into(), "Gradients".into()], "/w/b.jpg");
        assert_eq!(picker.held(), Some(1));
        picker.step(5);
        assert_eq!(picker.held(), Some(2));
        picker.step(-9);
        assert_eq!(picker.held(), Some(0));
        picker.show_folder(Some("Gradients".into()));
        assert_eq!(picker.list(), vec![2]);
        assert_eq!(picker.count_line(), "1 in Gradients");
        picker.show_folder(None);
        picker.filter(Colour::Hue(250));
        assert_eq!(picker.list(), vec![1]);
        picker.filter(Colour::Hue(250));
        assert_eq!(picker.colour, Colour::Any);
        picker.sort_by(Sort::Light);
        assert_eq!(picker.list(), vec![2, 0, 1]);
        picker.sort_by(Sort::Hue);
        assert_eq!(picker.list(), vec![0, 2, 1]);
        assert_eq!(picker.held(), Some(1), "the one in force stays in hand");
    }

    #[test]
    fn a_new_folder_is_shown_and_a_taken_name_is_not_made_twice() {
        let mut picker = Picker::new(vec![wall("a", "Art", None)], vec!["Art".into()], "");
        assert!(picker.named(" Space "));
        assert_eq!(picker.folder.as_deref(), Some("Space"));
        assert_eq!(picker.count_line(), "0 in Space");
        assert!(!picker.named("Art"));
        assert!(!picker.named("All"));
        assert!(!picker.named(""));
    }

    #[test]
    fn the_grid_steps_a_row_and_the_cover_one() {
        let walls = (0..10).map(|i| wall(&i.to_string(), "A", None)).collect();
        let mut picker = Picker::new(walls, vec!["A".into()], "");
        picker.row(true);
        assert_eq!(picker.pos, 1);
        picker.flip();
        picker.row(true);
        assert_eq!(picker.pos, 5);
        picker.row(true);
        assert_eq!(picker.pos, 9);
    }

    #[test]
    fn covers_sit_either_side_of_the_one_in_hand() {
        let (x, _, w, _) = cover_box(0, 500.0);
        assert!((x + w / 2.0 - 500.0).abs() < 0.01);
        let (left, ..) = cover_box(-1, 500.0);
        let (right, ..) = cover_box(1, 500.0);
        assert!(left < x && right > x + w / 2.0);
        assert!(cover_box(2, 500.0).0 > right);
    }

    #[test]
    fn found_in_folders_with_loose_ones_in_mine() {
        let root = std::env::temp_dir().join(format!("yoru-walls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let system = root.join("system");
        let own = root.join("own");
        for dir in [
            system.join("settings"),
            own.join("space-art"),
            own.join("empty"),
        ] {
            std::fs::create_dir_all(dir).expect("made");
        }
        for file in [
            system.join("settings/tide.jpg"),
            system.join("settings/notes.txt"),
            own.join("loose.png"),
            own.join("space-art/rain.ivf"),
        ] {
            std::fs::write(file, b"x").expect("written");
        }
        let (walls, folders) = find(&system, Some(&own));
        let _ = std::fs::remove_dir_all(&root);
        let names: Vec<(&str, &str, bool)> = walls
            .iter()
            .map(|w| (w.name.as_str(), w.folder.as_str(), w.video))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Tide", "Ferrix", false),
                ("Loose", "Mine", false),
                ("Rain", "Space art", true)
            ]
        );
        assert_eq!(folders, vec!["Ferrix", "Mine", "Empty", "Space art"]);
    }
}
