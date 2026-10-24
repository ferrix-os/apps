//! The pages backed by the desktop today (Ferrix's
//! `docs/RUNTIME-SETTINGS.md` §3.3, §7): Appearance, Windows, Displays,
//! Keyboard, Idle and lock -- every change through hyprix, at once and
//! kept -- and the System pages, shown as they stand until the settings
//! writer (`docs/SETTINGS-WRITER.md`) can change them.
//!
//! A row knows its setting by its [`Key`]; a press answers the
//! [`Effect`] the window carries out, and the state takes the new value
//! only once that is done (`State::applied`).

use crate::look::{ACCENTS, hex};
use crate::pages::{Effect, Key, Kind, Line, Note, Page, State};

/// hypridle's timeouts, as Settings reads and writes them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Idle {
    /// Lock after this many minutes; `None` never.
    pub lock: Option<u32>,
    /// The screen off after this many minutes; `None` never.
    pub off: Option<u32>,
    /// The file was written by hand: Settings leaves it alone.
    pub by_hand: bool,
}

/// The first line of a hypridle file Settings wrote.
pub const IDLE_MARK: &str =
    "# Written by Settings. Edit it there, or delete this line to keep your own.";

impl Idle {
    /// Read from `text`, hypridle's file.
    #[must_use]
    pub fn read(text: Option<&str>) -> Self {
        let Some(text) = text else {
            return Idle {
                lock: Some(5),
                off: Some(10),
                by_hand: false,
            };
        };
        let mut idle = Idle {
            lock: None,
            off: None,
            by_hand: !text.starts_with(IDLE_MARK),
        };
        // Each listener: its timeout, then what it does.
        let mut timeout = None;
        for line in text.lines().map(str::trim) {
            if let Some(value) = line
                .strip_prefix("timeout")
                .and_then(|v| v.trim().strip_prefix('='))
            {
                timeout = value.trim().parse::<u32>().ok().map(|s| s / 60);
            }
            if line.starts_with("on-timeout") {
                if line.contains("lock") {
                    idle.lock = timeout;
                } else if line.contains("dpms off") {
                    idle.off = timeout;
                }
            }
        }
        idle
    }

    /// hypridle's file for these timeouts.
    #[must_use]
    pub fn text(lock: Option<u32>, off: Option<u32>) -> String {
        let mut out = format!(
            "{IDLE_MARK}\n\ngeneral {{\n    lock_cmd = pidof hyprlock || hyprlock\n    before_sleep_cmd = loginctl lock-session\n    after_sleep_cmd = hyprctl dispatch dpms on\n}}\n"
        );
        if let Some(minutes) = lock {
            out.push_str(&format!(
                "\nlistener {{\n    timeout = {}\n    on-timeout = loginctl lock-session\n}}\n",
                minutes * 60
            ));
        }
        if let Some(minutes) = off {
            out.push_str(&format!(
                "\nlistener {{\n    timeout = {}\n    on-timeout = hyprctl dispatch dpms off\n    on-resume = hyprctl dispatch dpms on\n}}\n",
                minutes * 60
            ));
        }
        out
    }
}

/// The keyboard layouts Ferrix ships (`docs/RUNTIME-SETTINGS.md` §3.3):
/// `layout(variant)` and the name a person knows.
pub const LAYOUTS: [(&str, &str); 5] = [
    ("us", "English (US)"),
    ("gb", "English (UK)"),
    ("fr", "French"),
    ("de", "German"),
    ("de(nodeadkeys)", "German, no dead keys"),
];

/// The window layouts, the words the design gives them, and two lines of
/// what each does.
pub const WINDOW_LAYOUTS: [(&str, &str, &str, &str); 5] = [
    (
        "dwindle",
        "Spiral",
        "Each new window halves the",
        "space of the last.",
    ),
    (
        "master",
        "Main and stack",
        "One large window, the rest",
        "beside it.",
    ),
    (
        "monocle",
        "One at a time",
        "Every window fills the screen.",
        "",
    ),
    (
        "scrolling",
        "Scrolling",
        "Windows in a row you",
        "scroll through.",
    ),
    (
        "floating",
        "Floating",
        "Place and size windows yourself.",
        "",
    ),
];

/// Spacing's presets: inner and outer gaps.
const SPACING: [(&str, u32, u32); 3] = [
    ("Compact", 2, 6),
    ("Comfortable", 5, 20),
    ("Spacious", 10, 32),
];
/// Corners' presets: the rounding.
const CORNERS: [(&str, u32); 3] = [("Square", 0), ("Rounded", 8), ("Round", 16)];

/// The colours an accent gives the desktop, as hyprland writes them:
/// the focused border, the others, the shadow.
#[must_use]
pub fn accent_colours(hue: f32) -> [String; 3] {
    [
        format!("rgb({})", hex(0.72, 0.13, hue)),
        format!("rgb({})", hex(0.36, 0.03, hue)),
        format!("rgb({})", hex(0.12, 0.03, hue)),
    ]
}

/// A default for `key` when `settings.conf` does not hold it: hyprix's
/// own (`docs/RUNTIME-SETTINGS.md` §3.3).
fn default_of(key: &str) -> &'static str {
    match key {
        "general:gaps_in" => "5",
        "general:gaps_out" => "20",
        "general:border_size" => "1",
        "decoration:rounding" => "0",
        "decoration:inactive_opacity" => "1",
        "decoration:blur:enabled" => "0",
        "decoration:shadow:enabled" => "0",
        "animations:enabled" | "input:follow_mouse" => "1",
        "general:layout" => "dwindle",
        "hyprix:titlebars" => "0",
        "input:kb_layout" => "us",
        "input:repeat_rate" => "25",
        "input:repeat_delay" => "600",
        "misc:lock_grace" => "0",
        _ => "",
    }
}

/// A choice row: words, the options' words and their values.
fn choice<S: AsRef<str>>(title: &str, key: Key, options: &[(S, String)], now: &str) -> Line {
    let mut row = Line::new(Kind::Choice, title).key(key);
    row.options = options.iter().map(|(w, _)| w.as_ref().to_owned()).collect();
    row.raw = options.iter().map(|(_, v)| v.clone()).collect();
    row.current = row
        .raw
        .iter()
        .position(|v| v == now)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(-1);
    row
}

/// A switch row.
fn switch(title: &str, desc: &str, key: Key, on: bool) -> Line {
    let mut row = Line::new(Kind::Switch, title).desc(desc).key(key);
    row.on = on;
    row
}

/// A slider row: from, to, step, the value now and how it is said.
fn slider(
    title: &str,
    key: Key,
    (from, to, step): (f32, f32, f32),
    value: f32,
    unit: &str,
) -> Line {
    let mut row = Line::new(Kind::Slider, title).key(key);
    row.from = from;
    row.to = to;
    row.step = step;
    row.value = value;
    row.shown = format!("{}{unit}", trim(value));
    row
}

/// A number as written: no `.0`.
fn trim(value: f32) -> String {
    if value.fract().abs() < 0.001 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.2}").trim_end_matches('0').to_owned()
    }
}

/// A section heading.
fn section(title: &str) -> Line {
    Line::new(Kind::Section, title)
}

/// Minutes said as the design says them.
fn minutes(m: Option<u32>) -> String {
    m.map_or_else(|| "Never".to_owned(), |m| format!("{m} min"))
}

impl State {
    /// `key`'s value: as `settings.conf` holds it, else hyprix's default.
    #[must_use]
    pub fn value(&self, key: &str) -> String {
        self.values
            .get(key)
            .cloned()
            .unwrap_or_else(|| default_of(key).to_owned())
    }

    /// `key` as a number.
    fn number(&self, key: &str) -> f32 {
        self.value(key).trim().parse().unwrap_or(0.0)
    }

    /// `key` as on or off.
    fn on(&self, key: &str) -> bool {
        !matches!(self.value(key).trim(), "0" | "false" | "no" | "off" | "")
    }

    /// The accent the desktop's focused border is in, if it is one of the
    /// seven.
    #[must_use]
    pub fn desktop_accent(&self) -> Option<usize> {
        let border = self.value("general:col.active_border");
        ACCENTS
            .iter()
            .position(|(_, hue)| accent_colours(*hue)[0] == border)
    }

    /// The rows of a page this module draws.
    #[must_use]
    pub fn desktop_rows(&self) -> Vec<Line> {
        match self.page {
            Page::Appearance => self.appearance(),
            Page::Windows => self.windows(),
            Page::Displays => self.displays(),
            Page::Keyboard => self.keyboard(),
            Page::Idle => self.idle_rows(),
            Page::Computer => self.computer(),
            Page::Remote => vec![
                switch(
                    "Allow remote login",
                    "Anyone on your network who knows a user's password can sign in as that user.",
                    Key::Shown,
                    false,
                )
                .note(Note::NotKept),
                Line::new(
                    Kind::Empty,
                    "Turning it on comes with the settings writer; today sshd is started by the image.",
                ),
            ],
            page => vec![Line::new(
                Kind::Empty,
                &format!(
                    "{} is changed through the settings writer, which comes next; nothing here can be changed yet.",
                    page.label()
                ),
            )],
        }
    }

    /// Appearance: the desktop drawn small, the accent, then spacing,
    /// corners, see-through windows and animations -- or, in Advanced,
    /// each knob.
    fn appearance(&self) -> Vec<Line> {
        let mut rows = vec![Line::new(Kind::Preview, "")];
        rows.push(section("Accent colour"));
        let mut accents = Line::new(Kind::Accents, "").key(Key::Accent);
        accents.colors = ACCENTS.iter().map(|(_, h)| hex(0.72, 0.13, *h)).collect();
        accents.current = i32::try_from(self.accent).unwrap_or(0);
        rows.push(accents);
        rows.push(section("Windows"));
        if self.advanced {
            rows.push(slider(
                "Space between windows",
                Key::Setting("general:gaps_in"),
                (0.0, 20.0, 1.0),
                self.number("general:gaps_in"),
                " px",
            ));
            rows.push(slider(
                "Space at the screen's edge",
                Key::Setting("general:gaps_out"),
                (0.0, 48.0, 1.0),
                self.number("general:gaps_out"),
                " px",
            ));
            rows.push(slider(
                "Border",
                Key::Setting("general:border_size"),
                (0.0, 6.0, 1.0),
                self.number("general:border_size"),
                " px",
            ));
            rows.push(slider(
                "Corner rounding",
                Key::Setting("decoration:rounding"),
                (0.0, 24.0, 1.0),
                self.number("decoration:rounding"),
                " px",
            ));
            rows.push(slider(
                "Other windows' opacity",
                Key::Setting("decoration:inactive_opacity"),
                (0.5, 1.0, 0.05),
                self.number("decoration:inactive_opacity"),
                "",
            ));
            rows.push(switch(
                "Blur behind windows",
                "",
                Key::Setting("decoration:blur:enabled"),
                self.on("decoration:blur:enabled"),
            ));
            rows.push(switch(
                "Shadows",
                "",
                Key::Setting("decoration:shadow:enabled"),
                self.on("decoration:shadow:enabled"),
            ));
        } else {
            let gaps = (
                self.number("general:gaps_in") as u32,
                self.number("general:gaps_out") as u32,
            );
            let spacing: Vec<(&str, String)> = SPACING
                .iter()
                .map(|(w, i, o)| (*w, format!("{i},{o}")))
                .collect();
            rows.push(choice(
                "Spacing",
                Key::Spacing,
                &spacing,
                &format!("{},{}", gaps.0, gaps.1),
            ));
            let corners: Vec<(&str, String)> =
                CORNERS.iter().map(|(w, r)| (*w, r.to_string())).collect();
            rows.push(choice(
                "Corners",
                Key::Corners,
                &corners,
                &(self.number("decoration:rounding") as u32).to_string(),
            ));
            rows.push(switch(
                "See-through windows",
                "Windows you aren't using fade slightly and blur what's behind.",
                Key::SeeThrough,
                self.number("decoration:inactive_opacity") < 0.999,
            ));
        }
        rows.push(switch(
            "Animations",
            "Turn off to reduce motion.",
            Key::Setting("animations:enabled"),
            self.on("animations:enabled"),
        ));
        rows
    }

    /// Windows: the layout, title bars, focus.
    fn windows(&self) -> Vec<Line> {
        let mut layouts = Line::new(Kind::Layouts, "").key(Key::Setting("general:layout"));
        layouts.options = WINDOW_LAYOUTS.iter().map(|l| l.1.to_owned()).collect();
        layouts.values = WINDOW_LAYOUTS
            .iter()
            .flat_map(|l| [l.2.to_owned(), l.3.to_owned()])
            .collect();
        layouts.raw = WINDOW_LAYOUTS.iter().map(|l| l.0.to_owned()).collect();
        let now = self.value("general:layout");
        layouts.current = layouts
            .raw
            .iter()
            .position(|v| *v == now)
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1);
        vec![
            section("Layout"),
            layouts,
            section("Behaviour"),
            switch(
                "Title bars",
                "Draw a bar across the top of each window.",
                Key::Setting("hyprix:titlebars"),
                self.on("hyprix:titlebars"),
            ),
            switch(
                "Focus follows the pointer",
                "The window under the pointer takes your typing, without a click.",
                Key::Setting("input:follow_mouse"),
                self.on("input:follow_mouse"),
            ),
        ]
    }

    /// Displays: the arrangement to pick one, then its mode, rate, scale
    /// and rotation.
    fn displays(&self) -> Vec<Line> {
        if self.monitors.is_empty() {
            return vec![Line::new(
                Kind::Empty,
                "hyprix reports no displays: is the desktop running?",
            )];
        }
        let mut arrangement = Line::new(Kind::Monitors, "").key(Key::Display);
        // The arrangement scaled into the card, 688 × 240, centred.
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
        let fit = (560.0 / (x1 - x0).max(1.0)).min(180.0 / (y1 - y0).max(1.0));
        let (ox, oy) = (
            (688.0 - (x1 - x0) * fit) / 2.0,
            (240.0 - (y1 - y0) * fit) / 2.0,
        );
        for m in &self.monitors {
            let (w, h) = logical(m);
            arrangement
                .options
                .push(display_title(m, self.monitors.len()));
            arrangement
                .values
                .push(format!("{}×{} · {}×", m.width, m.height, trim(m.scale)));
            arrangement.nums.extend([
                ox + (m.x as f32 - x0) * fit + 2.0,
                oy + (m.y as f32 - y0) * fit,
                w * fit - 4.0,
                h * fit,
            ]);
        }
        arrangement.current = i32::try_from(self.display).unwrap_or(0);
        let mut rows = vec![arrangement];
        let Some(m) = self.monitors.get(self.display) else {
            return rows;
        };
        let mut heading = section(&display_title(m, self.monitors.len()));
        heading.title = [heading.title.clone(), m.model.clone(), m.name.clone()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("   ");
        rows.push(heading);
        // The modes it offers: sizes, and the rates at the size it is at.
        let mut sizes: Vec<String> = Vec::new();
        let mut rates: Vec<String> = Vec::new();
        let now_size = format!("{}x{}", m.width, m.height);
        for mode in &m.modes {
            let (size, rate) = mode.split_once('@').unwrap_or((mode.as_str(), ""));
            if !sizes.iter().any(|s| s == size) && sizes.len() < 4 {
                sizes.push(size.to_owned());
            }
            let rate = rate.trim_end_matches("Hz");
            let rate = trim(rate.parse::<f32>().unwrap_or(0.0));
            if size == now_size && !rates.contains(&rate) && rates.len() < 4 {
                rates.push(rate);
            }
        }
        if sizes.is_empty() {
            sizes.push(now_size.clone());
        }
        if rates.is_empty() {
            rates.push(trim(m.refresh));
        }
        let size_opts: Vec<(String, String)> = sizes
            .iter()
            .map(|s| (s.replace('x', "×"), s.clone()))
            .collect();
        let rate_opts: Vec<(String, String)> = rates
            .iter()
            .map(|r| (format!("{r} Hz"), r.clone()))
            .collect();
        rows.push(choice(
            "Resolution",
            Key::Monitor("res"),
            &size_opts,
            &now_size,
        ));
        rows.push(choice(
            "Refresh rate",
            Key::Monitor("hz"),
            &rate_opts,
            &trim(m.refresh),
        ));
        let scales: Vec<(&str, String)> =
            [("100%", 1.0), ("125%", 1.25), ("150%", 1.5), ("200%", 2.0)]
                .iter()
                .map(|(w, v)| (*w, trim(*v)))
                .collect();
        rows.push(choice(
            "Scale",
            Key::Monitor("scale"),
            &scales,
            &trim(m.scale),
        ));
        let rotations: Vec<(&str, String)> = [("0°", 0), ("90°", 1), ("180°", 2), ("270°", 3)]
            .iter()
            .map(|(w, v)| (*w, v.to_string()))
            .collect();
        rows.push(choice(
            "Rotation",
            Key::Monitor("rot"),
            &rotations,
            &m.transform.to_string(),
        ));
        rows
    }

    /// The `monitor` line for the shown display with `field` set to
    /// `value`.
    fn monitor_line(&self, field: &str, value: &str) -> Option<String> {
        let m = self.monitors.get(self.display)?;
        let mut size = format!("{}x{}", m.width, m.height);
        let mut rate = trim(m.refresh);
        let mut scale = trim(m.scale);
        let mut rot = m.transform.to_string();
        match field {
            "res" => value.clone_into(&mut size),
            "hz" => value.clone_into(&mut rate),
            "scale" => value.clone_into(&mut scale),
            _ => value.clone_into(&mut rot),
        }
        Some(format!(
            "{},{size}@{rate},{}x{},{scale},transform,{rot}",
            m.name, m.x, m.y
        ))
    }

    /// Keyboard: a line to try, the layout typed in, the layouts, repeat.
    fn keyboard(&self) -> Vec<Line> {
        let mut rows = Vec::new();
        let mut try_line = Line::new(Kind::Field, "")
            .desc("Type here to try your keyboard")
            .key(Key::Try);
        try_line.tag = "big".to_owned();
        rows.push(try_line);
        let mut typing = Line::new(Kind::Action, &self.typing_in)
            .desc("Typing in")
            .key(Key::NextLayout);
        typing.tag = "Switch layout".to_owned();
        typing.enabled = self.kb_layouts().len() > 1;
        rows.push(typing);
        rows.push(section("Layouts"));
        let chosen = self.kb_layouts();
        for (id, name) in LAYOUTS {
            let mut row = Line::new(Kind::Check, name).key(Key::KbLayout(id));
            row.on = chosen.iter().any(|c| c == id);
            if chosen.first().is_some_and(|c| c == id) {
                "Default".clone_into(&mut row.tag);
            }
            rows.push(row);
        }
        rows.push(section("Key repeat"));
        rows.push(slider(
            "Delay before repeating",
            Key::Setting("input:repeat_delay"),
            (200.0, 1000.0, 50.0),
            self.number("input:repeat_delay"),
            " ms",
        ));
        rows.push(slider(
            "Repeat speed",
            Key::Setting("input:repeat_rate"),
            (10.0, 50.0, 1.0),
            self.number("input:repeat_rate"),
            " / s",
        ));
        rows
    }

    /// The layouts chosen, as `layout(variant)`, the first the default.
    #[must_use]
    pub fn kb_layouts(&self) -> Vec<String> {
        let layouts = self.value("input:kb_layout");
        let variants = self.value("input:kb_variant");
        let variants: Vec<&str> = variants.split(',').collect();
        layouts
            .split(',')
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .enumerate()
            .map(|(i, l)| match variants.get(i).map(|v| v.trim()) {
                Some(v) if !v.is_empty() => format!("{l}({v})"),
                _ => l.to_owned(),
            })
            .collect()
    }

    /// Idle and lock: when it locks, when the screen goes off, the grace.
    fn idle_rows(&self) -> Vec<Line> {
        let lock: Vec<(String, String)> = [Some(1), Some(2), Some(5), Some(10), Some(15), None]
            .iter()
            .map(|m| (minutes(*m), m.map_or("never".to_owned(), |m| m.to_string())))
            .collect();
        let off: Vec<(String, String)> = [Some(2), Some(5), Some(10), Some(30), None]
            .iter()
            .map(|m| (minutes(*m), m.map_or("never".to_owned(), |m| m.to_string())))
            .collect();
        let said = |m: Option<u32>| m.map_or("never".to_owned(), |m| m.to_string());
        let note = if self.idle.by_hand {
            Note::Now
        } else {
            Note::SignIn
        };
        let mut rows = vec![
            choice(
                "Lock the screen after",
                Key::Idle("lock"),
                &lock,
                &said(self.idle.lock),
            )
            .note(note),
            choice(
                "Turn the screen off after",
                Key::Idle("off"),
                &off,
                &said(self.idle.off),
            )
            .note(note),
            slider(
                "Unlock without a password for",
                Key::Setting("misc:lock_grace"),
                (0.0, 10.0, 1.0),
                self.number("misc:lock_grace"),
                " s",
            )
            .desc("Moving the mouse right after the screen locks unlocks it again."),
        ];
        if self.idle.by_hand {
            rows.push(Line::new(
                Kind::Empty,
                "Your hypridle.conf was written by hand, so Settings leaves it as it is.",
            ));
        }
        rows
    }

    /// Name and time: the name, the clock, About.
    fn computer(&self) -> Vec<Line> {
        let mut name = Line::new(Kind::Name, "Computer name")
            .desc(&format!(
                "Not kept yet: it goes back to “{}” when you restart.",
                if self.hostname.is_empty() {
                    "ferrix"
                } else {
                    &self.hostname
                }
            ))
            .key(Key::Shown);
        name.shown.clone_from(&self.hostname);
        "Rename".clone_into(&mut name.tag);
        name.enabled = false;
        let mut about = Line::new(Kind::Info, "");
        for (label, value) in &self.about {
            about.options.push(label.clone());
            about.values.push(value.clone());
        }
        let mut rows = vec![name, section("About")];
        if !about.options.is_empty() {
            rows.push(about);
        }
        rows
    }

    /// A switch or tick turned: what it asks.
    #[must_use]
    pub fn desktop_flip(&self, row: &Line) -> Option<Effect> {
        let set = |pairs: &[(&str, String)]| {
            Some(Effect::Set(
                pairs
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), v.clone()))
                    .collect(),
            ))
        };
        match row.key {
            Key::Setting(key) => set(&[(key, if row.on { "0" } else { "1" }.to_owned())]),
            Key::SeeThrough => {
                if row.on {
                    set(&[
                        ("decoration:inactive_opacity", "1".to_owned()),
                        ("decoration:blur:enabled", "0".to_owned()),
                    ])
                } else {
                    set(&[
                        ("decoration:inactive_opacity", "0.9".to_owned()),
                        ("decoration:blur:enabled", "1".to_owned()),
                    ])
                }
            }
            Key::KbLayout(id) => {
                let mut chosen = self.kb_layouts();
                if row.on {
                    // The last one stays: a keyboard types in something.
                    if chosen.len() <= 1 {
                        return None;
                    }
                    chosen.retain(|c| c != id);
                } else {
                    chosen.push(id.to_owned());
                }
                let (layouts, variants): (Vec<String>, Vec<String>) = chosen
                    .iter()
                    .map(|c| match c.split_once('(') {
                        Some((l, v)) => (l.to_owned(), v.trim_end_matches(')').to_owned()),
                        None => (c.clone(), String::new()),
                    })
                    .unzip();
                set(&[
                    ("input:kb_layout", layouts.join(",")),
                    ("input:kb_variant", variants.join(",")),
                ])
            }
            _ => None,
        }
    }

    /// Option `option` of a row chosen: what it asks.
    #[must_use]
    pub fn pick(&mut self, row: &Line, option: usize) -> Option<Effect> {
        if self.page == Page::Wallpaper {
            return self.wall_pick(row, option);
        }
        let raw = row.raw.get(option).cloned();
        let set = |pairs: Vec<(&str, String)>| {
            Some(Effect::Set(
                pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
            ))
        };
        match row.key {
            Key::Setting(key) => set(vec![(key, raw?)]),
            Key::Spacing => {
                let (_, i, o) = SPACING.get(option)?;
                set(vec![
                    ("general:gaps_in", i.to_string()),
                    ("general:gaps_out", o.to_string()),
                ])
            }
            Key::Corners => set(vec![("decoration:rounding", raw?)]),
            Key::Accent => {
                let (_, hue) = ACCENTS.get(option)?;
                self.accent = option;
                let [active, inactive, shadow] = accent_colours(*hue);
                set(vec![
                    ("general:col.active_border", active),
                    ("general:col.inactive_border", inactive),
                    ("decoration:shadow:color", shadow),
                ])
            }
            Key::Display => {
                self.display = option.min(self.monitors.len().saturating_sub(1));
                None
            }
            Key::Monitor(field) => self.monitor_line(field, &raw?).map(Effect::Monitor),
            Key::Idle(which) => {
                let value = raw?;
                let minutes = value.parse::<u32>().ok();
                if self.idle.by_hand {
                    return None;
                }
                Some(match which {
                    "lock" => Effect::Idle {
                        lock: minutes,
                        off: self.idle.off,
                    },
                    _ => Effect::Idle {
                        lock: self.idle.lock,
                        off: minutes,
                    },
                })
            }
            _ => None,
        }
    }

    /// A slider moved: what it asks.
    #[must_use]
    pub fn slide(&self, row: &Line, value: f32) -> Option<Effect> {
        match row.key {
            Key::Setting(key) if (value - row.value).abs() > 0.0001 => {
                Some(Effect::Set(vec![(key.to_owned(), trim(value))]))
            }
            _ => None,
        }
    }

    /// A row's button pressed: what it asks.
    #[must_use]
    pub fn press(&self, row: &Line) -> Option<Effect> {
        match row.key {
            Key::NextLayout => Some(Effect::NextLayout),
            _ => None,
        }
    }

    /// A row's button pressed that opens the file chooser.
    pub fn press_opens(&mut self, row: &Line) {
        if matches!(row.key, Key::WallFolder | Key::WallPicture) {
            self.want_chooser = true;
        }
    }

    /// `effect` was done: take what it changed, and note what waits.
    pub fn applied(&mut self, effect: &Effect) {
        match effect {
            Effect::Set(pairs) => {
                for (key, value) in pairs {
                    let _ = self.values.insert(key.clone(), value.clone());
                }
            }
            Effect::Idle { lock, off } => {
                self.idle.lock = *lock;
                self.idle.off = *off;
                if !self.waiting.iter().any(|(p, _)| *p == Page::Idle) {
                    self.waiting.push((Page::Idle, "hypridle".to_owned()));
                }
            }
            Effect::Wall { set, reset } => {
                for key in reset {
                    let _ = self.values.remove(key);
                }
                for (key, value) in set {
                    let _ = self.values.insert(key.clone(), value.clone());
                }
            }
            Effect::GameMode { .. } | Effect::Monitor(_) | Effect::NextLayout => {}
        }
    }
}

/// What a person calls a display: the laptop's own panel is built in,
/// another one external, and one alone the display.
pub fn display_title(m: &crate::backend::Monitor, count: usize) -> String {
    if ["eDP", "LVDS", "DSI"].iter().any(|p| m.name.starts_with(p)) {
        "Built-in display".to_owned()
    } else if count > 1 {
        "External display".to_owned()
    } else {
        "Display".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{Idle, accent_colours};
    use crate::backend::Monitor;
    use crate::pages::{Effect, Key, Kind, Page, State};

    fn row(state: &State, key: &Key) -> crate::pages::Line {
        state
            .rows()
            .into_iter()
            .find(|r| r.key == *key)
            .expect("the row is there")
    }

    #[test]
    fn appearance_sets_both_gaps_the_corners_and_the_borders_in_the_accent() {
        let mut state = State::new(Page::Appearance);
        let spacing = row(&state, &Key::Spacing);
        assert_eq!(spacing.options, ["Compact", "Comfortable", "Spacious"]);
        assert_eq!(spacing.current, 1, "hyprix's 5 and 20 are Comfortable");
        assert_eq!(
            state.pick(&spacing, 2),
            Some(Effect::Set(vec![
                ("general:gaps_in".to_owned(), "10".to_owned()),
                ("general:gaps_out".to_owned(), "32".to_owned())
            ]))
        );
        let accent = row(&state, &Key::Accent);
        let Some(Effect::Set(pairs)) = state.pick(&accent, 1) else {
            panic!("the accent sets nothing");
        };
        assert_eq!(pairs[0].0, "general:col.active_border");
        assert!(pairs[0].1.starts_with("rgb("));
        state.applied(&Effect::Set(pairs));
        assert_eq!(state.desktop_accent(), Some(1));
        assert_eq!(
            accent_colours(195.0)[0],
            state.value("general:col.active_border")
        );
        // See-through: the others fade and blur, and back.
        let see = row(&state, &Key::SeeThrough);
        assert!(!see.on);
        let effect = state.flip(&see).expect("a change");
        state.applied(&effect);
        assert!(row(&state, &Key::SeeThrough).on);
        assert_eq!(state.value("decoration:blur:enabled"), "1");
        // Advanced shows the knobs.
        state.advanced = true;
        assert!(
            state
                .rows()
                .iter()
                .any(|r| r.key == Key::Setting("general:border_size"))
        );
    }

    #[test]
    fn keyboard_layouts_are_a_comma_list_the_first_the_default_and_one_stays() {
        let mut state = State::new(Page::Keyboard);
        let de = row(&state, &Key::KbLayout("de(nodeadkeys)"));
        let effect = state.flip(&de).expect("ticked");
        assert_eq!(
            effect,
            Effect::Set(vec![
                ("input:kb_layout".to_owned(), "us,de".to_owned()),
                ("input:kb_variant".to_owned(), ",nodeadkeys".to_owned())
            ])
        );
        state.applied(&effect);
        assert_eq!(state.kb_layouts(), ["us", "de(nodeadkeys)"]);
        assert_eq!(row(&state, &Key::KbLayout("us")).tag, "Default");
        let us = row(&state, &Key::KbLayout("us"));
        let effect = state.flip(&us).expect("unticked");
        state.applied(&effect);
        assert_eq!(state.kb_layouts(), ["de(nodeadkeys)"]);
        let last = row(&state, &Key::KbLayout("de(nodeadkeys)"));
        assert_eq!(state.flip(&last), None, "the last layout stays");
        // Repeat: a slider, said with its unit, live (no note).
        let delay = row(&state, &Key::Setting("input:repeat_delay"));
        assert_eq!(delay.shown, "600 ms");
        assert_eq!(delay.note.text(), "");
        assert_eq!(
            state.slide(&delay, 450.0),
            Some(Effect::Set(vec![(
                "input:repeat_delay".to_owned(),
                "450".to_owned()
            )]))
        );
    }

    #[test]
    fn a_display_picked_and_its_scale_chosen_is_one_monitor_line() {
        let mut state = State::new(Page::Displays);
        state.monitors = vec![
            Monitor {
                name: "eDP-1".to_owned(),
                width: 2560,
                height: 1600,
                refresh: 60.0,
                scale: 1.5,
                modes: vec![
                    "2560x1600@60.00Hz".to_owned(),
                    "2560x1600@120.00Hz".to_owned(),
                    "1920x1200@60.00Hz".to_owned(),
                ],
                ..Monitor::default()
            },
            Monitor {
                name: "DP-1".to_owned(),
                width: 3840,
                height: 2160,
                refresh: 60.0,
                scale: 1.5,
                x: 1707,
                ..Monitor::default()
            },
        ];
        let rows = state.rows();
        assert_eq!(rows.first().map(|r| r.kind), Some(Kind::Monitors));
        let rates = row(&state, &Key::Monitor("hz"));
        assert_eq!(rates.options, ["60 Hz", "120 Hz"]);
        let scale = row(&state, &Key::Monitor("scale"));
        assert_eq!(scale.current, 2);
        assert_eq!(
            state.pick(&scale, 3),
            Some(Effect::Monitor(
                "eDP-1,2560x1600@60,0x0,2,transform,0".to_owned()
            ))
        );
        let arrangement = rows.first().cloned().unwrap();
        assert_eq!(state.pick(&arrangement, 1), None);
        assert_eq!(state.display, 1);
        assert!(
            row(&state, &Key::Monitor("res"))
                .options
                .contains(&"3840×2160".to_owned())
        );
    }

    #[test]
    fn hypridle_is_read_written_and_a_hand_written_one_left_alone() {
        let text = Idle::text(Some(5), Some(10));
        let idle = Idle::read(Some(&text));
        assert_eq!(
            (idle.lock, idle.off, idle.by_hand),
            (Some(5), Some(10), false)
        );
        let never = Idle::read(Some(&Idle::text(None, Some(30))));
        assert_eq!((never.lock, never.off), (None, Some(30)));
        let hand = Idle::read(Some(
            "listener {\n timeout = 120\n on-timeout = loginctl lock-session\n}\n",
        ));
        assert!(hand.by_hand);
        assert_eq!(hand.lock, Some(2));
        let mut state = State::new(Page::Idle);
        state.idle = Idle::read(Some(&text));
        let lock = row(&state, &Key::Idle("lock"));
        assert_eq!(lock.current, 2);
        assert_eq!(lock.note.text(), "Takes effect after you sign in again");
        let effect = state.pick(&lock, 5).expect("never");
        assert_eq!(
            effect,
            Effect::Idle {
                lock: None,
                off: Some(10)
            }
        );
        state.applied(&effect);
        assert_eq!(state.badge(Page::Idle), 1);
        state.idle = hand;
        let lock = row(&state, &Key::Idle("lock"));
        assert_eq!(state.pick(&lock, 0), None);
    }
}
