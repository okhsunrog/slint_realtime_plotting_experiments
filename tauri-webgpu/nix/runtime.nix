{ pkgs ? import /nix/store/2g1ydkkflg5kl8hb1f8js0s0p9v4gl53-source {} }:
let cef = import ./default.nix { inherit pkgs; };
in pkgs.writeShellScript "wavebench-nix-runtime" ''
  set -eu
  unset LD_LIBRARY_PATH
  export CEF_PATH=${cef}
  export GDK_BACKEND=wayland
  # GTK resolves settings, icons and optional schemas independently of ELF lookup.
  export XDG_DATA_DIRS=${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk4}/share/gsettings-schemas/${pkgs.gtk4.name}:''${XDG_DATA_DIRS:-/run/current-system/sw/share}
  exec "$@"
''
