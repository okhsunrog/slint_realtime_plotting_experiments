#!/usr/bin/env bash
set -Eeuo pipefail
project=$(cd -- "$(dirname -- "$0")" && pwd)
cd "$project"
mode=${1:-wayland}
case "$mode" in wayland|x11) ;; *) echo 'Usage: ./run.sh [wayland|x11]' >&2; exit 2 ;; esac
export GDK_BACKEND="$mode"
export CEF_PATH=${CEF_PATH:-$(cat .cef-sdk-path)}
test -f "$CEF_PATH/libcef.so"
export TMPDIR="$project/../tmp"
export CARGO_HTTP_MULTIPLEXING=false
mkdir -p logs "$TMPDIR"
export CEF_SMOKE_TMP="$TMPDIR"
export RUST_LOG=${RUST_LOG:-info}
export LD_LIBRARY_PATH="$CEF_PATH:${LD_LIBRARY_PATH:-}"
# The outer shell keeps all stdout/stderr, including GPU/renderer subprocess logs.
nix-shell ../shell.nix --run 'export TMPDIR="$CEF_SMOKE_TMP"; vp run tauri dev' 2>&1 | tee -a "logs/runtime-$mode.log"
