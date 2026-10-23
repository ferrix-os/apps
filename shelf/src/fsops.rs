//! What shelf does to files: copy, move and rename them. Nothing here
//! deletes for good but [`crate::trash`]'s own emptying; a delete is a
//! move into the trash.
//!
//! A copy or a move never overwrites: a name already taken in the folder
//! it goes to becomes `name 2.ext`, `name 3.ext` ... as the design's
//! `uniq` does. A folder is never copied or moved into itself.

use std::io;
use std::path::{Path, PathBuf};

/// The extension of `name` as a person sees it: `.png`, `.tar.gz`; empty
/// for a name with none, or one that only starts with a dot.
#[must_use]
pub fn extension(name: &str) -> &str {
    let lower = name.to_lowercase();
    for double in [".tar.gz", ".tar.xz", ".tar.zst", ".tar.bz2"] {
        if lower.ends_with(double) && name.len() > double.len() {
            return name.get(name.len() - double.len()..).unwrap_or("");
        }
    }
    match name.rfind('.') {
        Some(0) | None => "",
        Some(at) => name.get(at..).unwrap_or(""),
    }
}

/// A name in `dir` like `name` that nothing there has yet.
#[must_use]
pub fn unique_name(dir: &Path, name: &str) -> String {
    if dir.join(name).symlink_metadata().is_err() {
        return name.to_owned();
    }
    let ext = extension(name);
    let base = name.get(..name.len() - ext.len()).unwrap_or(name);
    (2..)
        .map(|n| format!("{base} {n}{ext}"))
        .find(|candidate| dir.join(candidate).symlink_metadata().is_err())
        .unwrap_or_else(|| name.to_owned())
}

/// Why a new name cannot be given, if it cannot: empty, a `/` in it, `.`
/// or `..`, or taken in `dir` by something other than `keep`.
#[must_use]
pub fn name_problem(dir: &Path, name: &str, keep: Option<&str>) -> Option<&'static str> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Some("a name is needed");
    }
    if trimmed.contains('/') || trimmed.contains('\0') {
        return Some("a name cannot hold /");
    }
    if trimmed == "." || trimmed == ".." {
        return Some("that name is the folder's own");
    }
    if Some(trimmed) != keep && dir.join(trimmed).symlink_metadata().is_ok() {
        return Some("that name is taken here");
    }
    None
}

/// Whether `inside` is `dir` or under it.
#[must_use]
pub fn is_within(inside: &Path, dir: &Path) -> bool {
    inside.starts_with(dir)
}

/// Copy `from` (a file, a link, or a folder with all it holds) to `to`,
/// which must not be there. Links are copied as links.
///
/// # Errors
///
/// The first file that could not be read or written; what was copied
/// before it stays.
pub fn copy_tree(from: &Path, to: &Path) -> io::Result<u64> {
    let meta = from.symlink_metadata()?;
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(from)?;
        std::os::unix::fs::symlink(target, to)?;
        return Ok(0);
    }
    if meta.is_dir() {
        std::fs::create_dir(to)?;
        let mut bytes = 0;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            bytes += copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        std::fs::set_permissions(to, meta.permissions())?;
        return Ok(bytes);
    }
    let bytes = std::fs::copy(from, to)?;
    if let Ok(modified) = meta.modified()
        && let Ok(file) = std::fs::File::options().write(true).open(to)
    {
        let _ = file.set_modified(modified);
    }
    Ok(bytes)
}

/// Copy `from` into the folder `dir`, under its own name or the next free
/// one; the path it got.
///
/// # Errors
///
/// A folder into itself, or what [`copy_tree`] met.
pub fn copy_into(from: &Path, dir: &Path) -> io::Result<PathBuf> {
    if is_within(dir, from) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a folder cannot go into itself",
        ));
    }
    let name = file_name(from)?;
    let to = dir.join(unique_name(dir, &name));
    let _ = copy_tree(from, &to)?;
    Ok(to)
}

/// Move `from` into the folder `dir`, under its own name or the next free
/// one: a rename on the same file system, else a copy and then the
/// original removed once the copy is whole. The path it got.
///
/// # Errors
///
/// A folder into itself, or a copy that failed (the original then stays
/// whole, and the part copied is removed).
pub fn move_into(from: &Path, dir: &Path) -> io::Result<PathBuf> {
    if is_within(dir, from) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a folder cannot go into itself",
        ));
    }
    if from.parent() == Some(dir) {
        return Ok(from.to_path_buf());
    }
    let name = file_name(from)?;
    let to = dir.join(unique_name(dir, &name));
    move_to(from, &to)?;
    Ok(to)
}

/// Move `from` to `to`, which must not be there.
///
/// # Errors
///
/// As [`move_into`].
pub fn move_to(from: &Path, to: &Path) -> io::Result<()> {
    if to.symlink_metadata().is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "that name is taken",
        ));
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        // Another file system: copy, then take the original away.
        Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {
            if let Err(error) = copy_tree(from, to) {
                let _ = remove_tree(to);
                return Err(error);
            }
            remove_tree(from)
        }
        Err(error) => Err(error),
    }
}

/// Give `path` the name `name` in its own folder; the new path.
///
/// # Errors
///
/// A name [`name_problem`] refuses, or the rename's own error.
pub fn rename(path: &Path, name: &str) -> io::Result<PathBuf> {
    let dir = path.parent().unwrap_or(Path::new("/"));
    let old = file_name(path)?;
    if let Some(why) = name_problem(dir, name, Some(&old)) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, why));
    }
    let to = dir.join(name.trim());
    if to != path {
        std::fs::rename(path, &to)?;
    }
    Ok(to)
}

/// Make the folder `name` in `dir`; its path.
///
/// # Errors
///
/// A refused name, or the file system's error.
pub fn new_folder(dir: &Path, name: &str) -> io::Result<PathBuf> {
    if let Some(why) = name_problem(dir, name, None) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, why));
    }
    let to = dir.join(name.trim());
    std::fs::create_dir(&to)?;
    Ok(to)
}

/// Remove `path` and all it holds, for good: only the trash's own
/// emptying and a move's original after its copy come here.
///
/// # Errors
///
/// The first that could not be removed.
pub(crate) fn remove_tree(path: &Path) -> io::Result<()> {
    let meta = path.symlink_metadata()?;
    if meta.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

fn file_name(path: &Path) -> io::Result<String> {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no name"))
}

/// How a bulk rename names each of its files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameRule {
    /// Every `find` in the name replaced by `with`.
    Replace {
        /// What is looked for.
        find: String,
        /// What it becomes.
        with: String,
    },
    /// `base-01.ext`, `base-02.ext` ..., counting from `start`.
    Format {
        /// The name before the number.
        base: String,
        /// The first number.
        start: u32,
    },
}

/// One file of a bulk rename: its path, the name it would get, whether
/// that changes anything, and why it cannot, if it cannot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Planned {
    /// The file.
    pub path: PathBuf,
    /// Its name now.
    pub old: String,
    /// The name it would get.
    pub new: String,
    /// Why not, or `None`.
    pub problem: Option<&'static str>,
}

/// What `rule` would do to `paths`, each checked against its folder and
/// against the others' new names.
#[must_use]
pub fn plan_rename(paths: &[PathBuf], rule: &RenameRule) -> Vec<Planned> {
    let mut out: Vec<Planned> = paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let old = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let new = match rule {
                RenameRule::Replace { find, with } if !find.is_empty() => {
                    old.replace(find.as_str(), with)
                }
                RenameRule::Replace { .. } => old.clone(),
                RenameRule::Format { base, start } => {
                    let ext = if path.is_dir() { "" } else { extension(&old) };
                    let base = if base.trim().is_empty() {
                        "file"
                    } else {
                        base.trim()
                    };
                    let n = u64::from(*start) + i as u64;
                    format!("{base}-{n:02}{ext}")
                }
            };
            Planned {
                path: path.clone(),
                old,
                new: new.trim().to_owned(),
                problem: None,
            }
        })
        .collect();
    let renamed: Vec<(PathBuf, String)> = out
        .iter()
        .map(|p| {
            (
                p.path.parent().map(Path::to_path_buf).unwrap_or_default(),
                p.new.clone(),
            )
        })
        .collect();
    let olds: Vec<(PathBuf, String)> = out
        .iter()
        .map(|p| {
            (
                p.path.parent().map(Path::to_path_buf).unwrap_or_default(),
                p.old.clone(),
            )
        })
        .collect();
    for (i, planned) in out.iter_mut().enumerate() {
        let dir = renamed.get(i).map(|(d, _)| d.clone()).unwrap_or_default();
        if planned.new == planned.old {
            continue;
        }
        let twice = renamed
            .iter()
            .enumerate()
            .any(|(j, (d, n))| j != i && *d == dir && *n == planned.new);
        // Taken by a file that is not one of those being renamed.
        let taken_by_other = dir.join(&planned.new).symlink_metadata().is_ok()
            && !olds.iter().any(|(d, n)| *d == dir && *n == planned.new);
        planned.problem = if planned.new.is_empty() {
            Some("a name is needed")
        } else if planned.new.contains('/') {
            Some("a name cannot hold /")
        } else if twice {
            Some("two files would get this name")
        } else if taken_by_other {
            Some("that name is taken here")
        } else {
            None
        };
    }
    out
}

/// Carry out a plan [`plan_rename`] made with no problems: through
/// temporary names first, so that `a`→`b` and `b`→`a` both happen. The
/// new paths.
///
/// # Errors
///
/// A plan with a problem, or the first rename that failed.
pub fn apply_rename(plan: &[Planned]) -> io::Result<Vec<PathBuf>> {
    if let Some(bad) = plan.iter().find(|p| p.problem.is_some()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{}: {}", bad.old, bad.problem.unwrap_or("")),
        ));
    }
    let changing: Vec<&Planned> = plan.iter().filter(|p| p.new != p.old).collect();
    let mut steps = Vec::new();
    for (i, p) in changing.iter().enumerate() {
        let dir = p.path.parent().unwrap_or(Path::new("/"));
        let temp = dir.join(format!(".shelf-rename-{}-{i}", std::process::id()));
        std::fs::rename(&p.path, &temp)?;
        steps.push((temp, dir.join(&p.new)));
    }
    for (temp, to) in &steps {
        std::fs::rename(temp, to)?;
    }
    Ok(plan
        .iter()
        .map(|p| p.path.parent().unwrap_or(Path::new("/")).join(&p.new))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("shelf-fsops-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn names_taken_get_the_next_number() {
        let dir = scratch("names");
        let _ = std::fs::write(dir.join("photo.png"), b"x");
        let _ = std::fs::write(dir.join("photo 2.png"), b"x");
        let _ = std::fs::write(dir.join("a.tar.gz"), b"x");
        assert_eq!(unique_name(&dir, "photo.png"), "photo 3.png");
        assert_eq!(unique_name(&dir, "a.tar.gz"), "a 2.tar.gz");
        assert_eq!(unique_name(&dir, "new.txt"), "new.txt");
        assert_eq!(extension(".bashrc"), "");
        assert!(name_problem(&dir, "a/b", None).is_some());
        assert!(name_problem(&dir, "photo.png", None).is_some());
        assert!(name_problem(&dir, "photo.png", Some("photo.png")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copies_and_moves_keep_what_is_there_and_never_go_into_themselves() {
        let dir = scratch("copy");
        let (left, right) = (dir.join("left"), dir.join("right"));
        let _ = std::fs::create_dir_all(left.join("tree/inner"));
        let _ = std::fs::create_dir_all(&right);
        let _ = std::fs::write(left.join("tree/inner/file.txt"), b"hello");
        let _ = std::fs::write(right.join("tree"), b"in the way");
        let copied = copy_into(&left.join("tree"), &right).expect("copied");
        assert_eq!(copied, right.join("tree 2"));
        assert_eq!(
            std::fs::read(right.join("tree 2/inner/file.txt")).expect("read"),
            b"hello"
        );
        assert!(copy_into(&left, &left.join("tree")).is_err(), "into itself");
        let moved = move_into(&left.join("tree"), &right).expect("moved");
        assert_eq!(moved, right.join("tree 3"));
        assert!(!left.join("tree").exists());
        let renamed = rename(&moved, "kept").expect("renamed");
        assert_eq!(renamed, right.join("kept"));
        assert!(rename(&renamed, "tree").is_err(), "taken");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_bulk_rename_is_planned_checked_and_swaps_names() {
        let dir = scratch("bulk");
        for name in ["a.jpg", "b.jpg", "c.png"] {
            let _ = std::fs::write(dir.join(name), name);
        }
        let paths: Vec<PathBuf> = ["a.jpg", "b.jpg", "c.png"]
            .iter()
            .map(|n| dir.join(n))
            .collect();
        let plan = plan_rename(
            &paths,
            &RenameRule::Format {
                base: "photo".into(),
                start: 1,
            },
        );
        let names: Vec<&str> = plan.iter().map(|p| p.new.as_str()).collect();
        assert_eq!(names, ["photo-01.jpg", "photo-02.jpg", "photo-03.png"]);
        assert!(plan.iter().all(|p| p.problem.is_none()));
        let clash = plan_rename(
            &paths[..2],
            &RenameRule::Replace {
                find: "a".into(),
                with: "b".into(),
            },
        );
        assert_eq!(
            clash.first().and_then(|p| p.problem),
            Some("two files would get this name"),
            "b.jpg stays b.jpg"
        );
        let swap = vec![
            Planned {
                path: dir.join("a.jpg"),
                old: "a.jpg".into(),
                new: "b.jpg".into(),
                problem: None,
            },
            Planned {
                path: dir.join("b.jpg"),
                old: "b.jpg".into(),
                new: "a.jpg".into(),
                problem: None,
            },
        ];
        let _ = apply_rename(&swap).expect("swapped");
        assert_eq!(std::fs::read(dir.join("a.jpg")).expect("read"), b"b.jpg");
        let twice = plan_rename(
            &paths[..2],
            &RenameRule::Replace {
                find: ".jpg".into(),
                with: "".into(),
            },
        );
        assert!(twice.iter().all(|p| p.problem.is_none()));
        let same = plan_rename(
            &paths[..2],
            &RenameRule::Format {
                base: "x".into(),
                start: 1,
            },
        );
        assert!(same.iter().all(|p| p.problem.is_none()));
        let collide = plan_rename(
            &paths[..2],
            &RenameRule::Replace {
                find: "a".into(),
                with: "c".into(),
            },
        );
        assert!(collide.iter().all(|p| p.problem.is_none()), "c.jpg is free");
        let _ = std::fs::write(dir.join("z.jpg"), b"z");
        let taken = plan_rename(
            &paths[..1],
            &RenameRule::Replace {
                find: "a".into(),
                with: "z".into(),
            },
        );
        assert_eq!(
            taken.first().and_then(|p| p.problem),
            Some("that name is taken here")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
