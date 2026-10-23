//! The text `ferrofetch` reads, parsed.
//!
//! Each parser takes one file's contents as the kernel's `/proc` and `/sys`
//! write them (`src/lib/fs/procfs`, `src/lib/fs/sysfs`), and answers `None`
//! for text it does not recognise rather than a guess: a line `ferrofetch`
//! cannot fill is left out, never filled wrongly.

/// The parent's pid, from `/proc/<pid>/stat`.
///
/// The fourth field, counted after the *last* `)`: the second field is the
/// command's name in parentheses, and a name may itself hold a `)` or a
/// space.
#[must_use]
pub fn stat_parent(stat: &str) -> Option<u32> {
    let (_, after) = stat.rsplit_once(')')?;
    let mut fields = after.split_ascii_whitespace();
    let _state = fields.next()?;
    fields.next()?.parse().ok()
}

/// The real uid, from `/proc/<pid>/status`'s `Uid:` line: the first of its
/// four ids.
#[must_use]
pub fn status_uid(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))?
        .split_ascii_whitespace()
        .next()?
        .parse()
        .ok()
}

/// The name `/etc/passwd` gives `uid`: the first line whose third field is
/// it.
#[must_use]
pub fn passwd_name(passwd: &str, uid: u32) -> Option<&str> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        let _password = fields.next()?;
        let id: u32 = fields.next()?.parse().ok()?;
        (id == uid && !name.is_empty()).then_some(name)
    })
}

/// Whole seconds since boot, from `/proc/uptime`'s first field.
#[must_use]
pub fn uptime_seconds(uptime: &str) -> Option<u64> {
    let first = uptime.split_ascii_whitespace().next()?;
    let whole = first.split_once('.').map_or(first, |(whole, _)| whole);
    whole.parse().ok()
}

/// What `/proc/meminfo` says of memory, in KiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    /// `MemTotal`.
    pub total_kib: u64,
    /// `MemAvailable`: what could be had without swapping.
    pub available_kib: u64,
}

impl Memory {
    /// What is in use: all of it less what is available.
    #[must_use]
    pub const fn used_kib(&self) -> u64 {
        self.total_kib.saturating_sub(self.available_kib)
    }
}

/// `MemTotal` and `MemAvailable` from `/proc/meminfo`, or `None` without
/// both.
#[must_use]
pub fn meminfo(text: &str) -> Option<Memory> {
    let field = |name: &str| {
        text.lines().find_map(|line| {
            let value = line.strip_prefix(name)?.strip_prefix(':')?;
            value.split_ascii_whitespace().next()?.parse::<u64>().ok()
        })
    };
    Some(Memory {
        total_kib: field("MemTotal")?,
        available_kib: field("MemAvailable")?,
    })
}

/// What `/proc/loadavg` says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Loadavg<'a> {
    /// The three averages, as the kernel wrote them.
    pub averages: [&'a str; 3],
    /// The processes that exist, the denominator of the fourth field.
    pub processes: u64,
}

/// `/proc/loadavg`: `0.00 0.01 0.05 1/42 1234`.
#[must_use]
pub fn loadavg(text: &str) -> Option<Loadavg<'_>> {
    let mut fields = text.split_ascii_whitespace();
    let averages = [fields.next()?, fields.next()?, fields.next()?];
    let (_, processes) = fields.next()?.split_once('/')?;
    Some(Loadavg {
        averages,
        processes: processes.parse().ok()?,
    })
}

/// What `/proc/cpuinfo` says of the processors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cpu {
    /// The online processors: one `processor` line each.
    pub count: u32,
    /// The first processor's `cpu MHz`, in kHz, where the kernel knows it:
    /// only on x86-64, and only when its counter is the TSC.
    pub khz: Option<u64>,
    /// The processor's name, cleaned as neofetch cleans it: the first
    /// `model name` on x86-64, the cores' part names on Arm.
    pub model: crate::Text<64>,
    /// The highest rate the processor runs at, in kHz, from cpufreq or the
    /// processor itself, which the line prefers to `cpu MHz`, as fastfetch
    /// and neofetch do: the program fills it, not `/proc/cpuinfo`.
    pub max_khz: Option<u64>,
}

/// The most kinds of core [`CpuinfoParser`] names: a phone's three.
const MAX_KINDS: usize = 3;

/// `/proc/cpuinfo`, a line at a time, so that a machine with more
/// processors than a buffer holds is still counted whole.
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuinfoParser {
    /// What the lines so far said.
    cpu: Cpu,
    /// The block's `CPU implementer`, until its `CPU part`.
    implementer: Option<u32>,
    /// The kinds of Arm core seen, in order: implementer and part.
    kinds: [Option<(u32, u32)>; MAX_KINDS],
}

impl CpuinfoParser {
    /// Take one line, without its newline.
    pub fn line(&mut self, line: &str) {
        let Some((key, value)) = line.split_once(':') else {
            return;
        };
        let value = value.trim();
        match key.trim_end() {
            "processor" => self.cpu.count = self.cpu.count.saturating_add(1),
            "cpu MHz" if self.cpu.khz.is_none() => self.cpu.khz = megahertz_as_khz(value),
            "model name" if self.cpu.model.is_empty() => self.cpu.model = clean_model_name(value),
            "CPU implementer" => self.implementer = hex(value),
            "CPU part" => {
                if let (Some(implementer), Some(part)) = (self.implementer, hex(value)) {
                    let kind = Some((implementer, part));
                    if !self.kinds.contains(&kind)
                        && let Some(slot) = self.kinds.iter_mut().find(|slot| slot.is_none())
                    {
                        *slot = kind;
                    }
                }
            }
            _ => {}
        }
    }

    /// What the lines said.
    #[must_use]
    pub fn finish(self) -> Cpu {
        let mut cpu = self.cpu;
        if cpu.model.is_empty() {
            for (implementer, part) in self.kinds.iter().flatten() {
                let Some(name) = arm_core(*implementer, *part) else {
                    continue;
                };
                if !cpu.model.is_empty() {
                    cpu.model.push(" + ");
                }
                cpu.model.push(name);
            }
        }
        cpu
    }
}

/// `/proc/cpuinfo` whole: how many processors, what they are and how fast
/// the first says it is.
#[must_use]
pub fn cpuinfo(text: &str) -> Cpu {
    let mut parser = CpuinfoParser::default();
    text.lines().for_each(|line| parser.line(line));
    parser.finish()
}

/// `0x41` as a number.
fn hex(text: &str) -> Option<u32> {
    u32::from_str_radix(text.strip_prefix("0x")?, 16).ok()
}

/// The words a processor's name is better without, as neofetch and fastfetch
/// drop them: what every name says, and the core count the line gives anyway.
const DROPPED_WORDS: [&str; 8] = [
    "CPU",
    "Processor",
    "FPU",
    "APU",
    "Dual-Core",
    "Quad-Core",
    "Six-Core",
    "Eight-Core",
];

/// A `model name` as a fetch tool shows it: `Intel(R) Core(TM) i7-8700 CPU @
/// 3.20GHz` is `Intel Core i7-8700`, `AMD Ryzen 9 9900X 12-Core Processor`
/// is `AMD Ryzen 9 9900X`. Cut at the `@` of a rate (the line adds its own),
/// and at AMD's "with Radeon Graphics"; without the `(R)` and `(TM)` marks;
/// without the words [`DROPPED_WORDS`] names or a `<n>-Core`.
#[must_use]
pub fn clean_model_name(name: &str) -> crate::Text<64> {
    let name = name.split('@').next().unwrap_or_default();
    let mut out = crate::Text::new();
    let mut words = name.split_ascii_whitespace().peekable();
    while let Some(word) = words.next() {
        if matches!(word, "with" | "w/") && words.peek() == Some(&"Radeon") {
            break;
        }
        let mut bare = crate::Text::<64>::new();
        let mut rest = word;
        while !rest.is_empty() {
            let mark = ["(R)", "(r)", "(TM)", "(tm)"]
                .iter()
                .find_map(|mark| rest.find(mark).map(|at| (at, mark.len())));
            let Some((at, len)) = mark else {
                bare.push(rest);
                break;
            };
            bare.push(rest.get(..at).unwrap_or_default());
            rest = rest.get(at + len..).unwrap_or_default();
        }
        let bare = bare.as_str();
        let counted = bare
            .strip_suffix("-Core")
            .is_some_and(|count| !count.is_empty() && count.bytes().all(|b| b.is_ascii_digit()));
        if bare.is_empty() || counted || DROPPED_WORDS.contains(&bare) {
            continue;
        }
        if !out.is_empty() {
            out.push(" ");
        }
        out.push(bare);
    }
    out
}

/// An Arm core's name from `CPU implementer` and `CPU part`, for the parts
/// Ferrix's machines and QEMU have, as Linux's `arch/arm64/include/asm/cputype.h`
/// and `arch/arm/include/asm/cputype.h` number them.
#[must_use]
pub fn arm_core(implementer: u32, part: u32) -> Option<&'static str> {
    if implementer != 0x41 {
        return None;
    }
    Some(match part {
        0xC07 => "Cortex-A7",
        0xC08 => "Cortex-A8",
        0xC09 => "Cortex-A9",
        0xC0D => "Cortex-A12",
        0xC0E => "Cortex-A17",
        0xC0F => "Cortex-A15",
        0xD03 => "Cortex-A53",
        0xD04 => "Cortex-A35",
        0xD05 => "Cortex-A55",
        0xD07 => "Cortex-A57",
        0xD08 => "Cortex-A72",
        0xD09 => "Cortex-A73",
        0xD0A => "Cortex-A75",
        0xD0B => "Cortex-A76",
        0xD0C => "Neoverse-N1",
        0xD0D => "Cortex-A77",
        0xD40 => "Neoverse-V1",
        0xD41 => "Cortex-A78",
        0xD44 => "Cortex-X1",
        0xD46 => "Cortex-A510",
        0xD47 => "Cortex-A710",
        0xD48 => "Cortex-X2",
        0xD49 => "Neoverse-N2",
        0xD4D => "Cortex-A715",
        0xD0E => "Cortex-A76AE",
        0xD42 => "Cortex-A78AE",
        0xD4B => "Cortex-A78C",
        0xD4C => "Cortex-X1C",
        0xD4E => "Cortex-X3",
        0xD4F => "Neoverse-V2",
        0xD80 => "Cortex-A520",
        0xD81 => "Cortex-A720",
        0xD82 => "Cortex-X4",
        _ => return None,
    })
}

/// A cpufreq file's rate, `3600000\n` in kHz.
#[must_use]
pub fn cpufreq_khz(text: &str) -> Option<u64> {
    text.trim().parse().ok().filter(|&khz| khz > 0)
}

/// `2995.198` as kHz: whole megahertz and up to three places after the point.
fn megahertz_as_khz(text: &str) -> Option<u64> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let whole: u64 = whole.parse().ok()?;
    let mut khz = whole.checked_mul(1000)?;
    let mut scale = 100;
    for digit in fraction.bytes().take(3) {
        if !digit.is_ascii_digit() {
            return None;
        }
        khz = khz.saturating_add(u64::from(digit - b'0') * scale);
        scale /= 10;
    }
    Some(khz)
}

/// A DRM connector's first mode, from its `modes`: `1280x800`, the preferred
/// mode first, one a line.
#[must_use]
pub fn first_mode(modes: &str) -> Option<(u32, u32)> {
    let (width, height) = modes.lines().next()?.trim().split_once('x')?;
    Some((width.parse().ok()?, height.parse().ok()?))
}

/// The connector a `/sys/class/drm` entry is, if it is one: `card0-Virtual-1`
/// is `Virtual-1`, and `card0` itself, the card, is none.
#[must_use]
pub fn connector(entry: &str) -> Option<&str> {
    let (card, connector) = entry.split_once('-')?;
    let number = card.strip_prefix("card")?;
    (!number.is_empty()
        && number.bytes().all(|byte| byte.is_ascii_digit())
        && !connector.is_empty())
    .then_some(connector)
}

/// The names in a buffer of `struct linux_dirent64` records, as
/// `getdents64` fills it: a 64-bit inode, a 64-bit offset, a 16-bit record
/// length, a type byte, then the name and its NUL.
#[derive(Debug, Clone)]
pub struct Dirents<'a> {
    bytes: &'a [u8],
}

/// The offset of a record's length.
const RECORD_LENGTH: usize = 16;
/// The offset of a record's name.
const RECORD_NAME: usize = 19;

impl<'a> Dirents<'a> {
    /// The records in `bytes`, which is what one `getdents64` wrote.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
}

impl<'a> Iterator for Dirents<'a> {
    type Item = &'a [u8];

    /// The next name, or `None` at the end or at a record that does not fit
    /// in what is left.
    fn next(&mut self) -> Option<&'a [u8]> {
        let length = self.bytes.get(RECORD_LENGTH..RECORD_NAME - 1)?;
        let length = usize::from(u16::from_ne_bytes([*length.first()?, *length.get(1)?]));
        let record = self.bytes.get(..length).filter(|_| length > RECORD_NAME)?;
        self.bytes = self.bytes.get(length..).unwrap_or_default();
        Some(c_bytes(record.get(RECORD_NAME..)?))
    }
}

/// `bytes` up to its first NUL, or all of it without one.
#[must_use]
pub fn c_bytes(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    bytes.get(..end).unwrap_or_default()
}

/// `bytes` up to its first NUL as text, or `None` for an empty field or one
/// that is not UTF-8: a `uname` field, a `comm`.
#[must_use]
pub fn c_str(bytes: &[u8]) -> Option<&str> {
    let text = core::str::from_utf8(c_bytes(bytes)).ok()?.trim_end();
    (!text.is_empty()).then_some(text)
}

/// Bytes of one `struct new_utsname` field: `__NEW_UTS_LEN` and its NUL.
const UTS_FIELD: usize = 65;

/// Bytes of a `struct new_utsname`: six fields, alike on every architecture.
pub const UTSNAME: usize = 6 * UTS_FIELD;

/// A field of `struct new_utsname`, in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Uts {
    /// `sysname`: `Ferrix`.
    Sysname,
    /// `nodename`: the host's name.
    Nodename,
    /// `release`.
    Release,
    /// `version`: `#1 Ferrix 0.1.0`.
    Version,
    /// `machine`: `x86_64`.
    Machine,
}

/// `field` of the `struct new_utsname` in `names`, or `None` for one that
/// is empty or not UTF-8.
#[must_use]
pub fn uts_field(names: &[u8], field: Uts) -> Option<&str> {
    let at = field as usize * UTS_FIELD;
    c_str(names.get(at..at + UTS_FIELD)?)
}

/// The system's own version, from `uname`'s `version`: `#1 Ferrix 0.1.0` is
/// `Ferrix 0.1.0`, the build number in front taken off.
#[must_use]
pub fn os_version(version: &str) -> &str {
    version
        .strip_prefix('#')
        .and_then(|rest| rest.split_once(' '))
        .filter(|(build, _)| build.bytes().all(|byte| byte.is_ascii_digit()))
        .map_or(version, |(_, rest)| rest)
        .trim()
}

/// The arguments on a command line from `/proc/<pid>/cmdline`, the program's
/// own name left out: each NUL-terminated.
pub fn arguments(cmdline: &[u8]) -> impl Iterator<Item = &[u8]> {
    cmdline
        .strip_suffix(&[0])
        .unwrap_or(cmdline)
        .split(|&byte| byte == 0)
        .skip(1)
}

/// What the command line asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// The logo and the lines.
    Show(Options),
    /// The usage.
    Help,
    /// The version.
    Version,
}

/// How to show them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// The mark beside the lines.
    pub logo: bool,
    /// Colour, by the terminal's escapes.
    pub color: bool,
}

/// The [`Command`] `arguments` ask for, or the first argument that is not
/// one `ferrofetch` takes.
///
/// # Errors
///
/// The argument not understood.
pub fn command<'a>(arguments: impl IntoIterator<Item = &'a [u8]>) -> Result<Command, &'a [u8]> {
    let mut options = Options {
        logo: true,
        color: true,
    };
    for argument in arguments {
        match argument {
            b"-h" | b"--help" => return Ok(Command::Help),
            b"-V" | b"--version" => return Ok(Command::Version),
            b"--no-logo" => options.logo = false,
            b"--no-color" => options.color = false,
            other => return Err(other),
        }
    }
    Ok(Command::Show(options))
}
