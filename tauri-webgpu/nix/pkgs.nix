let
  cached = /nix/store/2g1ydkkflg5kl8hb1f8js0s0p9v4gl53-source;
  source = if builtins.pathExists cached then cached else builtins.fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/fe13242281a67641c0102cdb1087f0ebe586c7df.tar.gz";
    sha256 = "sha256-zp8ToDMnqZcu4o4IH9pdwXFXGp7yLm/M0dEnNWnyd7I=";
  };
in import source {}
