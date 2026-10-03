# Ferrix apps

> A component of [Ferrix](https://github.com/ferrix-os/ferrix), checked out at `src/user/apps` (its `components.toml`); build and test it from there with `cargo xtask`. Ferrix's [conventions](https://github.com/ferrix-os/ferrix/blob/main/docs/CONVENTIONS.md) apply, including one author per commit.

The programs a person starts on Ferrix, one folder each, described by its
`app.toml` ([docs/APPS.md](https://github.com/ferrix-os/ferrix/blob/main/docs/APPS.md)).
xtask finds every folder here and builds, gates, packages and installs it by
what its `app.toml` says; `cargo xtask apps` lists them.

<table>
  <tr>
    <td width="33%"><img src="https://raw.githubusercontent.com/ferrix-os/ferrix/main/docs/brand/screenshots/foot.png" alt="foot running zinc and ferrofetch on Ferrix"></td>
    <td width="33%"><img src="https://raw.githubusercontent.com/ferrix-os/ferrix/main/docs/brand/screenshots/btop.png" alt="btop monitoring a Ferrix system"></td>
    <td width="33%"><img src="https://raw.githubusercontent.com/ferrix-os/ferrix/main/docs/brand/screenshots/vkgears.png" alt="vkgears drawing Vulkan gears on Ferrix"></td>
  </tr>
  <tr>
    <td>foot, running zinc and <code>ferrofetch</code>.</td>
    <td>btop, watching Chrome and Ferrix's drivers.</td>
    <td>vkgears, drawing through Vulkan and Venus.</td>
  </tr>
</table>

- **Ferrix's own:** `badapple`, `ferrofetch`, `statd`.
- **Ported onto ferrousli**, Ferrix's C library, with its `tools/ports/`:
  `alsa-lib`, `alsa-utils`, `btop`, `curl`, `foot`, `git`, `sshdt`, `vkgears`.

A folder depends on Ferrix's crates by relative path from where this repository
is checked out, so it builds inside a Ferrix checkout, not alone.
