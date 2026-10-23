//! The window's tree, driven headless: what a person types and presses
//! reaches the state, and the state the tree.

use crate::apps::App;
use crate::look::Palette;
use crate::pages::{Kind, Nav, Page, State};
use crate::ui::{Action, View};
use crate::window::act;

/// The design's own fonts, beside the sources.
fn fonts() -> tole::Fonts {
    let mut fonts = tole::Fonts::new();
    let _ = fonts.add_dir(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/share/fonts"
    )));
    fonts
}

/// The design's applications, nothing in game mode yet.
fn state(page: Page) -> State {
    let mut state = State::new(page);
    state.user = "ada".to_owned();
    state.hostname = "ferrix".to_owned();
    state.games.apps = vec![
        App::new("supertuxkart.desktop", "SuperTuxKart", None, None),
        App::new("ioquake3.desktop", "ioquake3", None, None),
        App::new("foot.desktop", "Terminal", None, None),
    ];
    state
}

/// The tree over `state`, 1280 × 860.
fn view(state: &State) -> View {
    let mut view = View::new(fonts()).expect("the tree is made");
    view.resize((1280, 860));
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

/// Do what the tree asked, and show the state again.
fn settle(view: &mut View, state: &mut State) {
    for action in view.actions() {
        let _ = act(state, action);
    }
    view.show(state);
    view.ui.update();
}

#[test]
fn the_window_shows_its_groups_the_page_and_who_is_signed_in() {
    let mut state = state(Page::Game);
    let mut view = view(&state);
    for words in [
        "Settings",
        "Desktop",
        "Session",
        "Idle and lock",
        "Game mode",
        "ada on ferrix",
    ] {
        assert!(says(&mut view, words), "{words} is not shown");
    }
    assert!(says(&mut view, "SuperTuxKart"));
    // The ground is the design's dark one.
    let frame = view.frame((1280, 860), 1.0).expect("a frame");
    let ground = Palette::new(true, 250.0).bg;
    let pixel = frame.pixel(1200, 800).expect("a pixel");
    assert_eq!(
        (pixel.red(), pixel.green(), pixel.blue()),
        (ground.r, ground.g, ground.b)
    );
    // Light: the same tree, the other palette.
    state.dark = false;
    view.show(&state);
    let frame = view.frame((1280, 860), 1.0).expect("a frame");
    let ground = Palette::new(false, 250.0).bg;
    let pixel = frame.pixel(1200, 800).expect("a pixel");
    assert_eq!(
        (pixel.red(), pixel.green(), pixel.blue()),
        (ground.r, ground.g, ground.b)
    );
}

#[test]
fn typed_into_game_modes_filter_the_list_narrows() {
    let mut state = state(Page::Game);
    let mut view = view(&state);
    view.ui.set_window_active(true);
    view.focus_field();
    for c in "tux".chars() {
        let _ = view.ui.key_press(&tole::KeyEvent::named(&c.to_string()));
    }
    settle(&mut view, &mut state);
    assert_eq!(state.games.filter, "tux");
    let apps: Vec<String> = state
        .rows()
        .into_iter()
        .filter(|r| r.kind == Kind::App)
        .map(|r| r.title)
        .collect();
    assert_eq!(apps, ["SuperTuxKart"]);
    assert!(says(&mut view, "SuperTuxKart"));
    assert!(!says(&mut view, "ioquake3"));
}

#[test]
fn the_search_narrows_the_sidebar_and_says_when_nothing_matches() {
    let mut state = state(Page::Appearance);
    let mut view = view(&state);
    view.ui.set_window_active(true);
    let _ = act(&mut state, Action::Search("ssh".to_owned()));
    view.show(&state);
    view.ui.update();
    assert!(says(&mut view, "Remote login"));
    assert!(!says(&mut view, "Desktop"));
    let _ = act(&mut state, Action::Search("qwertz".to_owned()));
    view.show(&state);
    view.ui.update();
    assert!(says(&mut view, "No setting matches “qwertz”."));
}

#[test]
fn a_press_on_a_group_opens_it_and_on_a_page_shows_it() {
    let mut state = state(Page::Appearance);
    let mut view = view(&state);
    assert!(!says(&mut view, "Game mode"));
    let _ = act(
        &mut state,
        Action::Navigate(Nav::Group(crate::pages::Group::Session)),
    );
    view.show(&state);
    view.ui.update();
    assert!(says(&mut view, "Game mode"));
    let _ = act(&mut state, Action::Navigate(Nav::Page(Page::Game)));
    view.show(&state);
    view.ui.update();
    assert!(says(
        &mut view,
        "Apps you mark start in game mode from the launcher."
    ));
}
