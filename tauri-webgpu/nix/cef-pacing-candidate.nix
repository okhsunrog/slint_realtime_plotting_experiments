# Optional diagnostic SDK. The normal package remains cef-wayland-binary.nix.
{ pkgs ? import ./pkgs.nix }:
let
  base = pkgs.callPackage ./cef-wayland-binary.nix {};
  archiveHash = "46adddf6ce0e0f9d159662310851d7f1519ba69f3cc9b1bc6fd6a1cc94c7e7d1";
  libraryHash = "e90942406b71bea8e2d831e77819c86e04d6821768e28ba2ce6107ea1ae7f73b";
  originalArchive = "b20026d7ec4eaefbf7035427e4e34b3f4ccbf7d32f06d8814fb93d494891f836";
  originalLibrary = "a510fd54890b575f832700509bbd1775db9ce711ae463f749b2373960c5eb94d";
  provenance = {
    archive_sha256 = archiveHash;
    original_libcef_sha256 = libraryHash;
    cef_base_revision = "a09c27de3f5bb45f4cfb8656a46d85d272e1587e";
    chromium_base_revision = "b859317bf11f6be47f9b7799ec690a0a42a1fb33";
    deadline_patch_sha256 = "2f7fb7058e231abfb5d9d9949e1f3f4142192a8a9634abf04274f99961a2f01d";
    yield_patch_sha256 = "d3c693fecc3089eeceeecdbc7fdda2cead94f18ee318197c3333eb08e3ab80cc";
    fifo_patch_sha256 = "7b3030f06e585913ea7674f6f28b2dbc1de0fbba9bbc69f2adbb31c22872a964";
    status = "experimental; physical presentation validation pending";
    source = "https://github.com/okhsunrog/cef/tree/experiment/external-pump-yield";
  };
in base.overrideAttrs (old:
  assert pkgs.lib.hasInfix originalArchive old.installPhase;
  assert pkgs.lib.hasInfix originalLibrary old.installPhase;
{
  pname = "cef-wayland-pacing-candidate";
  version = "154.0.8037.98-yield-d3c693f";
  src = pkgs.requireFile {
    name = "cef-wayland-154-pump-yield.tar.zst";
    sha256 = archiveHash;
    meta.license = pkgs.lib.licenses.bsd3;
    message = ''
      Import the experimental archive once:
        nix-store --add-fixed sha256 /path/to/cef-wayland-154-pump-yield.tar.zst
      Expected SHA256: ${archiveHash}
    '';
  };
  installPhase = builtins.replaceStrings
    [ originalLibrary originalArchive ] [ libraryHash archiveHash ] old.installPhase;
  postInstall = (old.postInstall or "") + ''
    cp ${pkgs.writeText "pacing-provenance.json" (builtins.toJSON provenance)} "$out/nix-support/pacing-provenance.json"
  '';
})
