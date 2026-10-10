//! The command line.
//!
//! ```text
//! settings                  open the window
//! settings --page NAME      open it on a page (game-mode)
//! settings --list           print every application and its switch, no window
//! settings --version
//! settings --help
//! ```
//!
//! `--list` prints what the game-mode page lists, a line an application:
//! `on` or `off`, its desktop-file ID, its name. It reads the same list and
//! the same file as the window, so a script -- and the boot that tests the
//! app -- can see both without a screen.

use std::process::ExitCode;

use compositor_desktop::game_mode::{self, GameMode};

use crate::model::Page;
use crate::window::{Options, say, var};

/// What `--help` prints.
const USAGE: &str = "\
usage: settings [--page NAME] | --list | --version | --help

  --page NAME   open on a page: game-mode
  --list        print each application as `on|off ID Name`, and exit
  --version     print the version, and exit
  --help        print this, and exit
";

/// What the arguments ask for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asked {
    /// The window, on a page.
    Window(Options),
    /// `--list`.
    List,
    /// `--version`.
    Version,
    /// `--help`.
    Help,
    /// Something else, which is refused in these words.
    Refused(String),
}

/// Read the arguments after the program's name.
#[must_use]
pub fn parse(arguments: &[String]) -> Asked {
    let mut options = Options::default();
    let mut words = arguments.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--list" => return Asked::List,
            "--version" | "-V" => return Asked::Version,
            "--help" | "-h" => return Asked::Help,
            "--page" => {
                let Some(name) = words.next() else {
                    return Asked::Refused("--page needs a page's name: game-mode".to_owned());
                };
                let Some(page) = Page::named(name) else {
                    return Asked::Refused(format!("{name} is not a page; there is: game-mode"));
                };
                options.page = page;
            }
            other => return Asked::Refused(format!("{other} is not an option settings has")),
        }
    }
    Asked::Window(options)
}

/// The lines `--list` prints for the environment `var` answers for.
#[must_use]
pub fn list(var: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let games = game_mode::path(var)
        .and_then(|path| GameMode::load(&path).ok())
        .map(|(games, _)| games)
        .unwrap_or_default();
    crate::apps::installed(var)
        .iter()
        .map(|app| {
            let state = if games.contains(&app.id) { "on" } else { "off" };
            format!("{state} {} {}", app.id, app.name)
        })
        .collect()
}

/// The program.
#[must_use]
pub fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match parse(&arguments) {
        Asked::Window(options) => match u8::try_from(crate::window::run(&options)) {
            Ok(0) => ExitCode::SUCCESS,
            Ok(status) => ExitCode::from(status),
            Err(_) => ExitCode::FAILURE,
        },
        Asked::List => {
            for line in list(&var) {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Asked::Version => {
            println!("settings {}", env!("CARGO_PKG_VERSION"));
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
    use super::{Asked, list, parse};
    use crate::model::Page;
    use crate::testdir::TestDir;
    use crate::window::Options;

    fn asked(words: &[&str]) -> Asked {
        parse(
            &words
                .iter()
                .map(|word| (*word).to_owned())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn the_arguments() {
        assert_eq!(asked(&[]), Asked::Window(Options::default()));
        assert_eq!(
            asked(&["--page", "game-mode"]),
            Asked::Window(Options {
                page: Page::GameMode
            })
        );
        assert_eq!(asked(&["--list"]), Asked::List);
        assert_eq!(asked(&["--version"]), Asked::Version);
        assert_eq!(asked(&["-h"]), Asked::Help);
        assert!(matches!(asked(&["--page"]), Asked::Refused(_)));
        assert!(matches!(asked(&["--page", "sound"]), Asked::Refused(_)));
        assert!(matches!(asked(&["--frobnicate"]), Asked::Refused(_)));
    }

    #[test]
    fn the_list_is_each_application_and_its_switch_from_the_users_file() {
        let dir = TestDir::new("cli-list");
        let _ = dir.write(
            "share/applications/steam.desktop",
            "[Desktop Entry]\nType=Application\nName=Steam\nExec=/bin/true\n",
        );
        let _ = dir.write(
            "share/applications/term.desktop",
            "[Desktop Entry]\nType=Application\nName=Terminal\nExec=/bin/true\n",
        );
        let _ = dir.write("home/.config/ferrix/game-mode.conf", "steam.desktop\n");
        let share = dir.path().join("share").to_string_lossy().into_owned();
        let home = dir.path().join("home").to_string_lossy().into_owned();
        let var = move |name: &str| match name {
            "XDG_DATA_DIRS" => Some(share.clone()),
            "HOME" => Some(home.clone()),
            _ => None,
        };
        assert_eq!(
            list(&var),
            vec!["on steam.desktop Steam", "off term.desktop Terminal"]
        );
        // With no user there is no file, and nothing is on.
        let share = dir.path().join("share").to_string_lossy().into_owned();
        let nobody = move |name: &str| (name == "XDG_DATA_DIRS").then(|| share.clone());
        assert_eq!(
            list(&nobody),
            vec!["off steam.desktop Steam", "off term.desktop Terminal"]
        );
    }
}
