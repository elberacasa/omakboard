#!/usr/bin/env bash
# Install or update Omakboard's command for the current user.
#
# After `omarchy plugin add https://github.com/elberacasa/omakboard --enable`, the bar widget
# offers this script when the command is missing or out of date. To run it yourself:
#   bash ~/.config/omarchy/plugins/io.github.elberacasa.omakboard/install.sh
#
# Downloads the release binary from this repository's GitHub releases, checks its SHA-256,
# installs it to ~/.local/bin, installs the udev rule that lets your user reach the
# keyboard's configuration interface (sudo, once), and runs `omakboard doctor`.
#
# OMAKBOARD_VERSION=v0.1.0 pins a release; OMAKBOARD_BIN_DIR changes where the binary goes.
set -euo pipefail

VERSION=${OMAKBOARD_VERSION:-latest}
BIN_DIR=${OMAKBOARD_BIN_DIR:-$HOME/.local/bin}
RULE=/etc/udev/rules.d/70-omakboard.rules

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
die() { printf 'omakboard install: %s\n' "$*" >&2; exit 1; }

[ "$(id -u)" -ne 0 ] || die "run this as your user, not root; it asks for sudo once, for the udev rule"
[ "$(uname -s)" = Linux ] || die "Omakboard runs on Linux"
case "$(uname -m)" in
  x86_64) target=x86_64-unknown-linux-gnu ;;
  *) die "there is no prebuilt binary for $(uname -m) yet; install from the AUR (omakboard) or with cargo" ;;
esac
for tool in curl tar sha256sum install cmp; do
  command -v "$tool" >/dev/null 2>&1 || die "needs $tool"
done

if [ "$VERSION" = latest ]; then
  base="https://github.com/elberacasa/omakboard/releases/latest/download"
else
  base="https://github.com/elberacasa/omakboard/releases/download/$VERSION"
fi
name="omakboard-$target"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

say "Downloading $name.tar.gz"
curl -fsSL --proto '=https' --tlsv1.2 --retry 3 --retry-delay 2 -o "$tmp/$name.tar.gz" "$base/$name.tar.gz"
curl -fsSL --proto '=https' --tlsv1.2 --retry 3 --retry-delay 2 -o "$tmp/$name.tar.gz.sha256" "$base/$name.tar.gz.sha256"
(cd "$tmp" && sha256sum --check --status "$name.tar.gz.sha256") \
  || die "the download does not match its checksum; nothing was installed"
tar -xzf "$tmp/$name.tar.gz" -C "$tmp"

install -Dm755 "$tmp/$name/omakboard" "$BIN_DIR/omakboard"
say "Installed $("$BIN_DIR/omakboard" --version) to $BIN_DIR"

if [ -f /usr/lib/udev/rules.d/70-omakboard.rules ]; then
  say "The udev rule is already installed by a package"
elif ! cmp -s "$tmp/$name/70-omakboard.rules" "$RULE"; then
  say "Installing the udev rule, so your user can reach the keyboard's settings without root (sudo)"
  sudo install -Dm644 "$tmp/$name/70-omakboard.rules" "$RULE"
  sudo udevadm control --reload-rules
  sudo udevadm trigger --subsystem-match=hidraw --action=change
fi

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) say "Add $BIN_DIR to your PATH to run omakboard by name" ;;
esac

echo
# doctor exits non-zero while something still needs fixing, such as an unplugged keyboard.
"$BIN_DIR/omakboard" doctor || true
