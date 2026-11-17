//! The same MQTT as the WebSocket, over plain HTTPS requests: for networks
//! whose proxies or filters break WebSockets but let ordinary web requests
//! through. A client opens a session, then keeps one request waiting for
//! what's coming to it (answered as soon as there's something, or after a
//! while with nothing) and sends what it has in requests of their own, one
//! after another:
//!
//! - `POST /poll` → the session's id (32 hex digits; it's the session's
//!   only key)
//! - `POST /poll/<id>/up`, the body MQTT's bytes → 204
//! - `POST /poll/<id>/down` → 200 with MQTT's bytes (perhaps none, after
//!   a wait); 410 once the session's over
//! - `POST /poll/<id>/close`
//!
//! A session nobody's asked after for a while is closed.

use crate::{client_ip, session, App};
use axum::body::Bytes;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// How long a waiting request waits with nothing to say.
pub const WAIT: Duration = Duration::from_secs(20);
/// A session nobody's asked after for this long is closed.
pub const FORGET: Duration = Duration::from_secs(45);

pub struct Poll {
    ip: IpAddr,
    up: mpsc::Sender<Vec<u8>>,
    down: tokio::sync::Mutex<mpsc::Receiver<Vec<u8>>>,
    last: Mutex<Instant>,
}

#[derive(Clone, Default)]
pub struct Polls(Arc<Mutex<HashMap<String, Arc<Poll>>>>);

impl Polls {
    fn get(&self, id: &str) -> Option<Arc<Poll>> {
        let p = self.0.lock().unwrap_or_else(|e| e.into_inner()).get(id).cloned()?;
        *p.last.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
        Some(p)
    }

    fn remove(&self, id: &str) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
    }

    pub fn len(&self) -> usize {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Sessions over, or not asked after for a while, let go (which ends
    /// them: their `up` closes).
    pub fn sweep(&self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).retain(|_, p| !p.up.is_closed() && p.last.lock().unwrap_or_else(|e| e.into_inner()).elapsed() < FORGET);
    }
}

fn new_id() -> String {
    let mut b = [0u8; 16];
    let ok = std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut b)).is_ok();
    if !ok {
        // (no /dev/urandom: the standard library's own random keys)
        use std::hash::{BuildHasher, Hasher};
        for c in b.chunks_mut(8) {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u128(Instant::now().elapsed().as_nanos());
            c.copy_from_slice(&h.finish().to_le_bytes());
        }
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[derive(Clone)]
struct St {
    app: App,
    polls: Polls,
}

pub fn router(app: App) -> Router {
    let polls = app.polls.clone();
    let max = app.broker.lock().unwrap_or_else(|e| e.into_inner()).limits().max_payload;
    Router::new()
        .route("/poll", post(open).options(preflight))
        .route("/poll/:id/up", post(up).options(preflight))
        .route("/poll/:id/down", post(down).options(preflight))
        .route("/poll/:id/close", post(close).options(preflight))
        .layer(DefaultBodyLimit::max(max + 4096))
        .with_state(St { app, polls })
}

/// Any page may use it (the web game's on another address than the relay).
fn cors(mut r: Response) -> Response {
    let h = r.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

async fn preflight(_m: Method) -> Response {
    let mut r = StatusCode::NO_CONTENT.into_response();
    let h = r.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("POST"));
    h.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("content-type"));
    h.insert(header::ACCESS_CONTROL_MAX_AGE, HeaderValue::from_static("86400"));
    cors(r)
}

async fn open(headers: HeaderMap, ConnectInfo(addr): ConnectInfo<SocketAddr>, State(st): State<St>) -> Response {
    let ip = client_ip(&headers, addr, st.app.trust_cf);
    let (max, queue, per_ip, most) = {
        let mut b = st.app.broker.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(why) = b.admit(ip) {
            return cors((StatusCode::SERVICE_UNAVAILABLE, why).into_response());
        }
        let l = b.limits();
        (l.max_payload, l.max_queue, l.max_conns_per_ip, l.max_conns)
    };
    st.polls.sweep();
    {
        // sessions not yet said hello count too
        let m = st.polls.0.lock().unwrap_or_else(|e| e.into_inner());
        if m.len() >= most || m.values().filter(|p| p.ip == ip).count() >= per_ip {
            return cors((StatusCode::SERVICE_UNAVAILABLE, "too many").into_response());
        }
    }
    let (up_tx, up_rx) = mpsc::channel::<Vec<u8>>(64);
    let (down_tx, down_rx) = mpsc::channel::<Vec<u8>>(queue.max(16));
    let id = new_id();
    let p = Arc::new(Poll { ip, up: up_tx, down: tokio::sync::Mutex::new(down_rx), last: Mutex::new(Instant::now()) });
    st.polls.0.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), p);
    let app = st.app.clone();
    tokio::spawn(async move { session(up_rx, down_tx, ip, app, max).await });
    cors((StatusCode::OK, [(header::CONTENT_TYPE, "text/plain")], id).into_response())
}

async fn up(Path(id): Path<String>, State(st): State<St>, body: Bytes) -> Response {
    let Some(p) = st.polls.get(&id) else { return cors(StatusCode::GONE.into_response()) };
    if body.is_empty() {
        return cors(StatusCode::NO_CONTENT.into_response());
    }
    match tokio::time::timeout(Duration::from_secs(10), p.up.send(body.to_vec())).await {
        Ok(Ok(())) => cors(StatusCode::NO_CONTENT.into_response()),
        _ => {
            st.polls.remove(&id);
            cors(StatusCode::GONE.into_response())
        }
    }
}

async fn down(Path(id): Path<String>, State(st): State<St>) -> Response {
    let Some(p) = st.polls.get(&id) else { return cors(StatusCode::GONE.into_response()) };
    // one waiting request at a time
    let Ok(mut rx) = p.down.try_lock() else { return cors(StatusCode::CONFLICT.into_response()) };
    let mut out = Vec::new();
    match tokio::time::timeout(WAIT, rx.recv()).await {
        Ok(Some(b)) => out.extend(b),
        Ok(None) => {
            drop(rx);
            st.polls.remove(&id);
            return cors(StatusCode::GONE.into_response());
        }
        Err(_) => {}
    }
    // and whatever else is ready, together
    while out.len() < 512 * 1024 {
        match rx.try_recv() {
            Ok(b) => out.extend(b),
            Err(_) => break,
        }
    }
    *p.last.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    cors((StatusCode::OK, [(header::CONTENT_TYPE, "application/octet-stream")], out).into_response())
}

async fn close(Path(id): Path<String>, State(st): State<St>) -> Response {
    st.polls.remove(&id);
    cors(StatusCode::NO_CONTENT.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_have_unguessable_ids_and_are_let_go() {
        let a = new_id();
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(a, new_id());
        let polls = Polls::default();
        let (up, up_rx) = mpsc::channel(1);
        let (_down_tx, down) = mpsc::channel(1);
        let p = Arc::new(Poll { ip: "127.0.0.1".parse().unwrap(), up, down: tokio::sync::Mutex::new(down), last: Mutex::new(Instant::now()) });
        polls.0.lock().unwrap().insert(a.clone(), p.clone());
        polls.sweep();
        assert_eq!(polls.len(), 1, "a live session stays");
        // asked after long ago: let go
        *p.last.lock().unwrap() = Instant::now() - FORGET - Duration::from_secs(1);
        polls.sweep();
        assert_eq!(polls.len(), 0);
        // and one whose session's over (its `up` closed)
        polls.0.lock().unwrap().insert(a.clone(), p.clone());
        *p.last.lock().unwrap() = Instant::now();
        drop(up_rx);
        polls.sweep();
        assert_eq!(polls.len(), 0);
    }
}
