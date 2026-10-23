# lode-viewer

lode's log in a desktop window: a boot at a glance, and every record, read and queried with lode's own core. An app (Ferrix's [docs/APPS.md](https://github.com/ferrix-os/ferrix/blob/main/docs/APPS.md)): `app.toml` says what it installs and how it is built; its licence is `MIT`, and the IBM Plex faces it carries are `OFL-1.1` (`LICENSE-ibm-plex`). The records, their fields and lode's rules are Ferrix's [docs/LOGGING.md](https://github.com/ferrix-os/ferrix/blob/main/docs/LOGGING.md); the look is the customer's design (`Lode App.dc.html`), its palette and faces kept.

![The log viewer on Ferrix: the Logs page after `id=msr` was typed, 13 of 801 records, a run of nine folded into one line](screenshot.png)

*The Logs page after `/`, `id=msr` and Return were typed into it: 13 of the 801 test records, a run of nine alike but for their numbers folded into one line. On Ferrix (x86-64 under KVM, drawn on the CPU), the screen `cargo xtask test-compositor --boot lode-viewer` keeps, 2026-10-23; nothing edited after capture.*

- **Overview**: the boot picker and Live; a time track with the window's brush, the problem before and after, −10s/+10s and zooms, over swim lanes (the kernel, the two most troubled sources, the rest) marked where errors, warnings and held lines were; tiles for errors, warnings, lines a second and lines held, each with its line across the boot; the activity per 10 s (grey info, amber warning, red error) with the boot before beside it on request; the log rate against the limiter; the busiest sources; and the window's lines, warnings and worse first, with the way to all of them in Logs.
- **Logs**: a query in lode's filter language (`where priority <= warning or id = "hyprix*"`), saved queries (`@triage`, `@gpu`, `+ new`, kept in `~/.config/ferrix/lode/viewer-queries.conf`), chips and the count of what matches; facets with counts for the level (unspecified among them), source, transport and session; the table, where lode's word that the limiter held lines is a line of its own that expands into what it says, a run of lines alike but for their numbers folds into one (lode's own "message repeated" records joining the run they repeat), lode starting or losing records is a marker, and a level a display rule set has its note; a strip of where the problems are; and a record's every field, with ±30 s around it, its source alone, and Copy as JSON or its cursor to the clipboard.
- **Status**: at the sidebar's foot, whether lode runs, what the store takes, its tier and budget.
- **Display rules.** A record a lode from before the field-quality landing stored as info, or nobody gave a level, is shown at the level its words say, by lode's own reading of them (`ingest::guess_level`), and says so: "Stored as info; text says “refused”. Shown as warning by a display rule." The filter language and the facets see that level; Copy as JSON gives the record as stored.

```
lode-viewer                         lode's store (/var/log/journal, /run/log/journal), as the person running it may read it
lode-viewer FILE.jsonl ...          a JSONL export: lode -o json, or what cargo xtask box-log keeps
lode-viewer -D DIR                  a store in another directory
  --page logs --where 'EXPR' --boot -1 --zoom 0 --at 17:22:43 --select SEQNUM --theme dark --live
lode-viewer FILE.jsonl --render out.png [--size 1440x1000] [--scale 2]
                                    one frame into a PNG with no compositor: how it is shown on a host
lode-viewer FILE.jsonl --bench 100000
                                    the file grown to that many records, each step timed
```

**On a Linux desktop** (nazuna's): `bash src/user/apps/lode-viewer/install-linux.sh` in a Ferrix checkout builds it for the host and installs it into `~/.local` (`PREFIX` for elsewhere): the program in `bin/`, the fonts in `share/lode-viewer/fonts/`, the icon, and "Logs" in the launcher. It speaks Wayland only, so it needs a Wayland session -- GNOME (mutter), KDE Plasma, Hyprland or sway; it opened on nazuna's GNOME session. On an X11-only desktop it says so and stops; `--render` still draws a frame there.

**Several machines.** With no store of lode's on the machine and no file named, it opens the machines `lode-receive` keeps under `~/.local/share/ferrix/lode-receive` (`LODE_RECEIVE_DIR`, or `--machines DIR`): each directory there (`<machine-id>/`, or `host-<hostname>/`; dot-names skipped) is one machine's store, labelled from its `machine` file (`hostname=`, `last_seen=`). A machine picker beside the boot picker switches between them, Live follows the one shown from the start, and a machine that begins sending appears within ten seconds. `lode-viewer --import FILE.jsonl --into DIR` writes an export (a box log) as such a store, a segment a boot with its summary.

**Adding a machine** (the Machines page, or "Add machine…" in the machine picker), kept in `~/.config/ferrix/lode/machines.conf`, keys only -- no password is asked or kept; ssh runs with `BatchMode=yes`, `IdentitiesOnly=yes` when a key is named, and trusts a machine met for the first time (`StrictHostKeyChecking=accept-new`):

- **Live over SSH**: the viewer runs `lode -f -o json` on the machine over ssh, as `cargo xtask box-log` does, and keeps what comes in `<root>/host-<hostname>/` (or the machine id's directory), so the history stays when the link drops; a dropped link is made again every few seconds from the store's last record (`--after-cursor`), nothing lost or repeated. The page shows each link's state, its records, when the last came and its last error. `lode-viewer --pull [USER@]HOST[:PORT] [--key FILE] [--into ROOT] [--seconds N]` is the same link with no window.
- **Forwarded over TLS**, once `lode-receive` is installed here (until then the option is shown disabled): `lode-receive pair` makes the certificates and says what the machine needs; the viewer shows every command it will run on the machine -- the files copied with their modes, the line put in `/etc/ferrix/lode-upload.conf` in place of the machine's old one, the restart of `lode-upload.service` -- and does them over ssh as root only on "Apply on the machine", then checks `lode-receive` here and installs it as the user's service when it does not run. `lode-viewer --pair NAME --server-name HOST --on [USER@]HOST[:PORT] [--key FILE] [--yes]` is the same with no window: it shows, and with `--yes` does.

Keys: `/` types into the query, Return runs it, Escape leaves it; Up and Down (or `j`, `k`) move the selection; Page Up and Down scroll; Left and Right move the window 10 s; `o` and `l` the pages; `t` the palette; Ctrl+Q closes. The wheel scrolls, and over the track it zooms.

**How it is built.** The store is read with lode's own core, `ferrix-lode-core`: the segment format and its scan, the summaries, the records, the filter language, the JSON output. Live follows the newest segments from where the last read stopped, as `lode -f` does. The window is the desktop's own Wayland client runtime, `compositor-toolkit` (an xdg-shell toplevel, shared-memory buffers, the keyboard, the pointer, the clipboard through `ext_data_control`), drawn on the CPU with tiny-skia and `compositor-text`, so it needs no GPU and no C library: a static program of 3.4 MB on x86-64. Every frame is drawn whole from the state; the table draws only the lines in view, so 100,000 records cost a few milliseconds a frame.

```
cargo xtask check                                             # its fmt, clippy and tests
cargo xtask test-apps --arch x86_64                           # a boot that runs its [[smoke]] line
cargo xtask test-compositor --arch x86_64 --boot lode-viewer  # the desktop, the window, a query typed into it
```

The tests read `testdata/n150-boot-excerpt.jsonl`: 801 records of the N150's first boot under lode, from before lode read levels itself, with its private addresses replaced by documentation ones.

**Not yet**: the design's Timeline, Sources, Crashes (the sidebar's Crashes opens Logs on them), Boots and Rules & alerts pages; dragging the brush's edges (a press or a drag on the track moves the window); Export…; a merged view of several machines (they are shown one at a time); a link's ssh asking for a host key's change to be accepted (it is refused, as ssh refuses it); moving the window by its own header where a compositor draws no title bar.
