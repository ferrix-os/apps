//! Folder sizes, added up on a thread of their own so a large folder never
//! holds the window: a pane asks for the folders it shows, the thread
//! walks each (links not followed, other file systems not entered) and
//! sends back the bytes and files it found. Until one comes, a folder
//! shows its count of entries.

use std::collections::{HashMap, HashSet};
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

/// What a folder holds, all told.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Total {
    /// Bytes in its files.
    pub bytes: u64,
    /// Files, folders not counted.
    pub files: u64,
}

/// Walk `dir` and add up what it holds, without following links or
/// leaving its file system.
#[must_use]
pub fn measure(dir: &Path) -> Total {
    let Ok(top) = dir.symlink_metadata() else {
        return Total::default();
    };
    let device = top.dev();
    let mut total = Total::default();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in read.filter_map(Result::ok) {
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            if meta.is_dir() {
                if meta.dev() == device {
                    stack.push(entry.path());
                }
            } else if meta.is_file() {
                total.bytes += meta.len();
                total.files += 1;
            }
        }
    }
    total
}

/// The folder sizes known, and the thread that finds more.
#[derive(Debug)]
pub struct Sizes {
    known: HashMap<PathBuf, Total>,
    asked: HashSet<PathBuf>,
    ask: Option<Sender<PathBuf>>,
    answers: Option<Receiver<(PathBuf, Total)>>,
}

impl Sizes {
    /// A worker of its own; `threaded` false measures on the spot when
    /// asked (for a drawing that must be whole at once, and for tests).
    #[must_use]
    pub fn new(threaded: bool) -> Self {
        let (mut ask, mut answers) = (None, None);
        if threaded {
            let (ask_tx, ask_rx) = channel::<PathBuf>();
            let (answer_tx, answer_rx) = channel();
            let spawned = std::thread::Builder::new()
                .name("shelf-sizes".to_owned())
                // Until the window is gone and nobody asks or hears.
                .spawn(move || {
                    while let Ok(dir) = ask_rx.recv()
                        && answer_tx.send((dir.clone(), measure(&dir))).is_ok()
                    {
                    }
                });
            if spawned.is_ok() {
                ask = Some(ask_tx);
                answers = Some(answer_rx);
            }
        }
        Sizes {
            known: HashMap::new(),
            asked: HashSet::new(),
            ask,
            answers,
        }
    }

    /// What `dir` holds, if known; asked for if not.
    pub fn get(&mut self, dir: &Path) -> Option<Total> {
        if let Some(total) = self.known.get(dir) {
            return Some(*total);
        }
        if self.asked.insert(dir.to_path_buf()) {
            match &self.ask {
                Some(ask) => {
                    let _ = ask.send(dir.to_path_buf());
                }
                None => {
                    let total = measure(dir);
                    let _ = self.known.insert(dir.to_path_buf(), total);
                    return Some(total);
                }
            }
        }
        None
    }

    /// Take what the thread found since; whether anything came.
    pub fn poll(&mut self) -> bool {
        let Some(answers) = &self.answers else {
            return false;
        };
        let mut any = false;
        while let Ok((dir, total)) = answers.try_recv() {
            let _ = self.asked.remove(&dir);
            let _ = self.known.insert(dir, total);
            any = true;
        }
        any
    }

    /// Whether some folder asked for is still being added up.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.answers.is_some() && !self.asked.is_empty()
    }

    /// Forget `path`'s size and its folders' above it, after a change in
    /// it.
    pub fn forget(&mut self, path: &Path) {
        self.known.retain(|dir, _| !path.starts_with(dir));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_adds_up_its_files_once_the_thread_says() {
        let dir = std::env::temp_dir().join(format!("shelf-sizes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("a/b"));
        let _ = std::fs::write(dir.join("a/one"), [0_u8; 100]);
        let _ = std::fs::write(dir.join("a/b/two"), [0_u8; 50]);
        let _ = std::os::unix::fs::symlink("/usr", dir.join("a/link"));
        assert_eq!(
            measure(&dir.join("a")),
            Total {
                bytes: 150,
                files: 2
            }
        );
        let mut sizes = Sizes::new(true);
        let mut got = sizes.get(&dir.join("a"));
        for _ in 0..200 {
            if got.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            let _ = sizes.poll();
            got = sizes.get(&dir.join("a"));
        }
        assert_eq!(
            got,
            Some(Total {
                bytes: 150,
                files: 2
            })
        );
        sizes.forget(&dir.join("a/b/two"));
        assert!(!sizes.known.contains_key(&dir.join("a")));
        let mut now = Sizes::new(false);
        assert_eq!(
            now.get(&dir.join("a/b")),
            Some(Total {
                bytes: 50,
                files: 1
            })
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
