mod gpu;
mod renderer;
mod webgl;
use leptos::{mount::mount_to_body, prelude::*};
use plot_wire::{Batch, CAPACITY, CHANNELS, Config};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::{JsFuture, spawn_local};

#[wasm_bindgen(inline_js = "
const channels = new Map();
export function rpc(command,args) { return window.__TAURI__.core.invoke(command,args); }
export function make_channel(callback) { const c = new window.__TAURI__.core.Channel(callback); channels.set(c.id,c); return c.id; }
export function drop_channel(id) { const c=channels.get(id); if(c) c.onmessage=()=>{}; channels.delete(id); }
export function sleep(ms) { return new Promise(resolve=>setTimeout(resolve,ms)); }
export function protocol_base() { return window.__TAURI__.core.convertFileSrc('', 'wave-data'); }
export async function fetch_packet(url) {const r=await fetch(url,{cache:'no-store'}); if(!r.ok) throw new Error('Protocol HTTP '+r.status); return r.arrayBuffer();}
export async function adapter_details() {
  const adapter=await navigator.gpu.requestAdapter({powerPreference:'high-performance'});
  if(!adapter) throw new Error('No WebGPU adapter');
  const i=adapter.info;
  return {vendor:i.vendor,architecture:i.architecture,description:i.description,isFallbackAdapter:i.isFallbackAdapter};
}
")]
extern "C" {
    fn rpc(command: &str, args: &JsValue) -> js_sys::Promise;
    fn make_channel(callback: &js_sys::Function) -> u32;
    fn drop_channel(id: u32);
    fn sleep(ms: u32) -> js_sys::Promise;
    fn adapter_details() -> js_sys::Promise;
    fn protocol_base() -> String;
    fn fetch_packet(url: &str) -> js_sys::Promise;
}
async fn invoke(command: &str, args: Value) -> Result<JsValue, String> {
    JsFuture::from(rpc(
        command,
        &js_sys::JSON::parse(&args.to_string()).unwrap(),
    ))
    .await
    .map_err(|e| format!("{command}: {e:?}"))
}
async fn delay(ms: u32) {
    let _ = JsFuture::from(sleep(ms)).await;
}
fn now() -> f64 {
    web_sys::window().unwrap().performance().unwrap().now()
}
fn bounded_push(v: &mut VecDeque<f64>, x: f64) {
    if v.len() >= 16_384 {
        v.pop_front();
    }
    v.push_back(x);
}
fn percentile(v: &VecDeque<f64>, q: f64) -> f64 {
    let mut v = v.iter().copied().collect::<Vec<_>>();
    v.sort_by(f64::total_cmp);
    if v.is_empty() {
        0.
    } else {
        v[((v.len() - 1) as f64 * q).round() as usize]
    }
}
fn value(id: &str) -> String {
    let e = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .get_element_by_id(id)
        .unwrap();
    if let Some(s) = e.dyn_ref::<web_sys::HtmlSelectElement>() {
        s.value()
    } else {
        e.dyn_ref::<web_sys::HtmlInputElement>().unwrap().value()
    }
}
fn selected_config(mode: Option<&str>) -> Config {
    Config {
        mode: mode
            .map(str::to_owned)
            .unwrap_or_else(|| value("transport")),
        rate: value("rate").parse().unwrap_or(0),
        batch: value("batch").parse().unwrap_or(0),
        frequency: value("frequency").parse().unwrap_or(0.),
        amplitude: value("amplitude").parse().unwrap_or(0.),
    }
}
#[derive(Clone, Copy)]
struct Ui {
    status: RwSignal<String>,
    stats: RwSignal<String>,
    gpu: RwSignal<String>,
    runtime: RwSignal<String>,
    check: RwSignal<String>,
    view: RwSignal<String>,
    paused: RwSignal<bool>,
    busy: RwSignal<bool>,
    rows: RwSignal<Vec<Value>>,
    saved: RwSignal<String>,
}
#[derive(Default)]
struct Metrics {
    since: f64,
    frames: u64,
    packets: u64,
    gaps: u64,
    latency: VecDeque<f64>,
    processing: VecDeque<f64>,
    frame_times: VecDeque<f64>,
    submit: VecDeque<f64>,
    rendered: u64,
}
struct Engine {
    ui: Ui,
    gpu: Option<Rc<RefCell<renderer::Renderer>>>,
    generation: u32,
    session: u32,
    config: Config,
    callback: Option<Closure<dyn FnMut(JsValue)>>,
    channel: Option<u32>,
    ws: Option<web_sys::WebSocket>,
    ws_message: Option<Closure<dyn FnMut(web_sys::MessageEvent)>>,
    ws_error: Option<Closure<dyn FnMut(web_sys::Event)>>,
    expected: u32,
    expected_first: u64,
    clock_offset: f64,
    clock_rtt: f64,
    metrics: Metrics,
    visible: usize,
    offset: usize,
    paused: bool,
    dirty: bool,
    last_frame: f64,
    last_ui: f64,
    ring: Vec<f32>,
    write: usize,
    filled: usize,
    drag: Option<(f64, usize)>,
}
thread_local! {static ENGINE:RefCell<Option<Engine>>=const{RefCell::new(None)};}
thread_local! {static FAULTS:RefCell<Vec<String>>=const{RefCell::new(Vec::new())};}
fn engine<T>(f: impl FnOnce(&mut Engine) -> T) -> T {
    ENGINE.with_borrow_mut(|e| f(e.as_mut().unwrap()))
}
pub fn fail(message: String) {
    web_sys::console::error_1(&message.clone().into());
    FAULTS.with_borrow_mut(|f| f.push(message.clone()));
    ENGINE.with(|e| {
        if let Ok(e) = e.try_borrow() {
            if let Some(e) = e.as_ref() {
                e.ui.status.set(message)
            }
        }
    });
}
fn snapshot(e: &Engine) -> Value {
    let seconds = ((now() - e.metrics.since) / 1000.).max(0.001);
    let m = &e.metrics;
    json!({"mode":e.config.mode,"config":e.config,"seconds":seconds,"frames":m.frames,"packets":m.packets,
        "frames_per_s":m.frames as f64/seconds,"payload_mib_s":m.frames as f64*12./seconds/1048576.,
        "latency_p50_ms":percentile(&m.latency,0.5),"latency_p95_ms":percentile(&m.latency,0.95),
        "wasm_p95_ms":percentile(&m.processing,0.95),"raf_p50_ms":percentile(&m.frame_times,0.5),
        "raf_p95_ms":percentile(&m.frame_times,0.95),"raf_p99_ms":percentile(&m.frame_times,0.99),
        "submit_p95_ms":percentile(&m.submit,0.95),"render_fps":m.rendered as f64/seconds,"sequence_gaps":m.gaps,
        "clock_rtt_ms":e.clock_rtt,"dpr":web_sys::window().unwrap().device_pixel_ratio(),"canvas":[e.gpu.as_ref().map(|g|g.borrow().canvas.width()),e.gpu.as_ref().map(|g|g.borrow().canvas.height())],
        "runtime":e.ui.runtime.get_untracked(),"gpu":e.ui.gpu.get_untracked(),"renderer":if webgl_selected(){"webgl2"}else{"webgpu"},"compute_check":e.ui.check.get_untracked(),"errors":FAULTS.with_borrow(Clone::clone)})
}
fn receive(generation: u32, value: JsValue, json_mode: bool) -> Option<(u32, u32)> {
    let began = now();
    if engine(|e| e.generation != generation) {
        return None;
    }
    let packet = if json_mode {
        serde_wasm_bindgen::from_value::<Batch>(value).map_err(|e| e.to_string())
    } else {
        Batch::decode(&js_sys::Uint8Array::new(&value).to_vec()).map_err(str::to_owned)
    };
    let packet = match packet {
        Ok(p) => p,
        Err(e) => {
            fail(e);
            return None;
        }
    };
    engine(|e| {
        if e.session != 0 && e.session != packet.session {
            return None;
        }
        if packet.seq != e.expected || packet.first != e.expected_first {
            e.metrics.gaps += 1;
        }
        e.expected = packet.seq + 1;
        e.expected_first = packet.first + (packet.samples.len() / CHANNELS) as u64;
        let frames = packet.samples.len() / CHANNELS;
        if !e.paused {
            let first = frames.min(CAPACITY - e.write);
            e.ring[e.write * CHANNELS..(e.write + first) * CHANNELS]
                .copy_from_slice(&packet.samples[..first * CHANNELS]);
            if first < frames {
                e.ring[..(frames - first) * CHANNELS]
                    .copy_from_slice(&packet.samples[first * CHANNELS..]);
            }
            e.write = (e.write + frames) % CAPACITY;
            e.filled = (e.filled + frames).min(CAPACITY);
            if let Some(gpu) = &e.gpu {
                gpu.borrow_mut().upload(&packet.samples);
            }
            e.dirty = true;
        }
        e.metrics.frames += frames as u64;
        e.metrics.packets += 1;
        bounded_push(
            &mut e.metrics.latency,
            began + e.clock_offset - packet.created_ms,
        );
        bounded_push(&mut e.metrics.processing, now() - began);
        Some((packet.session, packet.seq))
    })
}
async fn start_stream(config: Config) -> Result<(), String> {
    config.validate()?;
    if let Some(select) = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .get_element_by_id("transport")
        .and_then(|e| e.dyn_into::<web_sys::HtmlSelectElement>().ok())
    {
        select.set_value(&config.mode);
    }
    FAULTS.with_borrow_mut(Vec::clear);
    let generation = engine(|e| {
        e.generation += 1;
        e.session = 0;
        if let Some(ws) = e.ws.take() {
            ws.set_onmessage(None);
            ws.set_onerror(None);
            let _ = ws.close();
        }
        if let Some(channel) = e.channel.take() {
            drop_channel(channel);
        }
        e.callback = None;
        e.ws_message = None;
        e.ws_error = None;
        e.expected = 0;
        e.expected_first = 0;
        e.config = config.clone();
        e.metrics = Metrics {
            since: now(),
            ..Default::default()
        };
        e.offset = 0;
        e.paused = false;
        e.ui.paused.set(false);
        e.write = 0;
        e.filled = 0;
        if let Some(gpu) = &e.gpu {
            gpu.borrow_mut().reset();
        }
        e.ui.status.set(format!(
            "Streaming {} · {} frames/s",
            config.mode, config.rate
        ));
        e.generation
    });
    invoke("stop_stream", json!({})).await?;
    let json_mode = config.mode == "json";
    let callback = Closure::<dyn FnMut(JsValue)>::new(move |value| {
        if let Some((session, seq)) = receive(generation, value, json_mode) {
            spawn_local(async move {
                if let Err(e) = invoke("acknowledge", json!({"session":session,"seq":seq})).await {
                    fail(e);
                }
            });
        }
    });
    let id = make_channel(callback.as_ref().unchecked_ref());
    engine(|e| {
        e.channel = Some(id);
        e.callback = Some(callback);
    });
    let response = invoke(
        "start_stream",
        json!({"config":config,"onBatch":format!("__CHANNEL__:{id}")}),
    )
    .await?;
    let response: Value = serde_wasm_bindgen::from_value(response).map_err(|e| e.to_string())?;
    let session = response["session"].as_u64().ok_or("Missing stream id")? as u32;
    engine(|e| e.session = session);
    if config.mode == "ws" {
        let ws = web_sys::WebSocket::new(response["url"].as_str().unwrap())
            .map_err(|e| format!("{e:?}"))?;
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
        let sender = ws.clone();
        let message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
            move |event: web_sys::MessageEvent| {
                if let Some((_, seq)) = receive(generation, event.data(), false) {
                    if let Err(e) = sender.send_with_str(&seq.to_string()) {
                        fail(format!("WebSocket ack: {e:?}"));
                    }
                }
            },
        );
        let error = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            fail("WebSocket disconnected; apply settings to reconnect".into())
        });
        ws.set_onmessage(Some(message.as_ref().unchecked_ref()));
        ws.set_onerror(Some(error.as_ref().unchecked_ref()));
        engine(|e| {
            e.ws = Some(ws);
            e.ws_message = Some(message);
            e.ws_error = Some(error);
        });
    } else if config.mode == "invoke" || config.mode == "protocol" {
        let protocol = config.mode == "protocol";
        let protocol_url = format!(
            "{}/{}",
            protocol_base().trim_end_matches('/'),
            response["protocolToken"].as_str().unwrap()
        );
        spawn_local(async move {
            let mut ack: Option<u32> = None;
            loop {
                if engine(|e| e.generation != generation) {
                    break;
                }
                let packet = if protocol {
                    let url = match ack {
                        Some(seq) => format!("{protocol_url}?ack={seq}"),
                        None => protocol_url.clone(),
                    };
                    JsFuture::from(fetch_packet(&url))
                        .await
                        .map_err(|e| format!("Protocol fetch: {e:?}"))
                } else {
                    invoke("pull_batch", json!({"session":session,"ack":ack})).await
                };
                match packet {
                    Ok(value) => {
                        if let Some((_, seq)) = receive(generation, value, false) {
                            ack = Some(seq);
                        } else {
                            break;
                        }
                    }
                    Err(e) => {
                        if engine(|s| s.generation == generation) {
                            fail(e);
                        }
                        break;
                    }
                }
            }
        });
    }
    Ok(())
}
async fn calibrate_clock() -> Result<(), String> {
    let mut best = (f64::INFINITY, 0.);
    for _ in 0..9 {
        let t = now();
        let backend = invoke("clock", json!({}))
            .await?
            .as_f64()
            .ok_or("Clock response")?;
        let end = now();
        if end - t < best.0 {
            best = (end - t, backend - (t + end) * 0.5);
        }
    }
    engine(|e| {
        e.clock_offset = best.1;
        e.clock_rtt = best.0;
    });
    Ok(())
}
async fn apply_settings() {
    if engine(|e| e.ui.busy.get_untracked()) {
        return;
    }
    engine(|e| e.ui.busy.set(true));
    if let Err(e) = start_stream(selected_config(None)).await {
        fail(e);
    }
    engine(|e| e.ui.busy.set(false));
}
fn webgl_selected() -> bool {
    web_sys::window()
        .unwrap()
        .location()
        .search()
        .unwrap_or_default()
        .trim_start_matches('?')
        .split('&')
        .any(|part| part == "renderer=webgl")
}
fn switch_renderer(webgl: bool) {
    let location = web_sys::window().unwrap().location();
    let old = location.search().unwrap_or_default();
    let mut parts = old
        .trim_start_matches('?')
        .split('&')
        .filter(|p| !p.is_empty() && !p.starts_with("renderer="))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if webgl {
        parts.push("renderer=webgl".into());
    }
    if let Err(e) = location.set_search(&parts.join("&")) {
        fail(format!("Switch renderer: {e:?}"));
    }
}
async fn initialize() -> Result<(), String> {
    let runtime = invoke("runtime_info", json!({})).await?;
    let runtime: Value = serde_wasm_bindgen::from_value(runtime).map_err(|e| e.to_string())?;
    engine(|e| {
        e.ui.runtime.set(format!(
            "{} · {}",
            runtime["name"].as_str().unwrap_or("unknown"),
            runtime["version"].as_str().unwrap_or("unknown")
        ))
    });
    let webgl = webgl_selected();
    let mut gpu = renderer::Renderer::new(webgl).await?;
    if !webgl {
        let info = JsFuture::from(adapter_details())
            .await
            .map_err(|e| format!("Adapter info: {e:?}"))?;
        let info: Value = serde_wasm_bindgen::from_value(info).map_err(|e| e.to_string())?;
        if info["isFallbackAdapter"].as_bool() != Some(false) {
            return Err("Hardware WebGPU adapter required".into());
        }
        gpu.description = format!(
            "{} {} · WebGPU · hardware adapter",
            info["vendor"].as_str().unwrap_or("unknown"),
            info["architecture"].as_str().unwrap_or("unknown")
        );
    }
    engine(|e| e.ui.gpu.set(gpu.description.clone()));
    let check = gpu.verify_compute().await?;
    engine(|e| {
        e.ui.check.set(check);
        e.gpu = Some(Rc::new(RefCell::new(gpu)));
    });
    // Frozen plots must still rebuild their backing surface on resize/DPI change.
    let resize = Closure::<dyn FnMut(web_sys::Event)>::new(|_| engine(|e| e.dirty = true));
    web_sys::window()
        .unwrap()
        .add_event_listener_with_callback("resize", resize.as_ref().unchecked_ref())
        .map_err(|e| format!("Resize listener: {e:?}"))?;
    resize.forget();
    calibrate_clock().await?;
    start_stream(selected_config(None)).await?;
    start_animation();
    if web_sys::window()
        .unwrap()
        .location()
        .search()
        .unwrap_or_default()
        .contains("benchmark")
    {
        spawn_local(async {
            compare().await;
        });
    }
    Ok(())
}
fn toggle_pause() {
    engine(|e| {
        e.paused = !e.paused;
        e.ui.paused.set(e.paused);
        e.offset = 0;
        e.dirty = true;
        if !e.paused {
            e.write = 0;
            e.filled = 0;
            if let Some(g) = &e.gpu {
                g.borrow_mut().reset();
            }
        }
    });
}
fn zoom(delta: f64) {
    engine(|e| {
        e.visible = ((e.visible as f64) * delta.exp())
            .round()
            .clamp(64., CAPACITY as f64) as usize;
        e.dirty = true;
    });
}
fn start_animation() {
    let callback = Rc::new(RefCell::new(None::<Closure<dyn FnMut(f64)>>));
    let next = callback.clone();
    *callback.borrow_mut() = Some(Closure::new(move |timestamp: f64| {
        let error = engine(|e| {
            if e.last_frame > 0. {
                bounded_push(&mut e.metrics.frame_times, timestamp - e.last_frame);
            }
            e.last_frame = timestamp;
            let mut error = None;
            if (!e.paused || e.dirty)
                && let Some(g) = &e.gpu
            {
                let t = now();
                if let Err(err) =
                    g.borrow_mut()
                        .render(e.visible, e.offset, e.config.amplitude as f32)
                {
                    error = Some(err);
                }
                bounded_push(&mut e.metrics.submit, now() - t);
                e.metrics.rendered += 1;
                e.dirty = false;
            }
            if timestamp - e.last_ui > 500. {
                let s = snapshot(e);
                e.ui.stats.set(format!("{:.1} MiB/s   |   {:.2} Mframes/s   |   {:.1} fps\nDelivery p95 {:.2} ms   |   WASM p95 {:.2} ms\nFrame p95 {:.2} ms   |   CPU submit p95 {:.2} ms   |   Gaps {}",
                    s["payload_mib_s"].as_f64().unwrap(),s["frames_per_s"].as_f64().unwrap()/1e6,s["render_fps"].as_f64().unwrap(),
                    s["latency_p95_ms"].as_f64().unwrap(),s["wasm_p95_ms"].as_f64().unwrap(),s["raf_p95_ms"].as_f64().unwrap(),s["submit_p95_ms"].as_f64().unwrap(),e.metrics.gaps));
                let span = e.visible.min(e.filled) as f64 / e.config.rate as f64;
                e.ui.view.set(format!(
                    "{:.3} s across screen · {} frames · {:.2}× DPR · {}",
                    span,
                    e.visible,
                    web_sys::window().unwrap().device_pixel_ratio(),
                    if e.paused { "Frozen history" } else { "Live" }
                ));
                e.last_ui = timestamp;
            }
            error
        });
        if let Some(e) = error {
            fail(e);
        }
        web_sys::window()
            .unwrap()
            .request_animation_frame(next.borrow().as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }));
    web_sys::window()
        .unwrap()
        .request_animation_frame(callback.borrow().as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
    // The callback owns its scheduling handle for the lifetime of this page.
}
async fn save_results() -> Result<(), String> {
    let report = engine(
        |e| json!({"live":snapshot(e),"runs":e.ui.rows.get_untracked(),"method":"2 s warmup, 6 s sample, two passes in opposite order; payload bytes exclude JSON/header overhead; CPU submit is not GPU execution time; no samples intentionally dropped; all modes have 4-packet credit window except sequential invoke/protocol pull"}),
    );
    let path = invoke("save_benchmark", json!({"report":report})).await?;
    engine(|e| {
        e.ui.saved
            .set(format!("Saved {}", path.as_string().unwrap_or_default()))
    });
    Ok(())
}
async fn compare() {
    if engine(|e| e.ui.busy.get_untracked()) {
        return;
    }
    engine(|e| {
        e.ui.busy.set(true);
        e.ui.rows.set(Vec::new());
    });
    let config = selected_config(None);
    let result: Result<(), String> = async {
        for (index, mode) in [
            "json", "binary", "invoke", "protocol", "ws", "ws", "protocol", "invoke", "binary",
            "json",
        ]
        .iter()
        .enumerate()
        {
            let mut c = config.clone();
            c.mode = (*mode).into();
            start_stream(c).await?;
            engine(|e| {
                e.ui.status
                    .set(format!("Comparison {}/10 · {mode} · warming up", index + 1))
            });
            delay(2000).await;
            engine(|e| {
                e.metrics = Metrics {
                    since: now(),
                    ..Default::default()
                };
                e.last_frame = 0.;
                e.ui.status.set(format!(
                    "Comparison {}/10 · {mode} · measuring 6 s",
                    index + 1
                ));
            });
            delay(6000).await;
            if let Some(error) = FAULTS.with_borrow(|f| f.first().cloned()) {
                return Err(error);
            }
            let mut row = engine(|e| snapshot(e));
            let session = engine(|e| e.session);
            let stats = invoke("stream_stats", json!({"session":session})).await?;
            row["backend"] = serde_wasm_bindgen::from_value(stats).map_err(|e| e.to_string())?;
            row["pass"] = json!(if index < 5 { 1 } else { 2 });
            web_sys::console::info_1(&format!("benchmark-run {row}").into());
            engine(|e| e.ui.rows.update(|rows| rows.push(row)));
        }
        engine(|e| {
            e.ui.status.set(
                "Comparison complete. Compare paired runs; repeat with another rate or batch size."
                    .into(),
            )
        });
        save_results().await
    }
    .await;
    engine(|e| e.ui.busy.set(false));
    if let Err(e) = result {
        fail(e);
    }
}
fn row_view(row: Value) -> impl IntoView {
    let mode = row["mode"].as_str().unwrap_or_default().to_owned();
    view! {<tr><td>{format!("{} / {}",mode,row["pass"])}</td><td>{format!("{:.1}",row["payload_mib_s"].as_f64().unwrap_or_default())}</td><td>{format!("{:.2}",row["latency_p95_ms"].as_f64().unwrap_or_default())}</td><td>{format!("{:.2}",row["wasm_p95_ms"].as_f64().unwrap_or_default())}</td><td>{format!("{:.2}",row["raf_p95_ms"].as_f64().unwrap_or_default())}</td><td>{format!("{:.1}",row["render_fps"].as_f64().unwrap_or_default())}</td><td>{row["sequence_gaps"].to_string()}</td></tr>}
}
#[component]
fn App() -> impl IntoView {
    let ui = Ui {
        status: RwSignal::new("Initializing renderer…".into()),
        stats: RwSignal::new("Waiting for samples".into()),
        runtime: RwSignal::new(String::new()),
        gpu: RwSignal::new("Requesting hardware adapter…".into()),
        check: RwSignal::new("Pending compute readback".into()),
        view: RwSignal::new(String::new()),
        paused: RwSignal::new(false),
        busy: RwSignal::new(true),
        rows: RwSignal::new(Vec::new()),
        saved: RwSignal::new(String::new()),
    };
    ENGINE.with_borrow_mut(|slot| {
        *slot = Some(Engine {
            ui,
            gpu: None,
            generation: 0,
            session: 0,
            config: Config {
                mode: "binary".into(),
                rate: 1_000_000,
                batch: 16384,
                frequency: 7.,
                amplitude: 1.,
            },
            callback: None,
            channel: None,
            ws: None,
            ws_message: None,
            ws_error: None,
            expected: 0,
            expected_first: 0,
            clock_offset: 0.,
            clock_rtt: 0.,
            metrics: Metrics::default(),
            visible: 262144,
            offset: 0,
            paused: false,
            dirty: true,
            last_frame: 0.,
            last_ui: 0.,
            ring: vec![0.; CAPACITY * CHANNELS],
            write: 0,
            filled: 0,
            drag: None,
        })
    });
    spawn_local(async {
        if let Err(e) = initialize().await {
            fail(e);
        }
        engine(|e| e.ui.busy.set(false));
    });
    view! {
        <main>
            <header><div><h1>"Wavebench"</h1><p>"Rust source → WASM → GPU"</p></div><div class="legend"><span class="a">"● Phase A"</span><span class="b">"● Phase B"</span><span class="c">"● Phase C"</span></div></header>
            <section class="toolbar" aria-label="Stream settings">
                <label>"Renderer"<select id="renderer" disabled=move||ui.busy.get() on:change=move|_|switch_renderer(value("renderer")=="webgl")><option value="webgpu" selected=!webgl_selected()>"WebGPU / compute"</option><option value="webgl" selected=webgl_selected()>"WebGL2 / no compute"</option></select></label>
                <label>"Transport"<select id="transport" disabled=move||ui.busy.get()><option value="binary">"Binary Channel"</option><option value="json">"JSON Channel"</option><option value="invoke">"Binary invoke pull"</option><option value="ws" selected>"Binary WebSocket"</option><option value="protocol">"Custom protocol fetch"</option></select></label>
                <label>"Frames / second"<select id="rate" disabled=move||ui.busy.get()><option value="20000">"20,000"</option><option value="200000">"200,000"</option><option value="1000000" selected>"1,000,000"</option><option value="4000000">"4,000,000"</option><option value="8000000">"8,000,000"</option></select></label>
                <label>"Frames / batch"<select id="batch" disabled=move||ui.busy.get()><option value="256">"256"</option><option value="1024">"1,024"</option><option value="4096">"4,096"</option><option value="16384" selected>"16,384"</option><option value="65536">"65,536"</option></select></label>
                <label>"Frequency, Hz"<input id="frequency" type="number" min="0.1" max="1000" step="0.1" value="7" disabled=move||ui.busy.get()/></label>
                <label>"Amplitude"<input id="amplitude" type="number" min="0.1" max="10" step="0.1" value="1" disabled=move||ui.busy.get()/></label>
                <button class="primary" disabled=move||ui.busy.get() on:click=move|_|spawn_local(apply_settings())>"Apply / restart"</button>
            </section>
            <section class="scope" aria-label="Live waveform">
                <div class="plotbar"><span>{move||ui.view.get()}</span><div><button disabled=move||ui.busy.get() on:click=move|_|toggle_pause()>{move||if ui.paused.get(){"Resume plot"}else{"Freeze plot"}}</button><button on:click=move|_|zoom(-0.4)>"Zoom in"</button><button on:click=move|_|zoom(0.4)>"Zoom out"</button></div></div>
                <div class="plot-wrap"><div class="axis"><span>{move||{let _=ui.view.get();engine(|e|format!("+{:.1} A",e.config.amplitude/0.72))}}</span><span>"0"</span><span>{move||{let _=ui.view.get();engine(|e|format!("−{:.1} A",e.config.amplitude/0.72))}}</span></div>
                <canvas id="plot" on:wheel=move|event:web_sys::WheelEvent|{event.prevent_default();zoom(event.delta_y()*0.001);}
                    on:dblclick=move|_|toggle_pause()
                    on:pointerdown=move|event:web_sys::PointerEvent|{engine(|e|if e.paused{e.drag=Some((event.client_x() as f64,e.offset));});}
                    on:pointerup=move|_|engine(|e|e.drag=None)
                    on:pointerleave=move|_|engine(|e|e.drag=None)
                    on:pointermove=move|event:web_sys::PointerEvent|{engine(|e|if let Some((x,offset))=e.drag{let width=e.gpu.as_ref().map(|g|g.borrow().canvas.client_width()).unwrap_or(1).max(1) as f64;e.offset=(offset as f64+(event.client_x() as f64-x)*e.visible as f64/width).clamp(0.,e.filled.saturating_sub(e.visible) as f64) as usize;e.dirty=true;});}></canvas></div>
                <div class="plotfoot"><span>"Wheel to zoom · double-click to freeze · drag frozen history"</span><span>"3 × f32 · 1,048,576-frame ring · GPU min/max"</span></div>
            </section>
            <section class="telemetry"><pre>{move||ui.stats.get()}</pre><div class="device"><strong>{move||ui.runtime.get()}</strong><strong>{move||ui.gpu.get()}</strong><span>{move||ui.check.get()}</span><button disabled=move||ui.runtime.get().starts_with("WebKitGTK") on:click=move|_|spawn_local(async{if let Err(e)=invoke("open_gpu",json!({})).await{fail(e);}})>"GPU diagnostics"</button></div></section>
            <section class="comparison"><div class="comparison-head"><div><h2>"Transport comparison"</h2><p>"Same source and load. Two passes, reverse order; 2 s warmup + 6 s per run."</p></div><div><button class="primary" disabled=move||ui.busy.get() on:click=move|_|spawn_local(compare())>{move||if ui.busy.get(){"Measuring…"}else{"Compare all · 80 s"}}</button><button on:click=move|_|spawn_local(async{if let Err(e)=save_results().await{fail(e);}})>"Save report"</button></div></div>
                <div class="table-scroll"><table><thead><tr><th>"Transport / pass"</th><th>"Payload MiB/s"</th><th>"Delivery p95 ms"</th><th>"WASM p95 ms"</th><th>"Frame p95 ms"</th><th>"Render fps"</th><th>"Gaps"</th></tr></thead><tbody>{move||ui.rows.get().into_iter().map(row_view).collect_view()}</tbody></table></div>
                <p class="method">"Delivery: packet creation → WASM entry, calibrated clocks. WASM: decode + ring copy + GPU upload call. Submit is CPU time, not GPU execution. Payload excludes transport encoding. Four packets in flight; invoke/protocol pulls are sequential."</p>
            </section>
            <footer role="status"><span>{move||ui.status.get()}</span><span>{move||ui.saved.get()}</span></footer>
        </main>
    }
}
#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}
