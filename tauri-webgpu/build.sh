#!/usr/bin/env bash
set -Eeuo pipefail
cd -- "$(dirname -- "$0")"
export TMPDIR="${WAVEBENCH_TMP:-${XDG_CACHE_HOME:-$HOME/.cache}/tauri-webgpu/tmp}"
mkdir -p "$TMPDIR" logs
wasm-pack build frontend --target web --release --out-dir ../dist/pkg 2>&1 | tee logs/frontend-build.log
cp frontend/index.html frontend/style.css dist/
export WAVEBENCH_TMP="$TMPDIR"
unset CEF_PATH LD_LIBRARY_PATH
nix-shell ./nix/shell.nix --run 'export TMPDIR="$WAVEBENCH_TMP"; cargo build -p tauri-webgpu-plot --release' 2>&1 | tee logs/backend-build.log
