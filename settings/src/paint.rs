//! One frame, into pixels.
//!
//! The frame is drawn whole every time, back to front: the window's ink,
//! the sidebar, the page's heading and explanation, the filter box, the
//! rows in view and the line at the foot. Everything is placed in the
//! window's logical pixels by [`crate::layout`] and drawn `scale` times as
//! large, so a screen with a buffer scale of two gets sharp text rather
//! than a stretched picture.
//!
//! Text is `compositor-text`'s (rustybuzz shaping, tiny-skia coverage), the
//! icons `compositor-image`'s, found by `compositor-desktop`'s icon lookup
//! at the size the launcher's rows would ask for.

use std::collections::HashMap;
use std::path::PathBuf;

use compositor_desktop::icon::{self, Found, Kind};
use compositor_text::tiny_skia::{
    self, FillRule, FilterQuality, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Transform,
};
use compositor_text::{
    Ellipsize, Font, FontDescription, Fonts, LayoutOptions, Size, Weight, plain_spans,
};

use crate::apps::App;
use crate::layout::{self, Geometry, Rect};
use crate::model::{EXPLANATION, Model, Page, Status};
use crate::theme::{self, Color};

/// The explanation's size and line height, and the gap between its
/// paragraphs.
const EXPLANATION_TEXT: (f32, f32, f32) = (13.5, 20.0, 8.0);

/// The fonts at one scale.
#[derive(Debug)]
struct Faces {
    /// The scale they were resolved for.
    scale: f32,
    /// The sidebar's title.
    title: Font,
    /// A page's heading.
    heading: Font,
    /// Names, the filter, the sidebar's entries.
    body: Font,
    /// The same, heavier: the page chosen.
    strong: Font,
    /// Descriptions and the foot.
    small: Font,
}

impl Faces {
    /// Resolve each at `scale`.
    fn new(fonts: &Fonts, scale: f32) -> Self {
        let font = |px: f32, weight: Weight| {
            let mut description = describe(px * scale);
            description.weight = weight;
            fonts.resolve(&description)
        };
        Self {
            scale,
            title: font(17.0, Weight::SEMI_BOLD),
            heading: font(24.0, Weight::SEMI_BOLD),
            body: font(14.5, Weight::NORMAL),
            strong: font(14.5, Weight::SEMI_BOLD),
            small: font(12.5, Weight::NORMAL),
        }
    }
}

/// The theme's families at `px` pixels.
fn describe(px: f32) -> FontDescription {
    FontDescription::new(&theme::FAMILIES, Size::Pixels(px))
}

/// What draws the window.
#[derive(Debug)]
pub struct Painter {
    /// Every face found.
    fonts: Fonts,
    /// The fonts at the scale last drawn at.
    faces: Faces,
    /// The icon file of each application, by its place among all of them.
    icons: Vec<Option<Found>>,
    /// The icons read, by file and size in pixels.
    pictures: HashMap<(PathBuf, u32), Option<Pixmap>>,
}

impl Painter {
    /// A painter for `apps` with `fonts`, finding the icons through the
    /// environment `var` answers for, as the launcher finds them.
    #[must_use]
    pub fn new(fonts: Fonts, apps: &[App], var: &dyn Fn(&str) -> Option<String>) -> Self {
        let data_dirs = compositor_desktop::entry::data_dirs(
            var("XDG_DATA_HOME").as_deref(),
            var("HOME").as_deref(),
            var("XDG_DATA_DIRS").as_deref(),
        );
        let dirs = icon::icon_dirs(&data_dirs, var("HOME").as_deref());
        // The launcher's own default theme name, which falls back to
        // `hicolor`, where the tree's icons and every app's are.
        let themes = icon::load_themes("default", &dirs, true);
        let names: Vec<Option<&str>> = apps.iter().map(|app| app.icon.as_deref()).collect();
        let icons = icon::lookup(&themes, &dirs, layout::ICON as i32, &names);
        let faces = Faces::new(&fonts, 1.0);
        Self {
            fonts,
            faces,
            icons,
            pictures: HashMap::new(),
        }
    }

    /// How many faces the painter has to draw text with. None is a window
    /// of shapes with no words, which the program says rather than shows.
    #[must_use]
    pub fn faces(&self) -> usize {
        self.fonts.faces().len()
    }

    /// How many of the applications have an icon that was found.
    #[must_use]
    pub fn icons_found(&self) -> usize {
        self.icons.iter().flatten().count()
    }

    /// The explanation's paragraphs laid out `width` logical pixels wide at
    /// `scale`: each layout, in buffer pixels.
    fn explanation(&mut self, width: f32, scale: f32) -> Vec<compositor_text::Layout> {
        let (px, line, _) = EXPLANATION_TEXT;
        let options = LayoutOptions {
            font: describe(px * scale),
            color: theme::MUTED.text(),
            max_width: Some((width * scale).max(1.0)),
            wrap: true,
            line_height: Some(line * scale),
            ..LayoutOptions::default()
        };
        EXPLANATION
            .split('\n')
            .map(|paragraph| self.fonts.layout(&plain_spans(paragraph), &options))
            .collect()
    }

    /// Where everything is in a window of `width` by `height` logical
    /// pixels: the explanation is wrapped to the page's width first, since
    /// the list starts where it ends.
    pub fn geometry(&mut self, width: u32, height: u32) -> Geometry {
        let (width, height) = (width as f32, height as f32);
        let (_, _, gap) = EXPLANATION_TEXT;
        let paragraphs = self.explanation(Geometry::text_width(width), 1.0);
        let text: f32 = paragraphs.iter().map(|layout| layout.height).sum();
        let gaps = gap * paragraphs.len().saturating_sub(1) as f32;
        Geometry::new(width, height, (text + gaps).ceil())
    }

    /// Draw `model` laid out as `geometry` into `pixmap`, whose pixels are
    /// `scale` to each logical one.
    pub fn draw(
        &mut self,
        pixmap: &mut PixmapMut<'_>,
        model: &Model,
        geometry: &Geometry,
        scale: u32,
    ) {
        let scale = scale.max(1) as f32;
        if (self.faces.scale - scale).abs() > f32::EPSILON {
            self.faces = Faces::new(&self.fonts, scale);
        }
        pixmap.fill(theme::INK.skia());
        self.sidebar(pixmap, model, geometry);
        match model.page {
            Page::GameMode => self.game_mode(pixmap, model, geometry),
        }
    }

    /// The sidebar: the program's name and the pages.
    fn sidebar(&mut self, pixmap: &mut PixmapMut<'_>, model: &Model, geometry: &Geometry) {
        let s = self.faces.scale;
        let bar = geometry.sidebar;
        fill(pixmap, &bar, 0.0, theme::SURFACE, s);
        let edge = Rect {
            x: bar.x + bar.w - 1.0,
            w: 1.0,
            ..bar
        };
        fill(pixmap, &edge, 0.0, theme::LINE, s);
        let title = Rect {
            x: 20.0,
            y: 0.0,
            w: (bar.w - 28.0).max(0.0),
            h: layout::HEADING,
        };
        let font = self.faces.title.clone();
        self.line(pixmap, &font, "Settings", &title, theme::TEXT);
        for (index, page) in Page::ALL.into_iter().enumerate() {
            let entry = geometry.page(index);
            let chosen = page == model.page;
            if chosen {
                fill(pixmap, &entry, 8.0, theme::SELECTED, s);
                let mark = Rect {
                    x: entry.x,
                    y: entry.y + 9.0,
                    w: 3.0,
                    h: entry.h - 18.0,
                };
                fill(pixmap, &mark, 1.5, theme::ACCENT, s);
            }
            let label = Rect {
                x: entry.x + 14.0,
                w: (entry.w - 22.0).max(0.0),
                ..entry
            };
            let font = if chosen {
                self.faces.strong.clone()
            } else {
                self.faces.body.clone()
            };
            let color = if chosen { theme::TEXT } else { theme::MUTED };
            self.line(pixmap, &font, page.title(), &label, color);
        }
    }

    /// The game-mode page.
    fn game_mode(&mut self, pixmap: &mut PixmapMut<'_>, model: &Model, geometry: &Geometry) {
        let s = self.faces.scale;
        let font = self.faces.heading.clone();
        self.line(
            pixmap,
            &font,
            Page::GameMode.title(),
            &geometry.heading,
            theme::TEXT,
        );

        let (_, _, gap) = EXPLANATION_TEXT;
        let mut top = geometry.explanation.y * s;
        for paragraph in self.explanation(geometry.explanation.w, s) {
            self.fonts.draw_layout(
                &paragraph,
                pixmap,
                (geometry.explanation.x * s).round(),
                top.round(),
            );
            top += paragraph.height + gap * s;
        }

        self.filter(pixmap, model, geometry);
        self.list(pixmap, model, geometry);
        self.foot(pixmap, model, geometry);
    }

    /// The filter box: what was typed and a caret, or what typing does.
    fn filter(&mut self, pixmap: &mut PixmapMut<'_>, model: &Model, geometry: &Geometry) {
        let s = self.faces.scale;
        let field = geometry.filter;
        bordered(pixmap, &field, 9.0, theme::SURFACE, theme::LINE, s);
        let count = format!("{} of {}", model.shown_len(), model.apps.len());
        let small = self.faces.small.clone();
        let count_width = self.fonts.measure(&small, &count) / s;
        let counter = Rect {
            x: field.x + field.w - 14.0 - count_width,
            w: count_width + 1.0,
            ..field
        };
        self.line(pixmap, &small, &count, &counter, theme::MUTED);
        let text = Rect {
            x: field.x + 14.0,
            w: (counter.x - 12.0 - field.x - 14.0).max(0.0),
            ..field
        };
        let body = self.faces.body.clone();
        let typed = if model.filter.is_empty() {
            // After the caret, not under it.
            let hint = Rect {
                x: text.x + 6.0,
                w: (text.w - 6.0).max(0.0),
                ..text
            };
            let _ = self.line(
                pixmap,
                &body,
                "Type to filter the applications",
                &hint,
                theme::MUTED,
            );
            0.0
        } else {
            self.line(pixmap, &body, &model.filter, &text, theme::TEXT)
        };
        let caret = Rect {
            x: text.x + typed.min(text.w) + 1.0,
            y: field.y + 10.0,
            w: 1.5,
            h: field.h - 20.0,
        };
        fill(pixmap, &caret, 0.0, theme::ACCENT, s);
    }

    /// The rows in view, in a bordered box, with what to say when there are
    /// none.
    fn list(&mut self, pixmap: &mut PixmapMut<'_>, model: &Model, geometry: &Geometry) {
        let s = self.faces.scale;
        let visible = geometry.visible_rows();
        let rows = visible.min(model.shown_len().saturating_sub(model.scroll));
        if rows == 0 {
            let message = if model.apps.is_empty() {
                "No application is installed where a launcher looks: \
                 /usr/share/applications and ~/.local/share/applications."
                    .to_owned()
            } else if visible == 0 {
                String::new()
            } else {
                format!("No application matches \u{201c}{}\u{201d}.", model.filter)
            };
            let place = Rect {
                h: layout::ROW,
                ..geometry.list
            };
            let font = self.faces.body.clone();
            let _ = self.line(pixmap, &font, &message, &place, theme::MUTED);
            return;
        }
        let frame = Rect {
            h: rows as f32 * layout::ROW,
            ..geometry.list
        };
        bordered(pixmap, &frame, 10.0, theme::SURFACE, theme::LINE, s);
        for in_view in 0..rows {
            let row = model.scroll.saturating_add(in_view);
            let Some(app) = model.row(row) else {
                continue;
            };
            let place = geometry.row(in_view);
            let inner = Rect {
                x: place.x + 1.0,
                y: place.y + 1.0,
                w: place.w - 2.0,
                h: place.h - 2.0,
            };
            if row == model.selected {
                fill(pixmap, &inner, 9.0, theme::SELECTED, s);
                let mark = Rect {
                    x: place.x + 1.0,
                    y: place.y + 14.0,
                    w: 3.0,
                    h: place.h - 28.0,
                };
                fill(pixmap, &mark, 1.5, theme::ACCENT, s);
            } else if model.hovered == Some(row) {
                fill(pixmap, &inner, 9.0, theme::HOVER, s);
            }
            if in_view > 0 && row != model.selected && row != model.selected.saturating_add(1) {
                let rule = Rect {
                    x: place.x + 14.0,
                    y: place.y,
                    w: place.w - 28.0,
                    h: 1.0,
                };
                fill(pixmap, &rule, 0.0, theme::LINE, s);
            }
            let icon = Rect {
                x: place.x + 16.0,
                y: place.y + (place.h - layout::ICON) / 2.0,
                w: layout::ICON,
                h: layout::ICON,
            };
            let drawn = model
                .app_index(row)
                .is_some_and(|at| self.icon(pixmap, at, &icon));
            if !drawn {
                self.initial(pixmap, app, &icon);
            }
            let switch = Geometry::switch(&place);
            let words_x = icon.x + icon.w + 14.0;
            let words_w = (switch.x - 16.0 - words_x).max(0.0);
            let about = app.about.as_deref().unwrap_or(&app.id);
            let name = Rect {
                x: words_x,
                y: place.y + 8.0,
                w: words_w,
                h: 22.0,
            };
            let below = Rect {
                x: words_x,
                y: place.y + 29.0,
                w: words_w,
                h: 18.0,
            };
            let body = self.faces.body.clone();
            let small = self.faces.small.clone();
            let _ = self.line(pixmap, &body, &app.name, &name, theme::TEXT);
            let _ = self.line(pixmap, &small, about, &below, theme::MUTED);
            draw_switch(pixmap, &switch, model.is_game(app), s);
        }
        // Where the view is in a list longer than it.
        if model.shown_len() > visible && visible > 0 {
            let whole = frame.h - 16.0;
            let share = visible as f32 / model.shown_len() as f32;
            let before = model.scroll as f32 / model.shown_len() as f32;
            let thumb = Rect {
                x: frame.x + frame.w - 5.0,
                y: frame.y + 8.0 + whole * before,
                w: 3.0,
                h: (whole * share).max(12.0),
            };
            fill(pixmap, &thumb, 1.5, theme::OFF, s);
        }
    }

    /// The line at the foot: what the last switch came to, and the keys.
    fn foot(&mut self, pixmap: &mut PixmapMut<'_>, model: &Model, geometry: &Geometry) {
        let s = self.faces.scale;
        let foot = geometry.footer;
        let small = self.faces.small.clone();
        let keys = "Up/Down select \u{b7} Enter switch \u{b7} Esc clear";
        let keys_width = self.fonts.measure(&small, keys) / s;
        // The keys go where there is room for them and the status both.
        let with_keys = foot.w > keys_width + 260.0;
        let status_width = if with_keys {
            foot.w - keys_width - 24.0
        } else {
            foot.w
        };
        let (text, color) = match &model.status {
            Status::Idle => (
                format!(
                    "{} of {} applications in game mode",
                    model.games_installed(),
                    model.apps.len()
                ),
                theme::MUTED,
            ),
            Status::Saved { name, on: true } => (
                format!("Saved: {name} is in game mode from its next start"),
                theme::MUTED,
            ),
            Status::Saved { name, on: false } => (
                format!("Saved: {name} is no longer started in game mode"),
                theme::MUTED,
            ),
            Status::Failed(why) => (format!("Not saved: {why}"), theme::WARNING),
        };
        let status = Rect {
            w: status_width.max(0.0),
            ..foot
        };
        let _ = self.line(pixmap, &small, &text, &status, color);
        if with_keys {
            let place = Rect {
                x: foot.x + foot.w - keys_width,
                w: keys_width + 1.0,
                ..foot
            };
            let _ = self.line(pixmap, &small, keys, &place, theme::MUTED);
        }
    }

    /// One line of `text` in `font`, centred down `place` and cut with an
    /// ellipsis at its width. Answers the width drawn, in logical pixels.
    fn line(
        &mut self,
        pixmap: &mut PixmapMut<'_>,
        font: &Font,
        text: &str,
        place: &Rect,
        color: Color,
    ) -> f32 {
        if text.is_empty() || place.w <= 0.0 {
            return 0.0;
        }
        let s = self.faces.scale;
        let run = self
            .fonts
            .shape_ellipsized(font, text, place.w * s, Ellipsize::End);
        let metrics = self.fonts.metrics(font);
        let baseline = place.y * s + (place.h * s + metrics.ascent - metrics.descent) / 2.0;
        self.fonts
            .draw_run(&run, pixmap, (place.x * s).round(), baseline, color.text());
        run.width / s
    }

    /// Draw the icon of the application at `at` into `place`. Answers
    /// whether there was one to draw.
    fn icon(&mut self, pixmap: &mut PixmapMut<'_>, at: usize, place: &Rect) -> bool {
        let s = self.faces.scale;
        let Some(found) = self.icons.get(at).and_then(Option::as_ref) else {
            return false;
        };
        let size = (place.w * s).round().max(1.0) as u32;
        let key = (found.path.clone(), size);
        let picture = self.pictures.entry(key).or_insert_with(|| {
            let fit = match found.kind {
                Kind::Svg => compositor_image::Fit::Within(size, size),
                Kind::Png => compositor_image::Fit::Natural,
            };
            let picture = compositor_image::load(&found.path, fit).ok()?;
            if picture.width() > size || picture.height() > size {
                return shrink(&picture, size);
            }
            Some(picture)
        });
        let Some(picture) = picture.as_ref() else {
            return false;
        };
        // Centred in its square, whatever its own shape.
        let x = (place.x * s + (place.w * s - picture.width() as f32) / 2.0).round() as i32;
        let y = (place.y * s + (place.h * s - picture.height() as f32) / 2.0).round() as i32;
        pixmap.draw_pixmap(
            x,
            y,
            picture.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
        true
    }

    /// What stands in for an icon that is not there: the name's first
    /// letter on a disc.
    fn initial(&mut self, pixmap: &mut PixmapMut<'_>, app: &App, place: &Rect) {
        let s = self.faces.scale;
        fill(pixmap, place, place.w / 2.0, theme::LINE, s);
        let letter: String = app
            .name
            .chars()
            .find(|c| c.is_alphanumeric())
            .map(|c| c.to_uppercase().collect())
            .unwrap_or_default();
        let font = self.faces.strong.clone();
        let width = self.fonts.measure(&font, &letter) / s;
        let centred = Rect {
            x: place.x + (place.w - width) / 2.0,
            w: width + 1.0,
            ..*place
        };
        let _ = self.line(pixmap, &font, &letter, &centred, theme::MUTED);
    }
}

/// `picture` scaled down to fit a square of `size`, keeping its shape.
fn shrink(picture: &Pixmap, size: u32) -> Option<Pixmap> {
    let longest = picture.width().max(picture.height()).max(1);
    let scale = size as f32 / longest as f32;
    let width = ((picture.width() as f32 * scale).round() as u32).max(1);
    let height = ((picture.height() as f32 * scale).round() as u32).max(1);
    let mut out = Pixmap::new(width, height)?;
    let paint = PixmapPaint {
        quality: FilterQuality::Bilinear,
        ..PixmapPaint::default()
    };
    out.draw_pixmap(
        0,
        0,
        picture.as_ref(),
        &paint,
        Transform::from_scale(scale, scale),
        None,
    );
    Some(out)
}

/// A rounded rectangle's outline, in buffer pixels.
fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    if r <= 0.0 {
        return Some(PathBuilder::from_rect(tiny_skia::Rect::from_xywh(
            x, y, w, h,
        )?));
    }
    // The cubic that best follows a quarter circle.
    let k = r * 0.552_284_8;
    let mut p = PathBuilder::new();
    p.move_to(x + r, y);
    p.line_to(x + w - r, y);
    p.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    p.line_to(x + w, y + h - r);
    p.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    p.line_to(x + r, y + h);
    p.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    p.line_to(x, y + r);
    p.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    p.close();
    p.finish()
}

/// Fill `place`, its corners rounded by `radius`, in `color`: logical
/// pixels, drawn `scale` times as large.
fn fill(pixmap: &mut PixmapMut<'_>, place: &Rect, radius: f32, color: Color, scale: f32) {
    let Some(path) = rounded(
        place.x * scale,
        place.y * scale,
        place.w * scale,
        place.h * scale,
        radius * scale,
    ) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color(color.skia());
    paint.anti_alias = radius > 0.0;
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
}

/// `place` filled in `inside` with a one-pixel edge in `edge`.
fn bordered(
    pixmap: &mut PixmapMut<'_>,
    place: &Rect,
    radius: f32,
    inside: Color,
    edge: Color,
    scale: f32,
) {
    fill(pixmap, place, radius, edge, scale);
    let inner = Rect {
        x: place.x + 1.0,
        y: place.y + 1.0,
        w: place.w - 2.0,
        h: place.h - 2.0,
    };
    fill(pixmap, &inner, (radius - 1.0).max(0.0), inside, scale);
}

/// A switch: a track in the accent with its knob to the right when on, a
/// grey one with its knob to the left when off.
fn draw_switch(pixmap: &mut PixmapMut<'_>, track: &Rect, on: bool, scale: f32) {
    let color = if on { theme::ACCENT } else { theme::OFF };
    fill(pixmap, track, track.h / 2.0, color, scale);
    let size = track.h - 6.0;
    let knob = Rect {
        x: if on {
            track.x + track.w - 3.0 - size
        } else {
            track.x + 3.0
        },
        y: track.y + 3.0,
        w: size,
        h: size,
    };
    let color = if on { theme::KNOB } else { theme::MUTED };
    fill(pixmap, &knob, size / 2.0, color, scale);
}

#[cfg(test)]
mod tests;
