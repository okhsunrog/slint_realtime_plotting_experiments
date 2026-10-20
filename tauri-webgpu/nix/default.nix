{ pkgs ? import ./pkgs.nix }:
pkgs.callPackage ./cef-wayland-binary.nix {}
