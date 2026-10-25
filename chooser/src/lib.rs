//! Ferrix's file chooser (`chooser`): tole's FileDialog -- the customer's
//! Key File Dialog design (`Fx.Controls.FileDialog`) -- in a window of its
//! own, which portald opens for `org.freedesktop.portal.FileChooser`.
//!
//! `chooser [--save] [--title T] [--name N] [--folder DIR] [--accept PATTERN]
//! [--accept-label L] [--action L]`: the path chosen on standard output and
//! status 0, or nothing and status 1 when the person cancels. `--accept`
//! is the dialog's own pattern (`*.png | [image]`).

use std::process::ExitCode;
use std::rc::Rc;

use compositor_toolkit::Event;
use tole::Element as _;
use tole::window::{Window, WindowOptions};
use tole::{Color, Ui};

tole::include_ui!();

/// What the chooser is asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Asked {
    /// The window's title and the dialog's.
    pub title: String,
    /// Saving: a name is typed, in the folder shown.
    pub save: bool,
    /// The name offered when saving.
    pub name: String,
    /// The folder it opens in.
    pub folder: String,
    /// The dialog's pattern of what it takes; empty for every file.
    pub accept: String,
    /// What the filter's first choice is called.
    pub accept_label: String,
    /// The accept button's words.
    pub action: String,
}

/// The arguments, read; `Err` with what is wrong.
///
/// # Errors
///
/// An argument it does not know, or one without its value.
pub fn read(args: &[String]) -> Result<Asked, String> {
    let mut asked = Asked {
        title: "Choose a file".to_owned(),
        folder: "~".to_owned(),
        accept_label: "Accepted files".to_owned(),
        ..Asked::default()
    };
    let mut words = args.iter();
    while let Some(word) = words.next() {
        let mut value = |name: &str| {
            words
                .next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match word.as_str() {
            "--save" => asked.save = true,
            "--title" => asked.title = value("--title")?,
            "--name" => asked.name = value("--name")?,
            "--folder" => asked.folder = value("--folder")?,
            "--accept" => asked.accept = value("--accept")?,
            "--accept-label" => asked.accept_label = value("--accept-label")?,
            "--action" => asked.action = value("--action")?,
            "--render" => {
                let _ = value("--render")?;
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if asked.action.is_empty() {
        asked.action = if asked.save { "Save" } else { "Open" }.to_owned();
    }
    Ok(asked)
}

/// A path the dialog gave, with `~` made the home it names.
#[must_use]
pub fn expanded(path: &str, home: Option<&str>) -> String {
    match (path.strip_prefix('~'), home) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
            format!("{}{rest}", home.trim_end_matches('/'))
        }
        _ => path.to_owned(),
    }
}

/// A string as the tree holds it.
fn s(value: impl AsRef<str>) -> Rc<str> {
    Rc::from(value.as_ref())
}

/// The faces: the system's, and Inter where the image or Ferrix's tree has
/// it.
fn fonts() -> tole::Fonts {
    let mut fonts = tole::Fonts::system();
    let tree = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../assets/fonts/inter"
    );
    for dir in ["/usr/share/fonts/inter", tree] {
        let path = std::path::Path::new(dir);
        if path.is_dir() {
            let _ = fonts.add_dir(path);
        }
    }
    fonts
}

/// The tree, told what it is for.
///
/// # Errors
///
/// The markup's items could not be made.
pub fn view(asked: &Asked) -> Result<(Ui, Chooser), String> {
    let mut ui = Ui::new(fonts());
    let chooser = Chooser::new(&mut ui, None).ok_or("the chooser's tree was not made")?;
    chooser.heading.set(&mut ui, s(&asked.title));
    chooser.save.set(&mut ui, asked.save);
    chooser.name.set(&mut ui, s(&asked.name));
    chooser.start.set(&mut ui, s(&asked.folder));
    chooser.accepts.set(&mut ui, s(&asked.accept));
    chooser.accepts_label.set(&mut ui, s(&asked.accept_label));
    chooser
        .accepts_noun
        .set(&mut ui, s(asked.accept_label.to_lowercase()));
    chooser.action.set(&mut ui, s(&asked.action));
    let places: [(&str, &str); 5] = [
        ("Home", "~"),
        ("Documents", "~/Documents"),
        ("Downloads", "~/Downloads"),
        ("Pictures", "~/Pictures"),
        ("Computer", "/"),
    ];
    chooser
        .names
        .set(&mut ui, places.iter().map(|(n, _)| s(*n)).collect());
    chooser
        .paths
        .set(&mut ui, places.iter().map(|(_, p)| s(*p)).collect());
    ui.open_popup(chooser.id());
    Ok((ui, chooser))
}

/// What the dialog answered since the last call: `Some(Some(path))` for a
/// choice, `Some(None)` for a cancel.
pub fn answered(ui: &mut Ui, chooser: &Chooser) -> Option<Option<String>> {
    for each in ui.take_signals() {
        match chooser.signal(&each) {
            Some(ChooserSignal::Picked(path)) => return Some(Some(path.to_string())),
            Some(ChooserSignal::Cancelled) => return Some(None),
            None => {}
        }
    }
    None
}

/// The dialog drawn into a PNG at `path`, for the README and a look.
fn render(asked: &Asked, path: &str) -> ExitCode {
    let Ok((mut ui, _chooser)) = view(asked) else {
        return ExitCode::from(2);
    };
    ui.update();
    let Some(mut frame) = tole::Frame::new((900, 580), 1.0, Color::hex(0x16_1826)) else {
        return ExitCode::from(2);
    };
    let _ = ui.render_into(&mut frame);
    let pixmap = frame.pixmap();
    let png = tole::png::encode(pixmap.width(), pixmap.height(), pixmap.data());
    if std::fs::write(path, png).is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    }
}

/// The program.
#[must_use]
pub fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let asked = match read(&args) {
        Ok(asked) => asked,
        Err(why) => {
            eprintln!("chooser: {why}");
            return ExitCode::from(2);
        }
    };
    if let Some(at) = args.iter().position(|w| w == "--render") {
        return render(
            &asked,
            args.get(at + 1).map_or("chooser.png", String::as_str),
        );
    }
    let (mut ui, chooser) = match view(&asked) {
        Ok(made) => made,
        Err(why) => {
            eprintln!("chooser: {why}");
            return ExitCode::from(2);
        }
    };
    let options = WindowOptions {
        title: asked.title.clone(),
        app_id: "rocks.magical.chooser".to_owned(),
        size: (900, 580),
        background: Color::hex(0x16_1826),
        ..WindowOptions::default()
    };
    let mut window = match Window::open(chooser.common(), options) {
        Ok(window) => window,
        Err(error) => {
            eprintln!("chooser: no compositor: {error}");
            return ExitCode::from(2);
        }
    };
    let home = std::env::var("HOME").ok();
    loop {
        let happened = match window.dispatch(&mut ui, None) {
            Ok(happened) => happened,
            Err(error) => {
                eprintln!("chooser: the compositor went away: {error}");
                return ExitCode::from(2);
            }
        };
        if happened
            .iter()
            .any(|event| matches!(event, Event::Closed(_) | Event::CloseRequested(_)))
        {
            return ExitCode::FAILURE;
        }
        match answered(&mut ui, &chooser) {
            Some(Some(path)) => {
                println!("{}", expanded(&path, home.as_deref()));
                return ExitCode::SUCCESS;
            }
            Some(None) => return ExitCode::FAILURE,
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Asked, answered, expanded, read, view};

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn the_arguments_say_what_it_is_for() {
        let asked = read(&args(&[
            "--save",
            "--name",
            "page.html",
            "--folder",
            "/tmp",
        ]))
        .expect("read");
        assert_eq!(
            asked,
            Asked {
                title: "Choose a file".to_owned(),
                save: true,
                name: "page.html".to_owned(),
                folder: "/tmp".to_owned(),
                accept: String::new(),
                accept_label: "Accepted files".to_owned(),
                action: "Save".to_owned(),
            }
        );
        assert!(read(&args(&["--title"])).is_err());
        assert!(read(&args(&["--nope"])).is_err());
    }

    #[test]
    fn the_home_is_spelled_out() {
        assert_eq!(
            expanded("~/Downloads/a.txt", Some("/home/ada")),
            "/home/ada/Downloads/a.txt"
        );
        assert_eq!(expanded("~", Some("/home/ada/")), "/home/ada");
        assert_eq!(expanded("~ada/x", Some("/home/ada")), "~ada/x");
        assert_eq!(expanded("/etc/x", None), "/etc/x");
    }

    /// Saving: a name typed and Return gives the folder and the name.
    #[test]
    fn a_name_typed_is_the_answer() {
        let dir = std::env::temp_dir().join(format!("chooser-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("made");
        let asked = read(&args(&[
            "--save",
            "--name",
            "page.html",
            "--folder",
            dir.to_str().expect("utf-8"),
        ]))
        .expect("read");
        let (mut ui, chooser) = view(&asked).expect("made");
        ui.set_window_active(true);
        ui.update();
        let _ = ui.key_press(&tole::KeyEvent::named("Return"));
        let got = answered(&mut ui, &chooser);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(got, Some(Some(format!("{}/page.html", dir.display()))));
    }
}
