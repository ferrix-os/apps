//! lode's store, read as `lode` reads it (`docs/LOGGING.md` §8, §9.1): the
//! segment files of `/var/log/journal` and `/run/log/journal`, each a
//! header and checksummed entries, with a sealed segment's summary beside
//! it. The viewer runs as the person who started it, with that person's
//! rights to the files (§12): a directory it may not read is said, never
//! worked around.
//!
//! Following -- the Live switch -- is what `lode -f` does: every so often
//! the newest segments of each class are read again from where the last
//! read stopped, and what is past the last sequence number held is new.

use std::fs;
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ferrix_lode_core::record::Record;
use ferrix_lode_core::segment::{self, Header};
use ferrix_lode_core::summary::Summary;

/// The persistent store.
pub const PERSISTENT: &str = "/var/log/journal";
/// The volatile store.
pub const VOLATILE: &str = "/run/log/journal";

/// What a look at the store found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// Whether lode's control socket is there: the service is running.
    pub running: bool,
    /// Bytes the segments take.
    pub used: u64,
    /// The tier the records are on: `disk` or `RAM`.
    pub tier: &'static str,
    /// Segments found.
    pub segments: usize,
    /// Directories that could not be read, and why.
    pub refused: Vec<String>,
}

/// One segment file.
#[derive(Clone, Debug)]
struct Segment {
    /// Its path.
    path: PathBuf,
    /// Its name's class.
    class: String,
    /// The first sequence number its name says.
    first: u64,
    /// Its header.
    header: Header,
    /// Its summary, if sealed.
    summary: Option<Summary>,
    /// Its size.
    size: u64,
}

/// A store: the directories it is read from, and where following stopped
/// in each segment.
#[derive(Debug)]
pub struct Store {
    /// The directories.
    dirs: Vec<PathBuf>,
    /// Each followed segment's path and the offset read to.
    places: Vec<(PathBuf, usize)>,
}

impl Store {
    /// The system's store: the persistent and the volatile directories, and
    /// the user's own directory in each (`user-<uid>`, §12.1).
    #[must_use]
    pub fn system() -> Self {
        Self::at(&[PathBuf::from(PERSISTENT), PathBuf::from(VOLATILE)])
    }

    /// A store in `dirs` (`lode -D`).
    #[must_use]
    pub fn at(dirs: &[PathBuf]) -> Self {
        Store {
            dirs: dirs.to_vec(),
            places: Vec::new(),
        }
    }

    /// Every segment that reads, oldest first, and the directories that
    /// could not be read.
    fn segments(&self) -> (Vec<Segment>, Vec<String>) {
        let mut found = Vec::new();
        let mut refused = Vec::new();
        let mut dirs: Vec<PathBuf> = Vec::new();
        for dir in &self.dirs {
            dirs.push(dir.clone());
            dirs.extend(user_dirs(dir));
        }
        for dir in dirs {
            let listing = match fs::read_dir(&dir) {
                Ok(listing) => listing,
                Err(error) if error.kind() == ErrorKind::PermissionDenied => {
                    refused.push(format!(
                        "{}: permission denied (root and members of adm read the log)",
                        dir.display()
                    ));
                    continue;
                }
                Err(_) => continue,
            };
            let mut denied = false;
            for item in listing.flatten() {
                match read_segment(&dir, &item) {
                    Ok(Some(segment)) => found.push(segment),
                    Ok(None) => {}
                    Err(()) => denied = true,
                }
            }
            if denied {
                refused.push(format!(
                    "{}: permission denied to its segments",
                    dir.display()
                ));
            }
        }
        found.sort_by_key(|each| each.first);
        (found, refused)
    }

    /// What is in the store and whether lode runs.
    #[must_use]
    pub fn status(&self) -> Status {
        let (segments, refused) = self.segments();
        let on_disk = segments
            .iter()
            .any(|each| each.path.starts_with(PERSISTENT));
        Status {
            running: Path::new("/run/lode/control").exists(),
            used: segments.iter().map(|each| each.size).sum(),
            tier: if on_disk || segments.is_empty() {
                "disk"
            } else {
                "RAM"
            },
            segments: segments.len(),
            refused,
        }
    }

    /// Every record of the boots kept, and the directories refused. Every
    /// segment is read whole -- the viewer shows a boot's every record --
    /// and following starts at each one's end.
    pub fn load(&mut self) -> (Vec<Record>, Vec<String>) {
        let (segments, refused) = self.segments();
        let mut records = Vec::new();
        self.places.clear();
        for each in &segments {
            let Ok(bytes) = fs::read(&each.path) else {
                continue;
            };
            let Some(scanned) = segment::scan(&bytes, 0) else {
                continue;
            };
            let host: Rc<str> = Rc::from(each.header.host.as_str());
            records.extend(scanned.records.into_iter().map(|found| {
                let mut record = found.record;
                record.boot_id = each.header.boot_id;
                record.host = Rc::clone(&host);
                record
            }));
            self.places.push((each.path.clone(), bytes.len()));
        }
        records.sort_by_key(|record| record.seqnum);
        (records, refused)
    }

    /// The records written since the last [`Store::load`] or follow: the
    /// two newest segments of each class read on from where the last read
    /// stopped, as `lode -f` reads them. Only records past `last` are new.
    pub fn follow(&mut self, last: u64) -> Vec<Record> {
        let (segments, _) = self.segments();
        let mut fresh = Vec::new();
        let mut seen: Vec<(String, usize)> = Vec::new();
        for each in segments.iter().rev() {
            let count = match seen.iter_mut().find(|(class, _)| *class == each.class) {
                Some((_, count)) => {
                    *count += 1;
                    *count
                }
                None => {
                    seen.push((each.class.clone(), 1));
                    1
                }
            };
            if count > 2 || each.summary.as_ref().is_some_and(|sum| sum.seqs.1 <= last) {
                continue;
            }
            let from = self
                .places
                .iter()
                .find(|(path, _)| *path == each.path)
                .map_or_else(|| each.header.encode().len(), |(_, offset)| *offset);
            let Some(bytes) = read_from(&each.path, from) else {
                continue;
            };
            let (records, used) = segment::entries(&bytes);
            let host: Rc<str> = Rc::from(each.header.host.as_str());
            fresh.extend(records.into_iter().map(|mut record| {
                record.boot_id = each.header.boot_id;
                record.host = Rc::clone(&host);
                record
            }));
            self.places.retain(|(path, _)| *path != each.path);
            self.places.push((each.path.clone(), from + used));
        }
        fresh.retain(|record| record.seqnum > last);
        fresh.sort_by_key(|record| record.seqnum);
        fresh
    }
}

/// A user's own directories in `dir`: `user-<uid>` (`docs/LOGGING.md`
/// §12.1).
fn user_dirs(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|listing| {
            listing
                .flatten()
                .filter(|item| {
                    item.file_name().to_string_lossy().starts_with("user-") && item.path().is_dir()
                })
                .map(|item| item.path())
                .collect()
        })
        .unwrap_or_default()
}

/// The segment `item` of `dir` is, `None` when it is no segment, or an
/// error when it is one this person may not read.
fn read_segment(dir: &Path, item: &fs::DirEntry) -> Result<Option<Segment>, ()> {
    let name = item.file_name().to_string_lossy().into_owned();
    if !segment::is_segment_name(&name) {
        return Ok(None);
    }
    let Some(first) = segment::seqnum_of_name(&name) else {
        return Ok(None);
    };
    let path = item.path();
    let Some(header) = header(&path) else {
        let denied =
            fs::File::open(&path).is_err_and(|error| error.kind() == ErrorKind::PermissionDenied);
        return if denied { Err(()) } else { Ok(None) };
    };
    let summary = fs::read(dir.join(format!("{name}.sum")))
        .ok()
        .and_then(|bytes| Summary::decode(&bytes));
    Ok(Some(Segment {
        class: segment::class_of_name(&name).unwrap_or_default().to_owned(),
        first,
        header,
        summary,
        size: item.metadata().map_or(0, |meta| meta.len()),
        path,
    }))
}

/// A segment's header, from its first bytes.
fn header(path: &Path) -> Option<Header> {
    let mut file = fs::File::open(path).ok()?;
    let mut buf = [0_u8; 512];
    let got = file.read(&mut buf).ok()?;
    Header::decode(buf.get(..got)?).map(|(header, _)| header)
}

/// A file's bytes from `offset` on.
fn read_from(path: &Path, offset: usize) -> Option<Vec<u8>> {
    let mut file = fs::File::open(path).ok()?;
    let _ = file.seek(SeekFrom::Start(offset as u64)).ok()?;
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::Store;
    use ferrix_lode_core::record::Record;
    use ferrix_lode_core::segment::{self, Header};

    /// A segment of `records` as lode writes one.
    fn segment_bytes(boot: [u8; 16], records: &[Record]) -> Vec<u8> {
        let header = Header {
            boot_id: boot,
            first_seqnum: records.first().map_or(1, |record| record.seqnum),
            created_us: 0,
            host: "box".to_owned(),
            machine_id: [0; 16],
        };
        let mut bytes = header.encode();
        for record in records {
            segment::encode_record(record, &mut bytes);
        }
        bytes
    }

    fn record(seq: u64, message: &str) -> Record {
        let mut record = Record::new(6, seq * 1_000_000, seq);
        record.seqnum = seq;
        record.push("MESSAGE", message.as_bytes());
        record
    }

    #[test]
    fn a_store_is_read_whole_then_followed_from_where_it_stopped() {
        let dir = std::env::temp_dir().join(format!("lode-viewer-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory");
        let boot = [7; 16];
        let name = segment::file_name("system", &boot, 1);
        let path = dir.join(&name);
        std::fs::write(
            &path,
            segment_bytes(boot, &[record(1, "one"), record(2, "two")]),
        )
        .expect("written");
        let mut store = Store::at(std::slice::from_ref(&dir));
        let (records, refused) = store.load();
        assert!(refused.is_empty());
        assert_eq!(records.len(), 2);
        assert_eq!(&*records[0].host, "box");
        assert_eq!(records[1].boot_id, boot);
        assert!(store.follow(2).is_empty());
        let mut more = std::fs::read(&path).expect("read");
        segment::encode_record(&record(3, "three"), &mut more);
        std::fs::write(&path, more).expect("appended");
        let fresh = store.follow(2);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].message(), b"three");
        assert!(store.follow(3).is_empty());
        let status = store.status();
        assert_eq!(status.segments, 1);
        assert!(status.used > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
