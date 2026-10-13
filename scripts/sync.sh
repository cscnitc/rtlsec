#!/usr/bin/env bash
# sync.sh — sync the rtlsec-tool crate to the build container and build/test it.
#
#   ./scripts/sync.sh build     # cargo build --release
#   ./scripts/sync.sh test      # cargo test
#   ./scripts/sync.sh run  <args...>   # run the built binary inside the tree
#
# Build host: `ssh moon`, container `rustdev`,
# Rust at ~/.cargo/bin inside the container. The RTL tree at ${RTL_TREE:-.} is
# mounted read-only in spirit — nothing is ever written into it.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CTR=rustdev
SRC='$HOME/rtlsec-tool'   # expanded remotely, inside the container
MODE="${1:-build}"; shift || true

sync() {
  local tgz=/tmp/rtlsec-tool.tgz
  tar czf "$tgz" -C "$HERE/.." "$(basename "$HERE")"
  scp -q -o BatchMode=yes "$tgz" moon:/tmp/
  ssh -o BatchMode=yes moon "docker cp /tmp/rtlsec-tool.tgz $CTR:/tmp/rtlsec-tool.tgz"
  # Keep target/ (build cache) across syncs — only refresh sources.
  ssh -o BatchMode=yes moon "docker exec $CTR bash -lc '
    cd \$HOME && mkdir -p rtlsec-tool && cd rtlsec-tool \
      && rm -rf src scripts Cargo.toml Cargo.lock PLAN.md README.md \
      && tar xzf /tmp/rtlsec-tool.tgz --strip-components=1 \
      && find src -name \"*.rs\" -exec touch {} +'"
}

cargo_run() {
  ssh -o BatchMode=yes moon "docker exec $CTR bash -lc '
    export PATH=\$HOME/.cargo/bin:\$PATH; cd $SRC && $*'"
}

case "$MODE" in
  build) sync; cargo_run "cargo build --release -j8 2>&1 | tail -20" ;;
  test)  sync; cargo_run "cargo test --release -j8 2>&1 | tail -25" ;;
  run)   sync; cargo_run "cargo build --release -j8 >/dev/null 2>&1 && ./target/release/rtlsec $*" ;;
  nodsync) cargo_run "cargo build --release -j8 2>&1 | tail -20" ;;
  *) echo "usage: $0 {build|test|run <args>|nodsync}" >&2; exit 2 ;;
esac
