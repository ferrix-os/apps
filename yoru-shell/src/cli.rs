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

/// The stash panel as drawn for a picture: five files of the four tinted
/// kinds.
#[must_use]
pub fn render_stash(size: (u32, u32), scale: f32) -> Option<tole::Frame> {
    use crate::stash::{Item, Kind};
    let items: Vec<Item> = [
        ("receipt-0417.pdf", Kind::Pdf),
        ("Quarterly report.pdf", Kind::Pdf),
        ("night-drive-theme.flac", Kind::Audio),
        ("layout.rs", Kind::Code),
        ("Weekly plan for the team.md", Kind::Text),
    ]
    .into_iter()
    .map(|(name, kind)| Item {
        link: name.to_owned(),
        target: format!("/home/ada/{name}").into(),
        name: name.to_owned(),
        kind,
        gone: false,
    })
    .collect();
    let mut view = crate::shelf::View::new(crate::look::fonts())?;
    view.ui.set_scale(scale);
    view.resize(size);
    view.show(&items);
    view.ui.update();
    let mut frame = tole::Frame::new(size, scale, tole::Color::hex(0x10_2a2c))?;
    let _ = view.ui.render_into(&mut frame);
    Some(frame)
}

/// A sidebar as drawn for a picture: the left with the clock on
/// 2026-10-25 21:40 (or the weather), or the right with three tasks (or
/// the quick settings, focus mode and do not disturb on).
#[must_use]
pub fn render_sidebar(
    todo: bool,
    weather: bool,
    quick: bool,
    theme: Option<crate::theme::Theme>,
    scale: f32,
) -> Option<tole::Frame> {
    use crate::widgets::{Arrangement, Side, Task};
    let tasks = [
        ("Water the plants", true),
        ("Call the bank", false),
        ("Pack for Sunday", false),
    ]
    .into_iter()
    .map(|(text, done)| Task {
        text: text.to_owned(),
        done,
    })
    .collect();
    let mut arrangement = Arrangement::default();
    if weather {
        arrangement.show(Side::Left, crate::widgets::Kind::Weather);
    }
    if quick {
        arrangement.show(Side::Right, crate::widgets::Kind::Settings);
    }
    let sample =
        crate::json::Json::parse(SAMPLE_FORECAST).and_then(|j| crate::weather::forecast(&j));
    let shown = crate::side::Shown {
        side: if todo || quick {
            Side::Right
        } else {
            Side::Left
        },
        arrangement,
        tasks,
        now: crate::widgets::days_from_civil(2026, 10, 25) * 86_400 + 21 * 3600 + 40 * 60 + 7,
        open: true,
        weather: crate::weather::Weather {
            on: weather,
            place: Some(crate::weather::Place::at(53.6, 10.0)),
        },
        place: "Hamburg, Germany".to_owned(),
        forecast: sample,
        status: "From Open-Meteo at 21:40".to_owned(),
        quick: crate::quick::Quick {
            focus: true,
            quiet: true,
            theme: theme.unwrap_or_default(),
            ..crate::quick::Quick::default()
        },
    };
    let size = (crate::side::OPEN, 900);
    let mut view = crate::side::View::new(crate::look::fonts())?;
    view.ui.set_scale(scale);
    view.resize(size);
    let tokens = theme.map_or(crate::theme::Tokens::NOCTURNE, |theme| {
        crate::theme::Tokens::of(&theme, &crate::theme::Wallpaper::default())
    });
    tokens.apply(&mut view.ui);
    view.show(&shown);
    view.ui.update();
    let [r, g, b] = tokens.bg;
    let ground = tole::Color::rgba(r / 2, g / 2, b / 2, 0xff);
    let mut frame = tole::Frame::new(size, scale, ground)?;
    let _ = view.ui.render_into(&mut frame);
    Some(frame)
}

/// A forecast as Open-Meteo answers it, for the picture: made up.
const SAMPLE_FORECAST: &str = r#"{"current":{"temperature_2m":11.4,"apparent_temperature":9.2,"weather_code":2,"is_day":0},
"hourly":{"time":["2026-10-25T22:00","2026-10-25T23:00","2026-10-26T00:00","2026-10-26T01:00","2026-10-26T02:00","2026-10-26T03:00","2026-10-26T04:00","2026-10-26T05:00"],
"temperature_2m":[11,10.6,10.1,9.8,9.5,9.1,8.8,8.6],"weather_code":[2,2,3,3,61,61,3,2]},
"daily":{"time":["2026-10-25","2026-10-26","2026-10-27","2026-10-28","2026-10-29","2026-10-30","2026-10-31"],
"weather_code":[2,61,3,0,1,80,71],"temperature_2m_max":[14,12,13,15,14,11,7],"temperature_2m_min":[8,7,6,6,7,5,1]}}"#;

/// The program.
#[must_use]
pub fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.iter().any(|word| word == "--version") {
        println!("yoru-shell {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if let Some(at) = arguments.iter().position(|word| word == "--render-sidebar") {
        let Some(path) = arguments.get(at + 1) else {
            eprintln!("yoru-shell: --render-sidebar needs a file");
            return ExitCode::FAILURE;
        };
        let todo = arguments.iter().any(|word| word == "--todo");
        let weather = arguments.iter().any(|word| word == "--weather");
        let quick = arguments.iter().any(|word| word == "--quick");
        // `--theme Sakura` or `--theme Sakura,Light`: drawn in that theme.
        let theme = arguments
            .iter()
            .position(|word| word == "--theme")
            .and_then(|at| arguments.get(at + 1))
            .map(|word| {
                let (mode, look) = word.split_once(',').unwrap_or((word, "Dark"));
                crate::theme::Theme::read(&format!("mode {mode}\nappearance {look}\n"))
            });
        let Some(frame) = render_sidebar(todo, weather, quick, theme, 1.0) else {
            eprintln!("yoru-shell: the sidebar could not be drawn");
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
    if let Some(at) = arguments.iter().position(|word| word == "--sidebar") {
        let Some(side) = arguments
            .get(at + 1)
            .and_then(|word| crate::widgets::Side::of(word))
        else {
            eprintln!("yoru-shell: --sidebar left or --sidebar right");
            return ExitCode::FAILURE;
        };
        return if crate::side::run(side) == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    if arguments.iter().any(|word| word == "--stash-panel") {
        return if crate::shelf::run() == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    if arguments.first().is_some_and(|word| word == "--stash") {
        return if crate::shelf::add(arguments.get(1..).unwrap_or_default()) == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    if let Some(at) = arguments.iter().position(|word| word == "--render-stash") {
        let Some(path) = arguments.get(at + 1) else {
            eprintln!("yoru-shell: --render-stash needs a file");
            return ExitCode::FAILURE;
        };
        let Some(frame) = render_stash((1000, 600), 1.0) else {
            eprintln!("yoru-shell: the stash could not be drawn");
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
    if arguments.iter().any(|word| word == "--power") {
        return if crate::power::run() == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    if let Some(at) = arguments.iter().position(|word| word == "--render-power") {
        let Some(path) = arguments.get(at + 1) else {
            eprintln!("yoru-shell: --render-power needs a file");
            return ExitCode::FAILURE;
        };
        let live = arguments.iter().any(|word| word == "--live");
        let asking = arguments
            .iter()
            .any(|word| word == "--confirm")
            .then_some(crate::power::Offer::ShutDown);
        let Some(frame) = crate::power::render(live, asking, (1280, 800), 1.0) else {
            eprintln!("yoru-shell: the power menu could not be drawn");
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
        // Above the icons, below the bar's edge.
        let (r, g, b) = at(400, 7);
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
