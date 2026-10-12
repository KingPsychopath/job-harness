#!/bin/sh
# Source installation. No network bootstrap, credentials, root or public package required.
set -eu
prefix="${HOME}/.local"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --prefix) [ "$#" -ge 2 ] || { printf '%s\n' 'Missing --prefix PATH' >&2; exit 2; }; prefix=$2; shift 2 ;;
    --help|-h) printf '%s\n' 'Usage: ./scripts/install.sh [--prefix PATH]' 'Requires an installed Rust toolchain; installs job-harness from this checkout.'; exit 0 ;;
    *) printf '%s\n' 'Unknown option. Use --help.' >&2; exit 2 ;;
  esac
done
command -v cargo >/dev/null 2>&1 || { printf '%s\n' 'Install Rust from https://rustup.rs, then run this script again.' >&2; exit 1; }
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
[ -f "$repo/Cargo.lock" ] || { printf '%s\n' 'Run this from a complete Job Harness checkout with Cargo.lock.' >&2; exit 1; }
cargo install --locked --path "$repo/crates/harness-cli" --root "$prefix"
printf '%s\n' "Installed: $prefix/bin/job-harness" 'Add its bin directory to PATH, then run job-harness --help.'
