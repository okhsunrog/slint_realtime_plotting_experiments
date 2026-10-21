#!/usr/bin/env bash
set -Eeuo pipefail
ulimit -c 0
cd -- "$(dirname -- "$0")"
export TMPDIR="${WAVEBENCH_TMP:-${XDG_CACHE_HOME:-$HOME/.cache}/tauri-webgpu/tmp}"
mkdir -p "$TMPDIR" logs
{
  variant="${WAVEBENCH_CEF_VARIANT:-stable}"
  if [ -f target/release/wavebench-cef.variant ]; then
    IFS= read -r built_variant < target/release/wavebench-cef.variant
    if [ "$built_variant" != "$variant" ]; then
      printf 'CEF build variant is %s, requested %s. Rebuild with WAVEBENCH_CEF_VARIANT=%s ./build.sh or select the built variant.\n' "$built_variant" "$variant" "$variant" >&2
      exit 2
    fi
  fi
  launcher=$(nix-build ./nix/runtime.nix --argstr variant "${WAVEBENCH_CEF_VARIANT:-stable}" --out-link "$TMPDIR/nix-runtime")
  binary="$PWD/target/release/wavebench-cef"
  "$launcher" "$binary" "$@"
} 2>&1 | tee -a logs/runtime.log
