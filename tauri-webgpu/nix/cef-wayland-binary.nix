# Binary SDK packaging, adapted from nixpkgs' cef-binary. No Chromium rebuild.
{ lib, stdenv, requireFile, zstd, patchelf, glib, gtk3, gtk4, gdk-pixbuf,
  graphene, nss, nspr, alsa-lib, libdrm, libgbm, libGL, vulkan-loader,
  cups, dbus, wayland, libxkbcommon, libx11, libxcomposite, libxdamage,
  libxext, libxfixes, libxrandr, libxcb, libxscrnsaver, libxrender,
  libxtst, libxi, libxshmfence, at-spi2-core, atk, at-spi2-atk, expat,
  libuuid, pciutils, libffi, cairo, pango, harfbuzz, udev, zlib }:
let
  archiveHash = "b20026d7ec4eaefbf7035427e4e34b3f4ccbf7d32f06d8814fb93d494891f836";
  revision = "a09c27de3f5bb45f4cfb8656a46d85d272e1587e";
  readerPatch = "1ad8496c4e63c3a3e276b72ff804c43fe7d82908a97121ecb04e7a40adc3c147";
  apiHash = "a15420be190a255a6479f26806ed67fd59e1eeae";
  runtimeLibraries = [
    stdenv.cc.cc.lib glib gtk3 gtk4 gdk-pixbuf graphene nss nspr alsa-lib
    libdrm libgbm libGL vulkan-loader cups dbus wayland libxkbcommon libx11
    libxcomposite libxdamage libxext libxfixes libxrandr libxcb libxscrnsaver
    libxrender libxtst libxi libxshmfence at-spi2-core atk at-spi2-atk expat
    libuuid pciutils libffi cairo pango harfbuzz udev zlib
  ];
  rpath = lib.makeLibraryPath runtimeLibraries;
in stdenv.mkDerivation {
  pname = "cef-wayland-binary";
  version = "154.0.8037.98-${builtins.substring 0 9 revision}-reader-${builtins.substring 0 7 readerPatch}";
  src = requireFile {
    name = "cef-wayland-154-reader-opt-in.tar.zst";
    sha256 = archiveHash;
    meta.license = lib.licenses.bsd3;
    message = ''
      Import the verified custom CEF archive once:
        nix-store --add-fixed sha256 /path/to/cef-wayland-154-reader-opt-in.tar.zst
      Expected SHA256: ${archiveHash}
    '';
  };
  nativeBuildInputs = [ zstd patchelf ];
  dontConfigure = true;
  dontBuild = true;
  dontStrip = true;
  dontPatchELF = true;
  installPhase = ''
    runHook preInstall
    echo 'a510fd54890b575f832700509bbd1775db9ce711ae463f749b2373960c5eb94d  Release/libcef.so' | sha256sum -c -
    grep -q cef_set_wayland_display include/internal/cef_types_linux.h
    sed -i 's/-O0/-O2/g' cmake/cef_variables.cmake
    mkdir -p "$out"
    cp -R . "$out/"
    patchelf --set-rpath '${rpath}:$ORIGIN' "$out/Release/libcef.so"
    patchelf --set-interpreter '${stdenv.cc.bintools.dynamicLinker}' \
      --set-rpath '${rpath}' "$out/Release/chrome-sandbox"
    patchelf --set-rpath '${lib.makeLibraryPath [ stdenv.cc.cc.lib ]}' \
      "$out/Release/libvk_swiftshader.so"
    # The bundled loader failed Dawn's vkDestroySurfaceKHR lookup on NixOS.
    # Keep one immutable Nix loader, with its own dependency RPATH intact.
    rm "$out/Release/libvulkan.so.1"
    ln -s '${vulkan-loader}/lib/libvulkan.so.1' "$out/Release/libvulkan.so.1"
    # cef-rs uses a flat runtime; Tauri also understands Release/Resources.
    for file in "$out"/Release/* "$out"/Resources/*; do
      ln -s "''${file#"$out"/}" "$out/$(basename "$file")"
    done
    mkdir -p "$out/nix-support"
    cat > "$out/nix-support/cef-provenance.json" <<JSON
    {"cef_base_revision":"${revision}","chromium_reader_patch_sha256":"${readerPatch}","chromium_version":"154.0.8037.98","api_version":999999,"api_hash":"${apiHash}","archive_sha256":"${archiveHash}","original_libcef_sha256":"a510fd54890b575f832700509bbd1775db9ce711ae463f749b2373960c5eb94d"}
    JSON
    runHook postInstall
  '';
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    test -f "$out/include/cef_version.h"
    test -d "$out/locales"
    test "$(readlink -f "$out/libvulkan.so.1")" = "$(readlink -f '${vulkan-loader}/lib/libvulkan.so.1')"
    LD_LIBRARY_PATH= ldd "$out/Release/libcef.so" > "$TMPDIR/cef-ldd.txt"
    cat "$TMPDIR/cef-ldd.txt"
    if grep -q 'not found' "$TMPDIR/cef-ldd.txt"; then exit 1; fi
    runHook postInstallCheck
  '';
  passthru = { inherit revision readerPatch apiHash runtimeLibraries; apiVersion = 999999; buildType = "Release"; };
  meta = {
    description = "CEF 154 SDK with native shared-display GTK Wayland embedding";
    license = lib.licenses.bsd3;
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
    platforms = [ "x86_64-linux" ];
  };
}
