//! The frame, drawn with the tree's own fonts and the system's own icons,
//! and read back pixel by pixel where a pixel says something.

use std::path::{Path, PathBuf};

use compositor_desktop::game_mode::GameMode;
use compositor_text::Fonts;
use compositor_text::tiny_skia::Pixmap;

use super::Painter;
use crate::layout::Geometry;
use crate::model::Model;
use crate::testdir::TestDir;
use crate::theme::{self, Color};

/// The Ferrix checkout this app is built in.
fn tree() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

/// The fonts an image carries: the tree's `assets/fonts`.
fn fonts() -> Fonts {
    let mut fonts = Fonts::new();
    for family in ["inter", "liberation"] {
        let _ = fonts.add_dir(&tree().join("assets/fonts").join(family));
    }
    assert!(!fonts.faces().is_empty(), "the tree's fonts are there");
    fonts
}

/// A desktop's applications: the system's own entries and icons, where a
/// launcher finds them, and one with no icon at all.
fn desktop(dir: &TestDir) -> impl Fn(&str) -> Option<String> + use<> {
    let shipped = tree().join("src/user/system/linux/compositor/desktop");
    for name in ["terminal", "pattern", "top"] {
        let text = std::fs::read_to_string(shipped.join(format!("applications/{name}.desktop")))
            .unwrap_or_default();
        let _ = dir.write(&format!("share/applications/{name}.desktop"), &text);
    }
    let _ = dir.write(
        "share/applications/steam.desktop",
        "[Desktop Entry]\nType=Application\nName=Steam\nGenericName=Game Store\nExec=/bin/true\n",
    );
    let _ = dir.write(
        "share/applications/settings.desktop",
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("share/settings.desktop"),
        )
        .unwrap_or_default(),
    );
    let icons = [
        ("index.theme", shipped.join("icons/index.theme")),
        (
            "scalable/apps/ferrix-terminal.svg",
            shipped.join("icons/ferrix-terminal.svg"),
        ),
        (
            "scalable/apps/ferrix-pattern.svg",
            shipped.join("icons/ferrix-pattern.svg"),
        ),
        (
            "scalable/apps/ferrix-monitor.svg",
            shipped.join("icons/ferrix-monitor.svg"),
        ),
        (
            "scalable/apps/ferrix-settings.svg",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("share/ferrix-settings.svg"),
        ),
    ];
    for (to, from) in icons {
        let text = std::fs::read_to_string(&from).unwrap_or_default();
        assert!(!text.is_empty(), "{} is there", from.display());
        let _ = dir.write(&format!("share/icons/hicolor/{to}"), &text);
    }
    let share = dir.path().join("share").to_string_lossy().into_owned();
    move |name: &str| (name == "XDG_DATA_DIRS").then(|| share.clone())
}

/// The colour at a logical point of a frame drawn at `scale`.
fn at(frame: &Pixmap, x: f32, y: f32, scale: u32) -> Color {
    let s = scale as f32;
    let pixel = frame
        .pixel((x * s) as u32, (y * s) as u32)
        .expect("the point is in the frame");
    Color(pixel.red(), pixel.green(), pixel.blue())
}

/// How many pixels of a logical rectangle are not `background`.
fn inked(frame: &Pixmap, place: &crate::layout::Rect, background: Color, scale: u32) -> usize {
    let s = scale as f32;
    let mut count = 0;
    for y in (place.y * s) as u32..((place.y + place.h) * s) as u32 {
        for x in (place.x * s) as u32..((place.x + place.w) * s) as u32 {
            let Some(pixel) = frame.pixel(x, y) else {
                continue;
            };
            if Color(pixel.red(), pixel.green(), pixel.blue()) != background {
                count += 1;
            }
        }
    }
    count
}

/// Draw `model` in a window of `width` by `height` at `scale`.
fn frame(
    painter: &mut Painter,
    model: &Model,
    (width, height): (u32, u32),
    scale: u32,
) -> (Pixmap, Geometry) {
    let geometry = painter.geometry(width, height);
    let mut frame = Pixmap::new(width * scale, height * scale).expect("a frame");
    painter.draw(&mut frame.as_mut(), model, &geometry, scale);
    // `SETTINGS_FRAMES=<dir>` keeps each frame, to look at: binary PPM.
    if let Some(dir) = std::env::var_os("SETTINGS_FRAMES") {
        let mut bytes = format!("P6\n{} {}\n255\n", frame.width(), frame.height()).into_bytes();
        for pixel in frame.pixels() {
            bytes.extend([pixel.red(), pixel.green(), pixel.blue()]);
        }
        let name = format!(
            "settings-{width}x{height}@{scale}-{}-{}.ppm",
            model.selected,
            model.filter.replace(' ', "_")
        );
        let _ = std::fs::write(Path::new(&dir).join(name), bytes);
    }
    (frame, geometry)
}

#[test]
fn the_page_shows_each_application_with_its_icon_and_its_switch() {
    let dir = TestDir::new("paint");
    let var = desktop(&dir);
    let apps = crate::apps::installed(&var);
    let names: Vec<&str> = apps.iter().map(|app| app.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Settings", "Steam", "Terminal", "Test Pattern", "Top"]
    );
    let mut painter = Painter::new(fonts(), &apps, &var);
    // Every application but Steam, which names no icon, has one.
    assert_eq!(painter.icons_found(), 4);
    let (games, _) = GameMode::parse("pattern.desktop\n");
    let mut model = Model::new(apps, games);
    model.select(2);
    for scale in [1, 2] {
        let (frame, g) = frame(&mut painter, &model, (920, 640), scale);
        assert_eq!(g.visible_rows(), 5, "the five rows fit");
        // The sidebar is the surface colour, the page the ink.
        assert_eq!(at(&frame, 100.0, 600.0, scale), theme::SURFACE);
        assert_eq!(
            at(&frame, g.footer.x + g.footer.w - 2.0, 2.0, scale),
            theme::INK
        );
        // The words are there: the heading, the explanation, the foot.
        assert!(inked(&frame, &g.heading, theme::INK, scale) > 300);
        assert!(inked(&frame, &g.explanation, theme::INK, scale) > 3000);
        assert!(inked(&frame, &g.footer, theme::INK, scale) > 300);
        for (row, on) in [(0, false), (1, false), (2, false), (3, true), (4, false)] {
            let place = g.row(row);
            let switch = Geometry::switch(&place);
            // The middle of the track, between its ends and off the knob's
            // edge: the accent when on, grey when off.
            let track = at(
                &frame,
                switch.x + switch.w / 2.0,
                switch.y + switch.h / 2.0,
                scale,
            );
            assert_eq!(
                track,
                if on { theme::ACCENT } else { theme::OFF },
                "row {row}'s switch"
            );
            // The knob is at the right when on and the left when off.
            let (knob_on, knob_off) = (
                at(
                    &frame,
                    switch.x + switch.w - 11.0,
                    switch.y + switch.h / 2.0,
                    scale,
                ),
                at(&frame, switch.x + 11.0, switch.y + switch.h / 2.0, scale),
            );
            if on {
                assert_eq!(knob_on, theme::KNOB, "row {row}'s knob");
            } else {
                assert_eq!(knob_off, theme::MUTED, "row {row}'s knob");
            }
            // The row's own background: selected for the third.
            let background = at(&frame, place.x + 8.0, place.y + 6.0, scale);
            assert_eq!(
                background,
                if row == 2 {
                    theme::SELECTED
                } else {
                    theme::SURFACE
                },
                "row {row}'s background"
            );
            // An icon, or the name's initial on a disc, where icons go.
            let icon = crate::layout::Rect {
                x: place.x + 16.0,
                y: place.y + 12.0,
                w: 32.0,
                h: 32.0,
            };
            assert!(
                inked(&frame, &icon, background, scale) > 200,
                "row {row}'s icon"
            );
        }
    }
}

#[test]
fn the_filter_and_a_refusal_are_shown() {
    let dir = TestDir::new("paint-filter");
    let var = desktop(&dir);
    let apps = crate::apps::installed(&var);
    let mut painter = Painter::new(fonts(), &apps, &var);
    let mut model = Model::new(apps, GameMode::default());
    let (empty, g) = frame(&mut painter, &model, (920, 640), 1);
    model.type_text("pat");
    assert_eq!(model.shown_len(), 1);
    let (typed, _) = frame(&mut painter, &model, (920, 640), 1);
    // One row is left, so where the second was is the page's ink again.
    let second = g.row(1);
    assert_eq!(
        at(&empty, second.x + 8.0, second.y + 6.0, 1),
        theme::SURFACE
    );
    assert_eq!(at(&typed, second.x + 8.0, second.y + 6.0, 1), theme::INK);
    // Nothing matching says so where the list was.
    model.type_text("zzz");
    let (none, _) = frame(&mut painter, &model, (920, 640), 1);
    assert!(inked(&none, &g.row(0), theme::INK, 1) > 300);
    // A write that was refused is said at the foot, in the warning colour.
    model.refused("the disk is full".to_owned());
    let (refused, _) = frame(&mut painter, &model, (920, 640), 1);
    let mut warned = false;
    for y in g.footer.y as u32..(g.footer.y + g.footer.h) as u32 {
        for x in g.footer.x as u32..(g.footer.x + 200.0) as u32 {
            let pixel = refused.pixel(x, y).expect("in the frame");
            warned |= Color(pixel.red(), pixel.green(), pixel.blue()) == theme::WARNING;
        }
    }
    assert!(warned, "the refusal is drawn in the warning colour");
}

#[test]
fn a_small_window_and_no_applications_draw_without_a_row() {
    let var = |_: &str| None;
    let mut painter = Painter::new(fonts(), &[], &var);
    let model = Model::new(Vec::new(), GameMode::default());
    for size in [(920, 640), (480, 360), (200, 100), (1, 1)] {
        let (frame, g) = frame(&mut painter, &model, size, 1);
        assert_eq!(frame.width(), size.0);
        assert!(g.visible_rows() <= 5);
    }
}
