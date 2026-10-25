//! The sidebar's quick settings (Ferrix's `docs/DESKTOP.md` §6.1): what the
//! design puts there, each on what this machine has.
//!
//! * Focus mode is `decoration:dim_inactive` for the session, as the dock's
//!   button sets it (`hyprctl keyword`).
//! * Auto-tile is `general:layout`, `dwindle` against Yoru's `floating`,
//!   and the theme colour the focused border, the others' and the shadow,
//!   in one of Settings' accents: kept as Settings keeps them, by
//!   `hyprctl setting`, so Settings shows the same.
//! * Do not disturb is notifyd's file (`quiet_file`): while it is there a
//!   notification goes to the list without a banner, unless it is critical.
//! * Brightness is the first backlight under `/sys/class/backlight`, where
//!   there is one.
//! * Volume and the microphone, Wi-Fi, Bluetooth, night light and a light
//!   appearance say why they are not here: no sound server is asked yet,
//!   no wireless device or gamma is there, and Nocturne is dark only.

use std::path::{Path, PathBuf};

use crate::hypr::Hypr;
use crate::json::Json;

/// Settings' accents (Settings' `look::ACCENTS`): a name and an OKLCH hue.
pub const ACCENTS: [(&str, f32); 7] = [
    ("Blue", 250.0),
    ("Teal", 195.0),
    ("Green", 150.0),
    ("Amber", 80.0),
    ("Orange", 45.0),
    ("Rose", 10.0),
    ("Violet", 300.0),
];

/// An OKLCH colour as sRGB bytes, clipped into the gamut (as Settings').
#[must_use]
pub fn oklch(l: f32, c: f32, h: f32) -> [u8; 3] {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    let r = 4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3;
    let g = -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3;
    let bl = -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3;
    let encode = |x: f32| {
        let x = x.clamp(0.0, 1.0);
        let v = if x <= 0.003_130_8 {
            12.92 * x
        } else {
            1.055 * x.powf(1.0 / 2.4) - 0.055
        };
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    [encode(r), encode(g), encode(bl)]
}

/// The colours an accent gives the desktop, as Settings writes them: the
/// focused border, the others, the shadow.
#[must_use]
pub fn accent_colours(hue: f32) -> [String; 3] {
    let hex = |l: f32, c: f32| {
        let [r, g, b] = oklch(l, c, hue);
        format!("rgb({r:02x}{g:02x}{b:02x})")
    };
    [hex(0.72, 0.13), hex(0.36, 0.03), hex(0.12, 0.03)]
}

/// A backlight: its folder, its brightness and the most it takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backlight {
    /// `/sys/class/backlight/NAME`.
    pub dir: PathBuf,
    /// `brightness`.
    pub now: u32,
    /// `max_brightness`.
    pub max: u32,
}

impl Backlight {
    /// Its brightness as a percentage.
    #[must_use]
    pub fn percent(&self) -> u32 {
        if self.max == 0 {
            0
        } else {
            (u64::from(self.now) * 100 / u64::from(self.max))
                .try_into()
                .unwrap_or(100)
        }
    }

    /// The value written for `percent`, never quite dark.
    #[must_use]
    pub fn value_of(&self, percent: u32) -> u32 {
        let wanted = u64::from(percent.clamp(1, 100)) * u64::from(self.max) / 100;
        u32::try_from(wanted).unwrap_or(self.max).max(1)
    }
}

/// The first backlight under `root` (`/sys/class/backlight`).
#[must_use]
pub fn backlight(root: &Path) -> Option<Backlight> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    dirs.sort();
    dirs.into_iter().find_map(|dir| {
        let read = |name: &str| {
            std::fs::read_to_string(dir.join(name))
                .ok()
                .and_then(|t| t.trim().parse::<u32>().ok())
        };
        Some(Backlight {
            now: read("brightness")?,
            max: read("max_brightness").filter(|m| *m > 0)?,
            dir: dir.clone(),
        })
    })
}

/// Whether a wireless network device is under `net` (`/sys/class/net`).
#[must_use]
pub fn has_wireless(net: &Path) -> bool {
    std::fs::read_dir(net).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|e| e.path().join("wireless").exists() || e.path().join("phy80211").exists())
    })
}

/// notifyd's do-not-disturb file: `$XDG_CONFIG_HOME/notifyd/do-not-disturb`,
/// else under `~/.config`.
#[must_use]
pub fn quiet_file() -> Option<PathBuf> {
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
        .map(|d| d.join("notifyd/do-not-disturb"))
}

/// What the quick settings show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Quick {
    /// Focus mode: the other windows dimmed.
    pub focus: bool,
    /// Auto-tile: windows tiled rather than floating.
    pub tiled: bool,
    /// The accent the focused border is in, if it is one of Settings'.
    pub accent: Option<usize>,
    /// Do not disturb.
    pub quiet: bool,
    /// The backlight, if there is one.
    pub backlight: Option<Backlight>,
    /// Whether a wireless device is there.
    pub wireless: bool,
    /// What the last change did, or why not.
    pub said: String,
}

/// A press in the quick settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// Focus mode on or off.
    Focus,
    /// Auto-tile on or off.
    Tile,
    /// Do not disturb on or off.
    Quiet,
    /// The theme colour: accent `n`.
    Accent(usize),
    /// The brightness, a percentage.
    Brightness(u32),
}

/// The value of a `getoption` answer (`int: 1`, then `set: true`): `1`.
#[must_use]
pub fn option_value(answer: &str) -> Option<String> {
    let line = answer.lines().find(|l| !l.trim().is_empty())?;
    if line.trim() == "no such option" {
        return None;
    }
    let (kind, value) = line.split_once(':')?;
    let value = value.trim();
    Some(if kind.trim() == "custom type" {
        value.split_whitespace().next().unwrap_or("").to_owned()
    } else {
        value.to_owned()
    })
}

/// The accent of `settings` (`j/settings`: the kept keys and values), if
/// its focused border is one of Settings' accents.
#[must_use]
pub fn accent_of(settings: &Json) -> Option<usize> {
    let Json::Array(items) = settings else {
        return None;
    };
    let border = items.iter().find_map(|item| {
        (item.get("key").and_then(Json::str) == Some("general:col.active_border"))
            .then(|| item.get("value").and_then(Json::str).map(str::to_owned))
            .flatten()
    })?;
    ACCENTS
        .iter()
        .position(|(_, hue)| accent_colours(*hue)[0] == border)
}

/// What the compositor is asked for `change` from `quick`: its requests.
#[must_use]
pub fn requests(quick: &Quick, change: &Change) -> Vec<String> {
    match change {
        Change::Focus => vec![format!(
            "keyword decoration:dim_inactive {}",
            i32::from(!quick.focus)
        )],
        Change::Tile => vec![format!(
            "setting general:layout {}",
            if quick.tiled { "floating" } else { "dwindle" }
        )],
        Change::Accent(n) => ACCENTS.get(*n).map_or_else(Vec::new, |(_, hue)| {
            let [active, inactive, shadow] = accent_colours(*hue);
            vec![
                format!("setting general:col.active_border {active}"),
                format!("setting general:col.inactive_border {inactive}"),
                format!("setting decoration:shadow:color {shadow}"),
            ]
        }),
        Change::Quiet | Change::Brightness(_) => Vec::new(),
    }
}

/// Read what is in force: the compositor's options, notifyd's file, the
/// machine's devices.
#[must_use]
pub fn read(hypr: Option<&Hypr>) -> Quick {
    let option = |key: &str| {
        hypr.and_then(|h| h.ask(&format!("getoption {key}")))
            .and_then(|answer| option_value(&answer))
    };
    Quick {
        focus: option("decoration:dim_inactive").is_some_and(|v| v != "0"),
        tiled: option("general:layout").is_some_and(|v| v != "floating"),
        accent: hypr
            .and_then(|h| h.ask("j/settings"))
            .and_then(|text| Json::parse(&text))
            .and_then(|json| accent_of(&json)),
        quiet: quiet_file().is_some_and(|f| f.exists()),
        backlight: backlight(Path::new("/sys/class/backlight")),
        wireless: has_wireless(Path::new("/sys/class/net")),
        said: String::new(),
    }
}

/// Make `change`: ask the compositor, write notifyd's file or the
/// backlight. What was done, or why not, for the shell's line.
pub fn change(hypr: Option<&Hypr>, quick: &Quick, change: &Change) -> String {
    match change {
        Change::Quiet => {
            let Some(file) = quiet_file() else {
                return "do not disturb: no $HOME".to_owned();
            };
            let done = if quick.quiet {
                std::fs::remove_file(&file).or_else(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        Ok(())
                    } else {
                        Err(e)
                    }
                })
            } else {
                file.parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(&file, ""))
            };
            match done {
                Ok(()) => format!("do not disturb {}", if quick.quiet { "off" } else { "on" }),
                Err(e) => format!("do not disturb: {}: {e}", file.display()),
            }
        }
        Change::Brightness(percent) => {
            let Some(light) = &quick.backlight else {
                return "brightness: no backlight".to_owned();
            };
            let value = light.value_of(*percent);
            match std::fs::write(light.dir.join("brightness"), value.to_string()) {
                Ok(()) => format!("brightness {percent}%"),
                Err(e) => format!("brightness: {e}"),
            }
        }
        _ => {
            let asked = requests(quick, change);
            let Some(hypr) = hypr else {
                return format!("{change:?}: no compositor");
            };
            for request in &asked {
                let said = hypr.ask(request).unwrap_or_default();
                if said.trim() != "ok" {
                    return format!("{request}: {}", said.trim());
                }
            }
            format!("{change:?}: ok")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ACCENTS, Backlight, Change, Quick, accent_colours, accent_of, backlight, option_value,
        requests,
    };
    use crate::json::Json;

    #[test]
    fn an_options_answer_is_its_value() {
        assert_eq!(option_value("int: 1\nset: true\n").as_deref(), Some("1"));
        assert_eq!(
            option_value("str: floating\nset: true").as_deref(),
            Some("floating")
        );
        assert_eq!(
            option_value("custom type: ff796cbf 0deg\nset: true").as_deref(),
            Some("ff796cbf")
        );
        assert_eq!(option_value("no such option"), None);
    }

    /// The accents are Settings': Blue's focused border is what Settings
    /// writes for it.
    #[test]
    fn the_accent_is_found_by_the_border_settings_keeps() {
        let blue = accent_colours(ACCENTS[0].1);
        assert!(
            blue[0].starts_with("rgb(") && blue[0].len() == 11,
            "{blue:?}"
        );
        let teal = &accent_colours(195.0)[0];
        let kept = Json::parse(&format!(
            "[{{\"key\": \"general:gaps_in\", \"value\": \"5\"}}, \
             {{\"key\": \"general:col.active_border\", \"value\": \"{teal}\"}}]"
        ))
        .expect("json");
        assert_eq!(accent_of(&kept), Some(1));
        let own = Json::parse(
            "[{\"key\": \"general:col.active_border\", \"value\": \"rgba(796cbfff)\"}]",
        )
        .expect("json");
        assert_eq!(accent_of(&own), None);
    }

    #[test]
    fn each_press_asks_the_compositor_what_the_design_says() {
        let off = Quick::default();
        assert_eq!(
            requests(&off, &Change::Focus),
            ["keyword decoration:dim_inactive 1"]
        );
        assert_eq!(
            requests(&off, &Change::Tile),
            ["setting general:layout dwindle"]
        );
        let on = Quick {
            focus: true,
            tiled: true,
            ..Quick::default()
        };
        assert_eq!(
            requests(&on, &Change::Focus),
            ["keyword decoration:dim_inactive 0"]
        );
        assert_eq!(
            requests(&on, &Change::Tile),
            ["setting general:layout floating"]
        );
        let accent = requests(&off, &Change::Accent(1));
        assert_eq!(accent.len(), 3);
        assert!(accent[0].starts_with("setting general:col.active_border rgb("));
        assert!(accent[2].starts_with("setting decoration:shadow:color rgb("));
        assert!(requests(&off, &Change::Accent(99)).is_empty());
        assert!(requests(&off, &Change::Quiet).is_empty());
    }

    #[test]
    fn a_backlight_is_read_and_never_set_dark() {
        let dir = std::env::temp_dir().join(format!("yoru-quick-{}", std::process::id()));
        let light = dir.join("intel_backlight");
        std::fs::create_dir_all(&light).expect("made");
        std::fs::write(light.join("brightness"), "300\n").expect("written");
        std::fs::write(light.join("max_brightness"), "1200\n").expect("written");
        let found = backlight(&dir).expect("found");
        assert_eq!(found.percent(), 25);
        assert_eq!(found.value_of(50), 600);
        assert_eq!(found.value_of(0), 12);
        let none = Backlight {
            dir: light,
            now: 0,
            max: 0,
        };
        assert_eq!(none.percent(), 0);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(backlight(&dir), None);
    }
}
