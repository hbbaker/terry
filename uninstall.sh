#!/usr/bin/env bash
# Undo ./install.sh, so terry can be installed again from scratch.
#
#   ./uninstall.sh           remove terry's settings, plugin, delta include and binary
#   ./uninstall.sh --purge   also remove the exclude list and the saved diffs
#
# It runs `terry uninstall` (see src/install.rs), which also removes the $(brew --prefix)/bin/terry
# link, then removes the binary: the one `cargo install` made in ~/.cargo/bin, and the link
# ~/.local/bin/terry that older installs made. Safe to run again. It keeps the lint tools and delta that install.sh added, and this checkout.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}/bin"
OLD_LINK="$HOME/.local/bin/terry"

find_terry() {
  local candidate
  for candidate in "$(command -v terry || true)" "$BIN_DIR/terry" "$OLD_LINK" "$REPO/target/release/terry"; do
    if [ -n "$candidate" ] && [ -x "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

if TERRY="$(find_terry)"; then
  "$TERRY" uninstall "$@"
else
  echo "terry binary not found; skipping settings, plugin and git config cleanup." >&2
  echo "Run ./install.sh, then ./uninstall.sh, to clean those too." >&2
fi

printf '\n==> Removing the terry binary\n'
if command -v cargo >/dev/null 2>&1 && cargo uninstall terry 2>/dev/null; then
  echo "  removed: cargo-installed terry"
else
  echo "  ok: no cargo-installed terry"
fi
if [ -L "$OLD_LINK" ]; then
  rm "$OLD_LINK"
  echo "  removed: old link $OLD_LINK"
fi
# `terry uninstall` removes the Homebrew link when it runs; when it could not, a link left behind
# now points at nothing. Only a dangling link is removed, so another program's terry stays.
if command -v brew >/dev/null 2>&1; then
  BREW_LINK="$(brew --prefix)/bin/terry"
  if [ -L "$BREW_LINK" ] && [ ! -e "$BREW_LINK" ]; then
    rm "$BREW_LINK"
    echo "  removed: dangling link $BREW_LINK"
  fi
fi

echo
echo "terry is uninstalled. Run ./install.sh to install it again."
