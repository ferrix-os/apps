//! The home folder shelf's screenshots and tests are drawn on: the
//! customer's design's folders (machine keys, .ssh, pictures, documents
//! with an archive, downloads), made under the temporary folder at run
//! time with set ages, the clock fixed, so a drawing does not change with
//! the day or the machine. The private keys only look like keys: no file
//! shaped like one is kept in the tree.

use std::path::{Path, PathBuf};

use crate::app::{Action, App, Disk, Loc};
use crate::store::{Quick, Settings, Tags};

/// The clock the drawings are made at: 2026-10-24, noon UTC.
pub const NOW: i64 = 1_792_843_200;

/// A tar of `files` (path, contents), for tests and the demo's home.
#[must_use]
pub fn tar_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    // Write `bytes` at `at`, where they fit.
    fn put(header: &mut [u8; 512], at: usize, bytes: &[u8]) {
        if let Some(place) = header.get_mut(at..at + bytes.len()) {
            place.copy_from_slice(bytes);
        }
    }
    for (path, body) in files {
        let mut header = [0_u8; 512];
        put(&mut header, 0, path.as_bytes());
        put(&mut header, 100, b"0000644");
        put(&mut header, 124, format!("{:011o}", body.len()).as_bytes());
        put(&mut header, 136, b"15000000000");
        put(&mut header, 156, b"0");
        put(&mut header, 257, b"ustar");
        put(&mut header, 148, b"        ");
        let sum: u32 = header.iter().map(|&b| u32::from(b)).sum();
        put(&mut header, 148, format!("{sum:06o}\0").as_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(body);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }
    out.resize(out.len() + 1024, 0);
    out
}

/// A zip of `files`, stored or deflated.
#[must_use]
pub fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (i, (path, body)) in files.iter().enumerate() {
        let deflate = i % 2 == 1;
        let data = if deflate {
            miniz_oxide::deflate::compress_to_vec(body, 6)
        } else {
            body.to_vec()
        };
        let local = out.len() as u32;
        let method: u16 = if deflate { 8 } else { 0 };
        let mut head = Vec::new();
        head.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04, 20, 0, 0, 0]);
        head.extend_from_slice(&method.to_le_bytes());
        head.extend_from_slice(&[0, 0, 0x21, 0x5b]); // time, date (2025)
        head.extend_from_slice(&0_u32.to_le_bytes()); // crc (not checked)
        head.extend_from_slice(&(data.len() as u32).to_le_bytes());
        head.extend_from_slice(&(body.len() as u32).to_le_bytes());
        head.extend_from_slice(&(path.len() as u16).to_le_bytes());
        head.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&head);
        out.extend_from_slice(path.as_bytes());
        out.extend_from_slice(&data);
        central.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02, 20, 0, 20, 0, 0, 0]);
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&[0, 0, 0x21, 0x5b]);
        central.extend_from_slice(&0_u32.to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(body.len() as u32).to_le_bytes());
        central.extend_from_slice(&(path.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 12]);
        central.extend_from_slice(&local.to_le_bytes());
        central.extend_from_slice(path.as_bytes());
    }
    let at = out.len() as u32;
    let n = files.len() as u16;
    out.extend_from_slice(&central);
    out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&at.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

/// A picture `w` by `h`: a gradient from `a` to `b` with a light disc, as
/// a PNG file's bytes.
#[must_use]
pub fn picture(w: u32, h: u32, a: [u8; 3], b: [u8; 3]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let t = (x + y) as f32 / (w + h) as f32;
            let (dx, dy) = (x as f32 - w as f32 * 0.7, y as f32 - h as f32 * 0.35);
            let sun = (dx * dx + dy * dy).sqrt() < h as f32 * 0.16;
            for (&from, &to) in a.iter().zip(b.iter()) {
                let (from, to) = (f32::from(from), f32::from(to));
                rgba.push(if sun {
                    250
                } else {
                    (from + (to - from) * t) as u8
                });
            }
            rgba.push(255);
        }
    }
    tole::png::encode(w, h, &rgba)
}

/// A file shaped like a private key, with nothing secret in it.
fn private(kind: &str, lines: usize) -> Vec<u8> {
    let mut text = format!("-----BEGIN {kind} PRIVATE KEY-----\n");
    for _ in 0..lines {
        text.push_str("not-a-real-key-not-a-real-key-not-a-real-key-not-a-real-key-not-\n");
    }
    text.push_str(&format!("-----END {kind} PRIVATE KEY-----\n"));
    text.into_bytes()
}

fn public(who: &str) -> Vec<u8> {
    format!(
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIK7v3pQm9yX2cF1nB8dLhT0sW4eRjZ6aUo5gNiVkY2Mx {who}\n"
    )
    .into_bytes()
}

/// Make `path` changed `age` seconds before [`NOW`].
fn set_age(path: &Path, age: i64) {
    let when = std::time::UNIX_EPOCH
        + std::time::Duration::from_secs(u64::try_from(NOW - age).unwrap_or(0));
    if let Ok(file) = std::fs::File::open(path) {
        let _ = file.set_modified(when);
    }
}

/// Make the demo's home under `home`, which is emptied first.
pub fn make_home(home: &Path) {
    let _ = std::fs::remove_dir_all(home);
    let day = 86_400;
    let gz = |tar: Vec<u8>| {
        let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3];
        out.extend(miniz_oxide::deflate::compress_to_vec(&tar, 6));
        out.extend_from_slice(&[0; 8]);
        out
    };
    let files: Vec<(&str, Vec<u8>, i64)> = vec![
        ("machines/keys/id_ed25519", private("OPENSSH", 5), 3600),
        ("machines/keys/id_ed25519.pub", public("deploy@workstation"), 3600),
        ("machines/keys/staging_rsa", private("OPENSSH", 40), 41 * day),
        ("machines/keys/staging_rsa.pub", public("staging@workstation"), 41 * day),
        ("machines/keys/archive/old_prod.pem", private("RSA", 25), 234 * day),
        (
            "machines/keys/keys-backup-2025.zip",
            zip_of(&[("id_rsa_old", &private("OPENSSH", 40)), ("id_rsa_old.pub", &public("old@workstation")), ("README.txt", b"Keys retired in 2025.\n")]),
            120 * day,
        ),
        ("machines/keys/.known_hosts", b"web-01 ssh-ed25519 AAAA\n".repeat(50), 3600),
        ("machines/configs/web-01.toml", b"[host]\nname = \"web-01\"\naddress = \"10.0.4.21\"\n".to_vec(), 21 * day),
        ("machines/configs/db-02.toml", b"[host]\nname = \"db-02\"\naddress = \"10.0.4.22\"\n".to_vec(), 21 * day),
        ("machines/inventory.yaml", b"machines:\n  - web-01\n  - db-02\n".repeat(60), 3 * day),
        (".ssh/id_rsa", private("OPENSSH", 40), 290 * day),
        (".ssh/id_rsa.pub", public("me@workstation"), 290 * day),
        (".ssh/config", b"Host web-01\n  HostName 10.0.4.21\n  User deploy\n  IdentityFile ~/machines/keys/id_ed25519\n".to_vec(), 6 * day),
        (".ssh/known_hosts", b"web-01 ssh-ed25519 AAAA\n".repeat(400), 3600),
        ("Pictures/harbour-dusk.png", picture(480, 300, [233, 120, 92], [44, 62, 120]), 3600),
        ("Pictures/desk-plant.png", picture(400, 400, [126, 166, 245], [47, 163, 90]), day),
        ("Pictures/Screenshots/shot-0912.png", picture(320, 200, [40, 44, 52], [120, 130, 150]), 42 * day),
        ("Pictures/Screenshots/shot-1003.png", picture(320, 200, [52, 40, 60], [150, 120, 160]), 21 * day),
        ("Pictures/Screenshots/shot-1022.png", picture(320, 200, [30, 60, 50], [110, 170, 140]), 2 * day),
        ("Documents/notes.md", b"# Notes\n\nThe staging key rotates on the first.\n".to_vec(), 9 * day),
        ("Documents/report-q3.pdf", b"%PDF-1.4\n%not a real report\n".repeat(900), 30 * day),
        (
            "Documents/design-assets.tar.gz",
            gz(tar_of(&[("logo.svg", b"<svg xmlns='http://www.w3.org/2000/svg'/>\n"), ("brand-guide.txt", &b"Use the accent sparingly.\n".repeat(400)), ("Fonts/Display.otf", &[7_u8; 4000]), ("Fonts/Text.otf", &[9_u8; 3500])])),
            14 * day,
        ),
        ("Downloads/deploy-key.pem", private("RSA", 25), day),
        ("Downloads/invoice.pdf", b"%PDF-1.4\n%not a real invoice\n".repeat(2800), 15 * day),
        ("Downloads/project-export.7z", [0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c].repeat(9000), 4 * day),
        ("Downloads/demo-recording.mp4", vec![0_u8; 240_000], 6 * day),
    ];
    for (name, bytes, age) in &files {
        let path = home.join(name);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, bytes);
        set_age(&path, *age);
    }
    for dir in [
        "machines/keys/archive",
        "machines/keys",
        "machines/configs",
        "machines",
        ".ssh",
        "Pictures/Screenshots",
        "Pictures",
        "Documents",
        "Downloads",
    ] {
        set_age(&home.join(dir), 2 * day);
    }
}

/// The demo's home for this process, made once.
#[must_use]
pub fn home() -> PathBuf {
    static MADE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    MADE.get_or_init(|| {
        let home = std::env::temp_dir().join(format!("shelf-demo-home-{}", std::process::id()));
        make_home(&home);
        home
    })
    .clone()
}

/// shelf as the design shows it, on the demo's home: two tabs, the first
/// with two panes (Pictures with a picture chosen, Downloads), the second
/// in the machine keys; the design's tags and pins; `dark` or light.
#[must_use]
pub fn app(dark: bool) -> App {
    let home = home();
    tole::fix_home_and_clock(Some(home.clone()), Some(NOW));
    let mut tags = Tags::in_memory();
    for (rel, tag) in [
        ("machines/keys/id_ed25519", "prod"),
        ("machines/keys/staging_rsa", "staging"),
        ("machines/configs/web-01.toml", "prod"),
        ("machines/inventory.yaml", "prod"),
        ("machines/inventory.yaml", "todo"),
        ("Pictures/harbour-dusk.png", "wallpaper"),
        ("Pictures/desk-plant.png", "wallpaper"),
        ("Pictures/desk-plant.png", "todo"),
        ("Documents/notes.md", "todo"),
        ("Downloads/deploy-key.pem", "prod"),
        ("Downloads/demo-recording.mp4", "todo"),
    ] {
        tags.set(&home.join(rel), tag, true);
    }
    let quick = Quick::in_memory(vec![
        home.join("machines/keys"),
        home.join("Documents/notes.md"),
    ]);
    let mut settings = Settings::in_memory();
    settings.dark = dark;
    let mut app = App::new(Some(home.join("Pictures")), settings, tags, quick, false);
    app.disk = Disk {
        total: 512 << 30,
        used: 182 << 30,
    };
    app.trash = Vec::new();
    app.act(Action::NewTab);
    app.act(Action::Go(Loc::Dir(home.join("machines/keys"))));
    app.act(Action::Tab(0));
    app.act(Action::ToggleDual);
    app.act(Action::Focus(1));
    app.act(Action::Go(Loc::Dir(home.join("Downloads"))));
    app.act(Action::Focus(0));
    let at = app
        .items(0)
        .unwrap_or_default()
        .iter()
        .position(|i| i.name == "harbour-dusk.png")
        .unwrap_or(0);
    app.act(Action::Click(0, at, false, false));
    app
}
