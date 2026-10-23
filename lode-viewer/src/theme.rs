//! The palette: the design's CSS variables, light and dark, by the same
//! names (`--bg`, `--ink3`, `--red-ink`, ...), and the few colours it
//! writes out whole.

/// A colour, with alpha, not premultiplied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

impl Color {
    /// An opaque colour from `0xRRGGBB`.
    #[must_use]
    pub const fn hex(rgb: u32) -> Self {
        Color((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
    }

    /// The same colour at `alpha` (0 to 1).
    #[must_use]
    pub fn alpha(self, alpha: f32) -> Self {
        Color(
            self.0,
            self.1,
            self.2,
            (alpha.clamp(0.0, 1.0) * 255.0) as u8,
        )
    }
}

/// Which palette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Light.
    #[default]
    Light,
    /// Dark.
    Dark,
}

/// The palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct Theme {
    pub mode: Mode,
    pub bg: Color,
    pub bg2: Color,
    pub bg3: Color,
    pub hover: Color,
    pub line: Color,
    pub line2: Color,
    pub line3: Color,
    pub bar: Color,
    pub ink: Color,
    pub inv: Color,
    pub ink2: Color,
    pub ink3: Color,
    pub ink4: Color,
    pub ink5: Color,
    pub red_ink: Color,
    pub amber_ink: Color,
    pub red_bg: Color,
    pub amber_bg: Color,
    pub amber_line: Color,
    pub blue: Color,
    pub blue_bg: Color,
    pub fill: Color,
}

/// The design's error red, the same in both palettes.
pub const RED: Color = Color::hex(0xd2_38_2b);
/// Its warning amber.
pub const AMBER: Color = Color::hex(0xe9_a9_1c);
/// The amber's edge.
pub const AMBER_EDGE: Color = Color::hex(0xc9_90_00);
/// Its running green.
pub const GREEN: Color = Color::hex(0x2f_a3_5a);
/// The Live switch when off.
pub const IDLE: Color = Color::hex(0x9a_9a_94);
/// White.
pub const WHITE: Color = Color::hex(0xff_ff_ff);

impl Theme {
    /// The palette for `mode`.
    #[must_use]
    pub const fn of(mode: Mode) -> Self {
        match mode {
            Mode::Light => Theme {
                mode,
                bg: Color::hex(0xff_ff_ff),
                bg2: Color::hex(0xfa_fa_f8),
                bg3: Color::hex(0xf4_f4_f2),
                hover: Color::hex(0xef_ef_eb),
                line: Color::hex(0xe6_e6_e2),
                line2: Color::hex(0xf0_f0_ec),
                line3: Color::hex(0xd6_d6_d0),
                bar: Color::hex(0xc8_c8_c2),
                ink: Color::hex(0x1a_1a_1a),
                inv: Color::hex(0xff_ff_ff),
                ink2: Color::hex(0x33_33_33),
                ink3: Color::hex(0x55_55_55),
                ink4: Color::hex(0x88_88_88),
                ink5: Color::hex(0x99_99_99),
                red_ink: Color::hex(0xc0_2c_20),
                amber_ink: Color::hex(0x7a_58_00),
                red_bg: Color::hex(0xfd_f0_ee),
                amber_bg: Color::hex(0xfd_f6_e3),
                amber_line: Color::hex(0xef_d6_8f),
                blue: Color::hex(0x2a_62_c9),
                blue_bg: Color(42, 98, 201, 26),
                fill: Color(0, 0, 0, 15),
            },
            Mode::Dark => Theme {
                mode,
                bg: Color::hex(0x1c_1d_21),
                bg2: Color::hex(0x17_18_1b),
                bg3: Color::hex(0x23_24_29),
                hover: Color::hex(0x2a_2c_31),
                line: Color::hex(0x2e_30_36),
                line2: Color::hex(0x27_29_2e),
                line3: Color::hex(0x3a_3c_43),
                bar: Color::hex(0x4a_4d_54),
                ink: Color::hex(0xe8_e8_e6),
                inv: Color::hex(0x14_15_18),
                ink2: Color::hex(0xcf_cf_cb),
                ink3: Color::hex(0xab_ab_a7),
                ink4: Color::hex(0x8b_8b_88),
                ink5: Color::hex(0x6f_6f_73),
                red_ink: Color::hex(0xff_7b_6e),
                amber_ink: Color::hex(0xf0_b9_3a),
                red_bg: Color::hex(0x3a_20_1e),
                amber_bg: Color::hex(0x35_2c_1b),
                amber_line: Color::hex(0x6b_54_20),
                blue: Color::hex(0x7e_a6_f5),
                blue_bg: Color(126, 166, 245, 41),
                fill: Color(255, 255, 255, 18),
            },
        }
    }

    /// The text colour of a level.
    #[must_use]
    pub fn level_ink(&self, level: u8) -> Color {
        match level {
            0..=3 => self.red_ink,
            4 => self.amber_ink,
            5 => self.blue,
            8 => self.ink4,
            _ => self.ink2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Mode, Theme};

    #[test]
    fn the_palettes_are_the_designs() {
        assert_eq!(
            Theme::of(Mode::Light).bg2,
            super::Color(0xfa, 0xfa, 0xf8, 255)
        );
        assert_eq!(
            Theme::of(Mode::Dark).ink,
            super::Color(0xe8, 0xe8, 0xe6, 255)
        );
    }
}
