#!/usr/bin/env bash
set -Eeuo pipefail
ulimit -c 0
cd -- "$(dirname -- "$0")"
infra=/home/okhsunrog/tmp_zfs/codex_tmp/cef-cloud-research
export CEF_PATH=${CEF_PATH:-$(cat "$infra/cef-wayland-smoke/.cef-sdk-path")}
export TMPDIR=/home/okhsunrog/tmp_zfs/codex_tmp/tauri-webgpu-build
export WAVEBENCH_TMP="$TMPDIR"
export GDK_BACKEND=wayland
mkdir -p "$TMPDIR" logs
nix-shell "$infra/shell.nix" --run 'export TMPDIR="$WAVEBENCH_TMP"; /home/okhsunrog/tmp_zfs/codex_tmp/cef-cloud-research/cef-wayland-smoke/configure-runtime.sh "$CEF_PATH"; exec ./target/release/tauri-webgpu-plot' 2>&1 | tee -a logs/runtime.log
