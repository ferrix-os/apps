//! The time zones offered: every zone file under `/usr/share/zoneinfo`,
//! by name, less the copies and the old names.

use std::io::Read as _;
use std::path::Path;

/// Where the zone files are.
pub const ZONEINFO: &str = "/usr/share/zoneinfo";

/// The default zone.
pub const DEFAULT: &str = "UTC";

/// Top-level folders of old names or copies of the others, skipped.
const SKIPPED: [&str; 9] = [
    "posix", "right", "Etc", "US", "Canada", "Brazil", "Chile", "Mexico", "SystemV",
];

/// Every zone under `root`, sorted, `UTC` among them: a file is a zone
/// when it starts with `TZif`. A top-level file is an old name (`EST`,
/// `Japan`) and is skipped, `UTC` aside, which is always offered.
#[must_use]
pub fn read(root: &Path) -> Vec<String> {
    let mut zones = vec![DEFAULT.to_owned()];
    walk(root, "", &mut zones);
    zones.sort();
    zones.dedup();
    zones
}

/// The zones under `dir`, named from `prefix`.
fn walk(dir: &Path, prefix: &str, zones: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let top = prefix.is_empty();
        if path.is_dir() {
            if top && SKIPPED.contains(&name.as_str()) {
                continue;
            }
            walk(&path, &format!("{prefix}{name}/"), zones);
        } else if !top && is_zone(&path) {
            zones.push(format!("{prefix}{name}"));
        }
    }
}

/// Whether the file at `path` starts with `TZif`.
fn is_zone(path: &Path) -> bool {
    let mut head = [0_u8; 4];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut head))
        .is_ok()
        && &head == b"TZif"
}

/// The zones of `zones` whose name holds `query`, case and the
/// underscore aside: `new york` finds `America/New_York`.
#[must_use]
pub fn filter<'a>(zones: &'a [String], query: &str) -> Vec<&'a String> {
    let fold = |text: &str| text.to_lowercase().replace('_', " ");
    let query = fold(query.trim());
    zones
        .iter()
        .filter(|zone| query.is_empty() || fold(zone).contains(&query))
        .collect()
}

/// A few zones, for drawings with no zone files.
#[must_use]
pub fn sample() -> Vec<String> {
    [
        "Africa/Cairo",
        "Africa/Johannesburg",
        "America/Chicago",
        "America/Los_Angeles",
        "America/New_York",
        "America/Sao_Paulo",
        "Asia/Kolkata",
        "Asia/Shanghai",
        "Asia/Tokyo",
        "Australia/Sydney",
        "Europe/Berlin",
        "Europe/London",
        "Europe/Paris",
        "Europe/Vienna",
        "Pacific/Auckland",
        "UTC",
    ]
    .iter()
    .map(|zone| (*zone).to_owned())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::{filter, read};

    #[test]
    fn zones_are_tzif_files_below_the_top_less_copies_and_old_names() {
        let dir = std::env::temp_dir().join(format!("installer-zones-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, body) in [
            ("Europe/Vienna", "TZif2..."),
            ("America/Argentina/Salta", "TZif2..."),
            ("America/New_York", "TZif2..."),
            ("posix/Europe/Vienna", "TZif2..."),
            ("right/UTC", "TZif2..."),
            ("US/Eastern", "TZif2..."),
            ("Japan", "TZif2..."),
            ("UTC", "TZif2..."),
            ("Europe/README", "not a zone"),
            ("zone.tab", "# a table"),
        ] {
            let file = dir.join(path);
            let _ = std::fs::create_dir_all(file.parent().expect("a folder"));
            let _ = std::fs::write(file, body);
        }
        let zones = read(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            zones,
            [
                "America/Argentina/Salta",
                "America/New_York",
                "Europe/Vienna",
                "UTC"
            ]
        );
        assert_eq!(filter(&zones, "new york"), [&zones[1]]);
        assert_eq!(filter(&zones, "").len(), 4);
    }

    #[test]
    fn no_zone_files_still_offer_utc() {
        assert_eq!(read(std::path::Path::new("/nonexistent/zoneinfo")), ["UTC"]);
    }
}
