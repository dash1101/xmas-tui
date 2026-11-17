//! xmas-relay: the village's own relay for online play. Clients (the
//! desktop app, the phone app, the web page) speak a little MQTT over a
//! WebSocket at /mqtt on the public port (or, where a network breaks
//! WebSockets, over plain HTTPS requests at /poll: see poll.rs); the admin console (stats,
//! connections, limits, bans) is on a port of its own, meant to be reached
//! only through Cloudflare Access.
//!
//! Settings (environment):
//! - RELAY_PORT (8080), ADMIN_PORT (8081), RELAY_DATA (/data: limits and
//!   bans are kept there)
//! - ADMIN_EMAIL: when set, the console answers only requests Cloudflare
//!   Access has signed in as that address (the Cf-Access-Authenticated-
//!   User-Email header). Unset, it wants the header present at all.
//! - TRUST_CF (1): take the client's address from Cloudflare's
//!   CF-Connecting-IP header (behind a Cloudflare Tunnel, every connection
//!   comes from the tunnel itself).

mod admin;
mod broker;
mod codec;
mod poll;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use broker::{Broker, Kept};
use futures_util::{SinkExt, StreamExt};
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone)]
pub struct App {
    pub broker: Arc<Mutex<Broker>>,
    pub data: PathBuf,
    pub admin_email: Option<String>,
    pub trust_cf: bool,
    /// Clients on HTTPS long polling (where WebSockets don't get through).
    pub polls: poll::Polls,
}

impl App {
    /// What's kept, written out (every minute, and on the way out).
    pub fn save_kept(&self) {
        let snap = self.broker.lock().unwrap_or_else(|e| e.into_inner()).snapshot();
        let tmp = self.data.join("kept.bin.tmp");
        if std::fs::write(&tmp, snap).is_ok() {
            let _ = std::fs::rename(&tmp, self.data.join("kept.bin"));
        }
    }

    /// The limits and bans, written out (after a change in the console).
    pub fn save(&self) {
        let kept = self.broker.lock().unwrap_or_else(|e| e.into_inner()).kept.clone();
        if let Ok(s) = serde_json::to_string_pretty(&kept) {
            let tmp = self.data.join("relay.json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, self.data.join("relay.json"));
            }
        }
    }
}

fn env(k: &str, d: &str) -> String {
    std::env::var(k).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| d.to_string())
}

#[tokio::main]
async fn main() {
    let data = PathBuf::from(env("RELAY_DATA", "/data"));
    let _ = std::fs::create_dir_all(&data);
    let kept: Kept = std::fs::read_to_string(data.join("relay.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let app = App {
        broker: Arc::new(Mutex::new(Broker::new(kept))),
        data,
        admin_email: std::env::var("ADMIN_EMAIL").ok().filter(|e| !e.trim().is_empty()).map(|e| e.trim().to_lowercase()),
        trust_cf: env("TRUST_CF", "1") != "0",
        polls: poll::Polls::default(),
    };
    // what was kept before a restart (linked devices' sides, listings…),
    // and the squares' settings, out again
    {
        let mut b = app.broker.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(bytes) = std::fs::read(app.data.join("kept.bin")) {
            let n = b.restore(&bytes);
            eprintln!("xmas-relay: {n} kept messages back from before");
        }
        b.say_squares();
    }
    // now and then: kept messages more than a week old forgotten (a linked
    // device's side: longer), and what's kept written out
    {
        let a = app.clone();
        tokio::spawn(async move {
            let mut k = 0u32;
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                k += 1;
                if k % 10 == 0 {
                    a.broker.lock().unwrap_or_else(|e| e.into_inner()).expire(7 * 24 * 3600);
                }
                a.save_kept();
            }
        });
    }
    // long-polling sessions: the ones over, or left, let go
    {
        let p = app.polls.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                p.sweep();
            }
        });
    }
    let public = Router::new()
        .route("/mqtt", get(ws))
        .route("/health", get(|| async { "ok" }))
        .route("/", get(|| async { "xmas-tui relay: the village's online play goes through here (WebSocket at /mqtt, or HTTPS long polling at /poll)\n" }))
        .with_state(app.clone())
        .merge(poll::router(app.clone()));
    let admin = admin::router(app.clone());
    let port: u16 = env("RELAY_PORT", "8080").parse().unwrap_or(8080);
    let aport: u16 = env("ADMIN_PORT", "8081").parse().unwrap_or(8081);
    let pl = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.expect("the relay's port");
    let al = tokio::net::TcpListener::bind(("0.0.0.0", aport)).await.expect("the admin port");
    eprintln!("xmas-relay {}: WebSocket on :{port}/mqtt (and long polling at /poll), admin console on :{aport}", env!("CARGO_PKG_VERSION"));
    if app.admin_email.is_none() {
        eprintln!("xmas-relay: ADMIN_EMAIL isn't set: the console takes anyone Cloudflare Access lets through");
    }
    let a = tokio::spawn(async move { axum::serve(al, admin.into_make_service_with_connect_info::<SocketAddr>()).await });
    let p = tokio::spawn(async move { axum::serve(pl, public.into_make_service_with_connect_info::<SocketAddr>()).await });
    // Ctrl-C, or `docker stop` (SIGTERM): what's kept written out, then
    // gone at once (open WebSockets aren't waited on)
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
    app.save_kept();
    eprintln!("xmas-relay: stopping (what's kept is written out)");
    p.abort();
    a.abort();
}

/// The client's address: Cloudflare's header (behind its tunnel), else the
/// connection's own.
pub fn client_ip(headers: &HeaderMap, addr: SocketAddr, trust_cf: bool) -> IpAddr {
    if trust_cf {
        if let Some(ip) = headers.get("cf-connecting-ip").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse().ok()) {
            return ip;
        }
    }
    addr.ip()
}

async fn ws(ws: WebSocketUpgrade, headers: HeaderMap, ConnectInfo(addr): ConnectInfo<SocketAddr>, State(app): State<App>) -> Response {
    let ip = client_ip(&headers, addr, app.trust_cf);
    let max = {
        let mut b = app.broker.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(why) = b.admit(ip) {
            return (StatusCode::SERVICE_UNAVAILABLE, why).into_response();
        }
        b.limits().max_payload
    };
    ws.protocols(["mqtt"]).max_message_size(max + 1024).max_frame_size(max + 1024).on_upgrade(move |socket| conn(socket, ip, app, max))
}

/// A WebSocket client: its frames in and out, through a session.
async fn conn(socket: WebSocket, ip: IpAddr, app: App, max: usize) {
    let (mut sink, mut stream) = socket.split();
    let queue = app.broker.lock().unwrap_or_else(|e| e.into_inner()).limits().max_queue;
    let (up_tx, up_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let (down_tx, mut down_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(queue.max(16));
    let reader = tokio::spawn(async move {
        while let Some(Ok(m)) = stream.next().await {
            match m {
                Message::Binary(b) => {
                    if up_tx.send(b).await.is_err() {
                        break;
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });
    let writer = tokio::spawn(async move {
        while let Some(b) = down_rx.recv().await {
            if sink.send(Message::Binary(b)).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });
    session(up_rx, down_tx, ip, app, max).await;
    reader.abort();
    // (what's queued still goes out, then the socket's closed)
    let _ = tokio::time::timeout(Duration::from_secs(2), writer).await;
}

/// One client, from its hello to its goodbye, whichever way its bytes come
/// (a WebSocket, or HTTPS long polling): what it sends arrives on `up` (it's
/// gone when that closes), and what goes to it is put on `down`.
pub async fn session(mut up: tokio::sync::mpsc::Receiver<Vec<u8>>, down: tokio::sync::mpsc::Sender<Vec<u8>>, ip: IpAddr, app: App, max: usize) {
    use codec::In;
    let mut reader = codec::Reader::new(max + 512);
    // the hello first, soon
    let hello = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(b) = up.recv().await {
            reader.feed(&b).ok()?;
            if let Ok(Some(p)) = reader.next() {
                return Some(p);
            }
        }
        None
    })
    .await;
    let Ok(Some(In::Connect { client, keepalive, will })) = hello else { return };
    let client: String = client.chars().take(64).collect();
    let (id, _) = app.broker.lock().unwrap_or_else(|e| e.into_inner()).join(ip, client, will, down.clone());
    if down.send(codec::connack(0)).await.is_err() {
        app.broker.lock().unwrap_or_else(|e| e.into_inner()).leave(id, false);
        return;
    }
    drop(down);
    // (silent past one and a half times its keep-alive: gone)
    let idle = Duration::from_secs((keepalive.clamp(10, 600) as u64 * 3) / 2);
    let mut clean = false;
    'conn: loop {
        let bytes = match tokio::time::timeout(idle, up.recv()).await {
            Ok(Some(b)) => b,
            _ => break,
        };
        if reader.feed(&bytes).is_err() {
            break;
        }
        loop {
            let p = match reader.next() {
                Ok(Some(p)) => p,
                Ok(None) => break,
                Err(_) => break 'conn,
            };
            let mut b = app.broker.lock().unwrap_or_else(|e| e.into_inner());
            if !b.has(id) {
                // (let go: kicked, banned, or taken over)
                break 'conn;
            }
            let ok = match p {
                In::Publish { topic, payload, retain } => b.publish(id, &topic, &payload, retain).is_ok(),
                In::Subscribe { id: pid, filters } => {
                    let granted = b.subscribe(id, &filters);
                    b.reply(id, codec::suback(pid, &granted)).is_ok()
                }
                In::Unsubscribe { id: pid, filters } => {
                    b.unsubscribe(id, &filters);
                    b.reply(id, codec::unsuback(pid)).is_ok()
                }
                In::Ping => b.reply(id, codec::pingresp()).is_ok(),
                In::Disconnect => {
                    clean = true;
                    break 'conn;
                }
                In::Connect { .. } => false,
                In::Other(_) => true,
            };
            if !ok {
                break 'conn;
            }
        }
    }
    app.broker.lock().unwrap_or_else(|e| e.into_inner()).leave(id, clean);
}
