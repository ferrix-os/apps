//! The mark, in text.
//!
//! `docs/brand/logo-mark.svg`: an F whose leg runs out into an X, a small
//! chip under its middle bar, and a rust slash where the X's other stroke
//! would be. Drawn here in ASCII, because a terminal's font may have nothing
//! else; the gaps between the pieces are wider than the SVG's, or a column
//! of text could not show them.

/// What a character of the mark is, which is what colours it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The F, its leg and the chip: the mark's grey.
    Body,
    /// The slash: rust.
    Slash,
    /// Nothing: a space.
    Space,
}

/// The mark, a row a line.
pub const ROWS: [&str; 12] = [
    r"#####################/  /######/",
    r"###################/  /######/",
    r"#####               /######/",
    r"#####             /######/",
    r"################\ \####/",
    r"##################\ \/",
    r"#####       \#######\",
    r"#####      /\ \#######\",
    r"#####    /###\  \#######\",
    r"#####  /######/   \#######\",
    r"#####  #####/       \#######\",
    r"#####  ###/           \#######\",
];

/// The column each row's slash starts at; past the slash's last row, past
/// the row's end.
const SLASH: [usize; ROWS.len()] = [24, 22, 20, 18, 18, 20, 32, 32, 32, 32, 32, 32];

/// The widest row, in columns: every character is one. The tests hold it to
/// [`ROWS`].
pub const WIDTH: usize = 32;

/// What the character at `column` of `row` is.
#[must_use]
pub fn part(row: usize, column: usize) -> Part {
    let Some(text) = ROWS.get(row) else {
        return Part::Space;
    };
    match text.as_bytes().get(column) {
        None | Some(b' ') => Part::Space,
        Some(_) if column >= SLASH[row] => Part::Slash,
        Some(_) => Part::Body,
    }
}
