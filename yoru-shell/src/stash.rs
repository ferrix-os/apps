//! The stash: files put aside to be picked up again, as the design's dock
//! keeps them. Each is a link in `~/.local/share/yoru-shell/stash/` to the
//! file where it is, so nothing is moved or copied and clearing the stash
//! removes only the links. The newest is first. Files dropped on the dock's
//! Stash button come in here, as `yoru-shell --stash FILE...` does; the
//! panel (`yoru-shell --stash-panel`) lists them, opens one with
//! `xdg-open`, and opens the folder or clears it.

use std::path::{Path, PathBuf};

/// The folder of links under `$XDG_DATA_HOME` (or `~/.local/share`).
#[must_use]
pub fn folder() -> Option<PathBuf> {
    let base = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .filter(|h| !h.is_empty())
                .map(|h| Path::new(&h).join(".local/share"))
        })?;
    Some(base.join("yoru-shell/stash"))
}

/// What a file is, for its tile's icon and tint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A PDF: pink.
    Pdf,
    /// Sound: teal.
    Audio,
    /// Source code: blue.
    Code,
    /// Words: violet.
    Text,
    /// A picture.
    Image,
    /// A video.
    Video,
    /// An archive.
    Archive,
    /// A folder.
    Folder,
    /// Anything else.
    Other,
}

impl Kind {
    /// The kind of the file named `name` (a folder when `dir`).
    #[must_use]
    pub fn of(name: &str, dir: bool) -> Self {
        if dir {
            return Kind::Folder;
        }
        let ext = name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "pdf" => Kind::Pdf,
            "flac" | "mp3" | "ogg" | "opus" | "wav" | "m4a" | "aac" => Kind::Audio,
            "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "c" | "h" | "cpp" | "hpp" | "go"
            | "java" | "sh" | "toml" | "json" | "yaml" | "yml" | "html" | "css" | "zig" | "lua" => {
                Kind::Code
            }
            "md" | "txt" | "rst" | "org" | "doc" | "docx" | "odt" | "rtf" | "tex" => Kind::Text,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "avif" | "bmp" => Kind::Image,
            "mp4" | "webm" | "mkv" | "mov" | "ivf" => Kind::Video,
            "zip" | "tar" | "gz" | "xz" | "zst" | "bz2" | "7z" | "rar" | "tgz" => Kind::Archive,
            _ => Kind::Other,
        }
    }

    /// Phosphor's icon for it.
    #[must_use]
    pub fn icon(self) -> &'static str {
        match self {
            Kind::Pdf => "file-pdf",
            Kind::Audio => "file-audio",
            Kind::Code => "file-code",
            Kind::Text => "file-text",
            Kind::Image => "file-image",
            Kind::Video => "file-video",
            Kind::Archive => "file-zip",
            Kind::Folder => "folder",
            Kind::Other => "file",
        }
    }

    /// Its tile's tint, `0xRRGGBB`, from Nocturne's accents.
    #[must_use]
    pub fn tint(self) -> u32 {
        match self {
            Kind::Pdf => 0xce_7bb4,
            Kind::Audio => 0x00_b2ba,
            Kind::Code => 0x6c_9fe6,
            Kind::Text => 0x96_8ae0,
            Kind::Image => 0xe4_97cb,
            Kind::Video => 0x46_cad0,
            Kind::Archive => 0xd6_a85c,
            Kind::Folder => 0xb5_abfc,
            Kind::Other => 0x93_97ab,
        }
    }
}

/// One file in the stash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// The link's name.
    pub link: String,
    /// The file it points at.
    pub target: PathBuf,
    /// The name shown: the file's.
    pub name: String,
    /// What it is.
    pub kind: Kind,
    /// The file is no longer there.
    pub gone: bool,
}

/// The files in the stash at `dir`, the newest first.
#[must_use]
pub fn list(dir: &Path) -> Vec<Item> {
    let mut found: Vec<(std::time::SystemTime, Item)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let link = entry.file_name().into_string().ok()?;
            let target = std::fs::read_link(entry.path()).ok()?;
            let when = std::fs::symlink_metadata(entry.path())
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            let meta = std::fs::metadata(&target);
            let name = target
                .file_name()
                .map_or_else(|| link.clone(), |n| n.to_string_lossy().into_owned());
            let kind = Kind::of(&name, meta.as_ref().is_ok_and(std::fs::Metadata::is_dir));
            Some((
                when,
                Item {
                    link,
                    target,
                    name,
                    kind,
                    gone: meta.is_err(),
                },
            ))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.link.cmp(&b.1.link)));
    found.into_iter().map(|(_, item)| item).collect()
}

/// How many files the stash at `dir` holds.
#[must_use]
pub fn count(dir: &Path) -> usize {
    std::fs::read_dir(dir).map_or(0, |entries| entries.flatten().count())
}

/// The dock's badge for `count` files: none for an empty stash, `9+` past
/// nine.
#[must_use]
pub fn badge(count: usize) -> String {
    match count {
        0 => String::new(),
        1..=9 => count.to_string(),
        _ => "9+".to_owned(),
    }
}

/// Put `files` in the stash at `dir`; how many came in (one already there
/// comes to the front again).
///
/// # Errors
///
/// The folder could not be made.
pub fn add(dir: &Path, files: &[PathBuf]) -> std::io::Result<usize> {
    std::fs::create_dir_all(dir)?;
    let held = list(dir);
    let mut added = 0;
    for file in files {
        let Ok(file) = std::path::absolute(file) else {
            continue;
        };
        if !file.exists() {
            continue;
        }
        if let Some(old) = held.iter().find(|item| item.target == file) {
            let _ = std::fs::remove_file(dir.join(&old.link));
        }
        let name = file
            .file_name()
            .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned());
        let mut link = dir.join(&name);
        let mut n = 2;
        while std::fs::symlink_metadata(&link).is_ok() {
            link = dir.join(format!("{n}-{name}"));
            n += 1;
        }
        if std::os::unix::fs::symlink(&file, &link).is_ok() {
            added += 1;
        }
    }
    Ok(added)
}

/// Take the file linked as `link` out of the stash at `dir`.
pub fn remove(dir: &Path, link: &str) {
    if !link.contains('/') {
        let _ = std::fs::remove_file(dir.join(link));
    }
}

/// Empty the stash at `dir`: the links go, the files stay.
pub fn clear(dir: &Path) {
    for item in list(dir) {
        remove(dir, &item.link);
    }
}

/// The files a drop of `text/uri-list` names: its `file://` lines,
/// percent-decoded, comments and other schemes left out.
#[must_use]
pub fn uris(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.strip_prefix("file://"))
        .map(|rest| {
            // `file://host/path`: the path starts at the first slash.
            let path = rest
                .find('/')
                .map_or(rest, |at| rest.get(at..).unwrap_or(rest));
            PathBuf::from(decode(path))
        })
        .collect()
}

/// `%XX` escapes decoded.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%'
            && let Some(hex) = text.get(i + 1..i + 3)
            && let Ok(value) = u8::from_str_radix(hex, 16)
        {
            out.push(value);
            i += 3;
            continue;
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_take_the_customer_s_tints() {
        assert_eq!(Kind::of("Report.PDF", false), Kind::Pdf);
        assert_eq!(Kind::of("song.flac", false), Kind::Audio);
        assert_eq!(Kind::of("main.rs", false), Kind::Code);
        assert_eq!(Kind::of("notes.md", false), Kind::Text);
        assert_eq!(Kind::of("x", true), Kind::Folder);
        assert_eq!(Kind::of("blob", false), Kind::Other);
        assert_eq!(Kind::Pdf.icon(), "file-pdf");
        assert_eq!(badge(0), "");
        assert_eq!(badge(3), "3");
        assert_eq!(badge(12), "9+");
    }

    #[test]
    fn a_drop_names_its_files() {
        let text = "# from a browser\r\nfile:///home/a/My%20File.pdf\r\nhttps://x.org/y\nfile://host/tmp/b.md\n";
        assert_eq!(
            uris(text),
            vec![
                PathBuf::from("/home/a/My File.pdf"),
                PathBuf::from("/tmp/b.md")
            ]
        );
    }

    #[test]
    fn files_come_in_newest_first_and_go_as_links() {
        let root = std::env::temp_dir().join(format!("yoru-stash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let stash = root.join("stash");
        let a = root.join("a/report.pdf");
        let b = root.join("b/report.pdf");
        let c = root.join("c/song.flac");
        for file in [&a, &b, &c] {
            std::fs::create_dir_all(file.parent().expect("a folder")).expect("made");
            std::fs::write(file, b"x").expect("written");
        }
        assert_eq!(add(&stash, &[a.clone(), b.clone()]).expect("added"), 2);
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(add(&stash, std::slice::from_ref(&c)).expect("added"), 1);
        assert_eq!(add(&stash, &[root.join("missing")]).expect("added"), 0);
        let items = list(&stash);
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names.first(), Some(&"song.flac"), "the newest first");
        assert_eq!(items.len(), 3, "two of the same name both kept");
        assert_eq!(count(&stash), 3);
        // The same file again comes to the front, not twice.
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(add(&stash, std::slice::from_ref(&a)).expect("added"), 1);
        let items = list(&stash);
        assert_eq!(items.len(), 3);
        assert_eq!(items.first().map(|i| i.target.clone()), Some(a.clone()));
        remove(&stash, &items.first().expect("one").link);
        assert_eq!(count(&stash), 2);
        clear(&stash);
        assert_eq!(count(&stash), 0);
        assert!(c.exists(), "clearing keeps the files");
        let _ = std::fs::remove_dir_all(&root);
    }
}
