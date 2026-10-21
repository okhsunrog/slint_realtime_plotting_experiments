#[cfg(any(
    all(feature = "cef", feature = "webkit"),
    not(any(feature = "cef", feature = "webkit"))
))]
compile_error!(
    "Choose exactly one runtime: cef (default) or webkit (--no-default-features --features custom-protocol,webkit)"
);
mod stream;
use plot_wire::Config;
use serde_json::{Value, json};
use stream::Streams;
use tauri::{
    Manager, State,
    ipc::{Channel, Response},
};

#[tauri::command]
fn runtime_info(app: tauri::AppHandle) -> Value {
    json!({"name": if cfg!(feature = "webkit") { "WebKitGTK" } else { "CEF" }, "version": app.webview_version().ok()})
}
#[tauri::command]
fn clock(state: State<Streams>) -> f64 {
    state.now()
}
#[tauri::command]
fn start_stream(config: Config, on_batch: Channel, state: State<Streams>) -> Result<Value, String> {
    state.start(config, on_batch)
}
#[tauri::command]
fn stop_stream(state: State<Streams>) {
    state.stop()
}
#[tauri::command]
fn acknowledge(session: u32, seq: u32, state: State<Streams>) {
    state.ack(session, seq)
}
#[tauri::command]
async fn pull_batch(
    session: u32,
    ack: Option<u32>,
    state: State<'_, Streams>,
) -> Result<Response, String> {
    state.pull(session, ack).await.map(Response::new)
}
#[tauri::command]
fn stream_stats(session: u32, state: State<Streams>) -> Result<Value, String> {
    state.stats(session)
}
#[tauri::command]
fn save_benchmark(report: Value) -> Result<String, String> {
    let path = std::env::var_os("WAVEBENCH_REPORT_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../logs/benchmark.json")
        });
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}
#[tauri::command]
async fn open_gpu(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(feature = "webkit")]
    {
        let _ = app;
        Err("chrome://gpu is available only in CEF".into())
    }
    #[cfg(feature = "cef")]
    {
        if let Some(w) = app.get_webview_window("gpu") {
            return w.set_focus().map_err(|e| e.to_string());
        }
        tauri::WebviewWindowBuilder::new(
            &app,
            "gpu",
            tauri::WebviewUrl::External("chrome://gpu".parse().unwrap()),
        )
        .title("CEF GPU diagnostics")
        .inner_size(1000., 800.)
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
    }
}
fn main() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();
    let logs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../logs");
    std::fs::create_dir_all(&logs).unwrap();
    #[cfg(feature = "cef")]
    let runtime = tauri_runtime_cef::Cef::default()
        .allow_chromium_command_line_args(
            std::env::var_os("WAVEBENCH_ALLOW_CHROMIUM_ARGS").is_some(),
        )
        .log_file(logs.join("cef.log"))
        .command_line_arg("--password-store", Some("basic"))
        .command_line_arg("--use-gl", Some("angle"))
        .command_line_arg("--use-angle", Some("gl"))
        .command_line_arg("--enable-unsafe-webgpu", None::<String>)
        .command_line_arg("--force-renderer-accessibility", None::<String>)
        .enable_features(["ForceEnableWebGpuInterop"])
        .child_process_command_line_args([
            ("--use-gl", Some("angle")),
            ("--use-angle", Some("gl")),
            ("--enable-features", Some("ForceEnableWebGpuInterop")),
            ("--enable-unsafe-webgpu", None),
            ("--force-renderer-accessibility", None),
        ]);
    #[cfg(feature = "cef")]
    let runtime = match std::env::var("WAVEBENCH_WAYLAND_READER").as_deref() {
        Ok("gtk") => runtime.command_line_arg("--wayland-external-reader", None::<String>),
        Ok("threaded") | Err(_) => runtime,
        Ok(other) => panic!("Unknown WAVEBENCH_WAYLAND_READER={other}; use gtk or threaded"),
    };
    #[cfg(feature = "webkit")]
    let runtime = {
        use gtk3::prelude::*;
        gtk3::init().expect("Initialize GTK3");
        if let Some(screen) = gtk3::gdk::Screen::default()
            && screen.resolution() <= 0.0
        {
            let configured = gtk3::Settings::default()
                .map(|s| s.property::<i32>("gtk-xft-dpi"))
                .unwrap_or(-1);
            let dpi = if configured > 0 {
                configured as f64 / 1024.0
            } else {
                96.0
            };
            // WebKitGTK 2.54 forwards GDK's unset (-1) sentinel as a negative DPR.
            // Set a valid font DPI before the first WebView; preserve valid settings.
            screen.set_resolution(dpi);
            println!("Initialized unset GDK font resolution to {dpi} DPI");
        }
        tauri_runtime_wry::Wry::default()
    };
    let mut context = tauri::generate_context!();
    #[cfg(feature = "webkit")]
    {
        context.config_mut().app.windows[0].url =
            tauri::WebviewUrl::App("index.html?renderer=webgl".into());
    }
    if let Ok(query) = std::env::var("WAVEBENCH_QUERY") {
        context.config_mut().app.windows[0].url =
            tauri::WebviewUrl::App(format!("index.html?{query}").into());
    }
    context.config_mut().app.windows[0].title = if cfg!(feature = "webkit") {
        "Wavebench · WebKitGTK / WebGL2"
    } else {
        "Wavebench · CEF / WebGPU + WebGL2"
    }
    .into();
    tauri::Builder::default().runtime(runtime)
        .register_asynchronous_uri_scheme_protocol("wave-data",|context,request,responder|{
            use tauri::http::{Response as HttpResponse,StatusCode};
            let origin=request.headers().get("Origin").and_then(|v|v.to_str().ok()).unwrap_or("tauri://localhost").to_owned();
            if context.webview_label()!="main" || !["tauri://localhost","http://tauri.localhost","http://localhost:8087"].contains(&origin.as_str()) || request.method()!="GET" {
                responder.respond(HttpResponse::builder().status(StatusCode::FORBIDDEN).body(Vec::new()).unwrap());return;
            }
            let token=request.uri().path().trim_matches('/').to_owned();
            let ack=request.uri().query().and_then(|q|q.strip_prefix("ack=")).and_then(|s|s.parse().ok());
            let streams=context.app_handle().state::<Streams>().inner().clone();
            tauri::async_runtime::spawn(async move{
                let (status,body)=match streams.protocol_pull(&token,ack).await {Ok(bytes)=>(StatusCode::OK,bytes),Err(e)=>(StatusCode::BAD_REQUEST,e.into_bytes())};
                responder.respond(HttpResponse::builder().status(status).header("Content-Type","application/octet-stream")
                    .header("Cache-Control","no-store").header("Access-Control-Allow-Origin",origin).header("Vary","Origin").body(body).unwrap());
            });
        })
        .invoke_handler(tauri::generate_handler![runtime_info,clock,start_stream,stop_stream,acknowledge,pull_batch,stream_stats,save_benchmark,open_gpu])
        .setup(|app|{
            // Only the browser process owns the source and listener. CEF also
            // executes main() in its renderer/GPU subprocesses.
            let rt=tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
            let listener=rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))?;
            let streams=Streams::new(listener.local_addr()?.port());
            rt.spawn(stream::serve(listener,streams.clone()));
            app.manage(streams);app.manage(rt);
            println!("{}",json!({"runtime":if cfg!(feature="webkit"){"WebKitGTK Wayland"}else{"CEF Wayland"},"version":app.handle().webview_version().ok()}));Ok(())
        })
        .build(context).expect("Build Wavebench")
        .run(move |app,event|{if matches!(event,tauri::RunEvent::Exit){app.state::<Streams>().stop();}});
}
