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
