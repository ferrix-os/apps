//! The command line.
//!
//! ```text
//! lode-viewer                       lode's store, in a window
//! lode-viewer FILE.jsonl ...        a JSONL export (lode -o json, xtask box-log)
//! lode-viewer -D DIR ...            a store in another directory (lode -D)
//! lode-viewer --machines DIR        each directory in DIR a machine's store (lode-receive's)
//! lode-viewer --import FILE.jsonl --into DIR
//!                                   write a JSONL export as a store, and exit
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
  --machines DIR    each directory in DIR is one machine's store, as lode-receive keeps them;
                    without a store or a file, ~/.local/share/ferrix/lode-receive (LODE_RECEIVE_DIR)
  --pull [USER@]HOST[:PORT] [--key FILE] [--into ROOT] [--seconds N]
                    follow a machine's lode over ssh into ROOT with no window, as the
                    Machines page does, for N seconds (default 30), and say what came
  --pair NAME --server-name HOST --on [USER@]HOST[:PORT] [--key FILE] [--yes]
                    set a machine up to send its log over TLS to lode-receive here: show
                    what would be done on it, and with --yes do it
  --no-remotes      do not follow the machines of ~/.config/ferrix/lode/machines.conf
  --import FILE.jsonl --into DIR
                    write a JSONL export (a box log) into DIR as lode's store, and exit
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
    /// A directory of machines' stores.
    pub machines: Option<PathBuf>,
    /// Where `--import` writes.
    pub into: Option<PathBuf>,
    /// `--import`.
    pub import: bool,
    /// Leave the machines of machines.conf unfollowed.
    pub no_remotes: bool,
    /// `--pull`: follow this machine with no window.
    pub pull: Option<String>,
    /// How long `--pull` follows, seconds.
    pub seconds: u64,
    /// `--key` for `--pull` and `--pair`.
    pub key: Option<String>,
    /// `--pair NAME`: set a machine up to forward over TLS, no window.
    pub pair: Option<String>,
    /// `--server-name` for `--pair`.
    pub server: Option<String>,
    /// `--on [USER@]HOST[:PORT]` for `--pair`.
    pub on: Option<String>,
    /// `--yes`: do what `--pair` shows.
    pub yes: bool,
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
    Run(Box<Options>),
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
                "--machines" => options.machines = Some(PathBuf::from(value("a directory")?)),
                "--import" => {
                    options.import = true;
                    options.files.push(PathBuf::from(value("a JSONL file")?));
                }
                "--into" => options.into = Some(PathBuf::from(value("a directory")?)),
                "--no-remotes" => options.no_remotes = true,
                "--pull" => options.pull = Some(value("[USER@]HOST[:PORT]")?),
                "--key" => options.key = Some(value("a key file")?),
                "--pair" => options.pair = Some(value("a machine's name")?),
                "--server-name" => options.server = Some(value("this host's name")?),
                "--on" => options.on = Some(value("[USER@]HOST[:PORT]")?),
                "--yes" => options.yes = true,
                "--seconds" => {
                    let text = value("seconds")?;
                    options.seconds = text.parse().map_err(|_| format!("{text}: not seconds"))?;
                }
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
    Asked::Run(Box::new(options))
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
    // Several machines' stores: asked for, or, when nothing else is, the
    // receiver's root where it has any and this machine has no store.
    let root = options.machines.clone().or_else(|| {
        if !options.files.is_empty() || !options.dirs.is_empty() {
            return None;
        }
        let root = receive_root()?;
        let none_here = Store::system().status().segments == 0;
        (none_here && !crate::store::machines(&root).is_empty()).then_some(root)
    });
    if let Some(root) = root {
        let machines = crate::store::machines(&root);
        say(&format!(
            "{} machines in {}",
            machines.len(),
            root.display()
        ));
        let mut app = App::new(
            Log::default(),
            Origin::File(root.clone()),
            "no machine".to_owned(),
        );
        if machines.is_empty() {
            app.notices.push(format!(
                "{}: no machine has sent its log here yet",
                root.display()
            ));
        }
        app.machines = machines;
        app.machines_root = Some(root.clone());
        app.open_machine(0);
        // What arrives from the machines is followed from the start.
        app.live = matches!(app.origin, Origin::Store(_));
        say(&format!(
            "read {} records of {}",
            app.log.entries.len(),
            app.origin_name
        ));
        return settle(app, options);
    }
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
    settle(app, options)
}

/// Where lode-receive keeps the machines that send to it:
/// `LODE_RECEIVE_DIR`, else `~/.local/share/ferrix/lode-receive`.
#[must_use]
pub fn receive_root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("LODE_RECEIVE_DIR") {
        return Some(PathBuf::from(dir));
    }
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
        })?;
    Some(data.join("ferrix/lode-receive"))
}

/// The state the options ask for, on a viewer whose records are read.
fn settle(mut app: App, options: &Options) -> App {
    if !options.no_remotes
        && let Some(path) = crate::remote::config_path()
    {
        if options.render.is_none() {
            app.load_remotes(path);
        } else {
            // A frame drawn with no window follows nothing.
            app.remotes = std::fs::read_to_string(&path)
                .map(|text| crate::remote::parse(&text))
                .unwrap_or_default();
            app.links = app.remotes.iter().map(|_| None).collect();
        }
    }
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
            if options.import {
                return import(&options);
            }
            if let Some(target) = &options.pull {
                return pull(target, &options);
            }
            if let Some(name) = &options.pair {
                return pair(name, &options);
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

/// `[USER@]HOST[:PORT]` as a machine's settings.
fn target_remote(
    target: &str,
    name: &str,
    kind: crate::remote::Kind,
    key: Option<String>,
) -> crate::remote::Remote {
    let (user, rest) = target.split_once('@').map_or(("", target), |(u, r)| (u, r));
    let (host, port) = rest
        .rsplit_once(':')
        .map_or((rest, "22"), |(host, port)| (host, port));
    crate::remote::Remote {
        name: name.to_owned(),
        kind,
        host: host.to_owned(),
        user: user.to_owned(),
        port: port.parse().unwrap_or(0),
        key,
    }
}

/// `--pair`: what TLS forwarding needs on a machine, shown; done with
/// `--yes`.
fn pair(name: &str, options: &Options) -> ExitCode {
    let (Some(server), Some(on)) = (&options.server, &options.on) else {
        say("err: --pair needs --server-name HOST and --on [USER@]HOST[:PORT]");
        return ExitCode::from(2);
    };
    let remote = target_remote(on, name, crate::remote::Kind::Tls, options.key.clone());
    if let Some(why) = remote.refused() {
        say(&format!("err: --on {on}: {why}"));
        return ExitCode::from(2);
    }
    let root = options.into.clone().or_else(receive_root);
    let plan = match crate::tls::pair(name, server, root.as_ref()) {
        Ok(plan) => plan,
        Err(why) => {
            say(&format!("err: {why}"));
            return ExitCode::FAILURE;
        }
    };
    say(&format!("on {on}, as shown and nothing more:"));
    for command in crate::tls::remote_commands(&plan) {
        say(&format!("  {command}"));
    }
    if !options.yes {
        say("nothing was done: --yes does it");
        return ExitCode::SUCCESS;
    }
    let result = crate::tls::apply(&remote, &plan, &|line| say(&line))
        .and_then(|()| crate::tls::ensure_running(root.as_ref()));
    match result {
        Ok(line) => {
            say(&line);
            ExitCode::SUCCESS
        }
        Err(why) => {
            say(&format!("err: {why}"));
            ExitCode::FAILURE
        }
    }
}

/// `--pull`: one machine followed over ssh, no window.
fn pull(target: &str, options: &Options) -> ExitCode {
    let host_name = target
        .rsplit_once('@')
        .map_or(target, |(_, rest)| rest)
        .split(':')
        .next()
        .unwrap_or(target)
        .to_owned();
    let remote = target_remote(
        target,
        &host_name,
        crate::remote::Kind::Ssh,
        options.key.clone(),
    );
    if let Some(why) = remote.refused() {
        say(&format!("err: --pull {target}: {why}"));
        return ExitCode::from(2);
    }
    let Some(root) = options.into.clone().or_else(receive_root) else {
        say("err: --pull needs --into ROOT, or a home directory");
        return ExitCode::from(2);
    };
    let link = crate::remote::Link::start(remote, root);
    let seconds = if options.seconds == 0 {
        30
    } else {
        options.seconds
    };
    let mut said = String::new();
    for _ in 0..seconds * 2 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let now = link.now();
        let line = format!(
            "{}: {} records{}",
            now.state,
            now.records,
            now.error
                .as_deref()
                .map_or_else(String::new, |error| format!(", last error: {error}"))
        );
        if line != said {
            say(&format!("pull {target}: {line}"));
            said = line;
        }
    }
    let now = link.now();
    link.stop();
    say(&format!(
        "pull {target}: {} records kept in {}",
        now.records,
        now.dir
            .map_or_else(|| "nothing yet".to_owned(), |dir| dir.display().to_string())
    ));
    if now.records > 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// `--import FILE --into DIR`: the files' records written as a store.
fn import(options: &Options) -> ExitCode {
    let Some(into) = &options.into else {
        say("err: --import needs --into DIR");
        return ExitCode::from(2);
    };
    let mut records = Vec::new();
    for file in &options.files {
        match std::fs::read_to_string(file) {
            Ok(text) => records.extend(crate::json::read(&text).records),
            Err(error) => {
                say(&format!("err: {}: {error}", file.display()));
                return ExitCode::FAILURE;
            }
        }
    }
    match crate::store::import(&records, into) {
        Ok(boots) => {
            say(&format!(
                "wrote {} records in {boots} segments to {}",
                records.len(),
                into.display()
            ));
            ExitCode::SUCCESS
        }
        Err(error) => {
            say(&format!("err: {}: {error}", into.display()));
            ExitCode::FAILURE
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
