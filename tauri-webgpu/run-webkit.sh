#!/usr/bin/env bash
set -Eeuo pipefail
cd -- "$(dirname -- "$0")"
export TMPDIR="${WAVEBENCH_TMP:-${XDG_CACHE_HOME:-$HOME/.cache}/tauri-webgpu/tmp}"
mkdir -p "$TMPDIR" logs
# Each runtime gets its own executable, without relocating Cargo caches.
unset LD_LIBRARY_PATH CEF_PATH
export GDK_BACKEND=wayland
nix-shell ./nix/webkit-shell.nix --run 'exec ./target/release/wavebench-webkit' 2>&1 | tee -a logs/webkit-runtime.log
