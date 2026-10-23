//! The trash, as the freedesktop.org Trash specification (1.0) keeps it,
//! so GNOME's Files, KDE's Dolphin and `gio trash` see what shelf put
//! there and shelf sees theirs: `$XDG_DATA_HOME/Trash` (`~/.local/share/
//! Trash`), each item in `files/` and, beside it in `info/`, a
//! `<name>.trashinfo` saying where it was and when it was deleted.
//!
//! An item on another file system than the trash is copied in and then
//! removed: the home trash, which the spec allows for any file. The
//! `$topdir/.Trash-$uid` trashes of other file systems are not read yet.

use std::io;
use std::path::{Path, PathBuf};

use crate::fsops;

/// The home trash.
#[must_use]
pub fn home_trash() -> PathBuf {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| tole::files::home().join(".local/share"));
    data.join("Trash")
}

/// One item in the trash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trashed {
    /// Its path under the trash's `files/`.
    pub path: PathBuf,
    /// Its name there.
    pub name: String,
    /// Where it was.
    pub original: PathBuf,
    /// When it was deleted, as written: `2026-10-24T12:00:00`.
    pub deleted: String,
    /// Its `.trashinfo` file.
    pub info: PathBuf,
}

/// `%`-escape a path for a `.trashinfo`'s `Path=`, as RFC 2396 URIs do:
/// everything but unreserved characters and `/`.
fn escape(path: &str) -> String {
    let mut out = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()/".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%'
            && let Some(hex) = text.get(i + 1..i + 3)
            && let Ok(v) = u8::from_str_radix(hex, 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Now, local time, as the spec writes `DeletionDate`.
fn deletion_date() -> String {
    let secs = tole::files::now() + tole::files::local_offset();
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let (y, m, d) = tole::files::civil_date(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
        rest / 3600,
        (rest / 60) % 60,
        rest % 60
    )
}

/// Put `path` in the trash; the item it became.
///
/// # Errors
///
/// No path, the trash cannot be made, or the move failed (the file then
/// stays where it was, and no `.trashinfo` is left).
pub fn trash(path: &Path) -> io::Result<Trashed> {
    trash_in(&home_trash(), path)
}

/// [`trash`], into the trash at `trash`.
///
/// # Errors
///
/// As [`trash`].
pub fn trash_in(trash: &Path, path: &Path) -> io::Result<Trashed> {
    let path = std::path::absolute(path)?;
    let _ = path.symlink_metadata()?;
    let files = trash.join("files");
    let infos = trash.join("info");
    std::fs::create_dir_all(&files)?;
    std::fs::create_dir_all(&infos)?;
    let own = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no name"))?;
    // The info file is made first and exclusively: that claims the name,
    // as the spec asks, against another program trashing at once.
    let mut name = own.clone();
    let mut n = 1;
    let (info, mut file) = loop {
        let info = infos.join(format!("{name}.trashinfo"));
        match std::fs::File::options()
            .write(true)
            .create_new(true)
            .open(&info)
        {
            Ok(file) if files.join(&name).symlink_metadata().is_err() => break (info, file),
            Ok(_) => {
                let _ = std::fs::remove_file(&info);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        n += 1;
        let ext = fsops::extension(&own);
        let base = own.get(..own.len() - ext.len()).unwrap_or(&own);
        name = format!("{base}.{n}{ext}");
    };
    let deleted = deletion_date();
    let text = format!(
        "[Trash Info]\nPath={}\nDeletionDate={deleted}\n",
        escape(&path.to_string_lossy())
    );
    use std::io::Write as _;
    if let Err(error) = file.write_all(text.as_bytes()) {
        let _ = std::fs::remove_file(&info);
        return Err(error);
    }
    drop(file);
    let into = files.join(&name);
    if let Err(error) = fsops::move_to(&path, &into) {
        let _ = std::fs::remove_file(&info);
        return Err(error);
    }
    Ok(Trashed {
        path: into,
        name,
        original: path,
        deleted,
        info,
    })
}

/// What is in the trash at `trash`, newest first.
#[must_use]
pub fn list_in(trash: &Path) -> Vec<Trashed> {
    let Ok(read) = std::fs::read_dir(trash.join("info")) else {
        return Vec::new();
    };
    let mut out: Vec<Trashed> = read
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let info = entry.path();
            let file = info.file_name()?.to_string_lossy().into_owned();
            let name = file.strip_suffix(".trashinfo")?.to_owned();
            let text = std::fs::read_to_string(&info).ok()?;
            let mut original = None;
            let mut deleted = String::new();
            for line in text.lines() {
                if let Some(p) = line.strip_prefix("Path=") {
                    let p = unescape(p);
                    // A relative path is relative to the trash's top
                    // folder; the home trash's are absolute.
                    original = Some(if p.starts_with('/') {
                        PathBuf::from(p)
                    } else {
                        trash
                            .parent()
                            .and_then(Path::parent)
                            .unwrap_or(Path::new("/"))
                            .join(p)
                    });
                } else if let Some(d) = line.strip_prefix("DeletionDate=") {
                    deleted = d.to_owned();
                }
            }
            let path = trash.join("files").join(&name);
            path.symlink_metadata().ok()?;
            Some(Trashed {
                path,
                name,
                original: original?,
                deleted,
                info,
            })
        })
        .collect();
    out.sort_by(|a, b| b.deleted.cmp(&a.deleted).then_with(|| a.name.cmp(&b.name)));
    out
}

/// What is in the home trash.
#[must_use]
pub fn list() -> Vec<Trashed> {
    list_in(&home_trash())
}

/// Put `item` back where it was; under the next free name if that is
/// taken now, and its folder made again if it went. The path it got.
///
/// # Errors
///
/// The move's.
pub fn restore(item: &Trashed) -> io::Result<PathBuf> {
    let dir = item.original.parent().unwrap_or(Path::new("/"));
    std::fs::create_dir_all(dir)?;
    let name = item
        .original
        .file_name()
        .map_or_else(|| item.name.clone(), |n| n.to_string_lossy().into_owned());
    let to = dir.join(fsops::unique_name(dir, &name));
    fsops::move_to(&item.path, &to)?;
    let _ = std::fs::remove_file(&item.info);
    Ok(to)
}

/// Delete `item` for good.
///
/// # Errors
///
/// The removal's.
pub fn delete(item: &Trashed) -> io::Result<()> {
    fsops::remove_tree(&item.path)?;
    let _ = std::fs::remove_file(&item.info);
    Ok(())
}

/// Delete everything in the trash at `trash` for good; how many items.
///
/// # Errors
///
/// The first removal that failed.
pub fn empty_in(trash: &Path) -> io::Result<usize> {
    let items = list_in(trash);
    for item in &items {
        delete(item)?;
    }
    Ok(items.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trashed_items_keep_where_they_were_and_come_back() {
        let dir = std::env::temp_dir().join(format!("shelf-trash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (home, trash) = (dir.join("home"), dir.join("Trash"));
        let _ = std::fs::create_dir_all(home.join("sub"));
        let _ = std::fs::write(home.join("my notes.txt"), b"one");
        let first = trash_in(&trash, &home.join("my notes.txt")).expect("trashed");
        assert!(!home.join("my notes.txt").exists());
        let info = std::fs::read_to_string(&first.info).expect("info");
        assert!(info.starts_with("[Trash Info]\nPath="), "{info}");
        assert!(info.contains("my%20notes.txt"), "{info}");
        assert!(info.contains("DeletionDate="), "{info}");
        // The same name again gets a name of its own in the trash.
        let _ = std::fs::write(home.join("my notes.txt"), b"two");
        let second = trash_in(&trash, &home.join("my notes.txt")).expect("trashed");
        assert_ne!(first.name, second.name);
        let listed = list_in(&trash);
        assert_eq!(listed.len(), 2);
        assert!(
            listed
                .iter()
                .all(|t| t.original == home.join("my notes.txt"))
        );
        // Restored, the first takes its name; the second, the next one.
        let back = restore(&first).expect("restored");
        assert_eq!(back, home.join("my notes.txt"));
        let again = restore(&second).expect("restored");
        assert_eq!(again, home.join("my notes 2.txt"));
        assert!(list_in(&trash).is_empty());
        let _ = trash_in(&trash, &home.join("sub")).expect("a folder too");
        assert_eq!(empty_in(&trash).expect("emptied"), 1);
        assert!(list_in(&trash).is_empty());
        assert_eq!(unescape(&escape("/a b/ü%")), "/a b/ü%");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
