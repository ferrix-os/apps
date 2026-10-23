//! The look: the design's colours, worked out from whether the window is
//! dark and from the accent chosen, and handed to the markup's `Look`
//! and to Fx.Controls' `Theme`, so tole's own controls (a text field, the
//! file chooser) are drawn in it too.
//!
//! The design gives every colour in OKLCH, a lightness, a chroma and a
//! hue: the greys are hue 260 with almost no chroma, the accent's three
//! uses are one hue at three lightnesses. Dark is the design's own; light
//! keeps each hue and turns the lightnesses over.

use tole::{Color, Ui};

use crate::ui::Look;

/// The accents the design offers: a name and a hue.
pub const ACCENTS: [(&str, f32); 7] = [
    ("Blue", 250.0),
    ("Teal", 195.0),
    ("Green", 150.0),
    ("Amber", 80.0),
    ("Orange", 45.0),
    ("Rose", 10.0),
    ("Violet", 300.0),
];

/// An OKLCH colour as sRGB bytes, clipped into the gamut.
#[must_use]
pub fn oklch(l: f32, c: f32, h: f32) -> [u8; 3] {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    let r = 4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3;
    let g = -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3;
    let bl = -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3;
    let encode = |x: f32| {
        let x = x.clamp(0.0, 1.0);
        let v = if x <= 0.003_130_8 {
            12.92 * x
        } else {
            1.055 * x.powf(1.0 / 2.4) - 0.055
        };
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    [encode(r), encode(g), encode(bl)]
}

/// `oklch` as `rrggbb`, as hyprland's colours are written.
#[must_use]
pub fn hex(l: f32, c: f32, h: f32) -> String {
    let [r, g, b] = oklch(l, c, h);
    format!("{r:02x}{g:02x}{b:02x}")
}

/// `oklch` as a tole colour.
#[must_use]
pub fn color(l: f32, c: f32, h: f32) -> Color {
    let [r, g, b] = oklch(l, c, h);
    Color::rgba(r, g, b, 0xff)
}

/// The palette for one choice: every token of the markup's `Look`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)]
pub struct Palette {
    pub bg: Color,
    pub card: Color,
    pub inset: Color,
    pub field: Color,
    pub hover: Color,
    pub sel: Color,
    pub line: Color,
    pub line2: Color,
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    pub track: Color,
    pub knob: Color,
    pub accent: Color,
    pub accent_text: Color,
    pub accent_soft: Color,
    pub warn: Color,
}

impl Palette {
    /// Dark or light, in the accent of hue `hue`.
    #[must_use]
    pub fn new(dark: bool, hue: f32) -> Self {
        let grey = |l: f32, c: f32| color(l, c, 260.0);
        if dark {
            Palette {
                bg: grey(0.165, 0.004),
                card: grey(0.2, 0.004),
                inset: grey(0.14, 0.004),
                field: grey(0.21, 0.004),
                hover: grey(0.25, 0.004),
                sel: grey(0.31, 0.005),
                line: grey(0.27, 0.005),
                line2: grey(0.3, 0.005),
                text: grey(0.95, 0.003),
                muted: grey(0.72, 0.006),
                faint: grey(0.62, 0.006),
                track: grey(0.34, 0.005),
                knob: grey(0.97, 0.003),
                accent: color(0.72, 0.13, hue),
                accent_text: color(0.84, 0.09, hue),
                accent_soft: color(0.3, 0.06, hue),
                warn: color(0.8, 0.11, 75.0),
            }
        } else {
            Palette {
                bg: grey(0.975, 0.003),
                card: grey(1.0, 0.0),
                inset: grey(0.955, 0.004),
                field: grey(1.0, 0.0),
                hover: grey(0.945, 0.004),
                sel: grey(1.0, 0.0),
                line: grey(0.9, 0.005),
                line2: grey(0.86, 0.005),
                text: grey(0.22, 0.006),
                muted: grey(0.47, 0.008),
                faint: grey(0.58, 0.008),
                track: grey(0.85, 0.006),
                knob: grey(1.0, 0.0),
                accent: color(0.56, 0.15, hue),
                accent_text: color(0.47, 0.14, hue),
                accent_soft: color(0.93, 0.04, hue),
                warn: color(0.62, 0.13, 70.0),
            }
        }
    }

    /// Set every token, the markup's and Fx.Controls' alike.
    pub fn apply(&self, ui: &mut Ui, dark: bool) {
        let look = Look::instance(ui);
        look.dark.set(ui, dark);
        for (prop, value) in [
            (look.bg, self.bg),
            (look.card, self.card),
            (look.inset, self.inset),
            (look.field, self.field),
            (look.hover, self.hover),
            (look.sel, self.sel),
            (look.line, self.line),
            (look.line2, self.line2),
            (look.text, self.text),
            (look.muted, self.muted),
            (look.faint, self.faint),
            (look.track, self.track),
            (look.knob, self.knob),
            (look.accent, self.accent),
            (look.accent_text, self.accent_text),
            (look.accent_soft, self.accent_soft),
            (look.warn, self.warn),
        ] {
            prop.set(ui, value);
        }
        // Fx.Controls' Theme: the same surfaces, ink and accent, and the
        // design's faces.
        let theme = crate::ui::fx_controls::Theme::instance(ui);
        theme.dark.set(ui, dark);
        for (prop, value) in [
            (theme.bg, self.card),
            (theme.bg2, self.bg),
            (theme.bg3, self.inset),
            (theme.hover, self.hover),
            (theme.line, self.line),
            (theme.line2, self.line),
            (theme.line3, self.line2),
            (theme.ink, self.text),
            (theme.ink2, self.text),
            (theme.ink3, self.muted),
            (theme.ink4, self.faint),
            (theme.ink5, self.faint),
            (theme.inv, self.bg),
            (theme.accent, self.accent),
            (theme.accent_inv, self.accent_soft),
            (theme.accent_over, self.accent_soft),
            (theme.on_accent, self.bg),
        ] {
            prop.set(ui, value);
        }
        theme
            .sans_family
            .set(ui, "Figtree, Inter, Liberation Sans, sans-serif".into());
        theme
            .mono_family
            .set(ui, "JetBrains Mono, Liberation Mono, monospace".into());
        theme.radius.set(ui, 8.0);
    }
}

#[cfg(test)]
mod tests {
    use super::{hex, oklch};

    #[test]
    fn oklch_turns_into_the_srgb_a_browser_shows() {
        assert_eq!(oklch(1.0, 0.0, 0.0), [255, 255, 255]);
        assert_eq!(oklch(0.0, 0.0, 0.0), [0, 0, 0]);
        // The design's blue accent, oklch(0.72 0.13 250), as Chrome draws
        // it in the screenshot of the design: about #5aa8f2.
        let [r, g, b] = oklch(0.72, 0.13, 250.0);
        assert!(
            r.abs_diff(0x5a) <= 8 && g.abs_diff(0xa8) <= 8 && b.abs_diff(0xf2) <= 8,
            "{}",
            hex(0.72, 0.13, 250.0)
        );
    }
}
