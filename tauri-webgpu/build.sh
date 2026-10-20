#!/usr/bin/env bash
set -Eeuo pipefail
cd -- "$(dirname -- "$0")"
infra=/home/okhsunrog/tmp_zfs/codex_tmp/cef-cloud-research
export TMPDIR=/home/okhsunrog/tmp_zfs/codex_tmp/tauri-webgpu-build
mkdir -p "$TMPDIR" logs
wasm-pack build frontend --target web --release --out-dir ../dist/pkg 2>&1 | tee logs/frontend-build.log
cp frontend/index.html frontend/style.css dist/
export CEF_PATH=${CEF_PATH:-$(cat "$infra/cef-wayland-smoke/.cef-sdk-path")}
export WAVEBENCH_TMP="$TMPDIR"
nix-shell "$infra/shell.nix" --run 'export TMPDIR="$WAVEBENCH_TMP"; cargo build -p tauri-webgpu-plot --release' 2>&1 | tee logs/backend-build.log
