#!/usr/bin/env bash
# Set up terry on this machine from this checkout. Safe to run again after a pull.
#
#   ./install.sh              everything; asks before it installs a missing lint tool
#   ./install.sh --yes        install missing lint tools and delta without asking
#   ./install.sh --no-plugin  skip the Claude Code pane plugin
#
# It installs the terry binary with `cargo install`, into ~/.cargo/bin (a folder rustup puts on
# PATH, so the pane and `terry exclude` find `terry` by name), then runs `terry install`, which
# does the rest: see src/install.rs. To undo it all, run ./uninstall.sh.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

command -v cargo >/dev/null 2>&1 || {
  echo "ERROR: cargo is not installed. Install Rust from https://rustup.rs, then run this again." >&2
  exit 1
}

printf '==> Installing the terry binary with cargo\n'
cargo install --path "$REPO" --locked --force --quiet
BIN_DIR="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}/bin"
exec "$BIN_DIR/terry" install "$@"
