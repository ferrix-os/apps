#!/usr/bin/env bash
# shelf on a Linux desktop: build it for this machine and install it for
# the user who runs this, with its icon and a launcher entry.
#
#   bash src/user/apps/shelf/install-linux.sh        # into ~/.local
#   PREFIX=/opt/shelf bash .../install-linux.sh       # elsewhere
#
# It builds inside the Ferrix checkout it sits in, with the toolchain that
# checkout pins. It needs a Wayland session (GNOME, KDE Plasma, Hyprland,
# sway): shelf speaks Wayland only.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
prefix=${PREFIX:-$HOME/.local}
target=${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/shelf/target}

# rustup takes the toolchain from the directory cargo runs in, not from
# --manifest-path: build from the app's folder, under the checkout's
# rust-toolchain.toml, wherever this was started from.
(cd "$here" && CARGO_TARGET_DIR=$target cargo build --release)

install -Dm755 "$target/release/shelf" "$prefix/bin/shelf"
install -Dm644 "$here/share/ferrix-shelf.svg" \
    "$prefix/share/icons/hicolor/scalable/apps/ferrix-shelf.svg"
mkdir -p "$prefix/share/applications"
sed "s|^Exec=.*|Exec=$prefix/bin/shelf %U|" "$here/share/shelf.desktop" \
    > "$prefix/share/applications/shelf.desktop.new"
mv "$prefix/share/applications/shelf.desktop.new" \
    "$prefix/share/applications/shelf.desktop"
if command -v update-desktop-database > /dev/null; then
    update-desktop-database "$prefix/share/applications" || true
fi
echo "shelf: installed $prefix/bin/shelf; start it as \`shelf\` or as Files in the launcher"
