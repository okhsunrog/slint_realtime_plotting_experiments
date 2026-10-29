//! The experimental render worker shares the production shader and playback clock.
use crate::{Metrics, bounded_push, gpu::Gpu, percentile, playback::PlaybackClock};
use plot_wire::{Batch, CAPACITY, CHANNELS, Config};
use serde_json::json;
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = "export function worker_now(){return performance.now()}")]
extern "C" {
    fn worker_now() -> f64;
}
struct State {
    gpu: Gpu,
    config: Option<Config>,
    session: u32,
    expected: u32,
    first: u64,
    metrics: Metrics,
    clock: PlaybackClock,
    clock_offset: f64,
    filled: usize,
    visible: usize,
    offset: usize,
    paused: bool,
    dirty: bool,
    head: u64,
    last_frame: f64,
    dimensions: (u32, u32, f64),
    check: String,
}
thread_local! { static STATE: RefCell<Option<State>> = const {RefCell::new(None)}; }
fn state<T>(f: impl FnOnce(&mut State) -> T) -> T {
    STATE.with_borrow_mut(|s| f(s.as_mut().unwrap()))
}
#[wasm_bindgen]
pub async fn worker_init(canvas: web_sys::OffscreenCanvas) -> Result<String, JsValue> {
    let gpu = Gpu::new_offscreen(canvas)
        .await
        .map_err(|e| JsValue::from_str(&e))?;
    let check = gpu
        .verify_compute()
        .await
        .map_err(|e| JsValue::from_str(&e))?;
    let result = json!({"gpu":gpu.description,"compute_check":check}).to_string();
    STATE.with_borrow_mut(|s| {
        *s = Some(State {
            gpu,
            config: None,
            session: 0,
            expected: 0,
            first: 0,
            metrics: Metrics::default(),
            clock: PlaybackClock::default(),
            clock_offset: 0.,
            filled: 0,
            visible: 262144,
            offset: 0,
            paused: false,
            dirty: true,
            head: 0,
            last_frame: 0.,
            dimensions: (1, 1, 1.),
            check,
        })
    });
    Ok(result)
}
#[wasm_bindgen]
pub fn worker_start(config: &str, session: u32, clock_offset: f64) -> Result<(), JsValue> {
    let config: Config =
        serde_json::from_str(config).map_err(|e| JsValue::from_str(&e.to_string()))?;
    config.validate().map_err(|e| JsValue::from_str(&e))?;
    state(|s| {
        s.config = Some(config);
        s.session = session;
        s.clock_offset = clock_offset;
        s.expected = 0;
        s.first = 0;
        s.metrics = Metrics {
            since: worker_now(),
            ..Default::default()
        };
        s.clock.reset();
        s.filled = 0;
        s.offset = 0;
        s.head = 0;
        s.last_frame = 0.;
        s.paused = false;
        s.dirty = true;
        s.gpu.reset();
    });
    Ok(())
}
#[wasm_bindgen]
pub fn worker_receive(buffer: &js_sys::ArrayBuffer) -> Result<u32, JsValue> {
    let began = worker_now();
    let packet =
        Batch::decode(&js_sys::Uint8Array::new(buffer).to_vec()).map_err(JsValue::from_str)?;
    state(|s| {
        if packet.session != s.session || packet.rate != s.config.as_ref().unwrap().rate {
            return Err(JsValue::from_str("Worker stream identity mismatch"));
        }
        if packet.seq != s.expected || packet.first != s.first {
            s.metrics.gaps += 1;
        }
        let frames = packet.samples.len() / CHANNELS;
        s.expected = packet.seq + 1;
        s.first = packet.first + frames as u64;
        if !s.paused {
            s.clock.receive(frames, began);
            s.filled = (s.filled + frames).min(CAPACITY);
            s.gpu.upload(&packet.samples);
            s.dirty = true;
        }
        s.metrics.frames += frames as u64;
        s.metrics.packets += 1;
        bounded_push(
            &mut s.metrics.latency,
            began + s.clock_offset - packet.created_ms,
        );
        bounded_push(&mut s.metrics.processing, worker_now() - began);
        Ok(packet.seq)
    })
}
#[wasm_bindgen]
pub fn worker_frame(timestamp: f64) -> Result<js_sys::Float64Array, JsValue> {
    state(|s| {
        if s.last_frame > 0. {
            bounded_push(&mut s.metrics.frame_times, timestamp - s.last_frame);
        }
        s.last_frame = timestamp;
        if let Some(c) = &s.config {
            if !s.paused || s.dirty {
                if !s.paused {
                    s.offset =
                        s.clock
                            .offset(timestamp, c.rate, c.batch as usize, s.filled, s.visible);
                }
                s.head = s.clock.received().saturating_sub(s.offset as u64);
                let start = worker_now();
                s.gpu
                    .render(s.visible, s.offset, c.amplitude as f32)
                    .map_err(|e| JsValue::from_str(&e))?;
                bounded_push(&mut s.metrics.submit, worker_now() - start);
                s.metrics.rendered += 1;
                s.dirty = false;
            }
        }
        Ok(js_sys::Float64Array::from(
            &[
                timestamp,
                s.head as f64,
                s.clock.received() as f64,
                s.offset as f64,
            ][..],
        ))
    })
}
#[wasm_bindgen]
pub fn worker_control(command: &str, a: f64, b: f64, c: f64) {
    state(|s| {
        match command {
            "resize" => {
                s.dimensions = (a as u32, b as u32, c);
                s.gpu.resize(a as u32, b as u32, c);
            }
            "zoom" => {
                s.visible = (s.visible as f64 * a.exp())
                    .round()
                    .clamp(64., CAPACITY as f64) as usize
            }
            "pan" => {
                s.offset = (s.offset as f64 + a * s.visible as f64 / b.max(1.))
                    .clamp(0., s.filled.saturating_sub(s.visible) as f64)
                    as usize
            }
            "pause" => {
                s.paused = !s.paused;
                if !s.paused {
                    s.clock.reset();
                    s.gpu.reset();
                    s.filled = 0;
                    s.offset = 0;
                    s.head = 0;
                }
            }
            _ => {}
        };
        s.dirty = true;
    });
}
#[wasm_bindgen]
pub fn worker_snapshot() -> String {
    state(|s| {
        let m = &s.metrics;
        let seconds = ((worker_now() - m.since) / 1000.).max(0.001);
        let rate = s.config.as_ref().map(|c| c.rate).unwrap_or(1);
        json!({"session":s.session,"config":s.config,"mode":"ws","execution":"worker","seconds":seconds,
            "frames":m.frames,"packets":m.packets,"frames_per_s":m.frames as f64/seconds,"payload_mib_s":m.frames as f64*12./seconds/1048576.,
            "latency_p95_ms":percentile(&m.latency,0.95),"wasm_p95_ms":percentile(&m.processing,0.95),"raf_p95_ms":percentile(&m.frame_times,0.95),"submit_p95_ms":percentile(&m.submit,0.95),"render_fps":m.rendered as f64/seconds,"sequence_gaps":m.gaps,
            "dpr":s.dimensions.2,"canvas":[s.dimensions.0,s.dimensions.1],"motion":"clocked","display_offset_frames":s.offset,
            "visible":s.visible,"span":s.visible.min(s.filled) as f64/rate as f64,"buffer_ms":s.offset as f64*1000./rate as f64,"paused":s.paused,
            "playback_underruns":s.clock.underruns,"playback_catchups":s.clock.catchups,"gpu":s.gpu.description,"renderer":"webgpu","compute_check":s.check,"errors":crate::FAULTS.with_borrow(Clone::clone)}).to_string()
    })
}
