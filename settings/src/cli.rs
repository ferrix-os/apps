//! The command line.
//!
//! ```text
//! settings                       open the window
//! settings --page NAME           open it on a page (game-mode, keyboard, ...)
//! settings --light               open it light rather than dark
//! settings --render PAGE FILE    draw one page into a PNG, no window
//!          [--light] [--size WxH] [--scale S] [--search TEXT]
//! settings --list                print every application and its switch, no window
//! settings --version
//! settings --help
//! ```
//!
//! `--list` prints what the game-mode page lists, a line an application:
//! `on` or `off`, its desktop-file ID, its name. It reads the same list and
//! the same file as the window, so a script -- and the boot that tests the
//! app -- can see both without a screen. `--render` draws the window
//! headless, from the same state a window opens with, for the README's
//! screenshots and a person comparing it with the design.

use std::path::PathBuf;
use std::process::ExitCode;

use compositor_desktop::game_mode::{self, GameMode};

use crate::pages::Page;
use crate::window::{Options, say, var};

/// What `--help` prints.
const USAGE: &str = "\
usage: settings [--page NAME] [--light]
       settings --render PAGE FILE [--light] [--size WxH] [--scale S] [--search TEXT]
       settings --list | --version | --help

  --page NAME     open on a page: displays appearance windows wallpaper keyboard
                  idle game-mode power users name-and-time remote-login logs boot
  --light         light rather than dark
  --render        draw PAGE into the PNG FILE with no window, and exit
  --list          print each application as `on|off ID Name`, and exit
  --version       print the version, and exit
  --help          print this, and exit
";

/// A drawing asked for by `--render`.
#[derive(Clone, Debug, PartialEq)]
pub struct Render {
    /// The window: page, palette.
    pub options: Options,
    /// Where the PNG goes.
    pub file: PathBuf,
    /// The window's size in logical pixels.
    pub size: (u32, u32),
    /// Buffer pixels to a logical one.
    pub scale: f32,
    /// The search typed, if any.
    pub search: Option<String>,
}

/// What the arguments ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Asked {
    /// The window, on a page.
    Window(Options),
    /// `--render`.
    Render(Render),
    /// `--list`.
    List,
    /// `--version`.
    Version,
    /// `--help`.
    Help,
    /// Something else, which is refused in these words.
    Refused(String),
}

/// The page called `name`, or why not.
fn page(name: Option<&String>) -> Result<Page, String> {
    let Some(name) = name else {
        return Err("--page needs a page's name, such as game-mode".to_owned());
    };
    Page::named(name).ok_or_else(|| {
        let names: Vec<&str> = Page::ALL.iter().map(|p| p.id()).collect();
        format!("{name} is not a page; there are: {}", names.join(" "))
    })
}

/// Read the arguments after the program's name.
#[must_use]
pub fn parse(arguments: &[String]) -> Asked {
    let mut options = Options::default();
    let mut render: Option<(Page, PathBuf)> = None;
    let mut size = (1280, 860);
    let mut scale = 1.0;
    let mut search = None;
    let mut words = arguments.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "--list" => return Asked::List,
            "--version" | "-V" => return Asked::Version,
            "--help" | "-h" => return Asked::Help,
            "--light" => options.dark = false,
            "--page" => match page(words.next()) {
                Ok(page) => options.page = page,
                Err(why) => return Asked::Refused(why),
            },
            "--render" => {
                let page = match page(words.next()) {
                    Ok(page) => page,
                    Err(why) => return Asked::Refused(why),
                };
                let Some(file) = words.next() else {
                    return Asked::Refused("--render needs a page and a PNG file".to_owned());
                };
                render = Some((page, PathBuf::from(file)));
            }
            "--size" => {
                let parsed = words.next().and_then(|v| {
                    let (w, h) = v.split_once('x')?;
                    Some((w.parse().ok()?, h.parse().ok()?))
                });
                match parsed {
                    Some(value) => size = value,
                    None => return Asked::Refused("--size takes WxH, such as 1280x860".to_owned()),
                }
            }
            "--scale" => match words.next().and_then(|v| v.parse::<f32>().ok()) {
                Some(value) if value > 0.0 => scale = value,
                _ => return Asked::Refused("--scale takes a number, such as 2".to_owned()),
            },
            "--search" => match words.next() {
                Some(text) => search = Some(text.clone()),
                None => return Asked::Refused("--search needs the text".to_owned()),
            },
            other => return Asked::Refused(format!("{other} is not an option settings has")),
        }
    }
    match render {
        Some((page, file)) => Asked::Render(Render {
            options: Options { page, ..options },
            file,
            size,
            scale,
            search,
        }),
        None => Asked::Window(options),
    }
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

/// Draw `render` into its file.
///
/// # Errors
///
/// The tree could not be made, drawn or written, in words.
pub fn render(render: &Render) -> Result<(), String> {
    let mut system = crate::window::system(&var);
    let mut state = crate::window::state(&render.options, &var, system.as_mut());
    if let Some(search) = &render.search {
        state.search(search);
    }
    let mut view = crate::ui::View::new(crate::ui::fonts()).ok_or("the tree could not be made")?;
    view.show(&state);
    let frame = view
        .frame(render.size, render.scale)
        .ok_or("the window could not be drawn")?;
    let png = tole::png::encode(frame.width(), frame.height(), frame.data());
    std::fs::write(&render.file, png).map_err(|e| format!("{}: {e}", render.file.display()))
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
        Asked::Render(asked) => match render(&asked) {
            Ok(()) => ExitCode::SUCCESS,
            Err(why) => {
                say(&format!("err: {why}"));
                ExitCode::FAILURE
            }
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
    use crate::pages::Page;
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
            asked(&["--page", "game-mode", "--light"]),
            Asked::Window(Options {
                page: Page::Game,
                dark: false,
            })
        );
        let Asked::Render(render) = asked(&["--render", "keyboard", "k.png", "--size", "800x600"])
        else {
            panic!("not a render");
        };
        assert_eq!(render.options.page, Page::Keyboard);
        assert_eq!(render.size, (800, 600));
        assert_eq!(asked(&["--list"]), Asked::List);
        assert_eq!(asked(&["--version"]), Asked::Version);
        assert_eq!(asked(&["-h"]), Asked::Help);
        assert!(matches!(asked(&["--page"]), Asked::Refused(_)));
        assert!(matches!(asked(&["--page", "sound"]), Asked::Refused(_)));
        assert!(matches!(
            asked(&["--render", "keyboard"]),
            Asked::Refused(_)
        ));
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
