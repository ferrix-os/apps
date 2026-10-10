//! The applications the game-mode page lists: the ones a launcher lists.

use compositor_desktop::entry::{self, Search};

/// One application.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct App {
    /// Its desktop-file ID, which is what the game-mode file names it by.
    pub id: String,
    /// `Name`, in the user's language.
    pub name: String,
    /// What it is, in a few words: `GenericName`, or else `Comment`.
    pub about: Option<String>,
    /// `Icon`: a name in the icon theme, or a file.
    pub icon: Option<String>,
    /// Everything the filter matches against, lowercased.
    haystack: String,
}

impl App {
    /// An application called `name`, known as `id`.
    #[must_use]
    pub fn new(id: &str, name: &str, about: Option<&str>, icon: Option<&str>) -> Self {
        let mut haystack = name.to_lowercase();
        haystack.push('\n');
        // Without the suffix every ID has, or `top` would match them all.
        haystack.push_str(&id.strip_suffix(".desktop").unwrap_or(id).to_lowercase());
        if let Some(about) = about {
            haystack.push('\n');
            haystack.push_str(&about.to_lowercase());
        }
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            about: about.map(str::to_owned),
            icon: icon.map(str::to_owned),
            haystack,
        }
    }

    /// Whether every word of `filter` is somewhere in the name, the ID or
    /// the description, whatever the case. An empty filter matches.
    #[must_use]
    pub fn matches(&self, filter: &str) -> bool {
        filter
            .split_whitespace()
            .all(|word| self.haystack.contains(&word.to_lowercase()))
    }
}

/// Every application installed, sorted by name as a launcher sorts them:
/// what `compositor-desktop` finds for [`Search::listing`], each entry that
/// is shown and has an ID. An entry a launcher hides (`NoDisplay`, `Hidden`,
/// a `TryExec` that is not there) is not offered, since nobody can start it
/// from one.
#[must_use]
pub fn installed(var: &dyn Fn(&str) -> Option<String>) -> Vec<App> {
    entry::find_programs(&Search::listing(var))
        .into_iter()
        .filter(|app| app.visible)
        .filter_map(|app| {
            let id = app.id.as_deref()?;
            let about = app
                .localized_generic_name
                .as_deref()
                .or(app.comment_text.as_deref());
            Some(App::new(
                id,
                &app.title_string(),
                about,
                app.icon_name.as_deref(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{App, installed};
    use crate::testdir::TestDir;

    #[test]
    fn the_filter_matches_every_word_anywhere_whatever_the_case() {
        let app = App::new(
            "chrome.desktop",
            "Chrome for Testing",
            Some("Web Browser"),
            None,
        );
        assert!(app.matches(""));
        assert!(app.matches("chr"));
        assert!(app.matches("TEST chrome"));
        assert!(app.matches("browser"));
        // The ID's suffix is every application's, so it matches none.
        assert!(!app.matches("desktop"));
        assert!(!app.matches("chrome steam"));
    }

    #[test]
    fn what_is_listed_is_what_a_launcher_shows() {
        let dir = TestDir::new("apps");
        let _ = dir.write(
            "share/applications/steam.desktop",
            "[Desktop Entry]\nType=Application\nName=Steam\nGenericName=Game Store\n\
             Comment=Play\nExec=/bin/true\nIcon=steam\n",
        );
        let _ = dir.write(
            "share/applications/alpha.desktop",
            "[Desktop Entry]\nType=Application\nName=alpha\nComment=First\nExec=/bin/true\n",
        );
        let _ = dir.write(
            "share/applications/hidden.desktop",
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=/bin/true\nNoDisplay=true\n",
        );
        let share = dir.path().join("share").to_string_lossy().into_owned();
        let var = move |name: &str| (name == "XDG_DATA_DIRS").then(|| share.clone());
        let apps = installed(&var);
        let listed: Vec<(&str, &str, Option<&str>, Option<&str>)> = apps
            .iter()
            .map(|app| {
                (
                    app.id.as_str(),
                    app.name.as_str(),
                    app.about.as_deref(),
                    app.icon.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            listed,
            vec![
                ("alpha.desktop", "alpha", Some("First"), None),
                ("steam.desktop", "Steam", Some("Game Store"), Some("steam")),
            ]
        );
    }
}
