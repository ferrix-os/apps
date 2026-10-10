//! The colours and the sizes: Ferrix's own, from its brand
//! (`docs/brand/BRAND.md`, the dark column), so Settings looks like the
//! system it sets.

use compositor_text::tiny_skia;

/// An opaque colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    /// As tiny-skia fills with it.
    #[must_use]
    pub fn skia(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.0, self.1, self.2, 255)
    }

    /// As text is drawn in it.
    #[must_use]
    pub const fn text(self) -> compositor_text::Rgba {
        compositor_text::Rgba {
            r: self.0,
            g: self.1,
            b: self.2,
            a: 255,
        }
    }
}

/// The window behind everything: the brand's ink.
pub const INK: Color = Color(0x0d, 0x0f, 0x12);
/// The sidebar, the filter box and the list: the brand's surface.
pub const SURFACE: Color = Color(0x16, 0x1a, 0x20);
/// Borders and separators: the brand's line.
pub const LINE: Color = Color(0x26, 0x2c, 0x35);
/// The row the pointer is over.
pub const HOVER: Color = Color(0x1b, 0x20, 0x28);
/// The row selected, and the page chosen in the sidebar.
pub const SELECTED: Color = Color(0x22, 0x28, 0x32);
/// Text.
pub const TEXT: Color = Color(0xee, 0xf1, 0xf5);
/// Secondary text.
pub const MUTED: Color = Color(0x9a, 0xa4, 0xb2);
/// The accent, the brand's rust: a switch that is on, the selection's bar.
pub const ACCENT: Color = Color(0xff, 0x7a, 0x2b);
/// A switch that is off.
pub const OFF: Color = Color(0x3a, 0x42, 0x4e);
/// A switch's knob.
pub const KNOB: Color = Color(0xf7, 0xf5, 0xf2);
/// Something that went wrong.
pub const WARNING: Color = Color(0xff, 0xb5, 0x47);

/// The families text is set in, best first: the brand's Inter, then what an
/// image without it still carries.
pub const FAMILIES: [&str; 3] = ["Inter", "Liberation Sans", "sans-serif"];
