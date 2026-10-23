//! The command line.
//!
//! ```text
//! lode-viewer                       lode's store, in a window
//! lode-viewer FILE.jsonl ...        a JSONL export (lode -o json, xtask box-log)
//! lode-viewer -D DIR ...            a store in another directory (lode -D)
//!   --page overview|logs            the page it opens on
//!   --theme light|dark              the palette
//!   --boot N                        the boot, as lode -b counts: 0 the last, -1 the one before
//!   --where EXPR                    the Logs page's query
//!   --zoom 30|60|300|0              the window's width in seconds, 0 the whole boot
//!   --at HH:MM:SS                   the window's middle
//!   --select SEQNUM                 the record whose pane is open
//!   --live                          follow the store from the start
//!   --render OUT.png [--size WxH] [--scale N]
//!                                   draw one frame into a PNG and exit, no compositor
//!   --bench N                       time reading, querying and drawing N records
//! lode-viewer --version | --help
//! ```

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use crate::app::{Action, App, Origin, Page};
use crate::data::Log;
use crate::store::Store;
use crate::theme::Mode;

/// What `--help` prints.
const USAGE: &str = "\
usage: lode-viewer [FILE.jsonl ...] [-D DIR ...] [options]

  FILE.jsonl        read a JSONL export (lode -o json, cargo xtask box-log) instead of the store
  -D DIR            read lode's store in DIR (default /var/log/journal and /run/log/journal)
  --page NAME       open on overview or logs
  --theme NAME      light or dark
  --boot N          the boot, as lode -b counts it: 0 the last, -1 the one before
  --where EXPR      the Logs page's query, in lode's filter language
  --zoom SECONDS    the time window's width: 30, 60, 300, or 0 for the whole boot
  --at HH:MM:SS     the time window's middle
  --select SEQNUM   open the pane of the record with this sequence number
  --live            follow the store as it grows
  --render OUT.png  draw one frame into a PNG and exit, with no compositor
  --size WxH        the frame's size in logical pixels (default 1440x1000)
  --scale N         buffer pixels to a logical one (default 1)
  --bench N         time reading, querying and drawing N records, and exit
  --version, --help
";

/// What the arguments asked for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Options {
    /// JSONL files.
    pub files: Vec<PathBuf>,
    /// Store directories.
    pub dirs: Vec<PathBuf>,
    /// The page.
    pub page: Option<Page>,
    /// The palette.
    pub mode: Option<Mode>,
    /// The boot offset.
    pub boot: Option<i64>,
    /// The query.
    pub query: Option<String>,
    /// The zoom, seconds.
    pub zoom: Option<u64>,
    /// The window's middle, seconds into the day.
    pub at: Option<u64>,
    /// The record selected.
    pub select: Option<u64>,
    /// Follow from the start.
    pub live: bool,
    /// Draw into this PNG.
    pub render: Option<PathBuf>,
    /// The frame's size.
    pub size: (u32, u32),
    /// The scale.
    pub scale: u32,
    /// Benchmark with this many records.
    pub bench: Option<usize>,
}

/// What the arguments ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum Asked {
    /// Run.
    Run(Options),
    /// `--version`.
    Version,
    /// `--help`.
    Help,
    /// Something refused, in these words.
    Refused(String),
}

/// Read the arguments after the program's name.
#[must_use]
pub fn parse(arguments: &[String]) -> Asked {
    let mut options = Options {
        size: (1440, 1000),
        scale: 1,
        ..Options::default()
    };
    let mut words = arguments.iter();
    while let Some(word) = words.next() {
        let mut value = |what: &str| {
            words
                .next()
                .cloned()
                .ok_or_else(|| format!("{word} needs {what}"))
        };
        let result: Result<(), String> = (|| {
            match word.as_str() {
                "--version" | "-V" => return Err("version".to_owned()),
                "--help" | "-h" => return Err("help".to_owned()),
                "-D" | "--directory" => options.dirs.push(PathBuf::from(value("a directory")?)),
                "--page" => {
                    let name = value("a page")?;
                    options.page = Some(
                        Page::named(&name)
                            .ok_or(format!("{name} is not a page: overview, logs"))?,
                    );
                }
                "--theme" => {
                    options.mode = Some(match value("light or dark")?.as_str() {
                        "light" => Mode::Light,
                        "dark" => Mode::Dark,
                        other => return Err(format!("{other} is not a theme: light, dark")),
                    });
                }
                "--boot" | "-b" => {
                    let text = value("a boot")?;
                    options.boot = Some(
                        text.parse()
                            .map_err(|_| format!("{text}: not a boot offset"))?,
                    );
                }
                "--where" => options.query = Some(value("an expression")?),
                "--zoom" => {
                    let text = value("seconds")?;
                    options.zoom = Some(text.parse().map_err(|_| format!("{text}: not seconds"))?);
                }
                "--at" => {
                    let text = value("a time")?;
                    options.at = Some(time_of_day(&text).ok_or(format!("{text}: not HH:MM:SS"))?);
                }
                "--select" => {
                    let text = value("a sequence number")?;
                    options.select =
                        Some(text.parse().map_err(|_| format!("{text}: not a number"))?);
                }
                "--live" | "-f" => options.live = true,
                "--render" => options.render = Some(PathBuf::from(value("a file")?)),
                "--size" => {
                    let text = value("WxH")?;
                    let (w, h) = text.split_once('x').ok_or(format!("{text}: not WxH"))?;
                    options.size = (
                        w.parse().map_err(|_| format!("{text}: not WxH"))?,
                        h.parse().map_err(|_| format!("{text}: not WxH"))?,
                    );
                }
                "--scale" => {
                    let text = value("a scale")?;
                    options.scale = text.parse().map_err(|_| format!("{text}: not a scale"))?;
                }
                "--bench" => {
                    let text = value("a count")?;
                    options.bench = Some(text.parse().map_err(|_| format!("{text}: not a count"))?);
                }
                other if other.starts_with('-') => {
                    return Err(format!("{other} is not an option lode-viewer has"));
                }
                file => options.files.push(PathBuf::from(file)),
            }
            Ok(())
        })();
        match result {
            Ok(()) => {}
            Err(word) if word == "version" => return Asked::Version,
            Err(word) if word == "help" => return Asked::Help,
            Err(why) => return Asked::Refused(why),
        }
    }
    Asked::Run(options)
}

/// `HH:MM:SS` as seconds into the day.
fn time_of_day(text: &str) -> Option<u64> {
    let mut parts = text.split(':').map(|part| part.parse::<u64>().ok());
    let (h, m) = (parts.next()??, parts.next()??);
    let s = parts.next().flatten().unwrap_or(0);
    (h < 24 && m < 60 && s < 60).then_some(h * 3600 + m * 60 + s)
}

/// Say one line of what the program did.
pub fn say(text: &str) {
    println!("lode-viewer: {text}");
}

/// The viewer for `options`: the records read, the state set as asked.
#[must_use]
pub fn open(options: &Options) -> App {
    let mut notices = Vec::new();
    let (log, origin, name) = if options.files.is_empty() {
        let mut store = if options.dirs.is_empty() {
            Store::system()
        } else {
            Store::at(&options.dirs)
        };
        let (records, refused) = store.load();
        for why in &refused {
            say(&format!("warn: {why}"));
        }
        notices.extend(refused);
        say(&format!("read {} records from lode's store", records.len()));
        (
            Log::new(records),
            Origin::Store(store),
            "lode store".to_owned(),
        )
    } else {
        let mut records = Vec::new();
        for file in &options.files {
            match std::fs::read_to_string(file) {
                Ok(text) => {
                    let read = crate::json::read(&text);
                    say(&format!(
                        "read {} records from {} ({} other lines)",
                        read.records.len(),
                        file.display(),
                        read.skipped
                    ));
                    records.extend(read.records);
                }
                Err(error) => {
                    say(&format!("err: {}: {error}", file.display()));
                    notices.push(format!("{}: {error}", file.display()));
                }
            }
        }
        let first = options.files.first().cloned().unwrap_or_default();
        let name = first.file_name().map_or_else(
            || "file".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        (Log::new(records), Origin::File(first), name)
    };
    let mut app = App::new(log, origin, name);
    app.notices = notices;
    if let Some(home) = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    {
        app.load_saved(home.join("ferrix/lode/viewer-queries.conf"));
    }
    if let Some(mode) = options.mode {
        app.mode = mode;
    }
    if let Some(offset) = options.boot
        && let Some(index) = app.log.boot_at(offset)
    {
        app.go_to_boot(index);
    }
    if let Some(zoom) = options.zoom {
        app.act(&Action::Zoom(zoom), 0.0);
    }
    if let Some(at) = options.at
        && let Some(boot) = app.boot()
    {
        let zone = i64::from(app.zone());
        let day = (boot.first_us as i64 / 1_000_000 + zone).div_euclid(86_400);
        let us = ((day * 86_400 + at as i64 - zone) * 1_000_000).max(0) as u64;
        app.cursor = us;
        app.refresh();
    }
    if let Some(query) = &options.query {
        app.query.set_text(query);
        app.draft.clone_from(query);
        if let Some(index) = app.saved.iter().position(|saved| saved.text == *query) {
            app.saved_in_use = Some(index);
        }
        app.refresh();
    }
    if let Some(page) = options.page {
        app.page = page;
    }
    if let Some(seq) = options.select
        && let Some(index) = app
            .log
            .entries
            .iter()
            .position(|entry| entry.record.seqnum == seq)
    {
        app.selected = Some(index);
        app.scroll_to(index);
    }
    if options.live && matches!(app.origin, Origin::Store(_)) {
        app.live = true;
    }
    app
}

/// The program.
#[must_use]
pub fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match parse(&arguments) {
        Asked::Run(options) => {
            if let Some(count) = options.bench {
                return bench(&options, count);
            }
            let app = open(&options);
            let status = match &options.render {
                Some(out) => crate::window::render(app, out, options.size, options.scale),
                None => crate::window::run(app),
            };
            ExitCode::from(u8::try_from(status).unwrap_or(1))
        }
        Asked::Version => {
            println!("lode-viewer {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Asked::Help => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Asked::Refused(why) => {
            say(&format!("err: {why}"));
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// `--bench N`: the files (or a made-up boot) grown to N records, then
/// each step timed: making the log, a query, the table's lines, and a
/// frame of each page.
fn bench(options: &Options, count: usize) -> ExitCode {
    let mut base = Vec::new();
    for file in &options.files {
        if let Ok(text) = std::fs::read_to_string(file) {
            base.extend(crate::json::read(&text).records);
        }
    }
    if base.is_empty() {
        say("err: --bench needs a JSONL file to grow");
        return ExitCode::from(2);
    }
    let mut records = Vec::with_capacity(count);
    let span = base
        .last()
        .map_or(0, |r| r.realtime_us)
        .saturating_sub(base.first().map_or(0, |r| r.realtime_us))
        + 1_000_000;
    let mut round = 0_u64;
    while records.len() < count {
        for record in &base {
            if records.len() >= count {
                break;
            }
            let mut copy = record.clone();
            copy.seqnum += round * 10_000_000;
            copy.realtime_us += round * span;
            records.push(copy);
        }
        round += 1;
    }
    let started = Instant::now();
    let log = Log::new(records);
    let built = started.elapsed();
    let mut app = App::new(
        log,
        Origin::File(options.files.first().cloned().unwrap_or_default()),
        "bench".to_owned(),
    );
    let started = Instant::now();
    app.act(&Action::Zoom(0), 0.0);
    let queried = started.elapsed();
    let started = Instant::now();
    app.query.set_text("priority <= warning or id = \"hypr*\"");
    app.refresh();
    let filtered = started.elapsed();
    app.query.set_text("");
    app.refresh();
    app.page = Page::Logs;
    app.scroll = app.table_height / 2.0;
    let frames = crate::window::bench_frames(&mut app, options.size);
    say(&format!(
        "{} records: log {:.0} ms, whole boot query and lines {:.0} ms ({} lines), filtered query {:.0} ms, \
         frames: {}",
        app.log.entries.len(),
        built.as_secs_f64() * 1e3,
        queried.as_secs_f64() * 1e3,
        app.lines.len(),
        filtered.as_secs_f64() * 1e3,
        frames
    ));
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::{Asked, parse};
    use crate::app::Page;

    fn asked(words: &[&str]) -> Asked {
        parse(
            &words
                .iter()
                .map(|word| (*word).to_owned())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn the_arguments() {
        let Asked::Run(options) = asked(&[
            "boot.jsonl",
            "--page",
            "logs",
            "--at",
            "17:22:43",
            "--size",
            "800x600",
        ]) else {
            panic!("a run");
        };
        assert_eq!(options.files.len(), 1);
        assert_eq!(options.page, Some(Page::Logs));
        assert_eq!(options.at, Some(17 * 3600 + 22 * 60 + 43));
        assert_eq!(options.size, (800, 600));
        assert_eq!(asked(&["--version"]), Asked::Version);
        assert!(matches!(asked(&["--page", "x"]), Asked::Refused(_)));
        assert!(matches!(asked(&["--frob"]), Asked::Refused(_)));
        assert!(matches!(asked(&["--zoom"]), Asked::Refused(_)));
    }
}
