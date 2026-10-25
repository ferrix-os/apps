//! The look: Nocturne's icons, as the vendored Phosphor font's glyphs
//! (Ferrix's `assets/fonts/phosphor`, MIT). The colours are the markup's
//! `Nocturne` singleton (`ui/nocturne.tole`).

/// Phosphor's code point for each icon the shell draws, from the
/// package's `src/regular/style.css` (`@phosphor-icons/web` 2.1.1).
const ICONS: [(&str, u32); 28] = [
    ("clock", 0xe19a),
    ("folder-simple", 0xe25a),
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
    ("cards", 0xe0f8),
    ("caret-left", 0xe138),
    ("caret-right", 0xe13a),
    ("check", 0xe182),
    ("folder", 0xe24a),
    ("folder-open", 0xe256),
    ("folder-plus", 0xe258),
    ("images", 0xe836),
    ("list-numbers", 0xe2f6),
    ("play", 0xe3d0),
    ("rainbow", 0xe598),
    ("sun-dim", 0xe474),
    ("x", 0xe4f6),
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

/// Where the shell finds its faces beside the system's: Phosphor from the
/// app's own folder (`share/fonts`, installed to
/// `/usr/share/fonts/yoru-shell`), and Inter where Ferrix's tree or the
/// image has it.
#[must_use]
pub fn fonts() -> tole::Fonts {
    let mut fonts = tole::Fonts::system();
    let own = concat!(env!("CARGO_MANIFEST_DIR"), "/share/fonts");
    let tree = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../assets/fonts/inter"
    );
    for (dir, family) in [
        (std::env::var("YORU_FONTS").ok(), "Phosphor"),
        (Some("/usr/share/fonts/yoru-shell".to_owned()), "Phosphor"),
        (Some(own.to_owned()), "Phosphor"),
        (Some("/usr/share/fonts/inter".to_owned()), "Inter Variable"),
        (Some(tree.to_owned()), "Inter Variable"),
    ] {
        let Some(dir) = dir else { continue };
        let path = std::path::Path::new(&dir);
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
