//! A stand-in for the installer daemon, on a socket of its own, for the
//! host tests and `--screenshot`: the disks a person might plausibly have
//! and an install that streams its steps. It writes nothing anywhere but
//! its socket.

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The disks the stand-in reports: the live medium, a 32 GiB virtio disk
/// with no table, a 256 GiB disk with an EFI partition, a Windows NTFS
/// partition and 100 GiB free, and a 4 GiB stick too small to take
/// Ferrix.
pub const DISKS: &str = "\
min 8589934592
live /dev/sda
disk /dev/sda 15523119104 mbr SanDisk Ultra Fit
part /dev/sda 1 2048 30318591 15521824768 fat FERRIX_LIVE
disk /dev/vdb 34359738368 none Virtio Block Device
free /dev/vdb 0 67108863 34359738368
disk /dev/nvme0n1 274877906944 gpt Samsung SSD 970 EVO Plus
part /dev/nvme0n1 1 2048 1050623 536870912 fat EFI system partition
part /dev/nvme0n1 2 1050624 327155711 166966919168 ntfs Windows
free /dev/nvme0n1 327155712 536868863 107373133824
disk /dev/sdb 4026531840 mbr Kingston DataTraveler
part /dev/sdb 1 2048 7864319 4025483264 fat STICK
ok
";

/// The steps the stand-in's install goes through.
pub const STEPS: [&str; 7] = [
    "Preparing the disk",
    "Making the file systems",
    "Copying the system",
    "Setting up the boot loader",
    "Creating your account",
    "Setting the keyboard, time zone and name",
    "Finishing",
];

/// What the stand-in heard and how it answers.
#[derive(Debug, Default)]
pub struct Heard {
    /// Each request's first line, in order.
    pub requests: Vec<String>,
    /// The last install request's `key=value` lines.
    pub install: Vec<String>,
}

/// A stand-in daemon, answering on its socket until dropped.
#[derive(Debug)]
pub struct Fake {
    /// Its socket.
    pub socket: PathBuf,
    /// What it heard.
    pub heard: Arc<Mutex<Heard>>,
}

/// How a stand-in behaves.
#[derive(Clone, Copy, Debug, Default)]
pub struct Behaviour {
    /// Whether its install fails at the fourth step.
    pub fail: bool,
    /// How long it waits between the stream's lines.
    pub pace: Duration,
    /// Whether `disks` answers with no disk but the live one.
    pub no_disks: bool,
}

impl Fake {
    /// A stand-in answering on `socket`, which is replaced.
    ///
    /// # Errors
    ///
    /// The socket could not be made.
    pub fn start(socket: &Path, behaviour: Behaviour) -> std::io::Result<Fake> {
        let _ = std::fs::remove_file(socket);
        let listener = UnixListener::bind(socket)?;
        let heard = Arc::new(Mutex::new(Heard::default()));
        let shared = Arc::clone(&heard);
        let _ = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                if serve(stream, behaviour, &shared).is_err() {
                    continue;
                }
            }
        });
        Ok(Fake {
            socket: socket.to_path_buf(),
            heard,
        })
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket);
    }
}

/// Answer one connection.
fn serve(stream: UnixStream, behaviour: Behaviour, heard: &Mutex<Heard>) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut out = stream;
    let mut first = String::new();
    let _ = reader.read_line(&mut first)?;
    let first = first.trim_end().to_owned();
    if let Ok(mut heard) = heard.lock() {
        heard.requests.push(first.clone());
    }
    match first.as_str() {
        "disks" => {
            if behaviour.no_disks {
                out.write_all(
                    DISKS
                        .lines()
                        .take(4)
                        .collect::<Vec<_>>()
                        .join("\n")
                        .as_bytes(),
                )?;
                out.write_all(b"\nok\n")?;
            } else {
                out.write_all(DISKS.as_bytes())?;
            }
        }
        "reboot" => out.write_all(b"ok\n")?,
        "install" => {
            let mut pairs = Vec::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line)? == 0 {
                    break;
                }
                let line = line.trim_end_matches('\n');
                if line.is_empty() {
                    break;
                }
                pairs.push(line.to_owned());
            }
            if let Ok(mut heard) = heard.lock() {
                heard.install = pairs;
            }
            let of = STEPS.len();
            for (at, step) in STEPS.iter().enumerate() {
                let n = at + 1;
                writeln!(out, "step {n} {of} {step}")?;
                writeln!(out, "log {step}")?;
                if behaviour.fail && n == 4 {
                    writeln!(out, "log grub-install: /boot/efi is not an EFI partition")?;
                    writeln!(
                        out,
                        "error The boot loader could not be installed: /boot/efi is not an EFI partition"
                    )?;
                    return Ok(());
                }
                writeln!(out, "progress {}", n * 100 / of)?;
                std::thread::sleep(behaviour.pace);
            }
            writeln!(out, "done")?;
        }
        other => writeln!(out, "error unknown request {other}")?,
    }
    Ok(())
}
