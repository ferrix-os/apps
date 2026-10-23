//! The viewer over real records: 801 of the N150's first boot under lode
//! (2026-10-23, before lode read levels itself, so all but one are stored
//! as info), kept in `testdata/`. Its private addresses are replaced with
//! documentation ones.

use std::collections::HashSet;

use crate::app::{Action, App, Origin, Page};
use crate::data::{Kind, Log};
use crate::lines::{Line, lines};
use crate::query::Query;
use crate::stats;

/// The excerpt.
const SAMPLE: &str = include_str!("../testdata/n150-boot-excerpt.jsonl");

fn log() -> Log {
    let read = crate::json::read(SAMPLE);
    assert_eq!(read.skipped, 0);
    assert_eq!(read.records.len(), 801);
    Log::new(read.records)
}

#[test]
fn the_sample_is_one_boot_in_time_order_with_its_sources() {
    let log = log();
    assert_eq!(log.boots.len(), 1);
    assert_eq!(log.boots[0].hex(), "062359e1029c423c0c0b4c9cf5216a14");
    assert_eq!(log.boots[0].host, "ferrix");
    assert!(
        log.entries
            .windows(2)
            .all(|pair| pair[0].time() <= pair[1].time())
    );
    for source in ["kernel", "hyprix", "intel", "lode", "sessiond"] {
        assert!(log.sources.find(source).is_some(), "{source} is a source");
    }
    // lode's start, and its folds of repeats.
    assert!(
        log.entries
            .iter()
            .any(|entry| entry.row.kind == Kind::Mark(crate::data::Mark::Started))
    );
    let folds = log
        .entries
        .iter()
        .filter(|entry| matches!(entry.row.kind, Kind::Folded(_)))
        .count();
    assert_eq!(folds, 12);
}

#[test]
fn display_rules_find_the_warnings_the_flat_levels_hide() {
    let log = log();
    let stored_info = log
        .entries
        .iter()
        .filter(|entry| entry.stored_level() == 6)
        .count();
    // All but lode's own notice of the session are stored as info.
    assert_eq!(stored_info, 800);
    let warnings: Vec<_> = log
        .entries
        .iter()
        .filter(|entry| entry.row.level == 4)
        .collect();
    assert!(!warnings.is_empty());
    assert!(warnings.iter().all(|entry| entry.row.note.is_some()));
    // lode's filter language sees the level shown.
    let mut query = Query::default();
    query.set_text("priority <= warning");
    let (found, facets) = query.run(&log);
    assert_eq!(found.len(), facets.levels[..5].iter().sum::<usize>());
    assert_eq!(
        found.len(),
        warnings.len() + log.entries.iter().filter(|e| e.row.level <= 3).count()
    );
}

#[test]
fn the_table_folds_intels_refusals_and_counts_lodes_repeats() {
    let log = log();
    let mut query = Query::default();
    query.set_text("id = intel and message ~ \"EXECBUFFER2\"");
    let (found, _) = query.run(&log);
    assert!(found.len() > FOLD_LEAST, "{} records", found.len());
    let table = lines(&log, &found, &HashSet::new());
    let folds: Vec<&Line> = table
        .iter()
        .filter(|line| matches!(line, Line::Fold { .. }))
        .collect();
    assert!(!folds.is_empty());
    let Line::Fold {
        first, lines, more, ..
    } = folds[0]
    else {
        unreachable!();
    };
    assert!(
        *lines > *more as u64 + 1,
        "lode's folded repeats are counted"
    );
    // Expanded, every record of the run is a line again.
    let open = crate::lines::lines(&log, &found, &HashSet::from([*first]));
    assert!(open.len() > table.len());
}

/// More than a run worth folding.
const FOLD_LEAST: usize = crate::lines::FOLD_MIN;

#[test]
fn the_histogram_counts_every_line_once() {
    let log = log();
    let boot = &log.boots[0];
    let width = stats::bucket_width(boot.span(), 10_000_000, 120);
    let count = usize::try_from(boot.span() / width + 1).unwrap_or(1);
    let buckets = stats::buckets(log.of_boot(0), boot.first_us, width, count);
    let total: usize = buckets.iter().map(stats::Bucket::total).sum();
    assert_eq!(total, 801);
    let warnings: usize = buckets.iter().map(|bucket| bucket.warnings).sum();
    assert_eq!(
        warnings,
        log.entries.iter().filter(|e| e.row.level == 4).count()
    );
    let tiles = stats::tiles(log.of_boot(0), boot.span());
    assert_eq!(tiles.warnings, warnings);
    assert!(tiles.peak.0 >= 1);
}

#[test]
fn a_frame_of_each_page_is_drawn_in_each_palette() {
    let mut app = App::new(
        log(),
        Origin::File("sample.jsonl".into()),
        "sample".to_owned(),
    );
    let mut text = crate::draw::Text::with(compositor_text::Fonts::new());
    let _ = text.fonts.add_dir(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/share/fonts"
    )));
    assert!(text.faces() >= 5, "the design's faces are in share/fonts");
    for mode in [crate::theme::Mode::Light, crate::theme::Mode::Dark] {
        app.mode = mode;
        let palette = crate::theme::Theme::of(mode);
        for page in [Page::Overview, Page::Logs] {
            app.page = page;
            let (frame, hits) =
                crate::window::frame(&app, &mut text, (1440, 900), 1).expect("a frame");
            // The sidebar is the palette's bg2, and the page drew text: some
            // pixel in the page is the palette's ink.
            let at = |x: u32, y: u32| {
                let pixel = frame.pixel(x, y).expect("in the frame");
                (pixel.red(), pixel.green(), pixel.blue())
            };
            assert_eq!(at(110, 600), (palette.bg2.0, palette.bg2.1, palette.bg2.2));
            let ink = (palette.ink.0, palette.ink.1, palette.ink.2);
            let inked = (300..1400)
                .step_by(2)
                .any(|x| (60..120).any(|y| at(x, y) == ink));
            assert!(inked, "{page:?} in {mode:?} drew its heading");
            assert!(
                hits.iter()
                    .any(|(_, action)| *action == Action::Page(Page::Logs))
            );
        }
    }
}

#[test]
fn machines_received_are_picked_one_at_a_time_and_followed() {
    let root = std::env::temp_dir().join(format!("lode-viewer-received-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let read = crate::json::read(SAMPLE);
    let (early, late) = read.records.split_at(400);
    let n150 = root.join("062359e1029c423c0c0b4c9cf5216a14");
    assert_eq!(crate::store::import(early, &n150).expect("written"), 1);
    std::fs::write(n150.join("machine"), "hostname=n150\n").expect("written");
    assert_eq!(
        crate::store::import(&late[..10], &root.join("host-pixel7")).expect("written"),
        1
    );
    let mut app = App::new(
        Log::default(),
        Origin::File(root.clone()),
        "none".to_owned(),
    );
    app.machines = crate::store::machines(&root);
    app.machines_root = Some(root.clone());
    app.open_machine(0);
    assert_eq!(app.origin_name, "n150 · 062359e1");
    assert_eq!(app.log.entries.len(), 400);
    app.act(&Action::Machine(1), 0.0);
    assert_eq!(app.origin_name, "pixel7");
    assert_eq!(app.log.entries.len(), 10);
    // Back on the box, the rest of its log arrives in a segment still being
    // written (no summary yet), and Live takes it.
    app.act(&Action::Machine(0), 0.0);
    app.live = true;
    let incoming = root.join("incoming");
    assert_eq!(crate::store::import(late, &incoming).expect("written"), 1);
    for item in std::fs::read_dir(&incoming).expect("listed").flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        if name.ends_with(".fxj") {
            std::fs::rename(item.path(), n150.join(name)).expect("moved");
        }
    }
    std::fs::remove_dir_all(&incoming).expect("removed");
    assert_eq!(app.follow(), late.len());
    assert_eq!(app.log.entries.len(), 801);
    // A machine that begins sending appears.
    assert!(!app.rescan_machines(&root));
    assert_eq!(
        crate::store::import(&late[..1], &root.join("host-third")).expect("written"),
        1
    );
    assert!(app.rescan_machines(&root));
    assert_eq!(app.machines.len(), 3);
    assert_eq!(app.machine, 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_add_machine_form_is_drawn_whole_in_each_palette_kind_and_key() {
    use crate::app::{FormField, KeyChoice};
    use crate::remote::Kind as Link;
    let mut app = App::new(
        log(),
        Origin::File("sample.jsonl".into()),
        "sample".to_owned(),
    );
    app.page = Page::Machines;
    app.reach = vec!["192.0.2.10".to_owned(), "nazuna".to_owned()];
    let mut text = crate::draw::Text::with(compositor_text::Fonts::new());
    let _ = text.fonts.add_dir(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/share/fonts"
    )));
    for mode in [crate::theme::Mode::Light, crate::theme::Mode::Dark] {
        app.mode = mode;
        for kind in [Link::Ssh, Link::Tls] {
            for key in [KeyChoice::Agent, KeyChoice::File, KeyChoice::Paste] {
                app.form.kind = kind;
                app.form.key = key;
                let (_, hits) =
                    crate::window::frame(&app, &mut text, (1440, 960), 1).expect("a frame");
                let has = |wanted: &Action| hits.iter().any(|(_, action)| action == wanted);
                // Both ways are offered, a disabled one answering with why.
                assert!(has(&Action::FormKind(Link::Ssh)) && has(&Action::FormKind(Link::Tls)));
                for choice in [KeyChoice::Agent, KeyChoice::File, KeyChoice::Paste] {
                    assert!(has(&Action::FormKeyChoice(choice)));
                }
                assert_eq!(has(&Action::FormChooseKey), key == KeyChoice::File);
                assert_eq!(has(&Action::FormPaste), key == KeyChoice::Paste);
                assert_eq!(
                    has(&Action::FormFocus(FormField::Server)),
                    kind == Link::Tls,
                    "the server field is there for TLS"
                );
                assert_eq!(has(&Action::FormServer(1)), kind == Link::Tls);
                // Nothing is placed past the window's right edge.
                assert!(hits.iter().all(|(place, _)| place.right() <= 1440.0));
            }
        }
    }
    // The help text is wrapped to the card, not cut at its edge.
    let help = "Keys only: the viewer never asks for a password and keeps none. ssh runs with BatchMode, and with IdentitiesOnly when a key is named.";
    let face = crate::draw::sans(12.0, 400);
    let lines = crate::draw::wrap_with(help, 600.0, |line| text.width(face, line));
    assert!(lines.len() >= 2);
    assert!(lines.iter().all(|line| text.width(face, line) <= 600.0));
    assert!(app.machines_height.get() > 600.0);
}

#[test]
fn a_query_pasted_over_a_selected_one_replaces_it() {
    // What the desktop boot (`test-compositor --boot lode-viewer`) pastes
    // after its typed `id=msr`: the counts it requires.
    let mut app = App::new(
        log(),
        Origin::File("sample.jsonl".into()),
        "sample".to_owned(),
    );
    app.act(&Action::Zoom(0), 0.0);
    app.act(&Action::EditQuery, 0.0);
    for c in "id=msr".chars() {
        let _ = app.type_key("x", &c.to_string(), (false, false));
    }
    let _ = app.type_key("Return", "", (false, false));
    assert_eq!(app.found.len(), 13);
    app.act(&Action::EditQuery, 0.0);
    let _ = app.type_key("a", "", (true, false));
    assert!(app.paste("id=usbhid"));
    let _ = app.type_key("Return", "", (false, false));
    assert_eq!(app.query.text, "id=usbhid");
    assert_eq!(app.found.len(), 14);
}
