#!/bin/sh
# Baut sunxi-fel (linux-sunxi/sunxi-tools) unter macOS mit Homebrew.
# Ergebnis: ./tools/sunxi-fel
set -eu
cd "$(dirname "$0")/.."

command -v brew >/dev/null || { echo "Homebrew wird benoetigt: https://brew.sh" >&2; exit 1; }
brew install libusb pkg-config dtc zstd

mkdir -p tools work
if [ ! -d work/sunxi-tools/.git ]; then
  git clone --depth 1 https://github.com/linux-sunxi/sunxi-tools work/sunxi-tools
fi

BREW_PREFIX="$(brew --prefix)"
PKG_CONFIG_PATH="$BREW_PREFIX/lib/pkgconfig" \
CFLAGS="-I$BREW_PREFIX/include" \
LDFLAGS="-L$BREW_PREFIX/lib" \
  make -C work/sunxi-tools sunxi-fel

cp work/sunxi-tools/sunxi-fel tools/sunxi-fel
echo "Fertig: tools/sunxi-fel"
./tools/sunxi-fel 2>&1 | head -n 1 || true
