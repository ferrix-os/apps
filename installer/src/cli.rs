//! The command line.
//!
//! ```text
//! ferrix-installer                   open the window
//! ferrix-installer --autostart       open it only on the live system
//! ferrix-installer --screenshot DIR  draw every page into DIR, no window
//! ferrix-installer --version
//! ferrix-installer --help
//! ```
//!
//! `--autostart` is what the live session's hyprland config runs at
//! sign-in: it reads `/proc/cmdline` and leaves at once, with success,
//! unless the kernel was started with the word `ferrix.live`.
//! `--screenshot` draws each page headless against a stand-in daemon on a
//! socket of its own (`src/fake.rs`), for the README's pictures.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::fake::{Behaviour, Fake};
use crate::pages::{Key, Page, State};
use crate::protocol;
use crate::window::say;

/// What `--help` prints.
const USAGE: &str = "\
usage: ferrix-installer [--autostart]
       ferrix-installer --screenshot DIR
       ferrix-installer --version | --help

  --autostart       open only when booted live (ferrix.live on the kernel's
                    command line); otherwise exit at once
  --screenshot DIR  draw every page into DIR as PNGs, with a stand-in daemon,
                    and exit
  --version         print the version, and exit
  --help            print this, and exit
";

/// What the arguments ask for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asked {
    /// The window.
    Window,
    /// The window, only when live.
    Autostart,
    /// Every page drawn into a folder.
    Screenshot(PathBuf),
    /// `--version`.
    Version,
    /// `--help`.
    Help,
    /// Something else, refused in these words.
    Refused(String),
}

/// Read the arguments after the program's name.
#[must_use]
pub fn parse(arguments: &[String]) -> Asked {
    let mut words = arguments.iter();
    let Some(first) = words.next() else {
        return Asked::Window;
    };
    let asked = match first.as_str() {
        "--autostart" => Asked::Autostart,
        "--version" | "-V" => Asked::Version,
        "--help" | "-h" => Asked::Help,
        "--screenshot" => match words.next() {
            Some(dir) => Asked::Screenshot(PathBuf::from(dir)),
            None => return Asked::Refused("--screenshot needs a folder".to_owned()),
        },
        other => return Asked::Refused(format!("{other} is not an option ferrix-installer has")),
    };
    match words.next() {
        Some(more) => Asked::Refused(format!("{more}: one option at a time")),
        None => asked,
    }
}

/// Whether the kernel's command line `cmdline` says the system is live:
/// the word `ferrix.live`, alone or with a value.
#[must_use]
pub fn is_live(cmdline: &str) -> bool {
    cmdline
        .split_whitespace()
        .any(|word| word == "ferrix.live" || word.starts_with("ferrix.live="))
}

/// A page to draw: its file's name, and how the state is made from the
/// state with the stand-in's disks.
type Shot = (&'static str, fn(&mut State));

/// The pictures `--screenshot` draws, in order.
const SHOTS: [Shot; 12] = [
    ("01-welcome", |_| {}),
    ("02-keyboard", |s| {
        s.page = Page::Keyboard;
        s.layout = "de".to_owned();
        s.variant = "nodeadkeys".to_owned();
        s.tried = "Grüße aus Wien".to_owned();
    }),
    ("03-disk-erase", |s| s.page = Page::Disk),
    ("04-disk-partition", |s| {
        s.page = Page::Disk;
        let _ = s.press(&Key::Disk("/dev/nvme0n1".to_owned()), 0);
        let _ = s.press(&Key::Region(1_050_624), 0);
    }),
    ("05-disk-free-space", |s| {
        s.page = Page::Disk;
        let _ = s.press(&Key::Disk("/dev/nvme0n1".to_owned()), 0);
    }),
    ("06-disk-none", |s| {
        s.page = Page::Disk;
        s.set_disks(protocol::parse_disks(
            "min 8589934592\nlive /dev/sda\ndisk /dev/sda 15523119104 mbr SanDisk Ultra Fit\nok\n",
        ));
    }),
    ("07-you", |s| {
        s.page = Page::You;
        fill_you(s);
        s.edited(&Key::Password, "ada1815");
        s.edited(&Key::Confirm, "ada1815");
    }),
    ("08-time-zone", |s| {
        s.page = Page::Zone;
        s.edited(&Key::ZoneSearch, "europe");
        s.zone = "Europe/Vienna".to_owned();
    }),
    ("09-summary", |s| {
        let _ = s.press(&Key::Disk("/dev/nvme0n1".to_owned()), 0);
        let _ = s.press(&Key::Region(1_050_624), 0);
        fill_you(s);
        s.layout = "de".to_owned();
        s.zone = "Europe/Vienna".to_owned();
        s.page = Page::Summary;
    }),
    ("10-installing", |s| {
        s.page = Page::Installing;
        s.details = true;
        for (at, step) in crate::fake::STEPS.iter().take(3).enumerate() {
            let n = u32::try_from(at + 1).unwrap_or(0);
            s.event(protocol::Event::Step(n, 7, (*step).to_owned()));
            s.event(protocol::Event::Log((*step).to_owned()));
        }
        for line in [
            "mkfs.btrfs -L ferrix /dev/nvme0n1p2",
            "mounted /dev/nvme0n1p2 on /mnt/target",
            "copying 41,208 files (2.1 GiB)",
        ] {
            s.event(protocol::Event::Log(line.to_owned()));
        }
        s.event(protocol::Event::Progress(38));
    }),
    ("11-done", |s| {
        s.page = Page::Installing;
        s.event(protocol::Event::Done);
    }),
    ("12-error", |s| {
        s.page = Page::Installing;
        for step in crate::fake::STEPS.iter().take(4) {
            s.event(protocol::Event::Log((*step).to_owned()));
        }
        s.event(protocol::Event::Log(
            "grub-install: /boot/efi is not an EFI partition".to_owned(),
        ));
        s.event(protocol::Event::Error(
            "The boot loader could not be installed: /boot/efi is not an EFI partition".to_owned(),
        ));
    }),
];

/// The You page answered as Ada would.
fn fill_you(state: &mut State) {
    state.edited(&Key::FullName, "Ada Lovelace");
    state.edited(&Key::Host, "analytical-engine");
    state.edited(&Key::Password, "correct horse battery");
    state.edited(&Key::Confirm, "correct horse battery");
}

/// The state every picture starts from: the stand-in's disks, read over
/// its socket as the window reads the daemon's, and a few zones.
///
/// # Errors
///
/// The stand-in could not be started or asked, in words.
pub fn shot_state() -> Result<State, String> {
    let socket =
        std::env::temp_dir().join(format!("ferrix-installer-shot-{}.sock", std::process::id()));
    let fake = Fake::start(&socket, Behaviour::default())
        .map_err(|e| format!("{}: {e}", socket.display()))?;
    let disks = protocol::disks(&fake.socket);
    let mut state = State {
        zones: crate::zones::sample(),
        ..State::default()
    };
    state.set_disks(disks);
    Ok(state)
}

/// Every picture, as `(name, state)`.
///
/// # Errors
///
/// As [`shot_state`].
pub fn shots() -> Result<Vec<(&'static str, State)>, String> {
    let base = shot_state()?;
    Ok(SHOTS
        .iter()
        .map(|(name, make)| {
            let mut state = base.clone();
            make(&mut state);
            (*name, state)
        })
        .collect())
}

/// Draw every page into `dir`.
///
/// # Errors
///
/// A page could not be drawn or written, in words.
pub fn screenshot(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (name, state) in shots()? {
        let mut view =
            crate::ui::View::new(crate::ui::own_fonts()).ok_or("the tree could not be made")?;
        view.resize(crate::window::SIZE);
        view.show(&state);
        // Where the choice is made below the fold, from its heading.
        if let Some(at) = state
            .rows()
            .iter()
            .position(|row| row.kind == crate::pages::Kind::Section && row.title == "Where")
        {
            view.reveal(at.saturating_sub(1));
        }
        let frame = view
            .frame(crate::window::SIZE, 1.0)
            .ok_or("the window could not be drawn")?;
        let png = tole::png::encode(frame.width(), frame.height(), frame.data());
        let file = dir.join(format!("{name}.png"));
        std::fs::write(&file, png).map_err(|e| format!("{}: {e}", file.display()))?;
        say(&format!("drew {}", file.display()));
    }
    Ok(())
}

/// The program.
#[must_use]
pub fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let status = |code: i32| match u8::try_from(code) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(code),
        Err(_) => ExitCode::FAILURE,
    };
    match parse(&arguments) {
        Asked::Window => status(crate::window::run()),
        Asked::Autostart => {
            let cmdline = std::fs::read_to_string("/proc/cmdline").unwrap_or_default();
            if is_live(&cmdline) {
                status(crate::window::run())
            } else {
                ExitCode::SUCCESS
            }
        }
        Asked::Screenshot(dir) => match screenshot(&dir) {
            Ok(()) => ExitCode::SUCCESS,
            Err(why) => {
                say(&format!("err: {why}"));
                ExitCode::FAILURE
            }
        },
        Asked::Version => {
            println!("ferrix-installer {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Asked::Help => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Asked::Refused(why) => {
            say(&format!("err: {why}"));
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Asked, is_live, parse};

    fn asked(words: &[&str]) -> Asked {
        parse(&words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>())
    }

    #[test]
    fn the_arguments() {
        assert_eq!(asked(&[]), Asked::Window);
        assert_eq!(asked(&["--autostart"]), Asked::Autostart);
        assert_eq!(asked(&["--version"]), Asked::Version);
        assert_eq!(
            asked(&["--screenshot", "shots"]),
            Asked::Screenshot("shots".into())
        );
        assert!(matches!(asked(&["--screenshot"]), Asked::Refused(_)));
        assert!(matches!(asked(&["--frobnicate"]), Asked::Refused(_)));
        assert!(matches!(asked(&["--version", "x"]), Asked::Refused(_)));
    }

    #[test]
    fn live_is_the_word_ferrix_live_on_the_command_line() {
        assert!(is_live("console=ttyS0 ferrix.live quiet\n"));
        assert!(is_live("ferrix.live=1"));
        assert!(!is_live("console=ttyS0 ferrix.liveish quiet"));
        assert!(!is_live(""));
    }
}
