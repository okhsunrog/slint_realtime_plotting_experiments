# Custom CEF binary SDK

This packages the already-built CEF 154 Wayland SDK. It does not compile Chromium.
The recipe follows nixpkgs' `cef-binary` (ELF patching and preserved SDK layout),
with the flat symlink view expected by cef-rs. It includes our custom Linux
experimental API 999999 and records the source revision and API hash under
`nix-support/cef-provenance.json`.

Import the existing archive once, then build:

```sh
nix-store --add-fixed sha256 \
  /path/to/cef-wayland-154-reader-opt-in.tar.zst
nix-build ./nix --no-out-link
```

Expected archive SHA256:
`b20026d7ec4eaefbf7035427e4e34b3f4ccbf7d32f06d8814fb93d494891f836`.
`requireFile` deliberately requires this artifact; it never silently downloads
an official, ABI-incompatible SDK. The archive stays outside the Git repository.

`pkgs.nix` pins the same nixpkgs revision used for runtime tests, reusing the
local store path when present and otherwise fetching with a verified hash.
To use a different nixpkgs instance, pass `pkgs`, or integrate the recipe with:

```nix
cef-wayland-binary = pkgs.callPackage ./cef-wayland-binary.nix {};
```

Only x86_64-linux is supported by this archive/bindings combination. The SDK's
generated version header identifies base CEF commit `a09c27de3`. The library also
contains the separately hashed Chromium reader-coordination patch recorded in
`nix-support/cef-provenance.json`; it is not an unmodified a09c27de3 build.
The GTK reader path remains opt-in; use `WAVEBENCH_WAYLAND_READER=gtk` in this demo.
The default retains the existing threaded watcher.

The package patches RPATHs, the sandbox interpreter, and retains headers,
resources and wrapper sources. It uses nixpkgs' Vulkan loader instead of the
bundled loader that failed hardware Dawn initialization. `installCheckPhase`
checks direct/transitive ELF dependencies and the selected loader. Runtime GPU
checks are separate and require a real GPU/compositor.

`cef-rs` upstream's `NIX_CEF_BINARY=1` resolves official CEF builds through its
Spotify index. Our Linux fork requires an explicit `CEF_PATH`; `shell.nix`
sets it to this custom package. `src-tauri/build.rs` adds the executable's own
Nix library RPATH, independently of libcef's RPATH. `runtime.nix` provides GTK
schema paths and unsets `LD_LIBRARY_PATH`, without modifying the immutable SDK.

The Tauri fork also unlinks previously staged runtime files before copying
again: a store-backed SDK carries read-only permissions into Cargo output.
This permits repeated builds without chmodding or changing `/nix/store`.
