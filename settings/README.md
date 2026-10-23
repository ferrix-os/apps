# settings

Ferrix's Settings, after the customer's design: a sidebar of four groups --
Desktop, Session, System, Advanced -- and thirteen pages, a search over them,
and the page shown, drawn with tole (Ferrix's
[docs/UI-FRAMEWORK.md](https://github.com/ferrix-os/ferrix/blob/develop/docs/UI-FRAMEWORK.md))
in the design's Figtree and JetBrains Mono, dark first, light too, in one of
seven accents. An app (Ferrix's
[docs/APPS.md](https://github.com/ferrix-os/ferrix/blob/develop/docs/APPS.md)):
`app.toml` says what it installs and how it is built; its licence is `MIT`,
the fonts' the SIL Open Font License (`LICENSE-figtree`,
`LICENSE-jetbrains-mono`).

- **The window** (`ui/*.tole`, `src/ui.rs`): the header with the search and
  who is signed in where; the sidebar, the shown page's group open, a badge
  on a page with changes waiting; the page, its title, then its rows, each a
  kind the markup draws -- a heading with its note, a card with a segmented
  choice, a switch, a slider, a tick box, lines of label and value, a line to
  type in.
- **The search** narrows the sidebar to the pages whose name or words match
  (`blur` finds Appearance, `ssh` Remote login), and says "No setting
  matches" when none does.
- **A note** under a setting says when its change takes effect: "Takes effect
  after you sign in again", after a restart, or "Not kept yet".
- **Game mode** lists every application a launcher lists, each with a switch:
  the user's `~/.config/ferrix/game-mode.conf`, one desktop-file ID a line,
  which the launcher reads when it starts one (Ferrix's
  [docs/SETTINGS.md](https://github.com/ferrix-os/ferrix/blob/develop/docs/SETTINGS.md)).
  Its filter has the keys when the page opens; Return turns the first
  application it leaves.
- **`settings --render PAGE FILE`** draws a page headless, `--light` for the
  light palette; **`settings --list`** prints game mode's list with no window.

```
cargo xtask check                                          # its fmt, clippy and tests
cargo xtask test-apps --arch x86_64                        # a boot that runs its [[smoke]] lines
cargo xtask test-compositor --arch x86_64 --boot settings  # game mode on Ferrix: a switch turned, the mark checked
```
