//! The window's tree, driven headless: what a person types and presses
//! reaches the state, the state the tree, and an install the stand-in
//! daemon streams reaches Done.

use std::time::Duration;

use crate::fake::{Behaviour, Fake};
use crate::pages::{Key, Page, State};
use crate::protocol;
use crate::ui::{Action, View};
use crate::window::{Answer, Outcome, System, act, answer};

/// A system that does what the window's does against a stand-in, but
/// waits for each answer, so a test reads what came.
struct Test {
    socket: std::path::PathBuf,
    keyboards: Vec<(String, String)>,
    answers: Vec<Answer>,
    reboots: usize,
}

impl System for Test {
    fn keyboard(&mut self, layout: &str, variant: &str) {
        self.keyboards.push((layout.to_owned(), variant.to_owned()));
    }
    fn disks(&mut self) {
        self.answers
            .push(Answer::Disks(protocol::disks(&self.socket)));
    }
    fn install(&mut self, install: protocol::Install) {
        let answers = &mut self.answers;
        protocol::install(&self.socket, &install, &mut |event| {
            answers.push(Answer::Install(event));
        });
    }
    fn reboot(&mut self) -> Result<(), String> {
        self.reboots += 1;
        protocol::reboot(&self.socket)
    }
}

/// A stand-in on a socket of the test's own.
fn fake(name: &str, behaviour: Behaviour) -> (Fake, Test) {
    let socket = std::env::temp_dir().join(format!(
        "ferrix-installer-{name}-{}.sock",
        std::process::id()
    ));
    let fake = Fake::start(&socket, behaviour).expect("the stand-in starts");
    let test = Test {
        socket: socket.clone(),
        keyboards: Vec::new(),
        answers: Vec::new(),
        reboots: 0,
    };
    (fake, test)
}

/// The tree over `state`, the window's size.
fn view(state: &State) -> View {
    let mut view = View::new(crate::ui::own_fonts()).expect("the tree is made");
    view.resize(crate::window::SIZE);
    view.show(state);
    view.ui.update();
    view
}

/// Whether a shown `Text` says `words`.
fn says(view: &mut View, words: &str) -> bool {
    let mut stack: Vec<tole::ItemId> = view.ui.roots().to_vec();
    while let Some(item) = stack.pop() {
        let shown = view.ui.common(item).is_some_and(|c| view.ui.get(c.visible));
        if !shown {
            continue;
        }
        if let Some(text) = view.ui.text(item)
            && &*view.ui.get(text.text) == words
        {
            return true;
        }
        stack.extend(view.ui.children(item).iter().copied());
    }
    false
}

/// Whether a shown `Text` starts with `words`.
fn says_start(view: &mut View, words: &str) -> bool {
    let mut stack: Vec<tole::ItemId> = view.ui.roots().to_vec();
    while let Some(item) = stack.pop() {
        let shown = view.ui.common(item).is_some_and(|c| view.ui.get(c.visible));
        if !shown {
            continue;
        }
        if let Some(text) = view.ui.text(item)
            && view.ui.get(text.text).starts_with(words)
        {
            return true;
        }
        stack.extend(view.ui.children(item).iter().copied());
    }
    false
}

/// Do what the tree asked and what the stand-in answered, and show the
/// state again.
fn settle(view: &mut View, state: &mut State, test: &mut Test) -> Outcome {
    let mut outcome = Outcome::Stay;
    for action in view.actions() {
        if act(state, action, test) == Outcome::Close {
            outcome = Outcome::Close;
        }
    }
    for each in std::mem::take(&mut test.answers) {
        answer(state, each);
    }
    view.show(state);
    view.ui.update();
    outcome
}

/// Press the row of `state`'s page whose key is `key`, as the tree does.
fn press(state: &mut State, test: &mut Test, key: Key, option: usize) {
    let row = state
        .rows()
        .into_iter()
        .find(|r| r.key == key)
        .unwrap_or_else(|| panic!("no row {key:?}"));
    let _ = act(state, Action::Pressed(row, option), test);
    for each in std::mem::take(&mut test.answers) {
        answer(state, each);
    }
}

#[test]
fn welcome_shows_the_mark_and_the_two_ways() {
    let state = State::default();
    let mut view = view(&state);
    for words in [
        "Welcome to Ferrix",
        "English",
        "Try Ferrix",
        "Install Ferrix",
        "Keyboard",
        "Time zone",
    ] {
        assert!(says(&mut view, words), "{words} is not shown");
    }
    assert!(!says(&mut view, "Continue"), "Welcome has no foot");
}

#[test]
fn try_ferrix_closes_the_window() {
    let (_fake, mut test) = fake("try", Behaviour::default());
    let mut state = State::default();
    let row = state
        .rows()
        .into_iter()
        .find(|r| r.key == Key::Start)
        .expect("the buttons");
    assert_eq!(
        act(&mut state, Action::Pressed(row, 0), &mut test),
        Outcome::Close
    );
}

#[test]
fn a_layout_chosen_is_sent_to_the_live_session() {
    let (_fake, mut test) = fake("keys", Behaviour::default());
    let mut state = State {
        page: Page::Keyboard,
        ..State::default()
    };
    press(&mut state, &mut test, Key::Layout("fr"), 0);
    press(&mut state, &mut test, Key::Variant, 2);
    assert_eq!(
        test.keyboards,
        [
            ("fr".to_owned(), String::new()),
            ("fr".to_owned(), "bepo".to_owned())
        ]
    );
    let mut view = view(&state);
    assert!(says(&mut view, "French"));
    assert!(says(&mut view, "BÉPO"));
}

#[test]
fn typed_into_the_try_line_the_keys_stay_in_it() {
    let mut state = State {
        page: Page::Keyboard,
        ..State::default()
    };
    let (_fake, mut test) = fake("type", Behaviour::default());
    let mut view = view(&state);
    view.ui.set_window_active(true);
    assert!(view.focus_field(Page::Keyboard));
    for c in "qwerty".chars() {
        let _ = view.ui.key_press(&tole::KeyEvent::named(&c.to_string()));
        let _ = settle(&mut view, &mut state, &mut test);
    }
    assert_eq!(state.tried, "qwerty");
}

#[test]
fn the_disk_page_offers_the_stand_ins_disks_and_warns_of_erasing() {
    let (_fake, mut test) = fake("disks", Behaviour::default());
    let mut state = State {
        page: Page::Keyboard,
        ..State::default()
    };
    let mut view = view(&state);
    let _ = act(&mut state, Action::Next, &mut test);
    let _ = settle(&mut view, &mut state, &mut test);
    assert_eq!(state.page, Page::Disk);
    for words in [
        "Virtio Block Device",
        "Samsung SSD 970 EVO Plus",
        "Kingston DataTraveler",
        "Erase disk and install Ferrix",
    ] {
        assert!(says(&mut view, words), "{words} is not shown");
    }
    assert!(
        says_start(
            &mut view,
            "Everything on Virtio Block Device (32 GiB, /dev/vdb) is erased"
        ),
        "the erase warning names the disk"
    );
    assert!(
        !says(&mut view, "SanDisk Ultra Fit"),
        "the live disk is offered"
    );
    press(
        &mut state,
        &mut test,
        Key::Disk("/dev/nvme0n1".to_owned()),
        0,
    );
    view.show(&state);
    let where_at = state
        .rows()
        .iter()
        .position(|r| r.title == "Where")
        .expect("the regions' heading");
    view.reveal(where_at);
    view.ui.update();
    assert!(says(&mut view, "Free space"));
    assert!(says(&mut view, "Partition 2 · NTFS · “Windows”"));
    assert!(
        !says_start(&mut view, "Everything on"),
        "free space erases nothing"
    );
}

#[test]
fn no_disk_but_the_live_one_says_so() {
    let (_fake, mut test) = fake(
        "none",
        Behaviour {
            no_disks: true,
            ..Behaviour::default()
        },
    );
    let mut state = State {
        page: Page::Disk,
        ..State::default()
    };
    let _ = crate::window::apply(&mut state, crate::pages::Effect::Disks, &mut test);
    for each in std::mem::take(&mut test.answers) {
        answer(&mut state, each);
    }
    let mut view = view(&state);
    assert!(says(&mut view, "No disk to install on"));
    assert!(!state.can_next());
}

#[test]
fn no_daemon_is_said_and_can_be_asked_again() {
    let mut test = Test {
        socket: std::env::temp_dir().join("ferrix-installer-nobody-here.sock"),
        keyboards: Vec::new(),
        answers: Vec::new(),
        reboots: 0,
    };
    let mut state = State {
        page: Page::Disk,
        ..State::default()
    };
    let _ = crate::window::apply(&mut state, crate::pages::Effect::Disks, &mut test);
    for each in std::mem::take(&mut test.answers) {
        answer(&mut state, each);
    }
    let mut view = view(&state);
    assert!(says(&mut view, "The disks could not be read"));
    assert!(says(&mut view, "Look again"));
}

/// The way from Welcome to Summary, answered.
fn to_summary(test: &mut Test) -> State {
    let mut state = State::default();
    press(&mut state, test, Key::Start, 1);
    let _ = act(&mut state, Action::Next, test);
    for each in std::mem::take(&mut test.answers) {
        answer(&mut state, each);
    }
    press(&mut state, test, Key::Disk("/dev/nvme0n1".to_owned()), 0);
    let _ = act(&mut state, Action::Next, test);
    assert_eq!(state.page, Page::You);
    for (key, text) in [
        (Key::FullName, "Ada Lovelace"),
        (Key::Password, "correct horse"),
        (Key::Confirm, "correct horse"),
    ] {
        let row = state
            .rows()
            .into_iter()
            .find(|r| r.key == key)
            .expect("a field");
        let _ = act(&mut state, Action::Edited(row, text.to_owned()), test);
    }
    let _ = act(&mut state, Action::Next, test);
    assert_eq!(state.page, Page::Zone);
    let _ = act(&mut state, Action::Next, test);
    assert_eq!(state.page, Page::Summary);
    state
}

#[test]
fn an_install_streams_to_done_and_restart_asks_the_daemon() {
    let (fake, mut test) = fake(
        "install",
        Behaviour {
            pace: Duration::ZERO,
            ..Behaviour::default()
        },
    );
    let mut state = to_summary(&mut test);
    let mut view = view(&state);
    assert!(says(&mut view, "Install"));
    assert!(says(&mut view, "Nothing is erased"));
    let _ = act(&mut state, Action::Next, &mut test);
    let _ = settle(&mut view, &mut state, &mut test);
    assert_eq!(state.page, Page::Done);
    assert!(says(&mut view, "Ferrix is installed"));
    assert!(says(
        &mut view,
        "Remove the installation medium, then restart."
    ));
    let heard = fake.heard.lock().expect("heard").install.clone();
    for pair in [
        "target=/dev/nvme0n1",
        "mode=free",
        "first=327155712",
        "last=536868863",
        "user=ada",
        "fullname=Ada Lovelace",
        "hostname=ferrix",
        "keyboard=us",
        "variant=",
        "timezone=UTC",
        "language=en",
    ] {
        assert!(
            heard.iter().any(|h| h == pair),
            "{pair} not sent: {heard:?}"
        );
    }
    press(&mut state, &mut test, Key::Restart, 0);
    assert_eq!(test.reboots, 1);
    assert_eq!(state.status, None, "the daemon said ok");
}

#[test]
fn a_failed_install_shows_why_and_the_log_and_goes_back_to_the_summary() {
    let (_fake, mut test) = fake(
        "fail",
        Behaviour {
            fail: true,
            ..Behaviour::default()
        },
    );
    let mut state = to_summary(&mut test);
    let mut view = view(&state);
    let _ = act(&mut state, Action::Next, &mut test);
    let _ = settle(&mut view, &mut state, &mut test);
    assert_eq!(state.page, Page::Failed);
    assert!(says(&mut view, "The installation failed"));
    assert!(says(
        &mut view,
        "grub-install: /boot/efi is not an EFI partition"
    ));
    assert!(says(&mut view, "Back to the summary"));
    let _ = act(&mut state, Action::Back, &mut test);
    assert_eq!(state.page, Page::Summary);
}

#[test]
fn every_screenshot_draws_in_nocturnes_ground() {
    for (name, state) in crate::cli::shots().expect("the shots") {
        let mut view = view(&state);
        let frame = view.frame(crate::window::SIZE, 1.0).expect("a frame");
        let pixel = frame.pixel(2, 600).expect("a pixel");
        assert_eq!(
            (pixel.red(), pixel.green(), pixel.blue()),
            (0x16, 0x18, 0x26),
            "{name}"
        );
    }
}
