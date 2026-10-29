# Experiment closeout — 2026-10-29

Further experiments were stopped at the owner's request. The application changes
and compact measurement reports are preserved on `experiment/cef-frame-pacing`.
The remaining presentation and renderer stalls described in the reports are not
claimed to be fully fixed.

## Preserved work

- Application: <https://github.com/okhsunrog/slint_realtime_plotting_experiments/tree/experiment/cef-frame-pacing>
- Tauri pump changes: <https://github.com/okhsunrog/tauri/tree/fix/cef-message-pump-interval>
- CEF Wayland support: <https://github.com/okhsunrog/cef/tree/feat/wayland-154>
- CEF pump experiments: <https://github.com/okhsunrog/cef/tree/experiment/external-pump-yield>
- Experimental SDK and provenance: <https://github.com/okhsunrog/cef/releases/tag/cef-154.0.8037.98-pacing.1>

The rented build VM, its 500 GB source/cache volume, and its dedicated security
group and keypair were deleted. Their absence was confirmed through the cloud
API. Rebuilding CEF will require restoring sources and creating a fresh build
cache; the published SDK can be downloaded without rebuilding CEF.

Local Chromium source/build trees, project build outputs, duplicate SDK archives
and large raw diagnostic captures were removed as part of closeout.
Source repositories, patches, build scripts, compact reports and provenance are
retained. Historical raw-capture paths in older reports are no longer guaranteed
to exist. Shared Nix store contents and unrelated project caches are untouched.

The temporary user-service setting `KWIN_DRM_OVERRIDE_SAFETY_MARGIN=2000` remains
installed. It is an experimental host setting, not a permanent NixOS change or a
claim that all microstutters are fixed. No compositor restart was performed
as part of closeout.
