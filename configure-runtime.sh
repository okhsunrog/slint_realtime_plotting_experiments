#!/usr/bin/env bash
set -Eeuo pipefail
sdk=${1:?CEF SDK path is required}
loader=${CEF_SYSTEM_VULKAN_LOADER:-}
if [ -z "$loader" ]; then exit 0; fi
test -f "$loader"
# Dawn searches beside the executable before the normal library search path.
# On NixOS use the same loader as Chromium and the host Mesa drivers. The
# bundled loader failed vkDestroySurfaceKHR lookup and selected SwiftShader.
if ! cmp -s "$loader" "$sdk/Release/libvulkan.so.1"; then
  mkdir -p "$sdk/host-runtime-backup"
  if [ ! -e "$sdk/host-runtime-backup/libvulkan.so.1" ]; then
    cp -L "$sdk/Release/libvulkan.so.1" "$sdk/host-runtime-backup/libvulkan.so.1"
  fi
  cp --remove-destination "$loader" "$sdk/Release/libvulkan.so.1"
  chmod u+w "$sdk/Release/libvulkan.so.1"
fi
# An existing Cargo build may not rerun its staging build script on launch.
# Use writable ordinary files so subsequent staging can overwrite them safely.
project=$(cd -- "$(dirname -- "$0")" && pwd)
for profile in debug release; do
  directory="$project/src-tauri/target/$profile"
  if [ -f "$directory/cef-wayland-smoke" ] && { [ -L "$directory/libvulkan.so.1" ] || ! cmp -s "$loader" "$directory/libvulkan.so.1"; }; then
    cp --remove-destination "$loader" "$directory/libvulkan.so.1"
    chmod u+w "$directory/libvulkan.so.1"
  fi
done
