{ pkgs ? import ./pkgs.nix }:
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [ pkg-config patchelf ];
  buildInputs = with pkgs; [ gtk3 webkitgtk_4_1 libsoup_3 openssl ];
  WAVEBENCH_NIX_RPATH = pkgs.lib.makeLibraryPath (with pkgs; [ gtk3 webkitgtk_4_1 libsoup_3 openssl libGL wayland glib cairo gdk-pixbuf dbus ]);
}
