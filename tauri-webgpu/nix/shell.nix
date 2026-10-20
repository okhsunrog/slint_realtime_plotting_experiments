{ pkgs ? import ./pkgs.nix }:
let cef = import ./default.nix { inherit pkgs; };
in pkgs.mkShell {
  nativeBuildInputs = with pkgs; [ pkg-config cmake ninja clang patchelf ];
  buildInputs = with pkgs; [ gtk4 glib openssl wayland ];
  CEF_PATH = "${cef}";
  LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
  WAVEBENCH_NIX_RPATH = pkgs.lib.makeLibraryPath cef.runtimeLibraries;
  # No runtime LD_LIBRARY_PATH: CEF and its dlopen dependencies use store RPATHs.
}
