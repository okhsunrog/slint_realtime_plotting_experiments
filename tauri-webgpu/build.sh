#!/usr/bin/env bash
set -Eeuo pipefail
cd -- "$(dirname -- "$0")"
case "${1:-cef}" in
  cef|webkit) ;;
  *) echo "Usage: $0 [cef|webkit]" >&2; exit 2 ;;
esac
export TMPDIR="${WAVEBENCH_TMP:-${XDG_CACHE_HOME:-$HOME/.cache}/tauri-webgpu/tmp}"
mkdir -p "$TMPDIR" logs
wasm-pack build frontend --target web --release --out-dir ../dist/pkg 2>&1 | tee logs/frontend-build.log
cp frontend/index.html frontend/style.css frontend/measure.js frontend/reclaim.js frontend/reclaim-worker.js dist/
export WAVEBENCH_TMP="$TMPDIR"
if [ "${1:-cef}" = webkit ]; then
  unset CEF_PATH LD_LIBRARY_PATH
  nix-shell ./nix/webkit-shell.nix --run 'export TMPDIR="$WAVEBENCH_TMP"; cargo build -p tauri-webgpu-plot --release --no-default-features --features custom-protocol,webkit' 2>&1 | tee logs/webkit-build.log
  cp target/release/tauri-webgpu-plot target/release/wavebench-webkit
  exit 0
fi
unset CEF_PATH LD_LIBRARY_PATH
mkdir -p target/release
printf 'building:%s\n' "${WAVEBENCH_CEF_VARIANT:-stable}" > target/release/wavebench-cef.variant
nix-shell ./nix/shell.nix --argstr variant "${WAVEBENCH_CEF_VARIANT:-stable}" --run 'export TMPDIR="$WAVEBENCH_TMP"; cargo build -p tauri-webgpu-plot --release' 2>&1 | tee logs/backend-build.log

cp target/release/tauri-webgpu-plot target/release/wavebench-cef

printf '%s\n' "${WAVEBENCH_CEF_VARIANT:-stable}" > target/release/wavebench-cef.variant
