use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use serde_json::{Value, json};
use tauri::Manager;

#[tauri::command]
fn runtime_info(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Value {
    let backend = window
        .window_handle()
        .map(|handle| match handle.as_raw() {
            RawWindowHandle::Wayland(_) => "wayland",
            RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_) => "x11",
            _ => "other",
        })
        .unwrap_or("unavailable");
    json!({
        "webviewVersion": app.webview_version().ok(),
        "nativeWindowBackend": backend,
        "session": std::env::var("XDG_SESSION_TYPE").ok(),
        "gdkBackendOverride": std::env::var("GDK_BACKEND").ok(),
        "waylandDisplay": std::env::var("WAYLAND_DISPLAY").ok(),
        "expectedBackend": "See runtime.log for the actual GTK/CEF backend; chrome://gpu for Vulkan",
    })
}

#[tauri::command]
async fn open_diagnostic(app: tauri::AppHandle, page: String) -> Result<(), String> {
    if !matches!(page.as_str(), "gpu" | "version") {
        return Err("unsupported diagnostic page".into());
    }
    let label = format!("diagnostic-{page}");
    if let Some(window) = app.get_webview_window(&label) {
        return window.set_focus().map_err(|e| e.to_string());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        label,
        tauri::WebviewUrl::External(
            format!("chrome://{page}")
                .parse()
                .expect("fixed internal diagnostic URL"),
        ),
    )
    .title(format!("CEF chrome://{page}"))
    .inner_size(1000., 800.)
    .build()
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn save_report(report: Value) -> Result<String, String> {
    let logs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../logs");
    std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    let path = logs.join("webgpu-report.json");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

pub fn run() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();
    let logs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../logs");
    std::fs::create_dir_all(&logs).expect("create diagnostic log directory");
    let runtime = tauri_runtime_cef::Cef::default()
        .log_file(logs.join("cef.log"))
        .command_line_arg("--use-gl", Some("angle"))
        .command_line_arg("--use-angle", Some("vulkan"))
        .command_line_arg("--enable-unsafe-webgpu", None::<String>)
        .command_line_arg("--force-renderer-accessibility", None::<String>)
        .enable_features(["Vulkan"])
        .child_process_command_line_args([
            ("--use-gl", Some("angle")),
            ("--use-angle", Some("vulkan")),
            ("--enable-features", Some("Vulkan")),
            ("--enable-unsafe-webgpu", None),
            ("--force-renderer-accessibility", None),
        ]);
    tauri::Builder::default()
        .runtime(runtime)
        .invoke_handler(tauri::generate_handler![
            runtime_info,
            open_diagnostic,
            save_report
        ])
        .run(tauri::generate_context!())
        .expect("error while running CEF Wayland smoke app");
}
