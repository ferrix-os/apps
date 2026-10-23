//! Archives, read in place: a zip, a tar, a gzipped tar (`.tar.gz`,
//! `.tgz`) is listed as a folder a pane walks into, read only, and can be
//! extracted beside itself into a new folder. Other kinds (`.7z`, `.rar`,
//! `.xz`, `.zst`) are named as archives but not read.
//!
//! The readers are small and take nothing on trust: a name with `..` or
//! a leading `/` is refused on extraction, sizes are checked against the
//! file, and a gzip is inflated only up to a limit.

use std::path::{Component, Path, PathBuf};

/// The most a gzipped tar is inflated to, to be read in place.
const INFLATE_LIMIT: usize = 512 << 20;

/// What kind of archive a name says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// `.zip`.
    Zip,
    /// `.tar`.
    Tar,
    /// `.tar.gz`, `.tgz`.
    TarGz,
    /// Named an archive, and not read here.
    Other(&'static str),
}

/// The format a file's name says, if it is an archive.
#[must_use]
pub fn format_of(name: &str) -> Option<Format> {
    let lower = name.to_lowercase();
    let ends = |ext: &str| lower.ends_with(ext) && lower.len() > ext.len();
    Some(if ends(".zip") {
        Format::Zip
    } else if ends(".tar.gz") || ends(".tgz") {
        Format::TarGz
    } else if ends(".tar") {
        Format::Tar
    } else if ends(".7z") {
        Format::Other("7z")
    } else if ends(".rar") {
        Format::Other("RAR")
    } else if ends(".tar.xz") || ends(".txz") || ends(".xz") {
        Format::Other("xz")
    } else if ends(".tar.zst") || ends(".zst") {
        Format::Other("Zstandard")
    } else if ends(".tar.bz2") || ends(".bz2") {
        Format::Other("bzip2")
    } else {
        return None;
    })
}

/// One entry of an archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Its path inside, `/`-separated, no leading or trailing `/`.
    pub path: String,
    /// Whether it is a folder.
    pub is_dir: bool,
    /// Its size unpacked; a folder's, all it holds.
    pub size: u64,
    /// When it changed, seconds since 1970.
    pub mtime: i64,
}

impl Entry {
    /// Its last name.
    #[must_use]
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

/// An archive's entries, and where each one's data starts.
#[derive(Clone, Debug, Default)]
pub struct Index {
    /// Every entry, folders made for the paths that name none.
    pub entries: Vec<Entry>,
    /// The bytes all the files hold unpacked.
    pub unpacked: u64,
}

impl Index {
    /// The entries directly in `dir` (`""` for the top), folders with the
    /// sum of what they hold.
    #[must_use]
    pub fn children(&self, dir: &str) -> Vec<Entry> {
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let mut out: Vec<Entry> = Vec::new();
        for entry in &self.entries {
            let Some(rest) = entry.path.strip_prefix(&prefix) else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            let (first, deeper) = match rest.split_once('/') {
                Some((first, _)) => (first, true),
                None => (rest, false),
            };
            let path = format!("{prefix}{first}");
            let size = if entry.is_dir { 0 } else { entry.size };
            match out.iter_mut().find(|e| e.path == path) {
                Some(seen) => {
                    seen.size += size;
                    seen.mtime = seen.mtime.max(entry.mtime);
                    seen.is_dir |= deeper;
                }
                None => out.push(Entry {
                    path,
                    is_dir: entry.is_dir || deeper,
                    size,
                    mtime: entry.mtime,
                }),
            }
        }
        out
    }

    /// The entry at `path`, a folder made up if only paths under it are
    /// named.
    #[must_use]
    pub fn entry(&self, path: &str) -> Option<Entry> {
        let (dir, _) = path.rsplit_once('/').unwrap_or(("", path));
        self.children(dir).into_iter().find(|e| e.path == path)
    }
}

fn le16(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from(u16::from_le_bytes(
        b.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn le32(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from(u32::from_le_bytes(
        b.get(at..at + 4)?.try_into().ok()?,
    )))
}

fn to_usize(v: u64) -> Option<usize> {
    usize::try_from(v).ok()
}

/// A DOS date and time as seconds since 1970 (taken as UTC).
fn dos_time(date: u64, time: u64) -> i64 {
    let year = 1980 + i64::try_from((date >> 9) & 0x7f).unwrap_or(0);
    let month = ((date >> 5) & 0xf).clamp(1, 12);
    let day = (date & 0x1f).max(1);
    // Days from civil (Hinnant).
    let (y, m) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * i64::try_from(m).unwrap_or(0) + 2) / 5 + i64::try_from(day).unwrap_or(1) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs =
        i64::try_from(((time >> 11) & 0x1f) * 3600 + ((time >> 5) & 0x3f) * 60 + (time & 0x1f) * 2)
            .unwrap_or(0);
    days * 86_400 + secs
}

/// A zip's central directory: entries, and for each its local header's
/// offset, method and packed size.
fn zip_entries(bytes: &[u8]) -> Result<Vec<(Entry, u64, u64, u64)>, String> {
    let bad = || "not a zip this reads (no central directory)".to_owned();
    let floor = bytes.len().saturating_sub(65_557);
    let end = (floor..bytes.len().saturating_sub(21))
        .rev()
        .find(|&at| bytes.get(at..at + 4) == Some(&[0x50, 0x4b, 0x05, 0x06]))
        .ok_or_else(bad)?;
    let count = le16(bytes, end + 10).ok_or_else(bad)?;
    let offset = le32(bytes, end + 16).ok_or_else(bad)?;
    if offset == 0xFFFF_FFFF || count == 0xFFFF {
        return Err("a zip64 archive, which is not read here".to_owned());
    }
    let mut at = to_usize(offset).ok_or_else(bad)?;
    let mut out = Vec::new();
    for _ in 0..count {
        if bytes.get(at..at + 4) != Some(&[0x50, 0x4b, 0x01, 0x02]) {
            return Err(bad());
        }
        let field = |o| le16(bytes, at + o).ok_or_else(bad);
        let word = |o| le32(bytes, at + o).ok_or_else(bad);
        let method = field(10)?;
        let (time, date) = (field(12)?, field(14)?);
        let packed = word(20)?;
        let size = word(24)?;
        let (name_len, extra_len, comment_len) = (field(28)?, field(30)?, field(32)?);
        let local = word(42)?;
        let name_at = at + 46;
        let name = bytes
            .get(name_at..name_at + to_usize(name_len).ok_or_else(bad)?)
            .ok_or_else(bad)?;
        let name = String::from_utf8_lossy(name).into_owned();
        let is_dir = name.ends_with('/');
        let path = name.trim_matches('/').to_owned();
        if !path.is_empty() {
            out.push((
                Entry {
                    path,
                    is_dir,
                    size: if is_dir { 0 } else { size },
                    mtime: dos_time(date, time),
                },
                local,
                method,
                packed,
            ));
        }
        at = name_at + to_usize(name_len + extra_len + comment_len).ok_or_else(bad)?;
    }
    Ok(out)
}

/// A tar's entries, and where each file's data is.
fn tar_entries(bytes: &[u8]) -> Result<Vec<(Entry, usize)>, String> {
    let octal = |field: &[u8]| -> u64 {
        let text = String::from_utf8_lossy(field);
        u64::from_str_radix(text.trim_matches(|c: char| c == '\0' || c == ' '), 8).unwrap_or(0)
    };
    let text = |field: &[u8]| -> String {
        let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
        String::from_utf8_lossy(field.get(..end).unwrap_or(field)).into_owned()
    };
    let mut out = Vec::new();
    let mut at = 0;
    let mut long_name: Option<String> = None;
    while let Some(header) = bytes.get(at..at + 512) {
        if header.iter().all(|&b| b == 0) {
            break;
        }
        let size = octal(header.get(124..136).unwrap_or_default());
        let mtime = i64::try_from(octal(header.get(136..148).unwrap_or_default())).unwrap_or(0);
        let kind = header.get(156).copied().unwrap_or(b'0');
        let data = at + 512;
        let len = to_usize(size).ok_or("a tar entry too large")?;
        if data.checked_add(len).is_none_or(|end| end > bytes.len()) {
            return Err("a tar cut short".to_owned());
        }
        let name = match long_name.take() {
            Some(name) => name,
            None => {
                let name = text(header.get(0..100).unwrap_or_default());
                let prefix = if header.get(257..262) == Some(b"ustar") {
                    text(header.get(345..500).unwrap_or_default())
                } else {
                    String::new()
                };
                if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                }
            }
        };
        match kind {
            b'L' => long_name = Some(text(bytes.get(data..data + len).unwrap_or_default())),
            b'x' => {
                // A pax header: its `path=` names the next entry.
                let records =
                    String::from_utf8_lossy(bytes.get(data..data + len).unwrap_or_default())
                        .into_owned();
                long_name = records
                    .lines()
                    .find_map(|line| line.split_once(" path=").map(|(_, p)| p.to_owned()));
            }
            b'0' | 0 | b'5' | b'2' | b'1' | b'7' => {
                let is_dir = kind == b'5' || name.ends_with('/');
                let path = name.trim_start_matches("./").trim_matches('/').to_owned();
                if !path.is_empty() && path != "." {
                    out.push((
                        Entry {
                            path,
                            is_dir,
                            size: if is_dir || kind == b'2' || kind == b'1' {
                                0
                            } else {
                                size
                            },
                            mtime,
                        },
                        if kind == b'0' || kind == 0 || kind == b'7' {
                            data
                        } else {
                            usize::MAX
                        },
                    ));
                }
            }
            _ => {}
        }
        at = data + len.div_ceil(512) * 512;
    }
    Ok(out)
}

/// A gzip file's contents, inflated up to [`INFLATE_LIMIT`].
fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.get(..3) != Some(&[0x1f, 0x8b, 8]) {
        return Err("not a gzip file".to_owned());
    }
    let flags = bytes.get(3).copied().unwrap_or(0);
    let mut at = 10;
    if flags & 4 != 0 {
        at += 2 + to_usize(le16(bytes, at).unwrap_or(0)).unwrap_or(0);
    }
    for bit in [8, 16] {
        if flags & bit != 0 {
            at += bytes
                .get(at..)
                .and_then(|rest| rest.iter().position(|&b| b == 0))
                .unwrap_or(0)
                + 1;
        }
    }
    if flags & 2 != 0 {
        at += 2;
    }
    let body = bytes.get(at..).ok_or("a gzip cut short")?;
    miniz_oxide::inflate::decompress_to_vec_with_limit(body, INFLATE_LIMIT)
        .map_err(|_| "a gzip that does not inflate, or larger than 512 MB unpacked".to_owned())
}

/// Read the archive at `path`.
///
/// # Errors
///
/// A kind not read here, or a file that is not what its name says.
pub fn read(path: &Path) -> Result<Index, String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let format = format_of(&name).ok_or("not an archive")?;
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut entries: Vec<Entry> = match format {
        Format::Zip => zip_entries(&bytes)?.into_iter().map(|(e, ..)| e).collect(),
        Format::Tar => tar_entries(&bytes)?.into_iter().map(|(e, _)| e).collect(),
        Format::TarGz => tar_entries(&gunzip(&bytes)?)?
            .into_iter()
            .map(|(e, _)| e)
            .collect(),
        Format::Other(kind) => return Err(format!("a {kind} archive, which shelf does not read")),
    };
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    entries.dedup_by(|a, b| a.path == b.path);
    let unpacked = entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
    Ok(Index { entries, unpacked })
}

/// `inner` as a path under `dest`, or `None` when it would leave it.
fn safe_join(dest: &Path, inner: &str) -> Option<PathBuf> {
    let rel = Path::new(inner);
    let mut out = dest.to_path_buf();
    for part in rel.components() {
        match part {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (out != dest).then_some(out)
}

/// Unpack the archive at `path` into the new folder `dest`, which must not
/// be there; how many files. Links are left out, and a name that would
/// leave `dest` is refused.
///
/// # Errors
///
/// What [`read`] says, a name that leaves the folder, or the file
/// system's error.
pub fn extract(path: &Path, dest: &Path) -> Result<usize, String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let format = format_of(&name).ok_or("not an archive")?;
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut files: Vec<(Entry, Vec<u8>)> = Vec::new();
    match format {
        Format::Zip => {
            for (entry, local, method, packed) in zip_entries(&bytes)? {
                if entry.is_dir {
                    files.push((entry, Vec::new()));
                    continue;
                }
                let at = to_usize(local).ok_or("a bad offset")?;
                if bytes.get(at..at + 4) != Some(&[0x50, 0x4b, 0x03, 0x04]) {
                    return Err(format!("{}: no local header", entry.path));
                }
                let skip = le16(&bytes, at + 26).unwrap_or(0) + le16(&bytes, at + 28).unwrap_or(0);
                let start = at + 30 + to_usize(skip).ok_or("a bad header")?;
                let data = bytes
                    .get(start..start + to_usize(packed).ok_or("too large")?)
                    .ok_or_else(|| format!("{}: cut short", entry.path))?;
                let out = match method {
                    0 => data.to_vec(),
                    8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, INFLATE_LIMIT)
                        .map_err(|_| format!("{}: does not inflate", entry.path))?,
                    other => {
                        return Err(format!(
                            "{}: packed with method {other}, not read here",
                            entry.path
                        ));
                    }
                };
                files.push((entry, out));
            }
        }
        Format::Tar | Format::TarGz => {
            let tar = if format == Format::TarGz {
                gunzip(&bytes)?
            } else {
                bytes
            };
            for (entry, data) in tar_entries(&tar)? {
                let body = if data == usize::MAX {
                    if !entry.is_dir {
                        continue;
                    }
                    Vec::new()
                } else {
                    tar.get(data..data + to_usize(entry.size).unwrap_or(0))
                        .unwrap_or_default()
                        .to_vec()
                };
                files.push((entry, body));
            }
        }
        Format::Other(kind) => return Err(format!("a {kind} archive, which shelf does not read")),
    }
    // Every name is checked before anything is written.
    let mut placed = Vec::new();
    for (entry, body) in files {
        let to = safe_join(dest, &entry.path)
            .ok_or_else(|| format!("{}: leaves the folder", entry.path))?;
        placed.push((entry, to, body));
    }
    std::fs::create_dir(dest).map_err(|e| e.to_string())?;
    let mut count = 0;
    for (entry, to, body) in placed {
        if entry.is_dir {
            std::fs::create_dir_all(&to).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut file = std::fs::File::options()
            .write(true)
            .create_new(true)
            .open(&to)
            .map_err(|e| format!("{}: {e}", entry.path))?;
        use std::io::Write as _;
        file.write_all(&body).map_err(|e| e.to_string())?;
        if let Ok(secs) = u64::try_from(entry.mtime) {
            let _ = file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs));
        }
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::{tar_of, zip_of};

    #[test]
    fn zips_and_tars_are_listed_as_folders_and_extracted_safely() {
        let dir = std::env::temp_dir().join(format!("shelf-archive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let files: [(&str, &[u8]); 3] = [
            ("README.txt", b"read me"),
            (
                "keys/id_old",
                b"old key, long enough to deflate well well well",
            ),
            ("keys/id_old.pub", b"pub"),
        ];
        let zip = dir.join("backup.zip");
        let _ = std::fs::write(&zip, zip_of(&files));
        let index = read(&zip).expect("read");
        let top = index.children("");
        assert_eq!(
            top.iter()
                .map(|e| (e.path.as_str(), e.is_dir))
                .collect::<Vec<_>>(),
            [("README.txt", false), ("keys", true)]
        );
        assert_eq!(
            top.iter().find(|e| e.is_dir).map(|e| e.size),
            Some((files[1].1.len() + files[2].1.len()) as u64)
        );
        assert_eq!(index.children("keys").len(), 2);
        assert_eq!(extract(&zip, &dir.join("out")).expect("extracted"), 3);
        assert_eq!(
            std::fs::read(dir.join("out/keys/id_old")).expect("read"),
            files[1].1
        );
        let tgz = dir.join("assets.tar.gz");
        let tar = tar_of(&files);
        let mut gz = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3];
        gz.extend(miniz_oxide::deflate::compress_to_vec(&tar, 6));
        gz.extend_from_slice(&[0; 8]);
        let _ = std::fs::write(&tgz, gz);
        let index = read(&tgz).expect("read");
        assert_eq!(index.children("keys").len(), 2);
        assert_eq!(
            index.unpacked,
            files.iter().map(|f| f.1.len() as u64).sum::<u64>()
        );
        assert_eq!(extract(&tgz, &dir.join("out2")).expect("extracted"), 3);
        // A name that climbs out is refused before anything is written.
        let evil = dir.join("evil.tar");
        let _ = std::fs::write(&evil, tar_of(&[("../escape.txt", b"no")]));
        assert!(extract(&evil, &dir.join("out3")).is_err());
        assert!(!dir.join("escape.txt").exists());
        assert!(!dir.join("out3").exists());
        assert_eq!(format_of("x.7z"), Some(Format::Other("7z")));
        assert!(read(&dir.join("x.7z")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
