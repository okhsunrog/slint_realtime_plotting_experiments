#!/usr/bin/env bash
set -Eeuo pipefail
ulimit -c 0
cd -- "$(dirname -- "$0")"
mode=${1:-wayland}
case "$mode" in wayland|x11) ;; *) echo 'Usage: ./run.sh [wayland|x11]' >&2; exit 2 ;; esac
export GDK_BACKEND="$mode"
export CEF_SMOKE_TMP="${WAVEBENCH_TMP:-${XDG_CACHE_HOME:-$HOME/.cache}/tauri-webgpu/tmp}"
export RUST_LOG=${RUST_LOG:-info}
mkdir -p logs "$CEF_SMOKE_TMP"
unset CEF_PATH LD_LIBRARY_PATH
nix-shell ../nix/shell.nix --run 'export TMPDIR="$CEF_SMOKE_TMP"; vp run tauri dev' 2>&1 | tee -a "logs/runtime-$mode.log"
