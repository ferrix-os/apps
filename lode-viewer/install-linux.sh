#!/usr/bin/env bash
# lode-viewer on a Linux desktop: build it for this machine and install it
# for the user who runs this, with its fonts, its icon and a launcher entry.
#
#   bash src/user/apps/lode-viewer/install-linux.sh        # into ~/.local
#   PREFIX=/opt/lode bash .../install-linux.sh              # elsewhere
#
# It builds inside the Ferrix checkout it sits in, with the toolchain that
# checkout pins. It needs a Wayland session (GNOME, KDE Plasma, Hyprland,
# sway): the viewer speaks Wayland only. Started with no arguments, it
# shows the machines lode-receive keeps under
# ~/.local/share/ferrix/lode-receive (or LODE_RECEIVE_DIR).
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
prefix=${PREFIX:-$HOME/.local}
target=${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/lode-viewer/target}

CARGO_TARGET_DIR=$target cargo build --release --manifest-path "$here/Cargo.toml"

install -Dm755 "$target/release/lode-viewer" "$prefix/bin/lode-viewer"
mkdir -p "$prefix/share/lode-viewer/fonts"
install -m644 "$here"/share/fonts/*.ttf "$prefix/share/lode-viewer/fonts/"
install -Dm644 "$here/LICENSE-ibm-plex" "$prefix/share/licenses/lode-viewer/LICENSE-ibm-plex"
install -Dm644 "$here/share/ferrix-lode.svg" \
    "$prefix/share/icons/hicolor/scalable/apps/ferrix-lode.svg"
# The image's entry, started from where this installs the program.
mkdir -p "$prefix/share/applications"
sed "s|^Exec=.*|Exec=$prefix/bin/lode-viewer|" "$here/share/lode-viewer.desktop" \
    > "$prefix/share/applications/lode-viewer.desktop.new"
mv "$prefix/share/applications/lode-viewer.desktop.new" \
    "$prefix/share/applications/lode-viewer.desktop"
if command -v update-desktop-database > /dev/null; then
    update-desktop-database "$prefix/share/applications" || true
fi
echo "lode-viewer: installed $prefix/bin/lode-viewer; start it as \`lode-viewer\` or as Logs in the launcher"
