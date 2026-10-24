//! What backs the settings (Ferrix's `docs/RUNTIME-SETTINGS.md`): the
//! desktop's own tools, behind one trait so the window's logic is tested
//! without a desktop.
//!
//! * `hyprctl setting KEY VALUE` -- hyprix checks the value, writes the
//!   line into the user's `~/.config/hypr/settings.conf` (which the
//!   configuration `source`s) and applies it at once; it answers `ok` or
//!   why not, and changes nothing then. `hyprctl settings -j` lists what
//!   the file holds. Appearance, Windows, Keyboard, the unlock grace and
//!   the wallpaper go this way.
//! * `hyprctl monitors -j` and `hyprctl keyword monitor ...`: the
//!   displays, live, and kept by hyprix in `monitors.conf`.
//! * `hyprctl switchxkblayout all next`: the layout typed in, live.
//! * `~/.config/hypr/hypridle.conf`: when the screen locks and goes off,
//!   read by hypridle when the session starts.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

/// One display as hyprix reports it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Monitor {
    /// Its connector: `eDP-1`, `Virtual-1`.
    pub name: String,
    /// Its maker and model, if it says.
    pub model: String,
    /// Its mode's width now, in pixels.
    pub width: u32,
    /// Its mode's height now, in pixels.
    pub height: u32,
    /// Its refresh rate now, in hertz.
    pub refresh: f32,
    /// Its scale.
    pub scale: f32,
    /// Its rotation, Wayland's transform 0..3.
    pub transform: u32,
    /// Its left edge in the desktop, in pixels.
    pub x: i32,
    /// Its top edge in the desktop, in pixels.
    pub y: i32,
    /// The modes it offers, as `WxH@Hz`.
    pub modes: Vec<String>,
}

/// The desktop's tools, as Settings uses them.
pub trait System {
    /// What `settings.conf` holds: each key and its value.
    fn settings(&mut self) -> BTreeMap<String, String>;
    /// What hyprix has in force for each of `keys` -- the theme's, the
    /// person's configuration's or `settings.conf`'s, whichever won --
    /// as the pages compare it. Empty where hyprix cannot be asked.
    fn options(&mut self, _keys: &[&str]) -> BTreeMap<String, String> {
        BTreeMap::new()
    }
    /// Set `key` to `value`, kept and applied: `Err` with hyprix's words
    /// when it refuses.
    fn set(&mut self, key: &str, value: &str) -> Result<(), String>;
    /// Take `key`'s line out of `settings.conf`, back to the default.
    fn reset(&mut self, key: &str) -> Result<(), String>;
    /// The displays.
    fn monitors(&mut self) -> Vec<Monitor>;
    /// A `monitor` line, applied and kept in `monitors.conf`.
    fn monitor(&mut self, line: &str) -> Result<(), String>;
    /// Type in the next layout.
    fn next_layout(&mut self) -> Result<(), String>;
    /// The layout typed in now, as its name: `English (US)`.
    fn layout_now(&mut self) -> Option<String>;
    /// The file at `path`, if it can be read.
    fn read(&mut self, path: &std::path::Path) -> Option<String>;
    /// Write `text` to `path`, whole.
    fn write(&mut self, path: &std::path::Path, text: &str) -> Result<(), String>;
    /// Ask the settings writer for `request`: what was done, or why not.
    fn change(&mut self, request: &ferrix_settings_client::Request) -> Result<String, String>;
}

/// The person as the settings writer's relay would ask them: never. On a
/// desktop that is a person's, they confirm in the compositor's own
/// prompt, and nothing is asked of this window; where a question comes
/// here instead, this window does not take passwords.
#[derive(Debug)]
struct NoPerson;

impl ferrix_settings_client::Person for NoPerson {
    fn ask(&mut self, _visible: bool, _text: &str) -> Option<ferrix_settings_client::Secret> {
        None
    }

    fn tell(&mut self, _text: &str, _error: bool) {}
}

/// The real desktop: `hyprctl` and the user's files.
#[derive(Debug, Default)]
pub struct Desktop;

impl Desktop {
    /// Run `hyprctl` with `args`: its output, or why it failed.
    fn hyprctl(args: &[&str]) -> Result<String, String> {
        let out = Command::new("hyprctl")
            .args(args)
            .output()
            .map_err(|e| format!("hyprctl could not be started: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if out.status.success() {
            Ok(text)
        } else {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            Err(if err.is_empty() { text } else { err })
        }
    }

    /// `hyprctl` answering `ok` or its refusal.
    fn ok(args: &[&str]) -> Result<(), String> {
        let said = Self::hyprctl(args)?;
        if said == "ok" || said.is_empty() {
            Ok(())
        } else {
            Err(said)
        }
    }
}

/// The options the pages show, asked of hyprix as they are in force.
pub const OPTION_KEYS: &[&str] = &[
    "general:gaps_in",
    "general:gaps_out",
    "general:border_size",
    "general:layout",
    "decoration:rounding",
    "decoration:inactive_opacity",
    "decoration:blur:enabled",
    "decoration:shadow:enabled",
    "animations:enabled",
    "hyprix:titlebars",
    "hyprix:own_frames",
    "hyprix:blur_in_software",
    "input:follow_mouse",
    "input:repeat_rate",
    "input:repeat_delay",
    "misc:lock_grace",
];

/// `hyprctl --batch` of a `getoption` for each of `keys`: each answer is a
/// `TYPE: VALUE` line and a `set:` line, or `no such option`. The value as
/// the pages compare it: a number without trailing zeros, a gap's first
/// side, a word as it is.
#[must_use]
pub fn parse_options(keys: &[&str], text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    for key in keys {
        let Some(line) = lines.next() else { break };
        if line.trim() == "no such option" {
            continue;
        }
        let _set = lines.next();
        let Some((kind, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        let value = match kind.trim() {
            "custom type" => value.split_whitespace().next().unwrap_or("").to_owned(),
            "float" => {
                let trimmed = value.trim_end_matches('0').trim_end_matches('.');
                if trimmed.is_empty() {
                    "0".to_owned()
                } else {
                    trimmed.to_owned()
                }
            }
            _ => value.to_owned(),
        };
        let _ = out.insert((*key).to_owned(), value);
    }
    out
}

impl System for Desktop {
    fn options(&mut self, keys: &[&str]) -> BTreeMap<String, String> {
        let batch: Vec<String> = keys.iter().map(|key| format!("getoption {key}")).collect();
        match Self::hyprctl(&["--batch", &batch.join(" ; ")]) {
            Ok(text) => parse_options(keys, &text),
            Err(_) => BTreeMap::new(),
        }
    }

    fn settings(&mut self) -> BTreeMap<String, String> {
        let Ok(text) = Self::hyprctl(&["settings", "-j"]) else {
            return BTreeMap::new();
        };
        let mut out = BTreeMap::new();
        if let Some(Json::Array(items)) = Json::parse(&text) {
            for item in items {
                if let (Some(key), Some(value)) = (item.get("key"), item.get("value")) {
                    let _ = out.insert(key.text(), value.text());
                }
            }
        }
        out
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        Self::ok(&["setting", key, value])
    }

    fn reset(&mut self, key: &str) -> Result<(), String> {
        Self::ok(&["setting", "reset", key])
    }

    fn monitors(&mut self) -> Vec<Monitor> {
        let Ok(text) = Self::hyprctl(&["monitors", "-j"]) else {
            return Vec::new();
        };
        let Some(Json::Array(items)) = Json::parse(&text) else {
            return Vec::new();
        };
        items.iter().map(monitor_of).collect()
    }

    fn monitor(&mut self, line: &str) -> Result<(), String> {
        Self::ok(&["keyword", "monitor", line])
    }

    fn next_layout(&mut self) -> Result<(), String> {
        Self::ok(&["switchxkblayout", "all", "next"])
    }

    fn layout_now(&mut self) -> Option<String> {
        let text = Self::hyprctl(&["devices", "-j"]).ok()?;
        let json = Json::parse(&text)?;
        let keyboards = json.get("keyboards")?;
        let Json::Array(list) = keyboards else {
            return None;
        };
        list.iter()
            .find_map(|k| k.get("active_keymap").map(Json::text))
    }

    fn read(&mut self, path: &std::path::Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn change(&mut self, request: &ferrix_settings_client::Request) -> Result<String, String> {
        ferrix_settings_client::Client::new()
            .apply(request, &mut NoPerson)
            .map(|outcome| outcome.text)
            .map_err(|refusal| format!("{}: {}", refusal.kind.title(), refusal.text))
    }

    fn write(&mut self, path: &std::path::Path, text: &str) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let new = path.with_extension("new");
        std::fs::write(&new, text).map_err(|e| format!("{}: {e}", new.display()))?;
        std::fs::rename(&new, path).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// A monitor from `hyprctl monitors -j`'s object.
fn monitor_of(item: &Json) -> Monitor {
    let num = |key: &str| item.get(key).and_then(Json::number).unwrap_or(0.0);
    let modes = match item.get("availableModes") {
        Some(Json::Array(list)) => list.iter().map(Json::text).collect(),
        _ => Vec::new(),
    };
    let model = [item.get("make"), item.get("model")]
        .into_iter()
        .flatten()
        .map(Json::text)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Monitor {
        name: item.get("name").map(Json::text).unwrap_or_default(),
        model,
        width: num("width") as u32,
        height: num("height") as u32,
        refresh: num("refreshRate") as f32,
        scale: num("scale").max(0.1) as f32,
        transform: num("transform") as u32,
        x: num("x") as i32,
        y: num("y") as i32,
        modes,
    }
}

/// The user's hypridle file: `$XDG_CONFIG_HOME/hypr/hypridle.conf`.
#[must_use]
pub fn hypridle_path(var: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    var("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            var("HOME")
                .filter(|h| !h.is_empty())
                .map(|h| PathBuf::from(h).join(".config"))
        })
        .map(|dir| dir.join("hypr/hypridle.conf"))
}

/// A JSON value: enough of it for what hyprctl answers.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number.
    Number(f64),
    /// A string.
    Str(String),
    /// A list.
    Array(Vec<Json>),
    /// An object, its members in order.
    Object(Vec<(String, Json)>),
}

impl Json {
    /// `text` read whole, or `None` when it is not JSON.
    #[must_use]
    pub fn parse(text: &str) -> Option<Json> {
        let mut at = Reader {
            chars: text.chars().collect(),
            at: 0,
        };
        let value = at.value()?;
        at.space();
        (at.at == at.chars.len()).then_some(value)
    }

    /// An object's member.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The value as words: a string as it is, a number or a flag written.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Json::Str(s) => s.clone(),
            Json::Number(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
            Json::Number(n) => n.to_string(),
            Json::Bool(b) => b.to_string(),
            _ => String::new(),
        }
    }

    /// The value as a number, a string of one too.
    #[must_use]
    pub fn number(&self) -> Option<f64> {
        match self {
            Json::Number(n) => Some(*n),
            Json::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }
}

/// A reader over JSON text.
struct Reader {
    chars: Vec<char>,
    at: usize,
}

impl Reader {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
    }

    fn eat(&mut self, c: char) -> bool {
        self.space();
        if self.peek() == Some(c) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn word(&mut self, word: &str) -> bool {
        let end = self.at + word.chars().count();
        if self
            .chars
            .get(self.at..end)
            .is_some_and(|s| s.iter().copied().eq(word.chars()))
        {
            self.at = end;
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Option<Json> {
        self.space();
        match self.peek()? {
            '{' => {
                self.at += 1;
                let mut members = Vec::new();
                if self.eat('}') {
                    return Some(Json::Object(members));
                }
                loop {
                    self.space();
                    let Json::Str(key) = self.string()? else {
                        return None;
                    };
                    if !self.eat(':') {
                        return None;
                    }
                    members.push((key, self.value()?));
                    if self.eat(',') {
                        continue;
                    }
                    return self.eat('}').then_some(Json::Object(members));
                }
            }
            '[' => {
                self.at += 1;
                let mut items = Vec::new();
                if self.eat(']') {
                    return Some(Json::Array(items));
                }
                loop {
                    items.push(self.value()?);
                    if self.eat(',') {
                        continue;
                    }
                    return self.eat(']').then_some(Json::Array(items));
                }
            }
            '"' => self.string(),
            't' => self.word("true").then_some(Json::Bool(true)),
            'f' => self.word("false").then_some(Json::Bool(false)),
            'n' => self.word("null").then_some(Json::Null),
            _ => {
                let start = self.at;
                while self
                    .peek()
                    .is_some_and(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E'))
                {
                    self.at += 1;
                }
                let text: String = self.chars.get(start..self.at)?.iter().collect();
                text.parse().ok().map(Json::Number)
            }
        }
    }

    fn string(&mut self) -> Option<Json> {
        if self.peek() != Some('"') {
            return None;
        }
        self.at += 1;
        let mut out = String::new();
        loop {
            let c = self.peek()?;
            self.at += 1;
            match c {
                '"' => return Some(Json::Str(out)),
                '\\' => {
                    let e = self.peek()?;
                    self.at += 1;
                    match e {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        'u' => {
                            let hex: String =
                                self.chars.get(self.at..self.at + 4)?.iter().collect();
                            self.at += 4;
                            out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                        }
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
    }
}

/// A desktop for tests and headless drawings: settings kept in a map,
/// the displays given, every call remembered.
#[derive(Debug, Default)]
pub struct Fake {
    /// What `settings.conf` holds.
    pub kept: BTreeMap<String, String>,
    /// The displays.
    pub displays: Vec<Monitor>,
    /// The files, by path.
    pub files: BTreeMap<PathBuf, String>,
    /// Each call, as `hyprctl` would have been asked.
    pub calls: Vec<String>,
    /// Keys that are refused, with the words.
    pub refuse: BTreeMap<String, String>,
}

impl System for Fake {
    fn settings(&mut self) -> BTreeMap<String, String> {
        self.kept.clone()
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.calls.push(format!("setting {key} {value}"));
        if let Some(why) = self.refuse.get(key) {
            return Err(why.clone());
        }
        let _ = self.kept.insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    fn reset(&mut self, key: &str) -> Result<(), String> {
        self.calls.push(format!("setting reset {key}"));
        let _ = self.kept.remove(key);
        Ok(())
    }

    fn monitors(&mut self) -> Vec<Monitor> {
        self.displays.clone()
    }

    fn monitor(&mut self, line: &str) -> Result<(), String> {
        self.calls.push(format!("keyword monitor {line}"));
        Ok(())
    }

    fn next_layout(&mut self) -> Result<(), String> {
        self.calls.push("switchxkblayout all next".to_owned());
        Ok(())
    }

    fn layout_now(&mut self) -> Option<String> {
        Some("English (US)".to_owned())
    }

    fn read(&mut self, path: &std::path::Path) -> Option<String> {
        self.files.get(path).cloned()
    }

    fn change(&mut self, request: &ferrix_settings_client::Request) -> Result<String, String> {
        self.calls.push(format!("change {}", request.name()));
        match self.refuse.get(request.name()) {
            Some(why) => Err(why.clone()),
            None => Ok(format!("{} done", request.name())),
        }
    }

    fn write(&mut self, path: &std::path::Path, text: &str) -> Result<(), String> {
        let _ = self.files.insert(path.to_path_buf(), text.to_owned());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Json, monitor_of};

    #[test]
    fn json_as_hyprctl_answers_it() {
        let text = r#"[{"key":"general:gaps_in","name":"gaps","value":"5"},
            {"key":"x","value":3.5,"on":true,"none":null,"s":"a\"bé"}]"#;
        let Some(Json::Array(items)) = Json::parse(text) else {
            panic!("not read");
        };
        assert_eq!(items[0].get("value").map(Json::text).as_deref(), Some("5"));
        assert_eq!(items[1].get("value").and_then(Json::number), Some(3.5));
        assert_eq!(items[1].get("s").map(Json::text).as_deref(), Some("a\"bé"));
        assert_eq!(Json::parse("[1,"), None);
    }

    #[test]
    fn a_monitor_from_hyprctl() {
        let text = r#"{"name":"Virtual-1","make":"QEMU","model":"virtio","width":1280,"height":800,
            "refreshRate":60.000,"x":0,"y":0,"scale":1.50,"transform":0,
            "availableModes":["1280x800@60.00Hz","1920x1080@60.00Hz"]}"#;
        let m = monitor_of(&Json::parse(text).unwrap());
        assert_eq!(
            (m.name.as_str(), m.width, m.height),
            ("Virtual-1", 1280, 800)
        );
        assert_eq!(m.model, "QEMU virtio");
        assert!((m.scale - 1.5).abs() < 0.001);
        assert_eq!(m.modes.len(), 2);
    }
}
