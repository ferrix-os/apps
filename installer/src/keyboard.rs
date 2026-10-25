//! The keyboard layouts the installer offers: xkb's names, a person's
//! names, and each layout's common variants.

/// A layout: its xkb name, its name in words, and its variants, each an
/// xkb variant and its name in words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// The xkb layout: `de`.
    pub id: &'static str,
    /// Its name: `German`.
    pub name: &'static str,
    /// Its variants beyond the default.
    pub variants: &'static [(&'static str, &'static str)],
}

/// Every layout offered, by name.
pub const LAYOUTS: &[Layout] = &[
    Layout {
        id: "us",
        name: "English (US)",
        variants: &[
            ("intl", "International, dead keys"),
            ("dvorak", "Dvorak"),
            ("colemak", "Colemak"),
        ],
    },
    Layout {
        id: "gb",
        name: "English (UK)",
        variants: &[("extd", "Extended"), ("dvorak", "Dvorak")],
    },
    Layout {
        id: "at",
        name: "German (Austria)",
        variants: &[("nodeadkeys", "No dead keys")],
    },
    Layout {
        id: "be",
        name: "Belgian",
        variants: &[("nodeadkeys", "No dead keys")],
    },
    Layout {
        id: "br",
        name: "Portuguese (Brazil)",
        variants: &[("nodeadkeys", "No dead keys")],
    },
    Layout {
        id: "ca",
        name: "French (Canada)",
        variants: &[("multix", "Multilingual")],
    },
    Layout {
        id: "ch",
        name: "German (Switzerland)",
        variants: &[
            ("fr", "French (Switzerland)"),
            ("nodeadkeys", "No dead keys"),
        ],
    },
    Layout {
        id: "cz",
        name: "Czech",
        variants: &[("qwerty", "QWERTY")],
    },
    Layout {
        id: "de",
        name: "German",
        variants: &[("nodeadkeys", "No dead keys"), ("neo", "Neo 2")],
    },
    Layout {
        id: "dk",
        name: "Danish",
        variants: &[("nodeadkeys", "No dead keys")],
    },
    Layout {
        id: "es",
        name: "Spanish",
        variants: &[("nodeadkeys", "No dead keys"), ("cat", "Catalan")],
    },
    Layout {
        id: "fi",
        name: "Finnish",
        variants: &[],
    },
    Layout {
        id: "fr",
        name: "French",
        variants: &[("nodeadkeys", "No dead keys"), ("bepo", "BÉPO")],
    },
    Layout {
        id: "gr",
        name: "Greek",
        variants: &[],
    },
    Layout {
        id: "hu",
        name: "Hungarian",
        variants: &[("qwerty", "QWERTY")],
    },
    Layout {
        id: "ie",
        name: "Irish",
        variants: &[],
    },
    Layout {
        id: "il",
        name: "Hebrew",
        variants: &[],
    },
    Layout {
        id: "in",
        name: "Indian",
        variants: &[],
    },
    Layout {
        id: "is",
        name: "Icelandic",
        variants: &[],
    },
    Layout {
        id: "it",
        name: "Italian",
        variants: &[("nodeadkeys", "No dead keys")],
    },
    Layout {
        id: "jp",
        name: "Japanese",
        variants: &[],
    },
    Layout {
        id: "kr",
        name: "Korean",
        variants: &[],
    },
    Layout {
        id: "latam",
        name: "Spanish (Latin America)",
        variants: &[("nodeadkeys", "No dead keys")],
    },
    Layout {
        id: "nl",
        name: "Dutch",
        variants: &[],
    },
    Layout {
        id: "no",
        name: "Norwegian",
        variants: &[("nodeadkeys", "No dead keys")],
    },
    Layout {
        id: "pl",
        name: "Polish",
        variants: &[],
    },
    Layout {
        id: "pt",
        name: "Portuguese",
        variants: &[("nodeadkeys", "No dead keys")],
    },
    Layout {
        id: "ro",
        name: "Romanian",
        variants: &[("std", "Standard")],
    },
    Layout {
        id: "ru",
        name: "Russian",
        variants: &[("phonetic", "Phonetic")],
    },
    Layout {
        id: "se",
        name: "Swedish",
        variants: &[("nodeadkeys", "No dead keys")],
    },
    Layout {
        id: "sk",
        name: "Slovak",
        variants: &[("qwerty", "QWERTY")],
    },
    Layout {
        id: "tr",
        name: "Turkish",
        variants: &[("f", "F")],
    },
    Layout {
        id: "ua",
        name: "Ukrainian",
        variants: &[],
    },
];

/// The layout called `id`.
#[must_use]
pub fn layout(id: &str) -> Option<&'static Layout> {
    LAYOUTS.iter().find(|layout| layout.id == id)
}

/// The layout and variant as words: `German, No dead keys`.
#[must_use]
pub fn describe(id: &str, variant: &str) -> String {
    let Some(layout) = layout(id) else {
        return id.to_owned();
    };
    match layout.variants.iter().find(|(v, _)| *v == variant) {
        Some((_, name)) => format!("{}, {name}", layout.name),
        None => layout.name.to_owned(),
    }
}

/// The `hyprctl` lines that make the live session type in `id` and
/// `variant`.
#[must_use]
pub fn hyprctl(id: &str, variant: &str) -> [Vec<String>; 2] {
    let line =
        |key: &str, value: &str| vec!["keyword".to_owned(), key.to_owned(), value.to_owned()];
    [
        line("input:kb_layout", id),
        line("input:kb_variant", variant),
    ]
}

#[cfg(test)]
mod tests {
    use super::{LAYOUTS, describe, hyprctl};

    #[test]
    fn at_least_fifteen_layouts_each_named_once() {
        assert!(LAYOUTS.len() >= 15);
        for (at, layout) in LAYOUTS.iter().enumerate() {
            assert!(
                LAYOUTS
                    .iter()
                    .skip(at + 1)
                    .all(|other| other.id != layout.id),
                "{} twice",
                layout.id
            );
        }
        for id in [
            "us", "gb", "de", "fr", "es", "it", "pt", "br", "ch", "at", "nl", "se", "no", "dk",
            "fi", "pl", "cz", "hu", "ru", "ua", "tr", "jp", "kr",
        ] {
            assert!(super::layout(id).is_some(), "{id} is missing");
        }
    }

    #[test]
    fn a_layout_and_variant_in_words_and_for_hyprctl() {
        assert_eq!(describe("de", "nodeadkeys"), "German, No dead keys");
        assert_eq!(describe("de", ""), "German");
        assert_eq!(hyprctl("de", "")[1], ["keyword", "input:kb_variant", ""]);
    }
}
