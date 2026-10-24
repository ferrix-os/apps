//! The System pages the settings writer backs (Ferrix's
//! `docs/SETTINGS-WRITER.md` §4, §12, §13): Name and time and Remote login.
//!
//! * **Name and time.** The computer's name (`set-hostname`, kept in
//!   `/etc/hostname` and set at every boot) and the time zone
//!   (`set-timezone`); the date and time are shown, and setting them is
//!   the writer's later half (`set-time`, parked).
//! * **Remote login.** `sshd.service` on or off for every boot, started or
//!   stopped at once (`service-enable-persistent ... now`).
//!
//! Each change is a typed request to `settingsd`; on a desktop that is a
//! person's, the person confirms it in the trusted prompt the compositor
//! draws, so this window asks for no password itself. Where the image
//! manifest says this build fixes them -- or says nothing readable -- the
//! pages show their values read-only, "Fixed by this build (profile max)",
//! in place of their buttons (ledger 3558 (14)).

use ferrix_settings_client::Request;
use ferrix_settings_client::manifest::Manifest;

use crate::pages::{Effect, Key, Kind, Line, State};

/// The unit Remote login switches.
pub const SSHD: &str = "sshd.service";

/// Where an enabled unit's links are, under `/etc/ferrix/units`.
const WANTS: [&str; 2] = [
    "/etc/ferrix/units/multi-user.target.wants/sshd.service",
    "/etc/ferrix/units/graphical.target.wants/sshd.service",
];

/// What the System pages show and what was typed into them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Machine {
    /// The line in place of the buttons when this build fixes the pages.
    pub fixed: Option<String>,
    /// The time zone `/etc/localtime` names.
    pub zone: String,
    /// The clock, as UTC `YYYY-MM-DD HH:MM`, when read.
    pub clock: String,
    /// Whether remote login starts at boot.
    pub remote: bool,
    /// The name typed, before Rename.
    pub name_typed: Option<String>,
    /// The zone typed, before Set.
    pub zone_typed: Option<String>,
}

impl Machine {
    /// The machine as it is, with `manifest` saying what the build allows.
    #[must_use]
    pub fn read(manifest: &Manifest) -> Machine {
        let zone = std::fs::read_link("/etc/localtime")
            .ok()
            .and_then(|target| {
                target
                    .strip_prefix("/usr/share/zoneinfo")
                    .ok()
                    .map(|zone| zone.display().to_string())
            })
            .unwrap_or_else(|| "UTC".to_owned());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|since| i64::try_from(since.as_secs()).ok())
            .unwrap_or(0);
        Machine {
            fixed: manifest.fixed().then(|| manifest.fixed_text()),
            zone,
            clock: utc(now),
            remote: WANTS
                .iter()
                .any(|path| std::fs::symlink_metadata(path).is_ok()),
            ..Machine::default()
        }
    }
}

/// `seconds` since the epoch as UTC `YYYY-MM-DD HH:MM`.
#[must_use]
pub fn utc(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let of_day = seconds.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        of_day / 3600,
        of_day % 3600 / 60
    )
}

/// A field with a button: Rename, Set.
fn field(title: &str, shown: &str, button: &str, key: Key, fixed: Option<&str>) -> Line {
    let mut row = Line::new(Kind::Name, title).key(key);
    shown.clone_into(&mut row.shown);
    button.clone_into(&mut row.tag);
    if let Some(fixed) = fixed {
        row.desc = fixed.to_owned();
        row.enabled = false;
    }
    row
}

impl State {
    /// Name and time: the name, the clock and the time zone, then About.
    #[must_use]
    pub fn computer_rows(&self) -> Vec<Line> {
        let fixed = self.machine.fixed.as_deref();
        let mut name = field(
            "Computer name",
            self.machine.name_typed.as_deref().unwrap_or(&self.hostname),
            "Rename",
            Key::HostName,
            fixed,
        );
        if fixed.is_none() {
            name.desc = "Other computers on your network see it by this name.".to_owned();
        }
        // Setting the clock is the writer's later half (B3's set-time,
        // parked): shown here, not changed yet.
        let mut clock = field(
            "Date and time (UTC)",
            &self.machine.clock,
            "Set",
            Key::None,
            None,
        )
        .note(crate::pages::Note::NotKept);
        clock.enabled = false;
        let zone = field(
            "Time zone",
            self.machine
                .zone_typed
                .as_deref()
                .unwrap_or(&self.machine.zone),
            "Set",
            Key::Zone,
            fixed,
        );
        let mut about = Line::new(Kind::Info, "");
        for (label, value) in &self.about {
            about.options.push(label.clone());
            about.values.push(value.clone());
        }
        let mut rows = vec![name, clock, zone, Line::new(Kind::Section, "About")];
        if !about.options.is_empty() {
            rows.push(about);
        }
        rows
    }

    /// Remote login: the switch, started or stopped at once.
    #[must_use]
    pub fn remote_rows(&self) -> Vec<Line> {
        let mut row = Line::new(Kind::Switch, "Allow remote login").key(Key::Remote);
        row.on = self.machine.remote;
        match &self.machine.fixed {
            Some(fixed) => {
                row.desc.clone_from(fixed);
                row.enabled = false;
            }
            None => {
                "Anyone on your network who knows a user's password can sign in as that user."
                    .clone_into(&mut row.desc);
            }
        }
        vec![row]
    }

    /// Text typed into one of these pages' fields.
    pub fn machine_edited(&mut self, row: &Line, text: &str) {
        let slot = match row.key {
            Key::HostName => &mut self.machine.name_typed,
            Key::Zone => &mut self.machine.zone_typed,
            _ => return,
        };
        *slot = Some(text.to_owned());
    }

    /// A button of these pages pressed: the request it makes.
    #[must_use]
    pub fn machine_press(&self, row: &Line) -> Option<Effect> {
        if self.machine.fixed.is_some() {
            return None;
        }
        let shown = || row.shown.trim().to_owned();
        let request = match row.key {
            Key::HostName => Request::SetHostname {
                name: self
                    .machine
                    .name_typed
                    .clone()
                    .unwrap_or_else(shown)
                    .trim()
                    .to_owned(),
            },
            Key::Zone => Request::SetTimezone {
                zone: self
                    .machine
                    .zone_typed
                    .clone()
                    .unwrap_or_else(shown)
                    .trim()
                    .to_owned(),
            },
            _ => return None,
        };
        Some(Effect::Change(request))
    }

    /// Remote login's switch turned.
    #[must_use]
    pub fn machine_flip(&self, row: &Line) -> Option<Effect> {
        if row.key != Key::Remote || self.machine.fixed.is_some() {
            return None;
        }
        Some(Effect::Change(Request::ServiceEnablePersistent {
            unit: SSHD.to_owned(),
            enabled: !row.on,
            now: true,
        }))
    }

    /// A change made: what it changed, kept here.
    pub fn machine_changed(&mut self, request: &Request) {
        match request {
            Request::SetHostname { name } => {
                name.clone_into(&mut self.hostname);
                self.machine.name_typed = None;
            }
            Request::SetTimezone { zone } => {
                zone.clone_into(&mut self.machine.zone);
                self.machine.zone_typed = None;
            }
            Request::ServiceEnablePersistent { enabled, .. } => self.machine.remote = *enabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Machine;
    use crate::pages::{Effect, Key, Page, State};
    use ferrix_settings_client::Request;

    fn state(fixed: Option<&str>) -> State {
        let mut state = State::new(Page::Computer);
        state.hostname = "ferrix".to_owned();
        state.machine = Machine {
            fixed: fixed.map(str::to_owned),
            zone: "UTC".to_owned(),
            clock: "2026-10-24 12:00".to_owned(),
            remote: false,
            ..Machine::default()
        };
        state
    }

    #[test]
    fn rename_asks_for_the_name_typed_and_no_row_says_not_kept() {
        let mut state = state(None);
        let rows = state.computer_rows();
        let name = rows
            .iter()
            .find(|row| row.key == Key::HostName)
            .unwrap()
            .clone();
        assert!(name.enabled);
        // The name and the zone are kept now; only the clock is not yet.
        for row in rows.iter().filter(|row| row.key != Key::None) {
            assert_ne!(row.note.text(), "Not kept yet", "{row:?}");
        }
        state.machine_edited(&name, "box");
        assert_eq!(
            state.machine_press(&name),
            Some(Effect::Change(Request::SetHostname {
                name: "box".to_owned()
            }))
        );
        state.machine_changed(&Request::SetHostname {
            name: "box".to_owned(),
        });
        assert_eq!(state.hostname, "box");
    }

    #[test]
    fn remote_login_switches_sshd_now() {
        let state = state(None);
        let row = state.remote_rows().remove(0);
        assert_eq!(
            state.machine_flip(&row),
            Some(Effect::Change(Request::ServiceEnablePersistent {
                unit: "sshd.service".to_owned(),
                enabled: true,
                now: true
            }))
        );
    }

    #[test]
    fn a_fixed_build_shows_its_values_read_only() {
        let state = state(Some("Fixed by this build (profile max)"));
        for row in state
            .computer_rows()
            .iter()
            .filter(|row| row.key != Key::None)
        {
            assert!(!row.enabled, "{row:?}");
            assert_eq!(row.desc, "Fixed by this build (profile max)");
            assert_eq!(state.machine_press(row), None);
        }
        let remote = state.remote_rows().remove(0);
        assert!(!remote.enabled);
        assert_eq!(state.machine_flip(&remote), None);
    }
}
