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
    let mut text = String::new();
    for (n, level) in levels.iter().enumerate() {
        text.push_str(&format!(
            "{{\"__REALTIME_TIMESTAMP\":\"{}\",\"__MONOTONIC_TIMESTAMP\":\"{}\",\"__SEQNUM\":\"{}\",\
             \"_BOOT_ID\":\"062359e1029c423c0c0b4c9cf5216a14\",\"PRIORITY\":\"{level}\",\
             \"_HOSTNAME\":\"ferrix\",\"MESSAGE\":\"line {n}\",\"SYSLOG_IDENTIFIER\":\"{}\",\
             \"_TRANSPORT\":\"console\"}}\n",
            1_792_776_100_000_000_u64 + n as u64 * 1_000_000,
            n as u64 * 1_000_000,
            n + 1,
            if n % 2 == 0 { "even" } else { "odd" }
        ));
    }
    let read = crate::json::read(&text);
    assert_eq!(read.records.len(), levels.len());
    Log::new(read.records)
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
