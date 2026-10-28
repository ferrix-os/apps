//! The command line: `shelf [FOLDER]` opens the window; `--version` says
//! which shelf it is; `--render FILE [--light] [--scale N]` draws the
//! demo's window (the README's pictures) into a PNG without a desktop;
//! `--demo` opens the window on the demo's home.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::app::App;
use crate::store::{Quick, Settings, Tags};

/// Say a line on standard output, as the boots that wait for one read it.
pub fn say(line: &str) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "shelf: {line}");
    let _ = out.flush();
}

/// The demo's window drawn at `scale`, `dark` or light, as a PNG.
#[must_use]
pub fn render_demo(dark: bool, scale: f32) -> Option<Vec<u8>> {
    render_scene(dark, scale, "main")
}

/// The demo's window in one of its scenes: `main` (the design's two
/// panes), `rename` (two pictures renamed together), `archive` (inside a
/// zip, one pane, the grid), `single` (one pane, its path bar in the tool
/// row), `own-frame` (the same, the window drawing its own frame: its
/// buttons at the tab row's end) or `trash`.
#[must_use]
pub fn render_scene(dark: bool, scale: f32, scene: &str) -> Option<Vec<u8>> {
    use crate::app::{Action, Loc};
    let mut app = crate::demo::app(dark);
    let home = crate::demo::home();
    match scene {
        "rename" => {
            app.act(Action::Click(0, 1, false, false));
            app.act(Action::Click(0, 2, true, false));
            app.act(Action::Rename);
        }
        "archive" => {
            app.act(Action::ToggleDual);
            app.act(Action::Go(Loc::Archive {
                file: home.join("Documents/design-assets.tar.gz"),
                inner: String::new(),
            }));
            app.act(Action::Grid(true));
            app.act(Action::Click(0, 1, false, false));
        }
        "single" | "own-frame" => app.act(Action::ToggleDual),
        _ => {}
    }
    let mut view = crate::view::View::new(crate::view::fonts())?;
    view.ui.set_scale(scale);
    view.set_own_frame(scene == "own-frame");
    view.show(&mut app);
    let size = (1240, 800);
    let frame = view.ui.render(size, tole::Color::TRANSPARENT)?;
    Some(tole::png::encode(
        frame.width(),
        frame.height(),
        frame.data(),
    ))
}

/// shelf's command line.
#[must_use]
pub fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        say_plain(&format!("shelf {}", env!("CARGO_PKG_VERSION")));
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        say_plain(
            "shelf [FOLDER] | --demo | --render FILE [--light] [--scale N] [--scene main|rename|archive|single|own-frame] | --version",
        );
        return ExitCode::SUCCESS;
    }
    if let Some(at) = args.iter().position(|a| a == "--render") {
        let Some(file) = args.get(at + 1) else {
            say("err: --render needs a file");
            return ExitCode::FAILURE;
        };
        let dark = !args.iter().any(|a| a == "--light");
        let scale = args
            .iter()
            .position(|a| a == "--scale")
            .and_then(|i| args.get(i + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0);
        let scene = args
            .iter()
            .position(|a| a == "--scene")
            .and_then(|i| args.get(i + 1))
            .map_or("main", String::as_str);
        return match render_scene(dark, scale, scene) {
            Some(png) if std::fs::write(file, &png).is_ok() => {
                say(&format!("drew {file}"));
                ExitCode::SUCCESS
            }
            _ => {
                say(&format!("err: {file} was not drawn"));
                ExitCode::FAILURE
            }
        };
    }
    let app = if args.iter().any(|a| a == "--demo") {
        crate::demo::app(true)
    } else {
        let dir = args.iter().find(|a| !a.starts_with("--")).map(|a| {
            let path = PathBuf::from(tole::expand_home(a));
            std::path::absolute(&path).unwrap_or(path)
        });
        App::new(dir, Settings::load(), Tags::load(), Quick::load(), true)
    };
    ExitCode::from(u8::try_from(crate::window::run(app)).unwrap_or(1))
}

fn say_plain(line: &str) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{line}");
}
