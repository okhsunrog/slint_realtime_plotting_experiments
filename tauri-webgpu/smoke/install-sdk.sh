#!/usr/bin/env bash
set -Eeuo pipefail
project=$(cd -- "$(dirname -- "$0")" && pwd)
archive=${1:?Usage: install-sdk.sh /path/to/cef-wayland-154-linux-x86_64.tar.zst}
expected=90ae4e47589368b446fb0e2312ee0b7ea6120f0591d770f939234a3ed7731cb2
test "$(sha256sum "$archive" | cut -d " " -f1)" = "$expected"
fingerprint=$(sha256sum "$archive" | cut -c1-16)
base="$project/../sdk/$fingerprint"
mkdir -p "$base"
tar --zstd -xf "$archive" -C "$base"
mapfile -t candidates < <(find "$base" -mindepth 2 -maxdepth 3 -path '*/include/cef_version.h' -printf '%h\n')
if [ "${#candidates[@]}" -ne 1 ]; then
  echo 'Expected exactly one extracted CEF SDK; select a fresh sdk directory.' >&2; exit 1
fi
sdk=$(dirname "${candidates[0]}")
test -f "$sdk/Release/libcef.so"
grep -q cef_set_wayland_display "$sdk/include/internal/cef_types_linux.h"
# cef-rs expects a flat runtime directory; preserve the original distribution layout.
for directory in Release Resources; do
  while IFS= read -r -d '' file; do
    name=$(basename "$file")
    if [ ! -e "$sdk/$name" ]; then ln -s "$directory/$name" "$sdk/$name"; fi
  done < <(find "$sdk/$directory" -mindepth 1 -maxdepth 1 -print0)
done
printf '%s\n' "$sdk" > "$project/.cef-sdk-path"
echo "CEF_PATH=$sdk"
