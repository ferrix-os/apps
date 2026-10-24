//! The look: Nocturne's icons, as the vendored Phosphor font's glyphs
//! (Ferrix's `assets/fonts/phosphor`, MIT). The colours are the markup's
//! `Nocturne` singleton (`ui/nocturne.tole`).

/// Phosphor's code point for each icon the shell draws, from the
/// package's `src/regular/style.css` (`@phosphor-icons/web` 2.1.1).
const ICONS: [(&str, u32); 14] = [
    ("clock", 0xe19a),
    ("crosshair-simple", 0xe1d8),
    ("gear-six", 0xe272),
    ("globe-simple", 0xe28e),
    ("magnifying-glass", 0xe30c),
    ("moon-stars", 0xe58e),
    ("pause", 0xe39e),
    ("sliders-horizontal", 0xe434),
    ("squares-four", 0xe464),
    ("terminal", 0xe47e),
    ("terminal-window", 0xeae8),
    ("timer", 0xe492),
    ("image", 0xe2ca),
    ("keyboard", 0xe2d8),
];

/// The glyph for Phosphor's icon `name`, as text in the `Phosphor` face;
/// empty for a name the shell does not carry.
#[must_use]
pub fn icon(name: &str) -> String {
    ICONS
        .iter()
        .find(|(known, _)| *known == name)
        .and_then(|(_, code)| char::from_u32(*code))
        .map(String::from)
        .unwrap_or_default()
}

/// Where the shell finds its faces beside the system's: Phosphor and Inter
/// where the image puts them, and on a development host the tree's.
#[must_use]
pub fn fonts() -> tole::Fonts {
    let mut fonts = tole::Fonts::system();
    let tree = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../assets/fonts");
    for dir in [
        std::env::var("YORU_FONTS").ok(),
        Some(format!("{tree}/phosphor")),
        Some(format!("{tree}/inter")),
    ]
    .into_iter()
    .flatten()
    {
        let path = std::path::Path::new(&dir);
        let family = if dir.ends_with("inter") {
            "Inter Variable"
        } else {
            "Phosphor"
        };
        if path.is_dir() && !has_family(&fonts, family) {
            let _ = fonts.add_dir(path);
        }
    }
    fonts
}

/// Whether `fonts` has a face of `family`.
fn has_family(fonts: &tole::Fonts, family: &str) -> bool {
    fonts
        .faces()
        .iter()
        .any(|face| face.families.iter().any(|name| name == family))
}

#[cfg(test)]
mod tests {
    use super::{fonts, icon};

    #[test]
    fn icons_are_private_use_glyphs() {
        assert_eq!(icon("terminal-window"), "\u{eae8}");
        assert_eq!(icon("no-such-icon"), "");
    }

    /// Phosphor's face is found and has the glyphs.
    #[test]
    fn the_icon_face_is_there() {
        let fonts = fonts();
        assert!(
            fonts
                .faces()
                .iter()
                .any(|face| face.families.iter().any(|name| name == "Phosphor")),
            "Phosphor's face"
        );
    }
}
