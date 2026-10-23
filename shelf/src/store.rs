//! What shelf keeps between runs, in plain text a person can read: the
//! tags given to files (`$XDG_DATA_HOME/shelf/tags`, a line per file: its
//! tags, a tab, its path), the places pinned to Quick access and the
//! settings (`$XDG_CONFIG_HOME/shelf/quick` and `settings`).
//!
//! Tags follow a file shelf moves or renames; a file moved by another
//! program loses them, as it would lose an extended attribute on another
//! file system.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where the tags are kept.
fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| tole::files::home().join(".local/share"))
        .join("shelf")
}

/// Where the pins and settings are kept.
fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| tole::files::home().join(".config"))
        .join("shelf")
}

/// A path written on one line: `\`, tab and newline escaped.
fn escape(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

fn unescape(text: &str) -> PathBuf {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    PathBuf::from(out)
}

/// The colours of the design's tags; any other tag takes one of these by
/// its name.
const COLORS: [(&str, u32); 4] = [
    ("prod", 0xe573_7aff),
    ("staging", 0xf0c6_74ff),
    ("todo", 0x7ea6_f6ff),
    ("wallpaper", 0xc3a6_f5ff),
];

/// More colours, for tags of a person's own.
const MORE: [u32; 6] = [
    0x7fd1_a8ff,
    0xf0a0_b4ff,
    0x9dba_f8ff,
    0xe8c9_7aff,
    0xa9b6_ccff,
    0xd49a_f0ff,
];

/// A tag's colour, as `#rrggbbaa`.
#[must_use]
pub fn tag_color(tag: &str) -> u32 {
    if let Some((_, color)) = COLORS.iter().find(|(name, _)| *name == tag) {
        return *color;
    }
    let sum = tag.bytes().fold(0_usize, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(usize::from(b))
    });
    MORE.get(sum % MORE.len()).copied().unwrap_or(0x8b8f_98ff)
}

/// The tags, by file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tags {
    by_path: BTreeMap<PathBuf, Vec<String>>,
    file: Option<PathBuf>,
}

impl Tags {
    /// The tags kept in `$XDG_DATA_HOME/shelf/tags`.
    #[must_use]
    pub fn load() -> Self {
        Self::load_from(data_dir().join("tags"))
    }

    /// The tags kept in `file`.
    #[must_use]
    pub fn load_from(file: PathBuf) -> Self {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        let by_path = text
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .map(|(tags, path)| {
                let tags: Vec<String> = tags
                    .split(',')
                    .filter(|t| !t.is_empty())
                    .map(str::to_owned)
                    .collect();
                (unescape(path), tags)
            })
            .filter(|(_, tags)| !tags.is_empty())
            .collect();
        Tags {
            by_path,
            file: Some(file),
        }
    }

    /// Tags kept nowhere, for a drawing or a test.
    #[must_use]
    pub fn in_memory() -> Self {
        Tags::default()
    }

    fn save(&self) {
        let Some(file) = &self.file else {
            return;
        };
        let mut text = String::new();
        for (path, tags) in &self.by_path {
            text.push_str(&format!("{}\t{}\n", tags.join(","), escape(path)));
        }
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let temp = file.with_extension("new");
        if std::fs::write(&temp, text).is_ok() {
            let _ = std::fs::rename(&temp, file);
        }
    }

    /// The tags of `path`.
    #[must_use]
    pub fn of(&self, path: &Path) -> &[String] {
        self.by_path.get(path).map_or(&[], Vec::as_slice)
    }

    /// Give `path` the tag `tag`, or take it away.
    pub fn set(&mut self, path: &Path, tag: &str, on: bool) {
        let tag = tag.trim().replace([',', '\t', '\n'], " ");
        if tag.is_empty() {
            return;
        }
        let tags = self.by_path.entry(path.to_path_buf()).or_default();
        let has = tags.contains(&tag);
        if on && !has {
            tags.push(tag);
        } else if !on && has {
            tags.retain(|t| *t != tag);
        }
        if tags.is_empty() {
            let _ = self.by_path.remove(path);
        }
        self.save();
    }

    /// Every tag given, with how many files have it, by name.
    #[must_use]
    pub fn all(&self) -> Vec<(String, usize)> {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for tags in self.by_path.values() {
            for tag in tags {
                *counts.entry(tag).or_default() += 1;
            }
        }
        counts.into_iter().map(|(t, n)| (t.to_owned(), n)).collect()
    }

    /// The files with `tag` that are still there.
    #[must_use]
    pub fn files_with(&self, tag: &str) -> Vec<PathBuf> {
        self.by_path
            .iter()
            .filter(|(path, tags)| tags.iter().any(|t| t == tag) && path.symlink_metadata().is_ok())
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// `from` (and all under it) is at `to` now.
    pub fn moved(&mut self, from: &Path, to: &Path) {
        let moving: Vec<PathBuf> = self
            .by_path
            .keys()
            .filter(|p| p.starts_with(from))
            .cloned()
            .collect();
        if moving.is_empty() {
            return;
        }
        for old in moving {
            if let Some(tags) = self.by_path.remove(&old) {
                let rest = old.strip_prefix(from).unwrap_or(Path::new(""));
                let new = if rest.as_os_str().is_empty() {
                    to.to_path_buf()
                } else {
                    to.join(rest)
                };
                let _ = self.by_path.insert(new, tags);
            }
        }
        self.save();
    }

    /// A copy of `from`'s tags for `to`, and all under it.
    pub fn copied(&mut self, from: &Path, to: &Path) {
        let copies: Vec<(PathBuf, Vec<String>)> = self
            .by_path
            .iter()
            .filter(|(p, _)| p.starts_with(from))
            .map(|(old, tags)| {
                let rest = old.strip_prefix(from).unwrap_or(Path::new(""));
                let new = if rest.as_os_str().is_empty() {
                    to.to_path_buf()
                } else {
                    to.join(rest)
                };
                (new, tags.clone())
            })
            .collect();
        if copies.is_empty() {
            return;
        }
        self.by_path.extend(copies);
        self.save();
    }
}

/// The places pinned to Quick access.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Quick {
    /// The pinned paths, in order.
    pub paths: Vec<PathBuf>,
    file: Option<PathBuf>,
}

impl Quick {
    /// The pins kept in `$XDG_CONFIG_HOME/shelf/quick`; none the first
    /// time.
    #[must_use]
    pub fn load() -> Self {
        let file = config_dir().join("quick");
        let paths = match std::fs::read_to_string(&file) {
            Ok(text) => text
                .lines()
                .filter(|l| !l.is_empty())
                .map(unescape)
                .collect(),
            Err(_) => Vec::new(),
        };
        Quick {
            paths,
            file: Some(file),
        }
    }

    /// Pins kept nowhere.
    #[must_use]
    pub fn in_memory(paths: Vec<PathBuf>) -> Self {
        Quick { paths, file: None }
    }

    /// Whether `path` is pinned.
    #[must_use]
    pub fn has(&self, path: &Path) -> bool {
        self.paths.iter().any(|p| p == path)
    }

    /// Pin `path`, or unpin it if it is.
    pub fn toggle(&mut self, path: &Path) {
        if self.has(path) {
            self.paths.retain(|p| p != path);
        } else {
            self.paths.push(path.to_path_buf());
        }
        self.save();
    }

    /// `from` is at `to` now.
    pub fn moved(&mut self, from: &Path, to: &Path) {
        let mut any = false;
        for p in &mut self.paths {
            if p.starts_with(from) {
                let rest = p.strip_prefix(from).unwrap_or(Path::new("")).to_path_buf();
                *p = if rest.as_os_str().is_empty() {
                    to.to_path_buf()
                } else {
                    to.join(rest)
                };
                any = true;
            }
        }
        if any {
            self.save();
        }
    }

    fn save(&self) {
        let Some(file) = &self.file else {
            return;
        };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text: String = self
            .paths
            .iter()
            .map(|p| format!("{}\n", escape(p)))
            .collect();
        let _ = std::fs::write(file, text);
    }
}

/// How shelf looks and lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Names starting with `.` are shown.
    pub hidden: bool,
    /// Folders before files, whatever the sort.
    pub folders_first: bool,
    /// The preview pane, while something is chosen.
    pub preview: bool,
    /// Extensions shown in names.
    pub extensions: bool,
    /// Folders' sizes added up.
    pub folder_sizes: bool,
    /// Rows closer together.
    pub compact: bool,
    /// The dark palette.
    pub dark: bool,
    /// `name`, `kind`, `size` or `modified`.
    pub sort: String,
    /// Upwards.
    pub ascending: bool,
    /// The grid rather than the list.
    pub grid: bool,
    /// The search's mode: 0 the name contains it, 1 a pattern, 2 a
    /// regular expression.
    pub search_mode: i32,
    file: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hidden: false,
            folders_first: true,
            preview: true,
            extensions: true,
            folder_sizes: true,
            compact: false,
            dark: true,
            sort: "name".to_owned(),
            ascending: true,
            grid: false,
            search_mode: 0,
            file: None,
        }
    }
}

impl Settings {
    /// The settings kept in `$XDG_CONFIG_HOME/shelf/settings`.
    #[must_use]
    pub fn load() -> Self {
        let file = config_dir().join("settings");
        let mut settings = Settings {
            file: Some(file.clone()),
            ..Settings::default()
        };
        if let Ok(text) = std::fs::read_to_string(&file) {
            for line in text.lines() {
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let (key, value) = (key.trim(), value.trim());
                let on = value == "true";
                match key {
                    "hidden" => settings.hidden = on,
                    "folders-first" => settings.folders_first = on,
                    "preview" => settings.preview = on,
                    "extensions" => settings.extensions = on,
                    "folder-sizes" => settings.folder_sizes = on,
                    "compact" => settings.compact = on,
                    "dark" => settings.dark = on,
                    "sort" if ["name", "kind", "size", "modified"].contains(&value) => {
                        settings.sort = value.to_owned()
                    }
                    "ascending" => settings.ascending = on,
                    "grid" => settings.grid = on,
                    "search-mode" => settings.search_mode = value.parse().unwrap_or(0).clamp(0, 2),
                    _ => {}
                }
            }
        }
        settings
    }

    /// Settings kept nowhere.
    #[must_use]
    pub fn in_memory() -> Self {
        Settings::default()
    }

    /// Write them down.
    pub fn save(&self) {
        let Some(file) = &self.file else {
            return;
        };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text = format!(
            "hidden={}\nfolders-first={}\npreview={}\nextensions={}\nfolder-sizes={}\ncompact={}\ndark={}\nsort={}\nascending={}\ngrid={}\nsearch-mode={}\n",
            self.hidden,
            self.folders_first,
            self.preview,
            self.extensions,
            self.folder_sizes,
            self.compact,
            self.dark,
            self.sort,
            self.ascending,
            self.grid,
            self.search_mode
        );
        let _ = std::fs::write(file, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_kept_follow_moves_and_count() {
        let dir = std::env::temp_dir().join(format!("shelf-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("a"));
        let _ = std::fs::write(dir.join("a/x\tname"), b"");
        let file = dir.join("tags");
        let mut tags = Tags::load_from(file.clone());
        tags.set(&dir.join("a/x\tname"), "prod", true);
        tags.set(&dir.join("a/x\tname"), "todo", true);
        tags.set(&dir.join("a"), "todo", true);
        let again = Tags::load_from(file.clone());
        assert_eq!(again.of(&dir.join("a/x\tname")), ["prod", "todo"]);
        assert_eq!(
            again.all(),
            [("prod".to_owned(), 1), ("todo".to_owned(), 2)]
        );
        tags.moved(&dir.join("a"), &dir.join("b"));
        assert_eq!(tags.of(&dir.join("b/x\tname")), ["prod", "todo"]);
        assert!(tags.of(&dir.join("a/x\tname")).is_empty());
        tags.set(&dir.join("b"), "todo", false);
        assert_eq!(tags.all(), [("prod".to_owned(), 1), ("todo".to_owned(), 1)]);
        assert_eq!(tag_color("prod"), 0xe573_7aff);
        assert_eq!(tag_color("mine"), tag_color("mine"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
