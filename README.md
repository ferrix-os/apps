# Ferrix apps

> A component of [Ferrix](https://github.com/ferrix-os/ferrix), checked out at `src/user/apps` (its `components.toml`); build and test it from there with `cargo xtask`. Ferrix's [conventions](https://github.com/ferrix-os/ferrix/blob/main/docs/CONVENTIONS.md) apply, including one author per commit.

The programs a person starts on Ferrix, one folder each, described by its
`app.toml` ([docs/APPS.md](https://github.com/ferrix-os/ferrix/blob/main/docs/APPS.md)).
xtask finds every folder here and builds, gates, packages and installs it by
what its `app.toml` says; `cargo xtask apps` lists them.

<table>
  <tr>
    <td width="33%"><a href="term/"><img src="term/screenshot.png" alt="term on Ferrix"></a><br><b><a href="term/">term</a></b></td>
    <td width="33%"><a href="foot/"><img src="foot/screenshot.png" alt="foot on Ferrix"></a><br><b><a href="foot/">foot</a></b></td>
    <td width="33%"><a href="waybar/"><img src="waybar/screenshot.png" alt="waybar on Ferrix"></a><br><b><a href="waybar/">waybar</a></b></td>
  </tr>
  <tr>
    <td width="33%"><a href="fuzzel/"><img src="fuzzel/screenshot.png" alt="fuzzel on Ferrix"></a><br><b><a href="fuzzel/">fuzzel</a></b></td>
    <td width="33%"><a href="hyprlock/"><img src="hyprlock/screenshot.png" alt="hyprlock on Ferrix"></a><br><b><a href="hyprlock/">hyprlock</a></b></td>
    <td width="33%"><a href="hypridle/"><img src="hypridle/screenshot.png" alt="hypridle on Ferrix"></a><br><b><a href="hypridle/">hypridle</a></b></td>
  </tr>
  <tr>
    <td width="33%"><a href="ferrofetch/"><img src="ferrofetch/screenshot.png" alt="ferrofetch on Ferrix"></a><br><b><a href="ferrofetch/">ferrofetch</a></b></td>
    <td width="33%"><a href="btop/"><img src="btop/screenshot.png" alt="btop on Ferrix"></a><br><b><a href="btop/">btop</a></b></td>
    <td width="33%"><a href="vkgears/"><img src="vkgears/screenshot.png" alt="vkgears on Ferrix"></a><br><b><a href="vkgears/">vkgears</a></b></td>
  </tr>
  <tr>
    <td width="33%"><a href="curl/"><img src="curl/screenshot.png" alt="curl on Ferrix"></a><br><b><a href="curl/">curl</a></b></td>
    <td width="33%"><a href="git/"><img src="git/screenshot.png" alt="git on Ferrix"></a><br><b><a href="git/">git</a></b></td>
    <td width="33%"><a href="statd/"><img src="statd/screenshot.png" alt="statd on Ferrix"></a><br><b><a href="statd/">statd</a></b></td>
  </tr>
  <tr>
    <td width="33%"><a href="alsa-utils/"><img src="alsa-utils/screenshot.png" alt="alsa-utils on Ferrix"></a><br><b><a href="alsa-utils/">alsa-utils</a></b></td>
    <td width="33%"><a href="alsa-lib/"><img src="alsa-lib/screenshot.png" alt="alsa-lib on Ferrix"></a><br><b><a href="alsa-lib/">alsa-lib</a></b></td>
    <td width="33%"><a href="sshdt/"><img src="sshdt/screenshot.png" alt="sshdt on Ferrix"></a><br><b><a href="sshdt/">sshdt</a></b></td>
  </tr>
  <tr>
    <td width="33%"><a href="badapple/"><img src="badapple/screenshot.png" alt="badapple on Ferrix"></a><br><b><a href="badapple/">badapple</a></b></td>
  </tr>
</table>

*Each app's own `screenshot.png`, on Ferrix (x86-64 under KVM): the desktop's clients and foot at main def906ba2 (2026-10-04), the others at 2f067e40f and badapple at fee29d168 (2026-10-03).*

- **The desktop's:** `term`, `waybar`, `fuzzel`, `hyprlock`, `hypridle`, which
  every desktop carries.
- **Ferrix's own:** `badapple`, `ferrofetch`, `statd`.
- **Ported onto ferrousli**, Ferrix's C library, with its `tools/ports/`:
  `alsa-lib`, `alsa-utils`, `btop`, `curl`, `foot`, `git`, `sshdt`, `vkgears`.

A folder depends on Ferrix's crates by relative path from where this repository
is checked out, so it builds inside a Ferrix checkout, not alone.
