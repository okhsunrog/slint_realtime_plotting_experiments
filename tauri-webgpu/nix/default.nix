{ pkgs ? import /nix/store/2g1ydkkflg5kl8hb1f8js0s0p9v4gl53-source {} }:
pkgs.callPackage ./cef-wayland-binary.nix {}
