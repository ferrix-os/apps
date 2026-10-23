//! Drawing in the window's logical pixels: boxes, rounded boxes, lines,
//! dots, hatching and text, each scaled to the buffer's pixels, and the
//! places the pointer can press, kept as the frame is drawn.
//!
//! The viewer is immediate: every frame is drawn whole from the state, and
//! what a press does is looked up in the regions the last frame left.

use std::collections::HashMap;
use std::path::Path;

use compositor_text::tiny_skia::{
    self, FillRule, LineCap, Mask, Paint, PathBuilder, PixmapMut, Stroke, StrokeDash, Transform,
};
use compositor_text::{Ellipsize, Font, FontDescription, Fonts, Rgba, Run, Size, Weight};

use crate::theme::Color;

/// A rectangle in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Left.
    pub x: f32,
    /// Top.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl Rect {
    /// A rectangle.
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect { x, y, w, h }
    }

    /// Whether the point is inside.
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    /// The right edge.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    /// The bottom edge.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// Smaller by `by` on every side.
    #[must_use]
    pub fn inset(&self, by: f32) -> Self {
        Rect::new(
            self.x + by,
            self.y + by,
            (self.w - 2.0 * by).max(0.0),
            (self.h - 2.0 * by).max(0.0),
        )
    }
}

/// The two families of the design.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    /// IBM Plex Sans.
    Sans,
    /// IBM Plex Mono.
    Mono,
}

/// A font: a family, a size in logical pixels (in quarters) and a weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Face {
    /// The family.
    pub family: Family,
    /// Quarter pixels.
    pub quarters: u16,
    /// 400, 500 or 600.
    pub weight: u16,
}

/// A sans face of `px` at `weight`.
#[must_use]
pub fn sans(px: f32, weight: u16) -> Face {
    Face {
        family: Family::Sans,
        quarters: (px * 4.0) as u16,
        weight,
    }
}

/// A mono face of `px` at `weight`.
#[must_use]
pub fn mono(px: f32, weight: u16) -> Face {
    Face {
        family: Family::Mono,
        quarters: (px * 4.0) as u16,
        weight,
    }
}

/// The families asked for, best first: the design's, then what an image
/// without them carries.
const SANS: [&str; 4] = ["IBM Plex Sans", "Inter", "Liberation Sans", "sans-serif"];
/// The same for the monospaced face.
const MONO: [&str; 3] = ["IBM Plex Mono", "Liberation Mono", "monospace"];

/// How many shaped runs are kept before the cache starts again.
const RUNS_MAX: usize = 6000;

/// Fonts, and what was shaped with them, at one scale.
#[derive(Debug)]
pub struct Text {
    /// Every face found.
    pub fonts: Fonts,
    /// The scale the fonts are resolved for.
    scale: f32,
    /// Resolved fonts.
    resolved: HashMap<Face, Font>,
    /// Shaped runs, by face and text.
    runs: HashMap<(Face, String), Run>,
}

impl Text {
    /// The fonts the viewer draws with: the design's own IBM Plex, from the
    /// directory the package installs them in or, on a development host,
    /// beside the sources, then the system's.
    #[must_use]
    pub fn load() -> Self {
        let mut fonts = Fonts::system();
        // Installed on a Linux desktop beside the program: `<prefix>/bin`
        // and `<prefix>/share/lode-viewer/fonts`.
        let beside = std::env::current_exe().ok().and_then(|exe| {
            let prefix = exe.parent()?.parent()?.to_path_buf();
            Some(
                prefix
                    .join("share/lode-viewer/fonts")
                    .to_string_lossy()
                    .into_owned(),
            )
        });
        for dir in [
            std::env::var("LODE_VIEWER_FONTS").ok(),
            Some("/usr/share/fonts/ibm-plex".to_owned()),
            beside,
            Some(concat!(env!("CARGO_MANIFEST_DIR"), "/share/fonts").to_owned()),
        ]
        .into_iter()
        .flatten()
        {
            let path = Path::new(&dir);
            if path.is_dir()
                && !fonts
                    .faces()
                    .iter()
                    .any(|face| face.families.iter().any(|name| name == "IBM Plex Sans"))
            {
                let _ = fonts.add_dir(path);
            }
        }
        Self::with(fonts)
    }

    /// Text drawn with `fonts`.
    #[must_use]
    pub fn with(fonts: Fonts) -> Self {
        Text {
            fonts,
            scale: 1.0,
            resolved: HashMap::new(),
            runs: HashMap::new(),
        }
    }

    /// How many faces there are.
    #[must_use]
    pub fn faces(&self) -> usize {
        self.fonts.faces().len()
    }

    /// Use `scale` from now on.
    pub fn set_scale(&mut self, scale: f32) {
        if (self.scale - scale).abs() > f32::EPSILON {
            self.scale = scale;
            self.resolved.clear();
            self.runs.clear();
        }
    }

    /// The font for `face`.
    fn font(&mut self, face: Face) -> Font {
        if let Some(font) = self.resolved.get(&face) {
            return font.clone();
        }
        let families: &[&str] = match face.family {
            Family::Sans => &SANS,
            Family::Mono => &MONO,
        };
        let mut description = FontDescription::new(
            families,
            Size::Pixels(f32::from(face.quarters) / 4.0 * self.scale),
        );
        description.weight = Weight(face.weight);
        let font = self.fonts.resolve(&description);
        let _ = self.resolved.insert(face, font.clone());
        font
    }

    /// `text` shaped in `face`, in buffer pixels.
    fn run(&mut self, face: Face, text: &str) -> Run {
        let key = (face, text.to_owned());
        if let Some(run) = self.runs.get(&key) {
            return run.clone();
        }
        if self.runs.len() >= RUNS_MAX {
            self.runs.clear();
        }
        let font = self.font(face);
        let run = self.fonts.shape(&font, text);
        let _ = self.runs.insert(key, run.clone());
        run
    }

    /// How wide `text` is in `face`, in logical pixels.
    pub fn width(&mut self, face: Face, text: &str) -> f32 {
        self.run(face, text).width / self.scale
    }
}

/// A frame being drawn, and the regions it leaves to be pressed.
pub struct Canvas<'a, 'p, A> {
    /// The buffer.
    pub pixmap: &'a mut PixmapMut<'p>,
    /// The text.
    pub text: &'a mut Text,
    /// Buffer pixels to a logical one.
    pub scale: f32,
    /// Where the pointer can press, and what that does: the last region
    /// that holds a point wins.
    pub hits: Vec<(Rect, A)>,
    /// Text is not drawn below this line, nor above `clip_top`: a scrolled
    /// area's edges.
    pub clip_bottom: f32,
    /// The top edge.
    pub clip_top: f32,
}

impl<'a, 'p, A> Canvas<'a, 'p, A> {
    /// A canvas over `pixmap` at `scale`.
    pub fn new(pixmap: &'a mut PixmapMut<'p>, text: &'a mut Text, scale: f32) -> Self {
        text.set_scale(scale);
        Canvas {
            pixmap,
            text,
            scale,
            hits: Vec::new(),
            clip_bottom: f32::MAX,
            clip_top: f32::MIN,
        }
    }

    /// The canvas's size in logical pixels.
    #[must_use]
    pub fn size(&self) -> (f32, f32) {
        (
            self.pixmap.width() as f32 / self.scale,
            self.pixmap.height() as f32 / self.scale,
        )
    }

    /// Say that pressing `place` does `action`.
    pub fn hit(&mut self, place: Rect, action: A) {
        self.hits.push((place, action));
    }

    /// Fill the whole frame.
    pub fn clear(&mut self, color: Color) {
        self.pixmap.fill(skia(color));
    }

    /// `place` filled, its corners rounded by `radius`.
    pub fn fill(&mut self, place: Rect, radius: f32, color: Color) {
        if place.w <= 0.0 || place.h <= 0.0 || color.3 == 0 {
            return;
        }
        let Some(path) = self.rounded(place, radius) else {
            return;
        };
        let paint = paint(color, radius > 0.0);
        self.pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    /// `place`'s outline, `width` wide, inside its edge.
    pub fn outline(&mut self, place: Rect, radius: f32, width: f32, color: Color) {
        let half = width / 2.0;
        let inner = Rect::new(
            place.x + half,
            place.y + half,
            place.w - width,
            place.h - width,
        );
        let Some(path) = self.rounded(inner, (radius - half).max(0.0)) else {
            return;
        };
        let stroke = Stroke {
            width: width * self.scale,
            ..Stroke::default()
        };
        self.pixmap.stroke_path(
            &path,
            &paint(color, true),
            &stroke,
            Transform::identity(),
            None,
        );
    }

    /// A dashed outline.
    pub fn dashed_outline(&mut self, place: Rect, radius: f32, color: Color) {
        let Some(path) = self.rounded(place.inset(0.5), radius) else {
            return;
        };
        let stroke = Stroke {
            width: self.scale,
            dash: StrokeDash::new(vec![3.0 * self.scale, 3.0 * self.scale], 0.0),
            ..Stroke::default()
        };
        self.pixmap.stroke_path(
            &path,
            &paint(color, true),
            &stroke,
            Transform::identity(),
            None,
        );
    }

    /// A box with an edge.
    pub fn boxed(&mut self, place: Rect, radius: f32, inside: Color, edge: Color) {
        self.fill(place, radius, inside);
        self.outline(place, radius, 1.0, edge);
    }

    /// A path of `points`, stroked, dashed if `dash` is not zero.
    pub fn polyline(&mut self, points: &[(f32, f32)], width: f32, color: Color, dash: f32) {
        let Some(path) = self.path(points, false) else {
            return;
        };
        let stroke = Stroke {
            width: width * self.scale,
            line_cap: LineCap::Butt,
            line_join: tiny_skia::LineJoin::Round,
            dash: (dash > 0.0)
                .then(|| StrokeDash::new(vec![dash * self.scale, dash * self.scale], 0.0))
                .flatten(),
            ..Stroke::default()
        };
        self.pixmap.stroke_path(
            &path,
            &paint(color, true),
            &stroke,
            Transform::identity(),
            None,
        );
    }

    /// The polygon of `points`, filled.
    pub fn polygon(&mut self, points: &[(f32, f32)], color: Color) {
        let Some(path) = self.path(points, true) else {
            return;
        };
        self.pixmap.fill_path(
            &path,
            &paint(color, true),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    /// A dot of `radius` at (`x`, `y`).
    pub fn dot(&mut self, x: f32, y: f32, radius: f32, color: Color) {
        let s = self.scale;
        let Some(path) = PathBuilder::from_circle(x * s, y * s, radius * s) else {
            return;
        };
        self.pixmap.fill_path(
            &path,
            &paint(color, true),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    /// `place` striped at 45° in `color` on `under`, as the design's held
    /// spans: three pixels of each.
    pub fn hatch(&mut self, place: Rect, color: Color, under: Color) {
        self.fill(place, 0.0, under);
        let s = self.scale;
        let (width, height) = (self.pixmap.width(), self.pixmap.height());
        let Some(mut mask) = Mask::new(width, height) else {
            return;
        };
        let Some(clip) = self.rounded(place, 0.0) else {
            return;
        };
        mask.fill_path(&clip, FillRule::Winding, false, Transform::identity());
        let mut builder = PathBuilder::new();
        let mut x = place.x - place.h;
        while x < place.right() {
            builder.move_to(x * s, place.bottom() * s);
            builder.line_to((x + place.h) * s, place.y * s);
            x += 6.0;
        }
        let Some(path) = builder.finish() else {
            return;
        };
        let stroke = Stroke {
            width: 3.0 * s * std::f32::consts::FRAC_1_SQRT_2 * 1.4,
            ..Stroke::default()
        };
        self.pixmap.stroke_path(
            &path,
            &paint(color, true),
            &stroke,
            Transform::identity(),
            Some(&mask),
        );
    }

    /// How wide `text` is in `face`.
    pub fn width(&mut self, face: Face, text: &str) -> f32 {
        self.text.width(face, text)
    }

    /// `text` in `face` with its top-left at (`x`, `y`) in a line `height`
    /// tall, centred in it, cut with `…` to `max` wide. Answers the width
    /// drawn.
    pub fn text(
        &mut self,
        face: Face,
        text: &str,
        (x, y, height): (f32, f32, f32),
        color: Color,
        max: Option<f32>,
    ) -> f32 {
        if text.is_empty() || y + height < self.clip_top || y > self.clip_bottom {
            return 0.0;
        }
        let s = self.scale;
        let mut run = self.text.run(face, text);
        if let Some(max) = max
            && run.width > max * s
        {
            let font = self.text.font(face);
            run = self
                .text
                .fonts
                .shape_ellipsized(&font, text, (max * s).max(1.0), Ellipsize::End);
        }
        let ascent = run.ascent;
        let descent = run.descent;
        let baseline = (y * s + (height * s - (ascent + descent)) / 2.0 + ascent).round();
        self.text.fonts.draw_run(
            &run,
            self.pixmap,
            (x * s).round(),
            baseline,
            Rgba {
                r: color.0,
                g: color.1,
                b: color.2,
                a: color.3,
            },
        );
        run.width / s
    }

    /// `text` broken into lines no wider than `max` in `face`.
    pub fn wrap(&mut self, face: Face, text: &str, max: f32) -> Vec<String> {
        wrap_with(text, max, |line| self.text.width(face, line))
    }

    /// `text` in `face` from (`x`, `y`), wrapped to `max` wide, a line
    /// `line` tall each. Answers the height drawn.
    pub fn wrapped(
        &mut self,
        face: Face,
        text: &str,
        (x, y, line): (f32, f32, f32),
        color: Color,
        max: f32,
    ) -> f32 {
        let lines = self.wrap(face, text, max);
        for (n, each) in lines.iter().enumerate() {
            let _ = self.text(face, each, (x, y + line * n as f32, line), color, Some(max));
        }
        line * lines.len() as f32
    }

    /// `text` right-aligned to `right`.
    pub fn text_right(
        &mut self,
        face: Face,
        text: &str,
        (right, y, height): (f32, f32, f32),
        color: Color,
    ) -> f32 {
        let width = self.width(face, text);
        self.text(face, text, (right - width, y, height), color, None)
    }

    /// `text` centred in `place`.
    pub fn text_center(&mut self, face: Face, text: &str, place: Rect, color: Color) -> f32 {
        let width = self.width(face, text);
        self.text(
            face,
            text,
            (place.x + (place.w - width) / 2.0, place.y, place.h),
            color,
            Some(place.w),
        )
    }

    /// A rounded rectangle's path, in buffer pixels.
    fn rounded(&self, place: Rect, radius: f32) -> Option<tiny_skia::Path> {
        let s = self.scale;
        let (x, y, w, h) = (place.x * s, place.y * s, place.w * s, place.h * s);
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let r = (radius * s).min(w / 2.0).min(h / 2.0);
        if r <= 0.0 {
            return Some(PathBuilder::from_rect(tiny_skia::Rect::from_xywh(
                x, y, w, h,
            )?));
        }
        // A quarter circle's control points are this far along its tangents.
        let k = r * 0.552_284_8;
        let mut path = PathBuilder::new();
        path.move_to(x + r, y);
        path.line_to(x + w - r, y);
        path.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
        path.line_to(x + w, y + h - r);
        path.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
        path.line_to(x + r, y + h);
        path.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
        path.line_to(x, y + r);
        path.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
        path.close();
        path.finish()
    }

    /// The path through `points`, closed if `closed`.
    fn path(&self, points: &[(f32, f32)], closed: bool) -> Option<tiny_skia::Path> {
        let s = self.scale;
        let (&(x0, y0), rest) = points.split_first()?;
        let mut path = PathBuilder::new();
        path.move_to(x0 * s, y0 * s);
        for &(x, y) in rest {
            path.line_to(x * s, y * s);
        }
        if closed {
            path.close();
        }
        path.finish()
    }
}

/// `text` broken at spaces into lines no wider than `max` by `width`; a
/// word wider than `max` alone is broken where it must be. Explicit line
/// breaks stay.
pub fn wrap_with(text: &str, max: f32, mut width: impl FnMut(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ').filter(|word| !word.is_empty()) {
            let tried = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if width(&tried) <= max {
                line = tried;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // A word too wide alone is broken by characters.
            for c in word.chars() {
                line.push(c);
                if width(&line) > max && line.chars().count() > 1 {
                    let last = line.pop().unwrap_or(c);
                    lines.push(std::mem::take(&mut line));
                    line.push(last);
                }
            }
        }
        lines.push(line);
    }
    lines
}

/// tiny-skia's colour.
fn skia(color: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(color.0, color.1, color.2, color.3)
}

/// A paint of one colour.
fn paint(color: Color, anti_alias: bool) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(skia(color));
    paint.anti_alias = anti_alias;
    paint
}

#[cfg(test)]
mod tests {
    use super::wrap_with;

    #[test]
    fn long_text_wraps_at_spaces_and_a_long_word_breaks() {
        let width = |text: &str| text.chars().count() as f32;
        assert_eq!(
            wrap_with("Keys only: ssh runs with BatchMode", 12.0, width),
            vec!["Keys only:", "ssh runs", "with", "BatchMode"]
        );
        assert_eq!(wrap_with("abcdefgh", 3.0, width), vec!["abc", "def", "gh"]);
        assert_eq!(wrap_with("a\nb", 10.0, width), vec!["a", "b"]);
        let help = "Keys only: the viewer never asks for a password and keeps none. ssh runs with BatchMode, and with IdentitiesOnly when a key is named.";
        let lines = wrap_with(help, 60.0, width);
        assert!(lines.iter().all(|line| line.chars().count() <= 60));
        assert_eq!(lines.join(" "), help);
    }
}
