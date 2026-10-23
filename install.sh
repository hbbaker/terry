#!/usr/bin/env bash
# Set up terry on this machine from this checkout. Safe to run again after a pull.
#
#   ./install.sh              everything; asks before it installs a missing lint tool
#   ./install.sh --yes        install missing lint tools and delta without asking
#   ./install.sh --no-plugin  skip the Claude Code pane plugin
#
# It builds the terry binary (cargo, release), then runs `terry install`, which does the rest:
# see src/install.rs.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

command -v cargo >/dev/null 2>&1 || {
  echo "ERROR: cargo is not installed. Install Rust from https://rustup.rs, then run this again." >&2
  exit 1
}

printf '==> Building terry\n'
cargo build --release --locked --quiet --manifest-path "$REPO/Cargo.toml"
exec "$REPO/target/release/terry" install "$@"
