fn main() {
    tauri_build::build();
    println!("cargo:rerun-if-env-changed=WAVEBENCH_NIX_RPATH");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux")
        && let Ok(rpath) = std::env::var("WAVEBENCH_NIX_RPATH")
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{rpath}");
    }
}
