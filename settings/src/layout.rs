//! Where everything is, in the window's logical pixels, and what is under
//! the pointer.
//!
//! ```text
//! +----------+---------------------------------------------+
//! | Settings |  Game mode                                  |
//! |          |  What it does, in a paragraph or two.       |
//! | Game mode|  [ filter ................................ ]|
//! |          |  +-----------------------------------------+|
//! |          |  | icon  Name                       (  o ) ||
//! |          |  | icon  Name                       ( o  ) ||
//! |          |  +-----------------------------------------+|
//! |          |  2 of 7 in game mode            the keys    |
//! +----------+---------------------------------------------+
//! ```
//!
//! Only whole rows are shown: the list is as tall as the rows that fit.

use crate::model::Page;

/// The sidebar's width, where the window is wide enough for it.
pub const SIDEBAR: f32 = 208.0;
/// The space around the page's content.
pub const PAD: f32 = 28.0;
/// One application's row.
pub const ROW: f32 = 56.0;
/// A page's entry in the sidebar.
pub const PAGE_ROW: f32 = 36.0;
/// The filter box.
pub const FILTER: f32 = 38.0;
/// The line at the foot.
pub const FOOTER: f32 = 40.0;
/// The heading's band at the top of a page, and the sidebar's title.
pub const HEADING: f32 = 64.0;
/// A switch: its track.
pub const SWITCH: (f32, f32) = (40.0, 22.0);
/// An application's icon.
pub const ICON: f32 = 32.0;
/// The size a window asks for where the compositor leaves it the choice.
pub const PREFERRED: (u32, u32) = (920, 640);

/// A rectangle.
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
    /// Whether the point is inside.
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// What is under a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// A page's entry in the sidebar.
    Page(Page),
    /// The row of the list in this place among the shown ones.
    Row(usize),
    /// That row's switch.
    Switch(usize),
    /// Nothing that does anything.
    Nothing,
}

/// Where the parts of the window are.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Geometry {
    /// The window's width.
    pub width: f32,
    /// The window's height.
    pub height: f32,
    /// The sidebar.
    pub sidebar: Rect,
    /// The page's heading.
    pub heading: Rect,
    /// The paragraphs under it.
    pub explanation: Rect,
    /// The filter box.
    pub filter: Rect,
    /// The list: whole rows only.
    pub list: Rect,
    /// The line at the foot.
    pub footer: Rect,
}

impl Geometry {
    /// Lay a window of `width` by `height` out, with an explanation
    /// `explanation` tall at [`Geometry::text_width`].
    #[must_use]
    pub fn new(width: f32, height: f32, explanation: f32) -> Self {
        let sidebar = Rect {
            x: 0.0,
            y: 0.0,
            w: Self::sidebar_width(width),
            h: height,
        };
        let x = sidebar.w + PAD;
        let w = (width - x - PAD).max(0.0);
        let heading = Rect {
            x,
            y: 0.0,
            w,
            h: HEADING,
        };
        let explanation = Rect {
            x,
            y: heading.y + heading.h,
            w,
            h: explanation,
        };
        let filter = Rect {
            x,
            y: explanation.y + explanation.h + 18.0,
            w,
            h: FILTER,
        };
        let footer = Rect {
            x,
            y: (height - FOOTER).max(0.0),
            w,
            h: FOOTER,
        };
        let top = filter.y + filter.h + 12.0;
        let room = (footer.y - 4.0 - top).max(0.0);
        let list = Rect {
            x,
            y: top,
            w,
            h: (room / ROW).floor() * ROW,
        };
        Self {
            width,
            height,
            sidebar,
            heading,
            explanation,
            filter,
            list,
            footer,
        }
    }

    /// How wide the sidebar is in a window `width` wide: its own width, or
    /// three tenths of a narrow window.
    #[must_use]
    pub fn sidebar_width(width: f32) -> f32 {
        SIDEBAR.min(width * 0.3).max(0.0)
    }

    /// How wide the page's text is in a window `width` wide.
    #[must_use]
    pub fn text_width(width: f32) -> f32 {
        (width - Self::sidebar_width(width) - 2.0 * PAD).max(0.0)
    }

    /// How many rows are in view.
    #[must_use]
    pub fn visible_rows(&self) -> usize {
        (self.list.h / ROW).round().max(0.0) as usize
    }

    /// Where the `index`th page's entry is in the sidebar.
    #[must_use]
    pub fn page(&self, index: usize) -> Rect {
        Rect {
            x: 8.0,
            y: HEADING + index as f32 * (PAGE_ROW + 4.0),
            w: (self.sidebar.w - 16.0).max(0.0),
            h: PAGE_ROW,
        }
    }

    /// Where the `in_view`th row in view is.
    #[must_use]
    pub fn row(&self, in_view: usize) -> Rect {
        Rect {
            x: self.list.x,
            y: self.list.y + in_view as f32 * ROW,
            w: self.list.w,
            h: ROW,
        }
    }

    /// Where a row's switch is.
    #[must_use]
    pub fn switch(row: &Rect) -> Rect {
        Rect {
            x: row.x + row.w - 16.0 - SWITCH.0,
            y: row.y + (row.h - SWITCH.1) / 2.0,
            w: SWITCH.0,
            h: SWITCH.1,
        }
    }

    /// What is at (`x`, `y`) when the list's first row in view is `scroll`
    /// of `shown`.
    #[must_use]
    pub fn hit(&self, x: f32, y: f32, scroll: usize, shown: usize) -> Hit {
        for (index, page) in Page::ALL.into_iter().enumerate() {
            if self.page(index).contains(x, y) {
                return Hit::Page(page);
            }
        }
        if !self.list.contains(x, y) {
            return Hit::Nothing;
        }
        let in_view = ((y - self.list.y) / ROW).floor().max(0.0) as usize;
        let row = scroll.saturating_add(in_view);
        if in_view >= self.visible_rows() || row >= shown {
            return Hit::Nothing;
        }
        // A little around the switch counts as the switch: it is small.
        let switch = Self::switch(&self.row(in_view));
        let reach = Rect {
            x: switch.x - 10.0,
            y: switch.y - 10.0,
            w: switch.w + 20.0,
            h: switch.h + 20.0,
        };
        if reach.contains(x, y) {
            Hit::Switch(row)
        } else {
            Hit::Row(row)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Geometry, Hit, PAD, ROW, SIDEBAR};
    use crate::model::Page;

    #[test]
    fn the_parts_stack_and_the_list_holds_whole_rows() {
        let g = Geometry::new(920.0, 640.0, 100.0);
        assert_eq!(g.sidebar.w, SIDEBAR);
        assert_eq!(g.heading.x, SIDEBAR + PAD);
        assert_eq!(g.list.w, 920.0 - SIDEBAR - 2.0 * PAD);
        assert!(g.explanation.y >= g.heading.y + g.heading.h);
        assert!(g.filter.y > g.explanation.y + g.explanation.h);
        assert!(g.list.y > g.filter.y + g.filter.h);
        assert!(g.list.y + g.list.h <= g.footer.y);
        assert_eq!(g.footer.y + g.footer.h, 640.0);
        assert_eq!(g.list.h % ROW, 0.0);
        assert_eq!(g.visible_rows(), (g.list.h / ROW) as usize);
        assert!(g.visible_rows() >= 5, "{}", g.visible_rows());
        assert_eq!(Geometry::text_width(920.0), g.list.w);
    }

    #[test]
    fn a_window_too_small_has_no_rows_and_does_not_go_negative() {
        let g = Geometry::new(300.0, 120.0, 200.0);
        assert_eq!(g.visible_rows(), 0);
        assert!(g.list.h == 0.0 && g.list.w >= 0.0);
        assert!(g.sidebar.w < SIDEBAR);
        assert_eq!(g.hit(200.0, 100.0, 0, 5), Hit::Nothing);
        let nothing = Geometry::new(0.0, 0.0, 0.0);
        assert_eq!(nothing.visible_rows(), 0);
    }

    #[test]
    fn a_point_is_a_page_a_row_or_its_switch() {
        let g = Geometry::new(920.0, 640.0, 100.0);
        let page = g.page(0);
        assert_eq!(
            g.hit(page.x + 5.0, page.y + 5.0, 0, 9),
            Hit::Page(Page::GameMode)
        );
        let second = g.row(1);
        assert_eq!(g.hit(second.x + 20.0, second.y + 20.0, 0, 9), Hit::Row(1));
        // Scrolled by three, the second row in view is the fifth.
        assert_eq!(g.hit(second.x + 20.0, second.y + 20.0, 3, 9), Hit::Row(4));
        let switch = Geometry::switch(&second);
        assert_eq!(g.hit(switch.x + 2.0, switch.y + 2.0, 0, 9), Hit::Switch(1));
        // Just outside the track still turns it.
        assert_eq!(g.hit(switch.x - 6.0, switch.y - 6.0, 0, 9), Hit::Switch(1));
        // A row past the end of a short list is nothing.
        assert_eq!(g.hit(second.x + 20.0, second.y + 20.0, 0, 1), Hit::Nothing);
        // Nor is the filter box, the heading or the foot.
        assert_eq!(
            g.hit(g.filter.x + 5.0, g.filter.y + 5.0, 0, 9),
            Hit::Nothing
        );
        assert_eq!(
            g.hit(g.footer.x + 5.0, g.footer.y + 5.0, 0, 9),
            Hit::Nothing
        );
    }
}
