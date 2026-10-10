//! Turning one switch in the user's game-mode file.
//!
//! The window holds a copy of the set to draw, but the file is the truth:
//! somebody may have edited it since it was read. So a switch is turned by
//! reading the file again, changing the one application in what was read,
//! and writing that back whole (`compositor_desktop::game_mode`, whose write
//! replaces the file in one rename). A hand edit made a moment ago to
//! another application's line is kept.

use std::path::Path;

use compositor_desktop::game_mode::GameMode;

/// Put application `id` in game mode in the file at `path`, or take it out,
/// and answer the set the file now holds.
///
/// # Errors
///
/// The file cannot be read or written, in words for the user; the file is
/// then as it was.
pub fn switch(path: &Path, id: &str, on: bool) -> Result<GameMode, String> {
    let (mut set, _complaints) = GameMode::load(path)
        .map_err(|error| format!("{} cannot be read: {error}", path.display()))?;
    if on && !compositor_desktop::game_mode::is_id(id) {
        return Err(format!("{id:?} is not a name the game-mode file can hold"));
    }
    let _changed = set.set(id, on);
    // Written even when the set did not change: the file may not exist yet,
    // and a switch the user turned should be on disk either way.
    set.save(path)
        .map_err(|error| format!("{} cannot be written: {error}", path.display()))?;
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::switch;
    use crate::testdir::TestDir;

    #[test]
    fn a_switch_changes_its_own_line_and_keeps_the_others() {
        let dir = TestDir::new("store");
        let file = dir.path().join("ferrix/game-mode.conf");
        // No file, no directory: the first switch makes both.
        let set = switch(&file, "steam.desktop", true).expect("written");
        assert_eq!(set.ids().collect::<Vec<_>>(), vec!["steam.desktop"]);

        // Somebody adds a line by hand; turning another switch keeps it.
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        let _ = std::fs::write(&file, format!("{text}hand.desktop\n"));
        let set = switch(&file, "pattern.desktop", true).expect("written");
        assert_eq!(
            set.ids().collect::<Vec<_>>(),
            vec!["hand.desktop", "pattern.desktop", "steam.desktop"]
        );
        let set = switch(&file, "steam.desktop", false).expect("written");
        assert_eq!(
            set.ids().collect::<Vec<_>>(),
            vec!["hand.desktop", "pattern.desktop"]
        );
        let on_disk = std::fs::read_to_string(&file).unwrap_or_default();
        assert!(
            on_disk.ends_with("hand.desktop\npattern.desktop\n"),
            "{on_disk}"
        );
    }

    #[test]
    fn a_file_that_cannot_be_written_is_said_and_nothing_changes() {
        let dir = TestDir::new("store-refused");
        let _ = dir.write("ferrix", "a file where the directory should be");
        let file = dir.path().join("ferrix/game-mode.conf");
        let refused = switch(&file, "steam.desktop", true).expect_err("refused");
        assert!(refused.contains("game-mode.conf cannot be"), "{refused}");
        let refused =
            switch(&dir.path().join("x/game-mode.conf"), "two words", true).expect_err("refused");
        assert!(refused.contains("not a name"), "{refused}");
    }
}
