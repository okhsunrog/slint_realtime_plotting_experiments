{ pkgs ? import ./pkgs.nix, variant ? "stable" }:
let cef = import ./default.nix { inherit pkgs variant; };
in pkgs.writeShellScript "wavebench-nix-runtime" ''
  set -eu
  unset LD_LIBRARY_PATH
  export CEF_PATH=${cef}
  export GDK_BACKEND=wayland
  ${pkgs.lib.optionalString (variant == "pacing") ''
    export WAVEBENCH_PUMP_MAX_DELAY_MS="''${WAVEBENCH_PUMP_MAX_DELAY_MS:-4}"
    export WAVEBENCH_PUMP_YIELD="''${WAVEBENCH_PUMP_YIELD:-1}"
    export WAVEBENCH_FRAME_SOURCE="''${WAVEBENCH_FRAME_SOURCE:-timer}"
    export WAVEBENCH_WAYLAND_READER="''${WAVEBENCH_WAYLAND_READER:-gtk}"
    export WAVEBENCH_QUERY="''${WAVEBENCH_QUERY:-motion=clocked&reclaim=worker}"
  ''}
  # GTK resolves settings, icons and optional schemas independently of ELF lookup.
  export XDG_DATA_DIRS=${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk4}/share/gsettings-schemas/${pkgs.gtk4.name}:''${XDG_DATA_DIRS:-/run/current-system/sw/share}
  exec "$@"
''
