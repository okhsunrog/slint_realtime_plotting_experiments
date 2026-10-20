#!/usr/bin/env bash
set -Eeuo pipefail
ulimit -c 0
cd -- "$(dirname -- "$0")"
export TMPDIR="${WAVEBENCH_TMP:-${XDG_CACHE_HOME:-$HOME/.cache}/tauri-webgpu/tmp}"
mkdir -p "$TMPDIR" logs
{
  launcher=$(nix-build ./nix/runtime.nix --out-link "$TMPDIR/nix-runtime")
  "$launcher" "$PWD/target/release/tauri-webgpu-plot"
} 2>&1 | tee -a logs/runtime.log
