//! The command line: `yoru-shell` puts the dock on the screen;
//! `--render FILE` draws it into a PNG instead, as the design's sample
//! desktop shows it, for the README and the tests; `--version`.

use std::process::ExitCode;

use crate::model::{State, launchers};
use crate::ui::{HEIGHT, View};

/// The dock as drawn for a picture: the design's sample desktop, with
/// windows on the first two workspaces and a terminal running.
#[must_use]
pub fn sample() -> State {
    let mut state = State::new(
        launchers(&[
            (
                "google-chrome.desktop".to_owned(),
                "/bin/chrome".to_owned(),
                None,
            ),
            ("settings.desktop".to_owned(), "settings".to_owned(), None),
        ]),
        // 2026-10-24 21:40, Yoru's hour.
        1_792_878_000,
    );
    let list = crate::json::Json::parse(
        r#"[{"id":1,"windows":3},{"id":2,"windows":2},{"id":3,"windows":1}]"#,
    )
    .unwrap_or(crate::json::Json::Null);
    state.workspaces(&list, 1);
    let clients =
        crate::json::Json::parse(r#"[{"class":"rocks.magical.term","workspace":{"id":1}}]"#)
            .unwrap_or(crate::json::Json::Null);
    state.clients(&clients);
    state
}

/// The dock drawn `width` logical pixels wide at `scale`, as RGBA rows.
#[must_use]
pub fn render(state: &State, width: u32, scale: f32) -> Option<tole::Frame> {
    let mut view = View::new(crate::look::fonts())?;
    view.ui.set_scale(scale);
    view.resize((width, HEIGHT));
    view.show(state);
    view.ui.update();
    let mut frame = tole::Frame::new((width, HEIGHT), scale, tole::Color::hex(0x16_1826))?;
    let _ = view.ui.render_into(&mut frame);
    Some(frame)
}

/// The switcher as drawn for a picture: the Settings app's six pictures
/// from Ferrix's tree in a folder of their own and the Nocturne gradient
/// in Yoru's, their colours read, the second in force.
#[must_use]
pub fn sample_walls(grid: bool) -> crate::walls::Picker {
    let tree = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
    let mut found = Vec::new();
    for (file, folder) in [
        ("settings/share/wallpapers/tide.jpg", "Ferrix"),
        ("settings/share/wallpapers/dune.jpg", "Ferrix"),
        ("settings/share/wallpapers/fern.jpg", "Ferrix"),
        ("settings/share/wallpapers/dusk.jpg", "Ferrix"),
        ("settings/share/wallpapers/ember.jpg", "Ferrix"),
        ("settings/share/wallpapers/graphite.jpg", "Ferrix"),
    ] {
        let path = format!("{tree}/{file}");
        let name = file.rsplit('/').next().unwrap_or(file);
        found.push(crate::walls::Wall {
            look: crate::walls::look_of(std::path::Path::new(&path)),
            path,
            name: crate::walls::shown_name(name),
            folder: folder.to_owned(),
            video: false,
        });
    }
    let current = found
        .get(1)
        .map(|wall| wall.path.clone())
        .unwrap_or_default();
    let mut picker = crate::walls::Picker::new(
        found,
        vec!["Ferrix".to_owned(), "Mine".to_owned()],
        &current,
    );
    if grid {
        picker.flip();
    }
    picker
}

/// The switcher drawn `size` logical pixels at `scale`.
#[must_use]
pub fn render_walls(
    picker: &crate::walls::Picker,
    size: (u32, u32),
    scale: f32,
) -> Option<tole::Frame> {
    let mut view = crate::picker::View::new(crate::look::fonts())?;
    view.ui.set_scale(scale);
    view.resize(size);
    view.show(picker);
    view.ui.update();
    let mut frame = tole::Frame::new(size, scale, tole::Color::hex(0x2b_2741))?;
    let _ = view.ui.render_into(&mut frame);
    Some(frame)
}

/// The program.
#[must_use]
pub fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.iter().any(|word| word == "--version") {
        println!("yoru-shell {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if arguments.iter().any(|word| word == "--walls") {
        return if crate::switch::run() == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    if let Some(at) = arguments.iter().position(|word| word == "--render-walls") {
        let Some(path) = arguments.get(at + 1) else {
            eprintln!("yoru-shell: --render-walls needs a file");
            return ExitCode::FAILURE;
        };
        let grid = arguments.iter().any(|word| word == "--grid");
        let Some(frame) = render_walls(&sample_walls(grid), (1600, 1000), 1.0) else {
            eprintln!("yoru-shell: the switcher could not be drawn");
            return ExitCode::FAILURE;
        };
        let pixmap = frame.pixmap();
        let png = tole::png::encode(pixmap.width(), pixmap.height(), pixmap.data());
        return match std::fs::write(path, png) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("yoru-shell: {path}: {error}");
                ExitCode::FAILURE
            }
        };
    }
    if let Some(at) = arguments.iter().position(|word| word == "--render") {
        let Some(path) = arguments.get(at + 1) else {
            eprintln!("yoru-shell: --render needs a file");
            return ExitCode::FAILURE;
        };
        let Some(frame) = render(&sample(), 1600, 2.0) else {
            eprintln!("yoru-shell: the dock could not be drawn");
            return ExitCode::FAILURE;
        };
        let pixmap = frame.pixmap();
        let png = tole::png::encode(pixmap.width(), pixmap.height(), pixmap.data());
        return match std::fs::write(path, png) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("yoru-shell: {path}: {error}");
                ExitCode::FAILURE
            }
        };
    }
    if crate::window::run() == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::{render, sample};

    /// The sample dock draws at its size, with the bar's ground across
    /// its middle and the workspace shown lit in the accent.
    #[test]
    fn the_dock_draws() {
        let frame = render(&sample(), 800, 1.0).expect("drawn");
        let pixmap = frame.pixmap();
        assert_eq!((pixmap.width(), pixmap.height()), (800, crate::ui::HEIGHT));
        let at = |x: u32, y: u32| {
            let pixel = pixmap.pixel(x, y).expect("inside").demultiply();
            (pixel.red(), pixel.green(), pixel.blue())
        };
        let (r, g, b) = at(400, 30);
        assert!(
            r < 0x30 && g < 0x30 && b < 0x40,
            "the bar's ground: {r} {g} {b}"
        );
        // Workspace 1's button: its accent ring at the left edge.
        let lit = (60..140).any(|x| {
            (12..18).any(|y| {
                let (r, g, b) = at(x, y);
                b > 0xb0 && r > 0x80 && g < r
            })
        });
        assert!(lit, "the shown workspace is lit");
    }
}
