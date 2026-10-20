use axum::{
    Router,
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use plot_wire::{Batch, Config, Generator};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::ipc::{Channel, InvokeResponseBody};
use tokio::sync::{Mutex as AsyncMutex, Notify, mpsc};

const WINDOW: u32 = 4;
#[derive(Default)]
struct Counters {
    packets: u64,
    frames: u64,
    generation_ms: f64,
    delivery_ms: f64,
    blocked_ms: f64,
}
pub struct Session {
    pub id: u32,
    token: String,
    config: Config,
    cancelled: AtomicBool,
    sent: AtomicU32,
    acknowledged: AtomicU32,
    gate: Mutex<()>,
    wake: Condvar,
    done: Notify,
    receiver: AsyncMutex<mpsc::Receiver<Vec<u8>>>,
    counters: Mutex<Counters>,
    started: Instant,
}
impl Session {
    fn stop(&self) {
        let _guard = self.gate.lock().unwrap();
        self.cancelled.store(true, Ordering::Release);
        self.wake.notify_all();
        self.done.notify_waiters();
    }
    fn ack(&self, seq: u32) {
        let _guard = self.gate.lock().unwrap();
        // Acknowledgements count delivered packets, never arbitrary future indices.
        let count = seq.saturating_add(1).min(self.sent.load(Ordering::Acquire));
        self.acknowledged.fetch_max(count, Ordering::Release);
        self.wake.notify_all();
    }
    fn wait(&self, duration: Duration) {
        let guard = self.gate.lock().unwrap();
        if !self.cancelled.load(Ordering::Acquire) {
            drop(self.wake.wait_timeout(guard, duration).unwrap());
        }
    }
    fn wait_for_credit(&self, seq: u32) {
        let mut guard = self.gate.lock().unwrap();
        while seq.saturating_sub(self.acknowledged.load(Ordering::Acquire)) >= WINDOW
            && !self.cancelled.load(Ordering::Acquire)
        {
            guard = self.wake.wait(guard).unwrap();
        }
    }
}
#[derive(Clone)]
pub struct Streams {
    current: Arc<Mutex<Option<Arc<Session>>>>,
    clock: Instant,
    next: Arc<AtomicU32>,
    pub port: u16,
}
impl Streams {
    pub fn new(port: u16) -> Self {
        Self {
            current: Default::default(),
            clock: Instant::now(),
            next: Arc::new(AtomicU32::new(1)),
            port,
        }
    }
    pub fn now(&self) -> f64 {
        self.clock.elapsed().as_secs_f64() * 1000.
    }
    pub fn session(&self, id: u32) -> Result<Arc<Session>, String> {
        self.current
            .lock()
            .unwrap()
            .as_ref()
            .filter(|s| s.id == id)
            .cloned()
            .ok_or("Stream replaced".into())
    }
    pub fn stop(&self) {
        if let Some(s) = self.current.lock().unwrap().take() {
            s.stop();
        }
    }
    pub fn start(&self, config: Config, channel: Channel) -> Result<Value, String> {
        config.validate()?;
        self.stop();
        let (tx, rx) = mpsc::channel(WINDOW as usize);
        let s = Arc::new(Session {
            id: self.next.fetch_add(1, Ordering::Relaxed),
            token: uuid::Uuid::new_v4().to_string(),
            config,
            cancelled: AtomicBool::new(false),
            sent: AtomicU32::new(0),
            acknowledged: AtomicU32::new(0),
            gate: Mutex::new(()),
            wake: Condvar::new(),
            done: Notify::new(),
            receiver: AsyncMutex::new(rx),
            counters: Mutex::new(Counters::default()),
            started: Instant::now(),
        });
        *self.current.lock().unwrap() = Some(s.clone());
        let answer = json!({"session":s.id,"url":format!("ws://127.0.0.1:{}/stream/{}",self.port,s.token),"protocolToken":s.token,"window":WINDOW});
        let state = self.clone();
        std::thread::Builder::new()
            .name("wave-source".into())
            .spawn(move || {
                let period = Duration::from_secs_f64(s.config.batch as f64 / s.config.rate as f64);
                let generator = Generator::new(s.config.clone());
                let mut deadline = Instant::now();
                let mut seq = 0u32;
                let mut first = 0u64;
                while !s.cancelled.load(Ordering::Acquire) {
                    let blocked = Instant::now();
                    s.wait_for_credit(seq);
                    let blocked_ms = blocked.elapsed().as_secs_f64() * 1000.;
                    while Instant::now() < deadline && !s.cancelled.load(Ordering::Acquire) {
                        s.wait(deadline.saturating_duration_since(Instant::now()));
                    }
                    if s.cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    let gen_start = Instant::now();
                    let samples = generator.batch(first);
                    let generation_ms = gen_start.elapsed().as_secs_f64() * 1000.;
                    let packet = Batch {
                        session: s.id,
                        seq,
                        first,
                        rate: s.config.rate,
                        created_ms: state.now(),
                        samples,
                    };
                    // Publish before send: a fast consumer may acknowledge inside send().
                    s.sent.store(seq + 1, Ordering::Release);
                    let delivery = Instant::now();
                    let result = match s.config.mode.as_str() {
                        "json" => channel
                            .send(InvokeResponseBody::Json(
                                serde_json::to_string(&packet).unwrap(),
                            ))
                            .map_err(|e| e.to_string()),
                        "binary" => channel
                            .send(InvokeResponseBody::Raw(packet.encode()))
                            .map_err(|e| e.to_string()),
                        _ => tx.blocking_send(packet.encode()).map_err(|e| e.to_string()),
                    };
                    if result.is_err() {
                        break;
                    }
                    {
                        let mut c = s.counters.lock().unwrap();
                        c.packets += 1;
                        c.frames += s.config.batch as u64;
                        c.generation_ms += generation_ms;
                        c.delivery_ms += delivery.elapsed().as_secs_f64() * 1000.;
                        c.blocked_ms += blocked_ms;
                    }
                    seq += 1;
                    first += s.config.batch as u64;
                    deadline = (deadline + period).max(Instant::now());
                }
                s.stop();
            })
            .map_err(|e| e.to_string())?;
        Ok(answer)
    }
    pub fn stats(&self, id: u32) -> Result<Value, String> {
        let s = self.session(id)?;
        let c = s.counters.lock().unwrap();
        let n = (c.packets as f64).max(1.);
        Ok(
            json!({"packets":c.packets,"frames":c.frames,"elapsed_s":s.started.elapsed().as_secs_f64(),
            "generation_ms":c.generation_ms/n,"delivery_ms":c.delivery_ms/n,"blocked_ms":c.blocked_ms,
            "inflight":s.sent.load(Ordering::Acquire).saturating_sub(s.acknowledged.load(Ordering::Acquire)),"window":WINDOW}),
        )
    }
    pub fn ack(&self, id: u32, seq: u32) {
        if let Ok(s) = self.session(id) {
            s.ack(seq)
        }
    }
    pub async fn protocol_pull(&self, token: &str, ack: Option<u32>) -> Result<Vec<u8>, String> {
        let id = self
            .current
            .lock()
            .unwrap()
            .as_ref()
            .filter(|s| s.config.mode == "protocol" && s.token == token)
            .map(|s| s.id)
            .ok_or("Unknown stream token")?;
        self.pull(id, ack).await
    }
    pub async fn pull(&self, id: u32, ack: Option<u32>) -> Result<Vec<u8>, String> {
        let s = self.session(id)?;
        if let Some(seq) = ack {
            s.ack(seq);
        }
        let mut rx = s.receiver.lock().await;
        tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .map_err(|_| "Stream timed out")?
            .ok_or("Stream stopped".into())
    }
}
pub async fn serve(listener: tokio::net::TcpListener, state: Streams) {
    let router = Router::new()
        .route("/stream/{token}", get(upgrade))
        .with_state(state);
    if let Err(e) = axum::serve(listener, router).await {
        eprintln!("WebSocket server: {e}");
    }
}
async fn upgrade(
    State(state): State<Streams>,
    Path(token): Path<String>,
    ws: WebSocketUpgrade,
) -> Response {
    let s = state
        .current
        .lock()
        .unwrap()
        .as_ref()
        .filter(|s| s.token == token && s.config.mode == "ws")
        .cloned();
    match s {
        Some(s) => ws.on_upgrade(move |socket| socket_stream(socket, s)),
        None => StatusCode::FORBIDDEN.into_response(),
    }
}
async fn socket_stream(socket: WebSocket, s: Arc<Session>) {
    let (mut sink, mut source) = socket.split();
    let read_session = s.clone();
    let reader = async move {
        while let Some(Ok(message)) = source.next().await {
            match message {
                Message::Text(text) => {
                    if let Ok(seq) = text.parse() {
                        read_session.ack(seq)
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    };
    let writer = async {
        let mut rx = s.receiver.lock().await;
        while let Some(packet) = rx.recv().await {
            if sink.send(Message::Binary(packet.into())).await.is_err() {
                break;
            }
        }
    };
    tokio::select! { _=reader=>{},_=writer=>{},_=s.done.notified()=>{} }
    s.stop();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn producer_obeys_credit_window_and_wakes_on_ack() {
        let streams = Streams::new(0);
        let (tx, rx) = std::sync::mpsc::channel();
        let channel = Channel::new(move |body| {
            tx.send(body).unwrap();
            Ok(())
        });
        let result = streams
            .start(
                Config {
                    mode: "binary".into(),
                    rate: 8_000_000,
                    batch: 64,
                    frequency: 7.,
                    amplitude: 1.,
                },
                channel,
            )
            .unwrap();
        let id = result["session"].as_u64().unwrap() as u32;
        for expected in 0..WINDOW {
            let InvokeResponseBody::Raw(bytes) = rx.recv_timeout(Duration::from_secs(2)).unwrap()
            else {
                panic!("Expected binary")
            };
            assert_eq!(Batch::decode(&bytes).unwrap().seq, expected);
        }
        assert!(rx.recv_timeout(Duration::from_millis(30)).is_err());
        streams.ack(id, 0);
        let InvokeResponseBody::Raw(bytes) = rx.recv_timeout(Duration::from_secs(2)).unwrap()
        else {
            panic!("Expected binary")
        };
        assert_eq!(Batch::decode(&bytes).unwrap().seq, WINDOW);
        streams.stop();
    }
}
