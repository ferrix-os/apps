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

/// The design's faces, and nothing of the machine's.
fn design_fonts() -> tole::Fonts {
    let mut fonts = tole::Fonts::new();
    let _ = fonts.add_dir(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/share/fonts"
    )));
    fonts
}

/// The viewer's tree over `app`, `size` logical pixels.
fn view_of(app: App, size: (u32, u32)) -> crate::ui::View {
    let mut view = crate::ui::View::new(
        std::rc::Rc::new(std::cell::RefCell::new(app)),
        design_fonts(),
    )
    .expect("the tree is made");
    view.resize(size);
    // Pastes come from the desktop, as in a window.
    view.ui.set_clipboard_external(true);
    view.show();
    view.ui.update();
    view
}

/// The first `Text` item, anywhere and shown, saying `label`, and where
/// it is.
fn text_at(view: &mut crate::ui::View, label: &str) -> Option<tole::Rect> {
    let mut stack: Vec<tole::ItemId> = view.ui.roots().to_vec();
    while let Some(item) = stack.pop() {
        let shown = view
            .ui
            .common(item)
            .is_some_and(|common| view.ui.get(common.visible));
        if !shown {
            continue;
        }
        if let Some(text) = view.ui.text(item)
            && &*view.ui.get(text.text) == label
        {
            return view.ui.rect_of(item);
        }
        stack.extend(view.ui.children(item).iter().rev());
    }
    None
}

/// A shown `Text` in the sidebar saying `label`, at or below `top`.
fn sidebar_text(view: &mut crate::ui::View, label: &str, top: f32) -> Option<tole::Rect> {
    let mut stack: Vec<tole::ItemId> = view.ui.roots().to_vec();
    while let Some(item) = stack.pop() {
        let shown = view
            .ui
            .common(item)
            .is_some_and(|common| view.ui.get(common.visible));
        if !shown {
            continue;
        }
        if let Some(text) = view.ui.text(item)
            && &*view.ui.get(text.text) == label
            && let Some(rect) = view.ui.rect_of(item)
            && rect.x < 220.0
            && rect.y >= top
        {
            return Some(rect);
        }
        stack.extend(view.ui.children(item).iter().rev());
    }
    None
}

/// Press and release the pointer on the text `label`; answers what the
/// tree asked of the state.
fn click(view: &mut crate::ui::View, label: &str) -> Vec<Action> {
    let rect = text_at(view, label).unwrap_or_else(|| panic!("`{label}` is shown"));
    view.ui
        .pointer_move(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    view.ui.pointer_press();
    view.ui.pointer_release();
    view.step().0
}

/// Type `keysym`, giving `text`, with Control as said.
fn key(view: &mut crate::ui::View, keysym: &str, text: &str, control: bool) {
    let event = tole::KeyEvent {
        key: keysym.into(),
        text: text.into(),
        shift: false,
        control,
        alt: false,
        repeat: false,
    };
    let _ = view.ui.key_press(&event);
    let _ = view.ui.key_release(&event);
}

#[test]
fn a_frame_of_each_page_is_drawn_in_each_palette() {
    let app = App::new(
        log(),
        Origin::File("sample.jsonl".into()),
        "sample".to_owned(),
    );
    let mut view = view_of(app, (1440, 900));
    assert!(
        view.ui.faces() >= 5,
        "the design's faces are in share/fonts"
    );
    for mode in [crate::theme::Mode::Light, crate::theme::Mode::Dark] {
        let palette = crate::theme::Theme::of(mode);
        for page in [Page::Overview, Page::Logs] {
            {
                let mut app = view.app.borrow_mut();
                app.mode = mode;
                app.page = page;
            }
            let frame = view.frame((1440, 900), 1.0).expect("a frame");
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
        }
    }
    // A press on the sidebar shows the page.
    view.app.borrow_mut().page = Page::Overview;
    view.show();
    view.ui.update();
    assert_eq!(click(&mut view, "Logs"), vec![Action::Page(Page::Logs)]);
    assert_eq!(view.app.borrow().page, Page::Logs);
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
    app.act(&Action::Machine(1));
    assert_eq!(app.origin_name, "pixel7");
    assert_eq!(app.log.entries.len(), 10);
    // Back on the box, the rest of its log arrives in a segment still being
    // written (no summary yet), and Live takes it.
    app.act(&Action::Machine(0));
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
    use crate::app::KeyChoice;
    use crate::remote::Kind as Link;
    let mut app = App::new(
        log(),
        Origin::File("sample.jsonl".into()),
        "sample".to_owned(),
    );
    app.page = Page::Machines;
    app.reach = vec!["192.0.2.10".to_owned(), "nazuna".to_owned()];
    let mut view = view_of(app, (1440, 960));
    for mode in [crate::theme::Mode::Light, crate::theme::Mode::Dark] {
        for kind in [Link::Ssh, Link::Tls] {
            for key in [KeyChoice::Agent, KeyChoice::File, KeyChoice::Paste] {
                {
                    let mut app = view.app.borrow_mut();
                    app.mode = mode;
                    app.form.kind = kind;
                    app.form.key = key;
                }
                view.show();
                view.ui.update();
                let mut shown = |label: &str| text_at(&mut view, label);
                // Both ways are offered, a disabled one answering with why.
                assert!(shown("Live over SSH").is_some() && shown("Forwarded over TLS").is_some());
                for choice in ["ssh's own keys and the agent", "A key file", "Paste a key"] {
                    assert!(shown(choice).is_some());
                }
                assert_eq!(shown("Choose…").is_some(), key == KeyChoice::File);
                assert_eq!(shown("Paste").is_some(), key == KeyChoice::Paste);
                assert_eq!(
                    shown("Server").is_some(),
                    kind == Link::Tls,
                    "the server field is there for TLS"
                );
                assert_eq!(shown("nazuna").is_some(), kind == Link::Tls);
                // The help text is wrapped to the card, not cut at its
                // edge, and nothing is placed past the window's right edge.
                let help = shown("Keys only: the viewer never asks for a password and keeps none. ssh runs with BatchMode, and with").or_else(|| shown("Keys only: the viewer never asks for a password and keeps none. ssh runs with BatchMode, and with IdentitiesOnly when a key is named."));
                let help = help.expect("the help is shown");
                assert!(help.x + help.w <= 1440.0);
            }
        }
    }
    // A press on TLS, then the key file, as a person would.
    let _ = click(&mut view, "Live over SSH");
    let _ = click(&mut view, "A key file");
    assert_eq!(view.app.borrow().form.key, KeyChoice::File);
    assert_eq!(view.app.borrow().form.kind, Link::Ssh);
}

#[test]
fn a_query_pasted_over_a_selected_one_replaces_it() {
    // What the desktop boot (`test-compositor --boot lode-viewer`) pastes
    // after its typed `id=msr`: the counts it requires, typed into tole's
    // field as a window hands keys over.
    let mut app = App::new(
        log(),
        Origin::File("sample.jsonl".into()),
        "sample".to_owned(),
    );
    app.act(&Action::Zoom(0));
    app.page = Page::Logs;
    let mut view = view_of(app, (1440, 960));
    let _ = click(&mut view, "where");
    for c in "id=msr".chars() {
        key(&mut view, &c.to_string(), &c.to_string(), false);
    }
    let _ = view.step();
    key(&mut view, "Return", "", false);
    let _ = view.step();
    assert_eq!(view.app.borrow().found.len(), 13);
    // Return gave the keys back: `/` is the window's again, and starts the
    // query (the window's loop does this for a key nobody took).
    assert!(!view.typing());
    view.focus_query();
    key(&mut view, "a", "", true);
    key(&mut view, "v", "", true);
    view.ui.paste("id=usbhid");
    let _ = view.step();
    key(&mut view, "Return", "", false);
    let _ = view.step();
    assert_eq!(view.app.borrow().query.text, "id=usbhid");
    assert_eq!(view.app.borrow().found.len(), 14);
}

#[test]
fn ctrl_v_is_a_paste_never_the_letter_and_tab_skips_hidden_fields() {
    use crate::app::FormField;
    let mut app = App::new(
        log(),
        Origin::File("sample.jsonl".into()),
        "sample".to_owned(),
    );
    app.act(&Action::FormFocus(FormField::Port));
    app.page = Page::Machines;
    let mut view = view_of(app, (1440, 960));
    key(&mut view, "v", "v", true);
    view.ui.paste("");
    let _ = view.step();
    assert_eq!(view.app.borrow().form.value(FormField::Port), "22");
    // From Port, Tab goes round to Name: no Key or Server field is shown.
    key(&mut view, "Tab", "", false);
    key(&mut view, "v", "v", true);
    view.ui.paste("n150\n");
    let _ = view.step();
    assert_eq!(view.app.borrow().form.value(FormField::Name), "n150");
    assert_eq!(view.app.borrow().form.focus, Some(FormField::Name));
}

/// A log of one record a second at these levels, from two sources in
/// turn so that no two in a row fold.
fn levels_log(levels: &[u8]) -> Log {
    Log::new(levels_records(levels, 0))
}

/// Records of `levels`, numbered on from `from`, a second apart.
fn levels_records(levels: &[u8], from: usize) -> Vec<ferrix_lode_core::record::Record> {
    let mut text = String::new();
    for (n, level) in levels.iter().enumerate() {
        let n = n + from;
        text.push_str(&format!(
            "{{\"__REALTIME_TIMESTAMP\":\"{}\",\"__MONOTONIC_TIMESTAMP\":\"{}\",\"__SEQNUM\":\"{}\",\
             \"_BOOT_ID\":\"062359e1029c423c0c0b4c9cf5216a14\",\"PRIORITY\":\"{level}\",\
             \"_HOSTNAME\":\"ferrix\",\"MESSAGE\":\"line {n}\",\"SYSLOG_IDENTIFIER\":\"{}\",\
             \"_TRANSPORT\":\"console\"}}\n",
            1_792_776_100_000_000_u64 + n as u64 * 1_000_000,
            n as u64 * 1_000_000,
            n + 1,
            if n.is_multiple_of(2) { "even" } else { "odd" }
        ));
    }
    let read = crate::json::read(&text);
    assert_eq!(read.records.len(), levels.len());
    read.records
}

/// The colours down the middle of the table's strip, top to bottom, each
/// run of one colour once: `R` red, `A` amber, `.` anything else; the
/// part-in-view outline's ink is passed over.
fn strip_runs(levels: &[u8]) -> String {
    let mut app = App::new(
        levels_log(levels),
        Origin::File("levels.jsonl".into()),
        "levels".to_owned(),
    );
    app.act(&Action::Zoom(0));
    app.page = Page::Logs;
    let mut view = view_of(app, (1440, 1000));
    let frame = view.frame((1440, 1000), 1.0).expect("a frame");
    // The strip: the Logs page's 12-wide canvas.
    let mut stack = vec![tole::Element::id(&view.logs)];
    let mut strip = None;
    while let Some(item) = stack.pop() {
        if matches!(view.ui.kind(item), Some(tole::Kind::Canvas(_)))
            && view
                .ui
                .common(item)
                .is_some_and(|common| (view.ui.get(common.width) - 12.0).abs() < 0.5)
        {
            strip = view.ui.rect_of(item);
        }
        stack.extend(view.ui.children(item).iter().rev());
    }
    let strip = strip.expect("the strip is drawn");
    let x = (strip.x + 6.0) as u32;
    let red = crate::theme::RED;
    let amber = crate::theme::AMBER;
    let ink = crate::theme::Theme::of(crate::theme::Mode::Light).ink;
    let mut out = String::new();
    for y in strip.y as u32..(strip.y + strip.h) as u32 {
        let pixel = frame.pixel(x, y).expect("in the frame");
        let rgb = (pixel.red(), pixel.green(), pixel.blue());
        let near = |c: crate::theme::Color| {
            rgb.0.abs_diff(c.0) < 24 && rgb.1.abs_diff(c.1) < 24 && rgb.2.abs_diff(c.2) < 24
        };
        let mark = if near(red) {
            'R'
        } else if near(amber) {
            'A'
        } else if near(ink) || rgb.0 < 140 {
            continue;
        } else {
            '.'
        };
        if !out.ends_with(mark) {
            out.push(mark);
        }
    }
    out
}

#[test]
fn a_run_of_errors_is_one_block_in_the_strip() {
    let runs = strip_runs(&[3; 50]);
    assert_eq!(runs.trim_end_matches('.'), "R", "one red block: {runs}");
}

#[test]
fn an_error_a_warning_and_an_error_are_three_touching_blocks() {
    let mut levels = vec![6_u8; 20];
    levels.extend([3, 4, 3]);
    levels.extend([6; 20]);
    let runs = strip_runs(&levels);
    assert_eq!(runs, ".RAR.", "three blocks, touching: {runs}");
}

#[test]
fn the_record_pane_names_its_fields_in_groups_and_copies_them_whole() {
    let mut app = App::new(
        log(),
        Origin::File("sample.jsonl".into()),
        "sample".to_owned(),
    );
    app.act(&Action::Zoom(0));
    app.page = Page::Logs;
    let index = app
        .log
        .entries
        .iter()
        .position(|entry| entry.record.seqnum == 317)
        .expect("record 317");
    app.act(&Action::Select(index));
    let mut view = view_of(app, (1440, 1000));
    for shown in [
        "ORIGIN",
        "STORAGE",
        "Program",
        "Transport",
        "Cursor",
        "Boot",
    ] {
        assert!(text_at(&mut view, shown).is_some(), "{shown} is shown");
    }
    assert!(
        text_at(&mut view, "_TRANSPORT").is_none(),
        "raw keys are not"
    );
    // The boot's id is whole, over two lines, not cut with an ellipsis.
    assert!(text_at(&mut view, "062359e1029c423c0c0b4c9cf5216a").is_some());
    // A press on a field copies its whole value.
    let _ = click(&mut view, "Boot");
    assert_eq!(
        view.app.borrow_mut().clipboard.take().as_deref(),
        Some("062359e1029c423c0c0b4c9cf5216a14")
    );
    // Raw names: as lode keeps them.
    let _ = click(&mut view, "raw names");
    assert!(text_at(&mut view, "_TRANSPORT").is_some());
}

/// A JSONL export of records with these fields, one a second, besides
/// what every record has.
fn jsonl(records: &[&[(&str, &str)]]) -> String {
    let mut text = String::new();
    for (n, fields) in records.iter().enumerate() {
        text.push_str(&format!(
            "{{\"__REALTIME_TIMESTAMP\":\"{}\",\"__MONOTONIC_TIMESTAMP\":\"{}\",\"__SEQNUM\":\"{}\",\
             \"_BOOT_ID\":\"062359e1029c423c0c0b4c9cf5216a14\",\"_HOSTNAME\":\"ferrix\",\"_TRANSPORT\":\"kernel\"",
            1_792_776_100_000_000_u64 + n as u64 * 1_000_000,
            n as u64 * 1_000_000,
            n + 1,
        ));
        for (key, value) in *fields {
            text.push_str(&format!(",\"{key}\":\"{value}\""));
        }
        text.push_str("}\n");
    }
    text
}

/// Crashes of three programs, an app's error shown without one, and the
/// screenshot lode kept of the third crash, in `dir`.
fn crash_fixture(dir: &std::path::Path) -> std::path::PathBuf {
    let crash = |program: &'static str,
                 signal: &'static str,
                 pc: &'static str|
     -> Vec<(&'static str, &'static str)> {
        vec![
            ("PRIORITY", "3"),
            ("SYSLOG_IDENTIFIER", "kernel"),
            ("MESSAGE", "a program ended by a signal"),
            ("MESSAGE_ID", "5a1d3f9c8e2b4c7d9f0a1b2c3d4e5f60"),
            ("CRASH_PID", "291"),
            ("CRASH_COMM", program),
            ("CRASH_SIGNAL", signal),
            ("CRASH_PC", pc),
            ("CRASH_TRAP", "Breakpoint"),
        ]
    };
    let plain = vec![
        ("PRIORITY", "6"),
        ("SYSLOG_IDENTIFIER", "hyprix"),
        ("MESSAGE", "frame drawn"),
    ];
    let web = crash("steamwebhelper", "SIGTRAP", "libcef.so+0x1d2e3f");
    let foot = crash("foot", "SIGSEGV", "0x4012ab");
    let app = vec![
        ("PRIORITY", "3"),
        ("SYSLOG_IDENTIFIER", "wine"),
        ("FERRIX_EVENT", "app-error"),
        ("_COMM", "endfield.exe"),
        ("MESSAGE", "SuspendThread loop failed"),
    ];
    let capture = vec![
        ("PRIORITY", "6"),
        ("SYSLOG_IDENTIFIER", "lode"),
        ("MESSAGE", "screenshot of the screen at record 6"),
        ("CAPTURE_OF", "6"),
        ("CAPTURE", "stored"),
        ("CAPTURE_FILE", "captures/shot.qoi"),
    ];
    let records: Vec<&[(&str, &str)]> = vec![
        &plain, &web, &plain, &foot, &app, &web, &capture, &plain, &web, &plain,
    ];
    std::fs::create_dir_all(dir.join("captures")).expect("made");
    // A 4x2 picture, red then blue.
    let xrgb: Vec<u8> = (0..8)
        .flat_map(|i| {
            if i % 2 == 0 {
                [0, 0, 255, 0]
            } else {
                [255, 0, 0, 0]
            }
        })
        .collect();
    let qoi = ferrix_lode_capture::qoi::encode(4, 2, 16, &xrgb).expect("encoded");
    std::fs::write(dir.join("captures/shot.qoi"), qoi).expect("written");
    let path = dir.join("crashes.jsonl");
    std::fs::write(&path, jsonl(&records)).expect("written");
    path
}

#[test]
fn crashes_are_grouped_newest_first_with_their_screenshot_and_their_log() {
    let dir = std::env::temp_dir().join(format!("lode-viewer-crashes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = crash_fixture(&dir);
    let read = crate::json::read(&std::fs::read_to_string(&path).expect("read"));
    let mut app = App::new(
        Log::new(read.records),
        Origin::File(path),
        "crashes".to_owned(),
    );
    assert_eq!(app.crashes(), 5, "the sidebar's count");
    app.act(&Action::Crashes);
    let (groups, _) = app.crash_groups();
    let named: Vec<(&str, usize)> = groups
        .iter()
        .map(|g| (g.program.as_str(), g.crashes.len()))
        .collect();
    assert_eq!(
        named,
        [("steamwebhelper", 3), ("endfield.exe", 1), ("foot", 1)],
        "newest first"
    );
    let mut view = view_of(app, (1440, 1000));
    assert!(text_at(&mut view, "3×").is_some());
    assert!(text_at(&mut view, "libcef.so+0x1d2e3f · SIGTRAP · last 17:21:48").is_some());
    let _ = click(&mut view, "steamwebhelper");
    assert!(text_at(&mut view, "steamwebhelper · SIGTRAP · 3×").is_some());
    // The newest is shown: its fields, the log around it, no screenshot.
    assert!(text_at(&mut view, "Code address").is_some());
    assert!(text_at(&mut view, "No screenshot was kept of it.").is_some());
    // The one before has its screenshot.
    let _ = click(&mut view, "pid 291 · libcef.so+0x1d2e3f · Breakpoint");
    let _ = view.step();
    assert_eq!(
        view.app.borrow().crash_pick,
        0,
        "the first row is the newest"
    );
    view.app.borrow_mut().crash_pick = 1;
    view.show();
    view.ui.update();
    assert!(text_at(&mut view, "No screenshot was kept of it.").is_none());
    let frame = view.frame((1440, 1000), 1.0).expect("a frame");
    let red = (0..frame.width())
        .flat_map(|x| (0..frame.height()).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let p = frame.pixel(x, y).expect("in it");
            p.red() == 255 && p.green() == 0 && p.blue() == 0
        })
        .count();
    assert!(
        red > 1000,
        "the screenshot is drawn, scaled up: {red} red pixels"
    );
    // To look at by hand: LODE_VIEWER_TEST_SHOTS=DIR keeps the frame.
    if let Some(shots) = std::env::var_os("LODE_VIEWER_TEST_SHOTS") {
        let png = tole::png::encode(frame.width(), frame.height(), frame.data());
        let _ = std::fs::write(std::path::Path::new(&shots).join("crashes.png"), png);
    }
    // Open in Logs: the record there, selected.
    let _ = click(&mut view, "Open in Logs");
    assert_eq!(view.app.borrow().page, Page::Logs);
    assert!(view.app.borrow().selected.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saved_queries_are_named_renamed_deleted_undone_and_kept_in_their_file() {
    let dir = std::env::temp_dir().join(format!("lode-viewer-saved-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("made");
    let file = dir.join("viewer-queries.conf");
    let mut app = App::new(
        log(),
        Origin::File("sample.jsonl".into()),
        "sample".to_owned(),
    );
    app.load_saved(file.clone());
    app.act(&Action::Zoom(0));
    app.act(&Action::QueryText("id=msr".to_owned()));
    app.act(&Action::QueryAccept);
    let mut view = view_of(app, (1440, 1000));
    let names = |view: &crate::ui::View| -> Vec<String> {
        view.app
            .borrow()
            .saved
            .iter()
            .map(|s| s.name.clone())
            .collect()
    };
    let type_in = |view: &mut crate::ui::View, text: &str| {
        for c in text.chars() {
            key(view, &c.to_string(), &c.to_string(), false);
        }
        key(view, "Return", "", false);
        let _ = view.step();
    };
    // + new asks for a name.
    let _ = click(&mut view, "+ new");
    assert!(text_at(&mut view, "a name, then Return").is_some());
    type_in(&mut view, "msr");
    assert_eq!(names(&view), ["triage", "gpu", "msr"]);
    assert!(
        std::fs::read_to_string(&file)
            .expect("kept")
            .contains("msr = id=msr")
    );
    // A double click renames; a built-in one renamed stays hidden.
    let rect = sidebar_text(&mut view, "@gpu", 0.0).expect("@gpu");
    for _ in 0..2 {
        view.ui.pointer_move(rect.x + 4.0, rect.y + 4.0);
        view.ui.pointer_press();
        view.ui.pointer_release();
    }
    let _ = view.step();
    type_in(&mut view, "graphics");
    assert_eq!(names(&view), ["triage", "graphics", "msr"]);
    let kept = std::fs::read_to_string(&file).expect("kept");
    assert!(kept.contains("hidden = gpu"), "{kept}");
    // Deleted from its menu, and undone from the toast.
    let rect = sidebar_text(&mut view, "@graphics", 0.0).expect("@graphics");
    view.ui.pointer_move(rect.x + 4.0, rect.y + 4.0);
    let _ = view.step();
    let dots = sidebar_text(&mut view, "···", rect.y).expect("its menu's dots");
    view.ui
        .pointer_move(dots.x + dots.w / 2.0, dots.y + dots.h / 2.0);
    view.ui.pointer_press();
    view.ui.pointer_release();
    let _ = view.step();
    let _ = click(&mut view, "Delete");
    assert_eq!(names(&view), ["triage", "msr"]);
    assert!(text_at(&mut view, "@graphics deleted").is_some());
    let _ = click(&mut view, "Undo");
    assert_eq!(names(&view), ["triage", "graphics", "msr"]);
    // The built-in ones back as they came.
    view.app.borrow_mut().act(&Action::SavedRestore);
    assert_eq!(names(&view), ["triage", "gpu", "graphics", "msr"]);
    // The file read again is the list.
    let mut again = App::new(
        log(),
        Origin::File("sample.jsonl".into()),
        "sample".to_owned(),
    );
    again.load_saved(file);
    let again: Vec<String> = again.saved.iter().map(|s| s.name.clone()).collect();
    assert_eq!(again, ["triage", "gpu", "graphics", "msr"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The table: its `contentY`, how far it can scroll, and whether it
/// follows its end.
fn table_state(view: &mut crate::ui::View) -> (f32, f32, bool) {
    let mut stack = vec![tole::Element::id(&view.logs)];
    while let Some(item) = stack.pop() {
        if let (Some(list), Some(common)) = (view.ui.list_view(item), view.ui.common(item)) {
            let y = view.ui.get(list.flick.content_y);
            let most = view.ui.get(list.flick.content_height) - view.ui.get(common.height);
            return (y, most.max(0.0), view.ui.get(list.follow_end));
        }
        stack.extend(view.ui.children(item).iter().copied());
    }
    panic!("no table");
}

#[test]
fn live_at_the_end_the_newest_row_stays_at_the_tables_foot_as_records_come() {
    // The customer's two: the view jumping past its last row as a record
    // came, and the view staying put while records came under it.
    let mut app = App::new(
        levels_log(&[6; 80]),
        Origin::File("levels.jsonl".into()),
        "levels".to_owned(),
    );
    app.act(&Action::Zoom(0));
    app.page = Page::Logs;
    app.live = true;
    let mut view = view_of(app, (1440, 1000));
    let (_, most, follow) = table_state(&mut view);
    assert!(most > 0.0 && !follow);
    // The wheel down to the end, over the table.
    let table = text_at(&mut view, "line 3").expect("a row is shown");
    view.ui.pointer_move(table.x + 20.0, table.y);
    for _ in 0..2000 {
        view.ui.pointer_wheel(1.0);
    }
    view.ui.update();
    view.pull();
    let (y, most, follow) = table_state(&mut view);
    assert!((y - most).abs() < 0.01 && follow, "{y} of {most}");
    // A hundred records a second, shown each 50 ms.
    let mut next = 80;
    for _ in 0..40 {
        let records = levels_records(&[6; 5], next);
        next += 5;
        view.pull();
        let added = view.app.borrow_mut().arrived(records);
        assert_eq!(added, 5);
        view.show();
        view.ui.update();
        let (y, most, follow) = table_state(&mut view);
        assert!(follow, "rows coming never stop the following");
        assert!(
            (y - most).abs() < 0.01,
            "the view at {y}, its end at {most}"
        );
        let last = format!("line {}", next - 1);
        let row = text_at(&mut view, &last).unwrap_or_else(|| panic!("{last} is not shown"));
        let end = text_at(&mut view, "— end of window · following —").expect("the end row");
        assert!(row.y < end.y && end.y < 1000.0, "{row:?} then {end:?}");
    }
    // Scrolled up: no longer following, and records leave the view be;
    // the Follow button is shown.
    view.ui.pointer_wheel(-20.0);
    view.ui.update();
    view.pull();
    let (y, _, follow) = table_state(&mut view);
    assert!(!follow);
    let records = levels_records(&[6; 5], next);
    let _ = view.app.borrow_mut().arrived(records);
    view.show();
    view.ui.update();
    assert_eq!(table_state(&mut view).0, y);
    let follow = text_at(&mut view, "↓ Follow").expect("the Follow button");
    view.ui.pointer_move(follow.x + 4.0, follow.y + 4.0);
    view.ui.pointer_press();
    view.ui.pointer_release();
    view.ui.update();
    let (y, most, follow) = table_state(&mut view);
    assert!(follow && (y - most).abs() < 0.01);
}

#[test]
fn a_tombstone_links_both_ways_its_storm_counts_and_its_screenshot_is_a_blob() {
    use ferrix_lode_core::blob;
    let dir = std::env::temp_dir().join(format!("lode-viewer-tombstones-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("blobs")).expect("made");
    // The screenshot, a 4x2 red picture, as lode keeps it: a blob in a
    // pack with no index yet, named by the capture record.
    let xrgb: Vec<u8> = (0..8).flat_map(|_| [0, 0, 255, 0]).collect();
    let qoi = ferrix_lode_capture::qoi::encode(4, 2, 16, &xrgb).expect("encoded");
    let id = blob::id_of(&qoi);
    let mut pack = blob::PACK_MAGIC.to_vec();
    pack.extend(blob::encode_entry(&id, "qoi", &qoi));
    std::fs::write(dir.join("blobs/pack-0000000000000001.fxb"), pack).expect("written");
    let reference = blob::Reference {
        id,
        size: qoi.len() as u64,
        kind: "qoi".to_owned(),
    }
    .text();
    let boot = "062359e1029c423c0c0b4c9cf5216a14";
    // Records 1..: two of the process's lines, the crash (3), its
    // capture, its tombstone, a storm's repeat record.
    let tail_first = format!("i=1;b={boot}");
    let tail_last = format!("i=2;b={boot}");
    let of = format!("i=3;b={boot}");
    let line = vec![
        ("PRIORITY", "6"),
        ("SYSLOG_IDENTIFIER", "foot"),
        ("MESSAGE", "drawing"),
    ];
    let crash = vec![
        ("PRIORITY", "3"),
        ("SYSLOG_IDENTIFIER", "kernel"),
        ("MESSAGE", "a program ended by a signal"),
        ("MESSAGE_ID", "5a1d3f9c8e2b4c7d9f0a1b2c3d4e5f60"),
        ("CRASH_PID", "291"),
        ("CRASH_COMM", "foot"),
        ("CRASH_PROGRAM", "/bin/foot"),
        ("CRASH_SIGNAL", "SIGSEGV"),
        ("CRASH_PC", "0x4012ab"),
        ("CRASH_MODULE", "foot+0x12ab"),
    ];
    let capture = vec![
        ("PRIORITY", "6"),
        ("SYSLOG_IDENTIFIER", "lode"),
        ("MESSAGE", "screenshot of the screen at record 3"),
        ("CAPTURE_OF", "3"),
        ("CAPTURE", "stored"),
        ("BLOB_SCREENSHOT", reference.as_str()),
    ];
    let tombstone = vec![
        ("PRIORITY", "3"),
        ("SYSLOG_IDENTIFIER", "lode"),
        ("MESSAGE", "tombstone: foot ended by SIGSEGV at foot+0x12ab"),
        (
            "MESSAGE_ID",
            ferrix_lode_core::tombstone::MESSAGE_ID_TOMBSTONE,
        ),
        ("TOMBSTONE", "1"),
        ("TOMBSTONE_SIGNATURE", "/bin/foot foot+0x12ab SIGSEGV"),
        ("TOMBSTONE_OF", of.as_str()),
        ("TOMBSTONE_TAIL_FIRST", tail_first.as_str()),
        ("TOMBSTONE_TAIL_LAST", tail_last.as_str()),
        ("TOMBSTONE_REPEATS", "1"),
    ];
    let storm = vec![
        ("PRIORITY", "4"),
        ("SYSLOG_IDENTIFIER", "lode"),
        ("MESSAGE", "foot crashed again, 12 times this boot"),
        ("MESSAGE_ID", ferrix_lode_core::tombstone::MESSAGE_ID_REPEAT),
        ("TOMBSTONE_SIGNATURE", "/bin/foot foot+0x12ab SIGSEGV"),
        ("TOMBSTONE_REPEATS", "12"),
        ("TOMBSTONE_FIRST", of.as_str()),
        ("TOMBSTONE_LAST", of.as_str()),
    ];
    let records: Vec<&[(&str, &str)]> = vec![&line, &line, &crash, &capture, &tombstone, &storm];
    let path = dir.join("tombstones.jsonl");
    std::fs::write(&path, jsonl(&records)).expect("written");
    let read = crate::json::read(&std::fs::read_to_string(&path).expect("read"));
    let mut app = App::new(Log::new(read.records), Origin::File(path), "t".to_owned());
    // Grouped by its signature's place, the storm's count shown.
    app.act(&Action::Crashes);
    let (groups, _) = app.crash_groups();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].address, "foot+0x12ab");
    assert_eq!(groups[0].repeats, 12, "the storm counts more than kept");
    // From the tombstone's log line to its crash, and back.
    assert_eq!(crate::crashes::linked(&app.log, 4).map(|l| l.0), Some(2));
    assert_eq!(crate::crashes::linked(&app.log, 5).map(|l| l.0), Some(2));
    assert_eq!(crate::crashes::tombstone_of(&app.log, 2), Some(4));
    assert_eq!(crate::crashes::tail_of(&app.log, 4), Some((0, 1)));
    app.act(&Action::ShowCrash(2));
    assert_eq!(app.crash_shown().map(|c| c.index), Some(2));
    let mut view = view_of(app, (1440, 1000));
    assert!(text_at(&mut view, "12×").is_some());
    assert!(text_at(&mut view, "Tombstone").is_some());
    assert!(text_at(&mut view, "Last lines").is_some());
    // The screenshot, read from the pack now the crash is shown.
    assert!(text_at(&mut view, "No screenshot was kept of it.").is_none());
    assert!(text_at(&mut view, "Its screenshot could not be read.").is_none());
    let frame = view.frame((1440, 1000), 1.0).expect("a frame");
    let red = (0..frame.width())
        .flat_map(|x| (0..frame.height()).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let p = frame.pixel(x, y).expect("in it");
            p.red() == 255 && p.green() == 0 && p.blue() == 0
        })
        .count();
    assert!(red > 1000, "the blob's picture is drawn: {red} red pixels");
    // The tombstone in Logs, then its crash again from the pane's link.
    view.app.borrow_mut().act(&Action::CrashTombstone);
    view.show();
    view.ui.update();
    assert_eq!(view.app.borrow().selected, Some(4));
    assert!(text_at(&mut view, "Its crash on the Crashes page ›").is_some());
    let _ = click(&mut view, "Its crash on the Crashes page ›");
    let _ = view.step();
    assert_eq!(view.app.borrow().page, Page::Crashes);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_screenshot_lode_could_not_keep_is_said_and_an_old_stores_file_still_read() {
    let dropped = vec![
        ("PRIORITY", "6"),
        ("SYSLOG_IDENTIFIER", "lode"),
        ("MESSAGE", "screenshot of the screen at record 1"),
        ("CAPTURE_OF", "1"),
        ("BLOB_DROPPED", "the blob queue was full"),
    ];
    let records: Vec<&[(&str, &str)]> = vec![&dropped];
    let read = crate::json::read(&jsonl(&records));
    let log = Log::new(read.records);
    assert_eq!(
        crate::crashes::screenshot_of(&log, 1, &[]),
        crate::crashes::Screenshot::Dropped("the blob queue was full".to_owned())
    );
    assert_eq!(
        crate::crashes::screenshot_of(&log, 2, &[]),
        crate::crashes::Screenshot::None
    );
}

/// How many of the window's three buttons -- 32 by 26 at the header's
/// right end, of a window 1440 wide -- are shown.
fn window_buttons_shown(view: &mut crate::ui::View) -> usize {
    let spots = [1440.0 - 104.0, 1440.0 - 70.0, 1440.0 - 36.0];
    let mut found: Vec<f32> = Vec::new();
    let mut stack: Vec<tole::ItemId> = view.ui.roots().to_vec();
    while let Some(item) = stack.pop() {
        let shown = view
            .ui
            .common(item)
            .is_some_and(|common| view.ui.get(common.visible));
        if !shown {
            continue;
        }
        if let Some(rect) = view.ui.rect_of(item)
            && spots.contains(&rect.x)
            && rect.y == 6.0
            && rect.w == 32.0
            && rect.h == 26.0
            && !found.contains(&rect.x)
        {
            found.push(rect.x);
        }
        stack.extend(view.ui.children(item).iter().rev());
    }
    found.len()
}

/// The window's own minimize, maximize and close, at the header's right
/// end, are there only where the window draws its own frame: the
/// compositor said `client_side`, or draws no title bars. Where it draws
/// its title bar over the window (Yoru's), that bar has them, and with both
/// the window had two rows of them (the N150, 2026-10-28): a click where
/// each would be asks nothing of the window. The Light and Dark switch
/// answers in both, beside the buttons or at the end.
#[test]
fn the_header_has_the_window_buttons_only_where_the_window_draws_its_frame() {
    let spots = [1440.0 - 88.0, 1440.0 - 54.0, 1440.0 - 20.0];
    let press = |view: &mut crate::ui::View, x: f32| {
        view.ui.pointer_move(x, 19.0);
        view.ui.pointer_press();
        view.ui.pointer_release();
        view.step().1
    };
    let fresh = || {
        view_of(
            App::new(
                log(),
                Origin::File("sample.jsonl".into()),
                "sample".to_owned(),
            ),
            (1440, 900),
        )
    };

    // The compositor's title bar: no buttons of the window's own.
    let mut view = fresh();
    assert_eq!(window_buttons_shown(&mut view), 0, "none of its own");
    for x in spots {
        let asked = press(&mut view, x);
        assert!(
            !asked.iter().any(|action| matches!(
                action,
                Action::WindowMinimize | Action::WindowMaximize | Action::WindowClose
            )),
            "a click at {x} asked {asked:?}"
        );
    }
    assert!(!click(&mut view, "Dark").is_empty(), "the switch answers");

    // Its own frame: each button shown asks what it shows.
    let mut view = fresh();
    view.set_own_frame(true);
    view.ui.update();
    assert_eq!(window_buttons_shown(&mut view), 3, "its own three");
    let asked: Vec<Vec<Action>> = spots.iter().map(|&x| press(&mut view, x)).collect();
    assert!(
        matches!(asked[0].as_slice(), [Action::WindowMinimize]),
        "minimize: {asked:?}"
    );
    assert!(
        matches!(asked[1].as_slice(), [Action::WindowMaximize]),
        "maximize: {asked:?}"
    );
    assert!(
        asked[2]
            .iter()
            .any(|action| matches!(action, Action::WindowClose)),
        "close: {asked:?}"
    );
    let mut view = fresh();
    view.set_own_frame(true);
    view.ui.update();
    assert!(
        !click(&mut view, "Dark").is_empty(),
        "the switch answers beside them"
    );
}
