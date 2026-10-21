{ pkgs ? import ./pkgs.nix, variant ? "stable" }:
if variant == "stable" then pkgs.callPackage ./cef-wayland-binary.nix {}
else if variant == "pacing" then import ./cef-pacing-candidate.nix { inherit pkgs; }
else throw "Unknown CEF variant '${variant}'; use stable or pacing"
