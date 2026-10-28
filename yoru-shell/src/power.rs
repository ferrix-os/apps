//! The power menu (`yoru-shell --power`, the dock's power button): an
//! overlay with Lock, Restart and Shut down (`ui/power.tole`). Restart and
//! Shut down ask once more, then call the init as the person at the machine
//! may (`svc reboot`, `svc poweroff`; Ferrix's `docs/INIT.md` §10.1); what
//! the init refuses is shown in the card. Lock is offered only where the
//! account can unlock again: not in the live session, whose person has no
//! password. Started again while it is up, it closes.

use std::os::fd::AsRawFd as _;
use std::os::unix::net::UnixListener;
use std::rc::Rc;
use std::time::{Duration, Instant};

use compositor_toolkit::Event;
use tole::window::{
    Anchor, KeyboardInteractivity, Layer, LayerOptions, Margin, Window, WindowOptions,
};
use tole::{Color, Element as _, Ui};

use crate::look::icon;
use crate::ui::{Choice, Power, PowerSignal};
use crate::window::say;

/// The surface's namespace.
pub const NAMESPACE: &str = "yoru-power";

/// The init's client.
const SVC: &str = "/bin/svc";

/// What the menu offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offer {
    /// The lock screen.
    Lock,
    /// `svc reboot`.
    Restart,
    /// `svc poweroff`.
    ShutDown,
}

impl Offer {
    /// The glyph, the label and the line under it.
    #[must_use]
    pub const fn shown(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Offer::Lock => ("lock", "Lock", "Your session stays as it is"),
            Offer::Restart => (
                "arrow-clockwise",
                "Restart",
                "Close everything and start again",
            ),
            Offer::ShutDown => ("power", "Shut down", "Close everything and switch off"),
        }
    }

    /// The question asked before it goes, what it means, and the button's
    /// word; `None` for one that goes at once.
    #[must_use]
    pub const fn question(self) -> Option<(&'static str, &'static str, &'static str)> {
        match self {
            Offer::Lock => None,
            Offer::Restart => Some((
                "Restart now?",
                "Every window closes, and work not saved is lost.",
                "Restart",
            )),
            Offer::ShutDown => Some((
                "Shut down now?",
                "Every window closes, and work not saved is lost.",
                "Shut down",
            )),
        }
    }

    /// The init's verb.
    #[must_use]
    pub const fn verb(self) -> Option<&'static str> {
        match self {
            Offer::Lock => None,
            Offer::Restart => Some("reboot"),
            Offer::ShutDown => Some("poweroff"),
        }
    }
}

/// What is offered, given whether the session is the live one, whose
/// person has no password and so could not unlock.
#[must_use]
pub fn offers(live: bool) -> Vec<Offer> {
    if live {
        vec![Offer::Restart, Offer::ShutDown]
    } else {
        vec![Offer::Lock, Offer::Restart, Offer::ShutDown]
    }
}

/// Whether the kernel's command line is the live session's
/// (`ferrix.live`, a word of its own).
#[must_use]
pub fn is_live(cmdline: &str) -> bool {
    cmdline.split_whitespace().any(|word| word == "ferrix.live")
}

/// A refusal as the card says it: the init's "Permission denied" as what
/// it means for the person, anything else as it came.
#[must_use]
pub fn shown_refusal(why: &str) -> String {
    if why.contains("Permission denied") {
        "Not allowed: only the person at this computer may.".to_owned()
    } else {
        why.to_owned()
    }
}

/// A string as the tree holds it.
fn s(value: impl AsRef<str>) -> Rc<str> {
    Rc::from(value.as_ref())
}

/// What a press asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    /// Close the menu.
    Close,
    /// Offer `n` of those shown.
    Choose(usize),
    /// Go ahead with the question's offer.
    Confirm,
}

/// The tree and what the program keeps of it.
pub struct View {
    /// The tree.
    pub ui: Ui,
    /// The menu.
    pub power: Power,
}

impl std::fmt::Debug for View {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("View").finish_non_exhaustive()
    }
}

impl View {
    /// The tree, drawn with `fonts`, offering `offers`.
    #[must_use]
    pub fn new(fonts: tole::Fonts, offers: &[Offer]) -> Option<Self> {
        let mut ui = Ui::new(fonts);
        let power = Power::new(&mut ui, None)?;
        power.ic_power.set(&mut ui, s(icon("power")));
        power.ic_close.set(&mut ui, s(icon("x")));
        let choices = offers
            .iter()
            .map(|offer| {
                let (glyph, label, hint) = offer.shown();
                Choice {
                    icon: s(icon(glyph)),
                    label: s(label),
                    hint: s(hint),
                }
            })
            .collect::<Vec<_>>();
        power.choices.set(&mut ui, choices);
        Some(View { ui, power })
    }

    /// Its size in logical pixels.
    pub fn resize(&mut self, (width, height): (u32, u32)) {
        let common = self.power.common();
        common.width.set(&mut self.ui, width as f32);
        common.height.set(&mut self.ui, height as f32);
    }

    /// Ask `offer`'s question.
    pub fn ask(&mut self, offer: Offer) {
        let Some((question, detail, verb)) = offer.question() else {
            return;
        };
        let ui = &mut self.ui;
        self.power.question.set(ui, s(question));
        self.power.detail.set(ui, s(detail));
        self.power.verb.set(ui, s(verb));
        self.power.status.set(ui, s(""));
        self.power.stage.set(ui, 1);
    }

    /// Say that `offer` is going, or why it did not.
    pub fn went(&mut self, offer: Offer, refused: Option<&str>) {
        let ui = &mut self.ui;
        match refused {
            None => {
                let going = if offer == Offer::Restart {
                    "Restarting..."
                } else {
                    "Shutting down..."
                };
                self.power.question.set(ui, s(going));
                self.power.detail.set(
                    ui,
                    s("Programs are being closed. This takes a few seconds."),
                );
                self.power.stage.set(ui, 2);
            }
            Some(why) => {
                self.power.status.set(ui, s(shown_refusal(why)));
                self.power.stage.set(ui, 1);
            }
        }
    }

    /// What the tree asked since the last call.
    pub fn take(&mut self) -> Vec<Asked> {
        let emitted = self.ui.take_signals();
        let mut asked = Vec::new();
        for each in &emitted {
            let Some(signal) = self.power.signal(each) else {
                continue;
            };
            asked.extend(match signal {
                PowerSignal::Closed => Some(Asked::Close),
                PowerSignal::Choose(i) => usize::try_from(i).ok().map(Asked::Choose),
                PowerSignal::Confirm => Some(Asked::Confirm),
            });
        }
        asked
    }
}

/// Ask the init for `verb`: `None` when it took it (or is still taking it
/// after a few seconds, the machine on its way down), else what it said.
fn ask_init(verb: &str) -> Option<String> {
    let child = std::process::Command::new(SVC)
        .arg(verb)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => return Some(format!("{SVC}: {error}")),
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return None,
            Ok(Some(_)) => {
                let output = child.wait_with_output().ok();
                let said = output
                    .map(|output| {
                        let mut text = String::from_utf8_lossy(&output.stderr).into_owned();
                        text.push_str(&String::from_utf8_lossy(&output.stdout));
                        text
                    })
                    .unwrap_or_default();
                let said = said.trim();
                return Some(if said.is_empty() {
                    format!("{SVC} {verb} failed")
                } else {
                    said.lines().last().unwrap_or(said).to_owned()
                });
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(None) => return None,
            Err(error) => return Some(format!("{SVC}: {error}")),
        }
    }
}

/// Run the menu until it is closed; the exit status.
#[must_use]
pub fn run() -> i32 {
    crate::switch::once(NAMESPACE, shown).unwrap_or_else(|| {
        say("the power menu was up: closed");
        0
    })
}

/// The menu's loop.
fn shown(listener: Option<&UnixListener>) -> i32 {
    let live = std::fs::read_to_string("/proc/cmdline").is_ok_and(|line| is_live(&line));
    let offered = offers(live);
    let Some(mut view) = View::new(crate::look::fonts(), &offered) else {
        say("err: the power menu's tree could not be made");
        return 1;
    };
    crate::theme::Live::new().tokens.apply(&mut view.ui);
    let layer = LayerOptions {
        output: None,
        layer: Layer::Overlay,
        namespace: NAMESPACE.to_owned(),
        size: (0, 0),
        anchor: Anchor::TOP
            .with(Anchor::BOTTOM)
            .with(Anchor::LEFT)
            .with(Anchor::RIGHT),
        exclusive_zone: -1,
        margin: Margin::default(),
        keyboard: KeyboardInteractivity::Exclusive,
    };
    let options = WindowOptions {
        title: "Power".to_owned(),
        app_id: NAMESPACE.to_owned(),
        size: (1280, 800),
        background: Color::rgba(0, 0, 0, 0),
        clipboard: false,
        layer: Some(layer),
        ..WindowOptions::default()
    };
    let mut window = match Window::open(view.power.common(), options) {
        Ok(window) => window,
        Err(error) => {
            say(&format!(
                "err: no compositor to put the power menu on: {error}"
            ));
            return 1;
        }
    };
    let watch = listener.map(|listener| window.client().watch_fd(listener.as_raw_fd()));
    let mut size = None;
    let mut asking: Option<Offer> = None;
    let mut going = false;
    loop {
        let happened = match window.dispatch(&mut view.ui, None) {
            Ok(happened) => happened,
            Err(error) => {
                say(&format!("err: the compositor went away: {error}"));
                return 1;
            }
        };
        let mut changed = false;
        if let Some(now) = window.size()
            && size != Some(now)
        {
            if size.is_none() {
                say(&format!(
                    "the power menu is up, {}x{}: {}",
                    now.0,
                    now.1,
                    offered
                        .iter()
                        .map(|offer| offer.shown().1)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            size = Some(now);
            view.resize(now);
            changed = true;
        }
        for event in happened {
            match event {
                Event::Closed(_) | Event::CloseRequested(_) => return 0,
                Event::Readable(id) if Some(id) == watch && !going => {
                    say("the power menu was asked again: closed");
                    return 0;
                }
                _ => {}
            }
        }
        for asked in view.take() {
            if going {
                continue;
            }
            match asked {
                Asked::Close => {
                    say("the power menu closed");
                    return 0;
                }
                Asked::Choose(n) => match offered.get(n) {
                    Some(Offer::Lock) => {
                        let hypr = crate::hypr::Hypr::from_env();
                        let done = hypr.is_some_and(|hypr| hypr.dispatch("exec hyprlock"));
                        say(&format!(
                            "power: lock: {}",
                            if done { "ok" } else { "refused" }
                        ));
                        return 0;
                    }
                    Some(&offer) => {
                        say(&format!("power: asked to confirm {}", offer.shown().1));
                        asking = Some(offer);
                        view.ask(offer);
                        changed = true;
                    }
                    None => {}
                },
                Asked::Confirm => {
                    let Some(offer) = asking else {
                        continue;
                    };
                    let Some(verb) = offer.verb() else {
                        continue;
                    };
                    say(&format!("power: {verb} confirmed"));
                    let refused = ask_init(verb);
                    match &refused {
                        None => {
                            say(&format!("power: the init took {verb}"));
                            going = true;
                        }
                        Some(why) => say(&format!("power: the init refused {verb}: {why}")),
                    }
                    view.went(offer, refused.as_deref());
                    changed = true;
                }
            }
        }
        if changed && let Err(error) = window.refresh(&mut view.ui) {
            say(&format!("err: the compositor went away: {error}"));
            return 1;
        }
    }
}

/// The menu drawn `size` logical pixels at `scale`, over Nocturne's
/// ground: with `asking`, its question.
#[must_use]
pub fn render(
    live: bool,
    asking: Option<Offer>,
    size: (u32, u32),
    scale: f32,
) -> Option<tole::Frame> {
    let mut view = View::new(crate::look::fonts(), &offers(live))?;
    view.ui.set_scale(scale);
    view.resize(size);
    if let Some(offer) = asking {
        view.ask(offer);
    }
    view.ui.update();
    let mut frame = tole::Frame::new(size, scale, Color::hex(0x2b_2741))?;
    let _ = view.ui.render_into(&mut frame);
    Some(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_live_session_offers_no_lock() {
        assert!(is_live(
            "BOOT_IMAGE=/k ferrix.root=tmpfs ferrix.live ferrix.efifb\n"
        ));
        assert!(!is_live("ferrix.root=PARTLABEL=root ferrix.lively"));
        assert_eq!(offers(true), [Offer::Restart, Offer::ShutDown]);
        assert_eq!(
            offers(false),
            [Offer::Lock, Offer::Restart, Offer::ShutDown]
        );
    }

    /// Restart and Shut down ask first and name the init's verbs; Lock
    /// goes at once.
    #[test]
    fn restart_and_shut_down_ask_first() {
        assert_eq!(Offer::Restart.verb(), Some("reboot"));
        assert_eq!(Offer::ShutDown.verb(), Some("poweroff"));
        assert!(Offer::Restart.question().is_some());
        assert!(Offer::ShutDown.question().is_some());
        assert_eq!(Offer::Lock.question(), None);
        assert_eq!(Offer::Lock.verb(), None);
    }

    /// Every glyph is in the icon face.
    #[test]
    fn its_glyphs_are_known() {
        for offer in [Offer::Lock, Offer::Restart, Offer::ShutDown] {
            assert!(!icon(offer.shown().0).is_empty(), "{:?}", offer);
        }
        assert!(!icon("power").is_empty());
    }

    #[test]
    fn a_refusal_is_said_plainly() {
        assert_eq!(
            shown_refusal(
                "svc: Permission denied: only root may change the system, and this is uid 1000"
            ),
            "Not allowed: only the person at this computer may."
        );
        assert_eq!(shown_refusal("svc: no init"), "svc: no init");
    }

    /// A press on a choice asks its question; a confirm is taken; a press
    /// outside closes.
    #[test]
    fn the_card_asks_and_closes() {
        let mut view = View::new(crate::look::fonts(), &offers(true)).expect("tree");
        view.resize((800, 600));
        view.ask(Offer::ShutDown);
        view.ui.update();
        assert!(view.take().is_empty());
        view.went(Offer::ShutDown, Some("Permission denied"));
        view.ui.update();
    }

    /// The menu draws: a card in the middle, lighter than the dimmed ground
    /// round it.
    #[test]
    fn the_menu_draws() {
        let frame = render(true, None, (800, 600), 1.0).expect("drawn");
        let pixmap = frame.pixmap();
        let at = |x: u32, y: u32| {
            let pixel = pixmap.pixel(x, y).expect("inside").demultiply();
            u32::from(pixel.red()) + u32::from(pixel.green()) + u32::from(pixel.blue())
        };
        assert!(
            at(400, 300) != at(20, 20),
            "the card stands out of the ground"
        );
        let asking = render(false, Some(Offer::ShutDown), (800, 600), 1.0).expect("drawn");
        assert_eq!(asking.pixmap().width(), 800);
    }
}
