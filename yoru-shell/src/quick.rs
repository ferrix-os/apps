//! The sidebar's quick settings (Ferrix's `docs/DESKTOP.md` §6.1): what the
//! design puts there, each on what this machine has.
//!
//! * Focus mode is `decoration:dim_inactive` for the session, as the dock's
//!   button sets it (`hyprctl keyword`).
//! * Auto-tile is `general:layout`, `dwindle` against Yoru's `floating`,
//!   kept as Settings keeps it, by `hyprctl setting`.
//! * The theme colour and the appearance are the theme engine's choice
//!   (`crate::theme`), its file written; every shell program redraws in
//!   it, and the dock hands the compositor the frames' colours.
//! * Do not disturb is notifyd's file (`quiet_file`): while it is there a
//!   notification goes to the list without a banner, unless it is critical.
//! * Brightness is the first backlight under `/sys/class/backlight`, where
//!   there is one.
//! * Volume and the microphone are the sound server's default sink and
//!   source (`crate::sound`, the PulseAudio protocol `pulsed` speaks): a
//!   slider sets the volume, the icon mutes. Without a server, or with no
//!   source (`pulsed` has none: there is no capture yet), they say so.
//! * Wi-Fi, Bluetooth and night light say why they are not here: Ferrix
//!   has no wireless or Bluetooth stack, and the compositor no gamma.

use std::path::{Path, PathBuf};

use crate::hypr::Hypr;

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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quick {
    /// Focus mode: the other windows dimmed.
    pub focus: bool,
    /// Auto-tile: windows tiled rather than floating.
    pub tiled: bool,
    /// The theme chosen.
    pub theme: crate::theme::Theme,
    /// Do not disturb.
    pub quiet: bool,
    /// The backlight, if there is one.
    pub backlight: Option<Backlight>,
    /// Whether a wireless device is there.
    pub wireless: bool,
    /// The sound server's sink: the volume.
    pub sink: Option<crate::sound::Device>,
    /// Its source: the microphone.
    pub source: Option<crate::sound::Device>,
    /// Why there is no sink or source to show: no server, or none listed.
    pub sound: Option<String>,
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
    /// The theme colour: `theme::Mode::all()`'s `n`.
    Theme(usize),
    /// The appearance: `theme::Appearance::ALL`'s `n`.
    Appearance(usize),
    /// The brightness, a percentage.
    Brightness(u32),
    /// The volume, a percentage.
    Volume(u32),
    /// The microphone's volume, a percentage.
    Mic(u32),
    /// The volume muted or not.
    Mute,
    /// The microphone muted or not.
    MicMute,
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
        _ => Vec::new(),
    }
}

/// Read what is in force: the compositor's options, notifyd's file, the
/// machine's devices.
#[must_use]
pub fn read(hypr: Option<&Hypr>) -> Quick {
    let (sink, source, sound) = match crate::sound::devices() {
        Ok((sink, source)) => (sink, source, None),
        Err(why) => (None, None, Some(why)),
    };
    let option = |key: &str| {
        hypr.and_then(|h| h.ask(&format!("getoption {key}")))
            .and_then(|answer| option_value(&answer))
    };
    Quick {
        focus: option("decoration:dim_inactive").is_some_and(|v| v != "0"),
        tiled: option("general:layout").is_some_and(|v| v != "floating"),
        theme: crate::theme::theme_file()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map_or_else(crate::theme::Theme::default, |text| {
                crate::theme::Theme::read(&text)
            }),
        quiet: quiet_file().is_some_and(|f| f.exists()),
        backlight: backlight(Path::new("/sys/class/backlight")),
        wireless: has_wireless(Path::new("/sys/class/net")),
        sink,
        source,
        sound,
        said: String::new(),
    }
}

/// The device `change` is for, and its name.
fn sound_of<'a>(
    quick: &'a Quick,
    change: &Change,
) -> (
    crate::sound::Which,
    Option<&'a crate::sound::Device>,
    &'static str,
) {
    if matches!(change, Change::Mic(_) | Change::MicMute) {
        (
            crate::sound::Which::Source,
            quick.source.as_ref(),
            "microphone",
        )
    } else {
        (crate::sound::Which::Sink, quick.sink.as_ref(), "volume")
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
        Change::Theme(n) | Change::Appearance(n) => {
            let mut theme = quick.theme;
            let named = if let Change::Theme(_) = change {
                let Some(mode) = crate::theme::Mode::all().get(*n).copied() else {
                    return "theme: no such theme".to_owned();
                };
                theme.mode = mode;
                format!("theme {}", mode.name())
            } else {
                let Some(look) = crate::theme::Appearance::ALL.get(*n).copied() else {
                    return "appearance: no such appearance".to_owned();
                };
                theme.appearance = look;
                format!("appearance {}", look.name())
            };
            let Some(file) = crate::theme::theme_file() else {
                return format!("{named}: no $HOME");
            };
            crate::widgets::keep(&file, &theme.text());
            named
        }
        Change::Volume(percent) | Change::Mic(percent) => {
            let (which, device, name) = sound_of(quick, change);
            let Some(device) = device else {
                return format!("{name}: none");
            };
            match crate::sound::set_volume(which, device, *percent) {
                Ok(()) => format!("{name} {percent}%"),
                Err(e) => format!("{name}: {e}"),
            }
        }
        Change::Mute | Change::MicMute => {
            let (which, device, name) = sound_of(quick, change);
            let Some(device) = device else {
                return format!("{name}: none");
            };
            match crate::sound::set_mute(which, device, !device.muted) {
                Ok(()) => format!("{name} {}", if device.muted { "unmuted" } else { "muted" }),
                Err(e) => format!("{name}: {e}"),
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
    use super::{Backlight, Change, Quick, backlight, option_value, requests};

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
        assert!(
            requests(&off, &Change::Theme(1)).is_empty(),
            "a file, not a request"
        );
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
