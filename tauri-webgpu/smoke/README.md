# CEF Wayland / Vulkan smoke app

Generated with `vpx create-tauri-app` (vanilla TypeScript), then migrated following
https://v3.tauri.app/de/start/migrate/from-tauri-2/ . Uses the local Tauri v3
checkout and the custom Linux x86_64 cef-rs bindings through Cargo path patches.
The opener plugin was removed. Runtime is selected explicitly with `Cef`.

## Prepare and launch

From this directory, after the cloud collector has verified the SDK archive:

```sh
vp install
vp run build
./install-sdk.sh
./run.sh wayland
```

`run.sh` opens the task's Nix shell; it does not change system configuration.
The shell selects NixOS's Vulkan loader. `configure-runtime.sh` installs that
loader into the extracted SDK and existing app output; the verified archive
stays unchanged. The original loader is retained under `host-runtime-backup`.
On this host the bundled loader failed Dawn's `vkDestroySurfaceKHR` lookup and
caused a SwiftShader fallback; the system loader selects Intel Xe-LPG.
`GDK_BACKEND=wayland` selects native Wayland. For comparison, close all app
processes, then run `./run.sh x11`. The same custom CEF supports both paths.
Do not run the two backends concurrently against the same Chromium cache.

The selected SDK is recorded in `.cef-sdk-path`; override with `CEF_PATH` if
needed. SDK archives are extracted separately by fingerprint. Old versions can
be kept for comparisons. Do not use a stock CEF SDK with these bindings.

## Acceptance

- The report must say `nativeWindowBackend: wayland`.
- The compute test must pass all 1024 read-back values. A missing GPU or shader,
  mapping or validation error is a failure, even if the window renders.
- The animated WebGPU canvas must resize cleanly. Test maximize, restore and
  moving between outputs, including fractional/HiDPI scaling.
- `chrome://gpu` must identify the Intel hardware adapter and list WebGPU and
  WebGPU interop as hardware accelerated. Its Vulkan Information must list the
  Intel device. The top-level `Vulkan: Disabled` item refers to Chromium's
  compositor and is expected: ANGLE/OpenGL presents the window, while Dawn
  runs WebGPU on Vulkan via `ForceEnableWebGpuInterop`. Chromium 154's Linux
  default backend selection in `webgpu_decoder_impl.cc` chooses Vulkan for
  this interop path; OpenGLES is not selected. SwiftShader or llvmpipe does
  not satisfy acceptance.
- Check `chrome://version` for the actual Chromium version and browser switches.
  Inspect GPU subprocess arguments when diagnosing propagation of ANGLE,
  Vulkan and WebGPU flags.
- Test typing Russian text, clipboard paste, IME, tab focus, select dropdown,
  right-click menu and JavaScript dialog. Check popup clipping at window edges.
- Close every window; check that renderer/GPU/helper processes exit.

Logs: `logs/runtime-wayland.log` or `runtime-x11.log`, `logs/cef.log`.
The Save report button writes `logs/webgpu-report.json`.

## Current limitations

GTK4 exposes its `wl_surface` and display but not its `xdg_surface` through its
public API. The latter remains null, invoking CEF's fallback popup subsurfaces:
menus can be clipped at the host edge and lack outside-click dismissal.
Wayland has no public global cursor position or X11 taskbar/workspace hints.
Reparenting a native webview to another window is rejected. Hiding a whole
window is supported through GTK; hiding just one native Wayland webview is not
implemented by the CEF patch yet and emits a warning. Physical multi-monitor moves, complex IME and touch input still require
manual acceptance. Isolated KWin tests use the real Intel GPU.

## Incremental CEF rebuild

The VM and its disk must be retained until the user explicitly confirms runtime
acceptance. Sources, Docker image, `out/Release_GN_x64`, `.ninja_log` and all
objects stay on the existing 500 GB disk. Stopping/starting the VM preserves
that disk. A new Ninja invocation prints a fresh counter for remaining work;
it does not imply recompiling previously completed objects.

The task's `container/incremental-build.sh` refuses to interrupt an active build.
It can apply a patch relative to `chromium/src/cef`, then launches the existing
`offline-build.sh` at `-j32`. `run-incremental-build.sh` refreshes CEF generated
API files and GN before Ninja. It does not run `clean`, `sync`, reinstall apt
packages or re-extract sources. Changes to Chromium itself need a separately
validated patch in `chromium/src`, not replaying all CEF patches on top.

Copy the incremental scripts next to the VM's existing `run-offline-build.sh`.
After starting the retained VM and reconnecting SSH, invoke:

```sh
ssh -F ../ssh_config cef-builder \
  'bash /home/okhsunrog/tmp_zfs/codex_tmp/cef-wayland-build/incremental-build.sh'
```

An optional argument is an absolute, already uploaded CEF patch path.
Changes to public CEF headers require regenerating cef-rs bindings as well.
Changes limited to Tauri/Rust/TypeScript need no Chromium rebuild.
Remote failure/completion guards stop CPU billing by powering off the VM;
they do not delete the VM, disk, source or cache. Local cleanup is guarded
against deletion until runtime acceptance has been confirmed explicitly.
