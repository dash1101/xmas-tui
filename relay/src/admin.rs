//! The admin console: a page (stats, connections, limits, bans) and the
//! calls it makes. It answers only requests Cloudflare Access has signed in
//! (and, with ADMIN_EMAIL set, only as that address); changes need a header
//! a page elsewhere can't send, so another site can't make them.

use crate::broker::{Limits, Schedule, ServerVillage};
use crate::App;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

pub fn router(app: App) -> Router {
    Router::new()
        .route("/", get(page))
        .route("/api/stats", get(stats))
        .route("/api/conns", get(conns))
        .route("/api/limits", post(limits))
        .route("/api/kick", post(kick))
        .route("/api/ban", post(ban))
        .route("/api/unban", post(unban))
        .route("/api/clear", post(clear))
        .route("/api/squares", get(squares).post(set_squares))
        .route("/api/state", get(state))
        .route("/api/servers", post(set_servers))
        .route("/api/public", post(set_public))
        .route("/api/ban-village", post(ban_village))
        .route("/api/unban-village", post(unban_village))
        .route("/api/ban-player", post(ban_player))
        .route("/api/unban-player", post(unban_player))
        .route("/api/moderate", post(moderate))
        .route("/api/say", post(say))
        .route("/api/welcome", post(set_welcome))
        .route("/api/schedules", post(set_schedules))
        .route("/api/keep-out", post(keep_out))
        .route("/api/let-back", post(let_back))
        .route("/api/update", post(update))
        .route("/api/min-version", post(min_version))
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .with_state(app)
}

/// Cloudflare Access, checked properly: the signed token it puts on every
/// request it lets through, against the team's own keys.
pub struct Access {
    /// The team's domain ("<team>.cloudflareaccess.com").
    team: String,
    /// The console application's AUD tag.
    aud: String,
    keys: std::sync::RwLock<Option<jsonwebtoken::jwk::JwkSet>>,
}

#[derive(Deserialize)]
struct Claims {
    #[serde(default)]
    email: Option<String>,
}

impl Access {
    pub fn from_env() -> Option<std::sync::Arc<Access>> {
        let team = std::env::var("ACCESS_TEAM").ok().map(|t| t.trim().trim_start_matches("https://").trim_end_matches('/').to_lowercase()).filter(|t| !t.is_empty())?;
        let aud = std::env::var("ACCESS_AUD").ok().map(|a| a.trim().to_string()).filter(|a| !a.is_empty())?;
        let team = if team.contains('.') { team } else { format!("{team}.cloudflareaccess.com") };
        let a = std::sync::Arc::new(Access { team, aud, keys: std::sync::RwLock::new(None) });
        // the team's keys: now, and every hour (Cloudflare turns them over)
        let w = a.clone();
        tokio::spawn(async move {
            loop {
                let url = format!("https://{}/cdn-cgi/access/certs", w.team);
                let got = tokio::task::spawn_blocking(move || ureq::get(&url).timeout(std::time::Duration::from_secs(15)).call().ok()?.into_string().ok().and_then(|t| serde_json::from_str::<jsonwebtoken::jwk::JwkSet>(&t).ok())).await.ok().flatten();
                match got {
                    Some(k) => *w.keys.write().unwrap_or_else(|e| e.into_inner()) = Some(k),
                    None => eprintln!("xmas-relay: couldn't fetch Cloudflare Access's keys from {} (the console turns everyone away until it can)", w.team),
                }
                let wait = if w.keys.read().unwrap_or_else(|e| e.into_inner()).is_some() { 3600 } else { 60 };
                tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            }
        });
        Some(a)
    }

    /// The token, signed by the team's keys, for this application, from
    /// this team, not out of date: whose (their email), or nobody's.
    fn who(&self, token: &str) -> Option<String> {
        let kid = jsonwebtoken::decode_header(token).ok()?.kid?;
        let keys = self.keys.read().unwrap_or_else(|e| e.into_inner());
        let jwk = keys.as_ref()?.find(&kid)?;
        let key = jsonwebtoken::DecodingKey::from_jwk(jwk).ok()?;
        let mut v = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
        v.set_audience(&[self.aud.as_str()]);
        v.set_issuer(&[format!("https://{}", self.team)]);
        let data = jsonwebtoken::decode::<Claims>(token, &key, &v).ok()?;
        Some(data.claims.email.unwrap_or_default().trim().to_lowercase())
    }
}

/// Access's token on a request: its header, or its cookie.
fn token_of(headers: &HeaderMap) -> Option<String> {
    if let Some(t) = headers.get("cf-access-jwt-assertion").and_then(|v| v.to_str().ok()) {
        return Some(t.trim().to_string());
    }
    headers.get_all("cookie").iter().filter_map(|v| v.to_str().ok()).flat_map(|c| c.split(';')).find_map(|kv| kv.trim().strip_prefix("CF_Authorization=").map(|t| t.to_string()))
}

/// The host a request was sent to, without its port.
fn host_of(headers: &HeaderMap) -> String {
    let h = headers.get("host").and_then(|v| v.to_str().ok()).unwrap_or("").trim().to_lowercase();
    if let Some(rest) = h.strip_prefix('[') {
        return format!("[{}]", rest.split(']').next().unwrap_or(""));
    }
    h.split(':').next().unwrap_or("").to_string()
}

/// Who's asking: signed in through Cloudflare Access (its signed token,
/// when ACCESS_TEAM and ACCESS_AUD are set; else its email header) as the
/// admin; or, with ADMIN_OPEN=1 (an SSH tunnel), asked for at localhost
/// itself. A change also needs the console's own header, and mustn't come
/// from another site's page.
async fn guard(State(app): State<App>, headers: HeaderMap, req: Request, next: Next) -> Response {
    let open = std::env::var("ADMIN_OPEN").is_ok_and(|v| v == "1");
    let ok = if open {
        // (a page elsewhere can't reach it by pointing its own name here)
        matches!(host_of(&headers).as_str(), "localhost" | "127.0.0.1" | "[::1]")
    } else if let Some(acc) = &app.access {
        match (token_of(&headers).and_then(|t| acc.who(&t)), &app.admin_email) {
            (Some(got), Some(want)) => &got == want,
            (Some(_), None) => true,
            (None, _) => false,
        }
    } else {
        // (the email header alone: only with the admin's address set)
        let who = headers.get("cf-access-authenticated-user-email").and_then(|v| v.to_str().ok()).map(|v| v.trim().to_lowercase());
        matches!((&app.admin_email, &who), (Some(want), Some(got)) if want == got)
    };
    if !ok {
        return (StatusCode::FORBIDDEN, "sign in through Cloudflare Access (as the relay's admin)\n").into_response();
    }
    if req.method() != axum::http::Method::GET {
        let cross = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()).is_some_and(|v| v == "cross-site");
        let origin_elsewhere = headers.get("origin").and_then(|v| v.to_str().ok()).is_some_and(|o| {
            let o = o.trim().to_lowercase();
            let o = o.split("://").nth(1).unwrap_or(&o).to_string();
            let oh = if o.starts_with('[') { format!("[{}]", o[1..].split(']').next().unwrap_or("")) } else { o.split(':').next().unwrap_or("").to_string() };
            oh != host_of(&headers)
        });
        if headers.get("x-relay-admin").is_none() || cross || origin_elsewhere {
            return (StatusCode::FORBIDDEN, "changes come from the console\n").into_response();
        }
    }
    next.run(req).await
}

/// The console's page: its script allowed by a one-time mark, nothing
/// else (no other site's script, no framing, no sending elsewhere).
async fn page() -> Response {
    let mut b = [0u8; 16];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut b));
    let nonce: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let html = PAGE.replacen("<script>", &format!("<script nonce=\"{nonce}\">"), 1);
    let csp = format!("default-src 'none'; script-src 'nonce-{nonce}'; style-src 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'");
    (
        [
            (axum::http::header::CONTENT_SECURITY_POLICY, csp),
            (axum::http::header::X_FRAME_OPTIONS, "DENY".to_string()),
            (axum::http::header::REFERRER_POLICY, "no-referrer".to_string()),
            (axum::http::header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Html(html),
    )
        .into_response()
}

async fn stats(State(app): State<App>) -> Json<serde_json::Value> {
    let b = app.broker.lock().unwrap_or_else(|e| e.into_inner());
    Json(json!({ "stats": b.stats(), "limits": b.limits(), "bans": b.kept.bans, "polling": app.polls.len(), "version": env!("CARGO_PKG_VERSION") }))
}

async fn conns(State(app): State<App>) -> Json<serde_json::Value> {
    let b = app.broker.lock().unwrap_or_else(|e| e.into_inner());
    // (who each is, where the relay's host knows: the villages it keeps)
    let mut known: std::collections::HashMap<u64, (String, String)> = std::collections::HashMap::new();
    for (slot, (_, r)) in b.reports.iter() {
        for p in r.get("players").and_then(|p| p.as_array()).into_iter().flatten() {
            if let Some(conn) = p.get("conn").and_then(|c| c.as_u64()) {
                known.insert(conn, (p.get("name").and_then(|n| n.as_str()).unwrap_or("").chars().take(40).collect(), r.get("name").and_then(|n| n.as_str()).unwrap_or(slot).chars().take(60).collect()));
            }
        }
    }
    let list: Vec<serde_json::Value> = b
        .conns()
        .into_iter()
        .map(|(id, ip, client, secs, subs, sent, got, dropped, agent, host, country)| {
            let (name, village) = known.get(&id).cloned().unwrap_or_default();
            json!({ "id": id, "ip": ip, "client": client, "secs": secs, "subs": subs, "sent": sent, "got": got, "dropped": dropped, "platform": crate::broker::platform_of(&agent), "agent": agent, "host": host, "country": country, "name": name, "village": village })
        })
        .collect();
    Json(json!(list))
}

async fn limits(State(app): State<App>, Json(l): Json<Limits>) -> Response {
    // (sane bounds, whatever's typed)
    let l = Limits {
        max_conns: l.max_conns.clamp(1, 100_000),
        max_conns_per_ip: l.max_conns_per_ip.clamp(1, 1000),
        max_payload: l.max_payload.clamp(1024, 1024 * 1024),
        msgs_per_sec: l.msgs_per_sec.clamp(0.1, 1000.0),
        burst: l.burst.clamp(1.0, 10_000.0),
        max_subs: l.max_subs.clamp(1, 10_000),
        max_retained: l.max_retained.clamp(10, 1_000_000),
        max_retained_payload: l.max_retained_payload.clamp(256, 1024 * 1024),
        max_retained_bytes: l.max_retained_bytes.clamp(1024 * 1024, 2048 * 1024 * 1024),
        max_queue: l.max_queue.clamp(16, 100_000),
        closed: l.closed,
    };
    app.broker.lock().unwrap_or_else(|e| e.into_inner()).kept.limits = l;
    app.save();
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
struct Id {
    id: u64,
}

#[derive(Deserialize)]
struct Ip {
    ip: String,
}

#[derive(Deserialize)]
struct Prefix {
    prefix: String,
}

async fn kick(State(app): State<App>, Json(Id { id }): Json<Id>) -> Response {
    let ok = app.broker.lock().unwrap_or_else(|e| e.into_inner()).kick(id);
    if ok { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND }.into_response()
}

async fn ban(State(app): State<App>, Json(Ip { ip }): Json<Ip>) -> Response {
    let Ok(ip) = ip.trim().parse() else { return (StatusCode::BAD_REQUEST, "not an address").into_response() };
    // (the relay's own network: its host, or a proxy with everyone behind it)
    if crate::broker::internal(ip) {
        return (StatusCode::BAD_REQUEST, "that address is on the relay's own network (its host, or a proxy in front of it): blocking it would block the host, or everyone").into_response();
    }
    app.broker.lock().unwrap_or_else(|e| e.into_inner()).ban(ip);
    app.save();
    StatusCode::NO_CONTENT.into_response()
}

async fn unban(State(app): State<App>, Json(Ip { ip }): Json<Ip>) -> Response {
    let Ok(ip) = ip.trim().parse() else { return (StatusCode::BAD_REQUEST, "not an address").into_response() };
    app.broker.lock().unwrap_or_else(|e| e.into_inner()).unban(ip);
    app.save();
    StatusCode::NO_CONTENT.into_response()
}

async fn clear(State(app): State<App>, Json(Prefix { prefix }): Json<Prefix>) -> Json<serde_json::Value> {
    // (never the relay's own: its settings, its public list, the host's)
    let n = if prefix.starts_with(crate::broker::PREFIX) && !prefix.starts_with(crate::broker::ADMIN) && !crate::broker::ADMIN.starts_with(&prefix) {
        app.broker.lock().unwrap_or_else(|e| e.into_inner()).clear_retained(&prefix)
    } else {
        0
    };
    Json(json!({ "cleared": n }))
}

/// The public squares, by name, as the game has them.
pub const SQUARES: [&str; 4] = ["North Pole Square", "Candy Cane Lane", "Frosty Hollow", "Pine Peak Lodge"];

async fn squares(State(app): State<App>) -> Json<serde_json::Value> {
    let b = app.broker.lock().unwrap_or_else(|e| e.into_inner());
    Json(json!({ "names": SQUARES, "squares": b.kept.squares }))
}

/// Each square's settings (only the squares', only settings the game
/// knows): kept, and out to everyone in them at once.
async fn set_squares(State(app): State<App>, Json(v): Json<std::collections::BTreeMap<String, serde_json::Value>>) -> Response {
    const KEYS: [&str; 8] = ["time", "critters", "most_at_once", "events", "sleigh_minutes", "chattiness", "bushes", "traffic"];
    let mut clean = std::collections::BTreeMap::new();
    for (name, rules) in v {
        let Some(obj) = rules.as_object().filter(|_| SQUARES.contains(&name.as_str())) else { continue };
        let kept: serde_json::Map<String, serde_json::Value> = obj.iter().filter(|(k, _)| KEYS.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
        if !kept.is_empty() {
            clean.insert(name, serde_json::Value::Object(kept));
        }
    }
    {
        let mut b = app.broker.lock().unwrap_or_else(|e| e.into_inner());
        b.kept.squares = clean;
        b.say_squares();
        // (and to the host, keeping them)
        b.say_host();
    }
    app.save();
    StatusCode::NO_CONTENT.into_response()
}

/// Everything the console shows beyond the counts: the host and what it
/// reports, the server villages, the public list, bans, announcements.
async fn state(State(app): State<App>) -> Json<serde_json::Value> {
    let b = app.broker.lock().unwrap_or_else(|e| e.into_inner());
    let k = &b.kept;
    let reports: serde_json::Map<String, serde_json::Value> = b
        .reports
        .iter()
        .filter(|(_, (at, v))| at.elapsed().as_secs() < 120 && v.is_object())
        .map(|(slot, (at, v))| {
            let mut v = v.clone();
            v["age"] = json!(at.elapsed().as_secs());
            (slot.clone(), v)
        })
        .collect();
    // each player the host reports: where they're connected from, and with what
    let mut reports = reports;
    for (_, r) in reports.iter_mut() {
        if let Some(ps) = r.get_mut("players").and_then(|p| p.as_array_mut()) {
            for p in ps.iter_mut().filter(|p| p.is_object()) {
                // the connection the relay said this player's messages came
                // from (a host from 1.25.3 on); an older host's report has
                // only the id the player gave, so it's marked as such
                let from = match p.get("conn") {
                    Some(c) => c.as_u64().and_then(|c| b.conn_info(c)).map(|(ip, country, agent, _)| (ip, country, agent)),
                    None => {
                        p["claimed"] = json!(true);
                        p["id"].as_u64().and_then(|id| b.where_from(id as u32))
                    }
                };
                match from {
                    Some((ip, country, agent)) => {
                        p["ip"] = json!(ip);
                        p["country"] = json!(country);
                        p["platform"] = json!(crate::broker::platform_of(&agent));
                        p["agent"] = json!(agent);
                    }
                    None => p["unverified"] = json!(true),
                }
            }
        }
    }
    // the villages in use right now, named where the relay knows the name
    // (the host's own, and the listed ones); a private one is just a topic
    let mut names: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new();
    for (slot, r) in reports.iter() {
        if let Some(code) = r["code"].as_str() {
            names.insert(crate::broker::topic_of("village", code), (r["name"].as_str().unwrap_or(slot).to_string(), r["kind"].as_str().unwrap_or("").to_string()));
        }
    }
    if let Some(ls) = reports.get("_public").and_then(|r| r["listings"].as_array()) {
        for l in ls {
            if let Some(code) = l["code"].as_str() {
                names.entry(crate::broker::topic_of("village", code)).or_insert((l["name"].as_str().unwrap_or("").to_string(), "listed".to_string()));
            }
        }
    }
    let villages: Vec<serde_json::Value> = b
        .villages()
        .into_iter()
        .take(300)
        .map(|(topic, who)| {
            let (name, kind) = names.get(&topic).cloned().unwrap_or_default();
            let short: String = topic.rsplit('/').next().unwrap_or("").chars().take(8).collect();
            let who: Vec<serde_json::Value> = who.into_iter().take(60).map(|(ip, client, agent, host, country)| json!({ "ip": ip, "client": client, "agent": agent, "host": host, "country": country })).collect();
            json!({ "topic": short, "name": name, "kind": kind, "who": who })
        })
        .collect();
    Json(json!({
        "keepouts": k.keepouts,
        "min_version": k.min_version,
        "villages": villages,
        "update": updater(&app.data),
        "host_here": b.host_here(),
        "host_token": app.host_token.is_some(),
        "host_squares": k.host_squares,
        "servers": k.servers,
        "village_bans": k.village_bans,
        "player_bans": k.player_bans,
        "featured": k.featured,
        "order": k.order,
        "words": k.words,
        "muted": k.muted,
        "welcome": k.welcome,
        "schedules": k.schedules,
        "reports": reports,
        "now": crate::broker::now_secs(),
    }))
}

#[derive(Deserialize)]
struct MinVersion {
    version: String,
}

/// The oldest game let in (empty: any); those already here and older are
/// let go.
async fn min_version(State(app): State<App>, Json(m): Json<MinVersion>) -> Response {
    let v = m.version.trim();
    if !v.is_empty() && crate::broker::version_of(v).is_none() {
        return (StatusCode::BAD_REQUEST, "a version is like 1.25.4").into_response();
    }
    let n = app.broker.lock().unwrap_or_else(|e| e.into_inner()).set_min_version(v);
    app.save();
    Json(json!({ "let_go": n })).into_response()
}

/// The updater beside the relay (see DEPLOY.md): heard from lately or not,
/// its last word, and whether an update's been asked for and not yet
/// taken up.
fn updater(data: &std::path::Path) -> serde_json::Value {
    let read = |n: &str| std::fs::read_to_string(data.join(n)).ok();
    let alive = read("update.alive").and_then(|s| s.trim().parse::<u64>().ok()).is_some_and(|t| crate::broker::now_secs().saturating_sub(t) < 60);
    let status = read("update.status").filter(|s| s.len() < 4096).and_then(|s| serde_json::from_str::<serde_json::Value>(s.trim()).ok());
    json!({ "alive": alive, "status": status, "asked": data.join("update.request").exists(), "relay": env!("CARGO_PKG_VERSION") })
}

/// Update in place: asked of the updater beside the relay, by a file in
/// the data folder (the relay itself can't touch Docker, and shouldn't).
async fn update(State(app): State<App>) -> Response {
    if updater(&app.data)["alive"] != json!(true) {
        return (StatusCode::CONFLICT, "no updater is running beside the relay (see DEPLOY.md: COMPOSE_PROFILES=updater)").into_response();
    }
    match std::fs::write(app.data.join("update.request"), crate::broker::now_secs().to_string()) {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("couldn't ask for it: {e}")).into_response(),
    }
}

#[derive(Deserialize)]
struct KeepOut {
    code: String,
    #[serde(default)]
    id: serde_json::Value,
    #[serde(default)]
    name: String,
    #[serde(default)]
    why: String,
    /// By their address too.
    #[serde(default)]
    ip: bool,
    /// (Letting back: the key as the console shows it.)
    #[serde(default)]
    key: String,
    /// The connection to act on (the relay's word), over the id given.
    #[serde(default)]
    conn: Option<u64>,
    /// The village's name as the host has it (to send them home from it).
    #[serde(default)]
    slot: String,
}

/// A player kept out of one village the host keeps (sent home from it, if
/// they're there): the rest of the relay stays theirs.
/// The player id a connection gave the relay (in its client id), when the
/// console names a connection: the one the relay saw send what was said.
fn by_conn(app: &App, conn: Option<u64>) -> Option<u32> {
    let (.., client) = app.broker.lock().unwrap_or_else(|e| e.into_inner()).conn_info(conn?)?;
    crate::broker::player_of(&client)
}

async fn keep_out(State(app): State<App>, Json(k): Json<KeepOut>) -> Response {
    let Some(id) = by_conn(&app, k.conn).or(id_of(&k.id)).filter(|&i| i >= crate::broker::HOST_IDS) else { return (StatusCode::BAD_REQUEST, "not a player's id").into_response() };
    if crate::broker::norm_code(&k.code).len() != 12 {
        return (StatusCode::BAD_REQUEST, "not a village's code").into_response();
    }
    let found = {
        let mut b = app.broker.lock().unwrap_or_else(|e| e.into_inner());
        let found = b.keep_out(&k.code, id, &k.name, &k.why, k.ip);
        if !k.slot.is_empty() {
            b.host_cmd(json!({ "kick": { "slot": k.slot, "id": id } }));
        }
        found
    };
    app.save();
    // (not connected under that id: only the id's noted, and it changes
    // each time their game starts; the console says so)
    Json(json!({ "found": found })).into_response()
}

async fn let_back(State(app): State<App>, Json(k): Json<KeepOut>) -> Response {
    app.broker.lock().unwrap_or_else(|e| e.into_inner()).let_back(&k.code, &k.key);
    app.save();
    StatusCode::NO_CONTENT.into_response()
}

/// The squares' and server villages' settings: only those the game knows.
const KEYS: [&str; 8] = ["time", "critters", "most_at_once", "events", "sleigh_minutes", "chattiness", "bushes", "traffic"];

fn clean_rules(v: &serde_json::Value) -> serde_json::Value {
    let m: serde_json::Map<String, serde_json::Value> = v.as_object().map(|o| o.iter().filter(|(k, _)| KEYS.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
    serde_json::Value::Object(m)
}

/// Twelve letters and digits nobody can guess (a private village's code).
fn new_code() -> String {
    const ABC: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut raw = [0u8; 12];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut raw)).is_err() {
        // (no /dev/urandom: the clock, stirred)
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        for (i, r) in raw.iter_mut().enumerate() {
            *r = (n >> ((i * 7) % 96)) as u8 ^ (i as u8).wrapping_mul(97);
        }
    }
    raw.iter().map(|b| ABC[(*b as usize) % ABC.len()] as char).collect()
}

fn norm(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

fn seed() -> u32 {
    let c = new_code();
    c.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619)).max(1)
}

/// The server villages, as the console has them: named, coded (a new
/// one's code made here), each within bounds; out to the host at once.
async fn set_servers(State(app): State<App>, Json(list): Json<Vec<ServerVillage>>) -> Response {
    let squares: Vec<String> = SQUARES.iter().map(|s| s.to_lowercase()).collect();
    let mut names = std::collections::HashSet::new();
    let mut out = vec![];
    for v in list.into_iter().take(32) {
        let name: String = v.name.trim().chars().filter(|c| !c.is_control()).take(40).collect();
        if name.is_empty() || squares.contains(&name.to_lowercase()) || !names.insert(name.to_lowercase()) {
            continue;
        }
        let code = norm(&v.code);
        let code = if code.len() == 12 { code } else { new_code() };
        out.push(ServerVillage { name, code, public: v.public, limit: v.limit.clamp(2, 48), seed: if v.seed == 0 { seed() } else { v.seed }, rules: clean_rules(&v.rules) });
    }
    {
        let mut b = app.broker.lock().unwrap_or_else(|e| e.into_inner());
        b.kept.servers = out;
        b.say_host();
    }
    app.save();
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
struct Public {
    featured: Vec<String>,
    order: Vec<String>,
    words: Vec<String>,
    host_squares: bool,
}

fn list_of(v: Vec<String>, n: usize, len: usize) -> Vec<String> {
    v.into_iter().map(|s| s.trim().chars().filter(|c| !c.is_control()).take(len).collect::<String>()).filter(|s| !s.is_empty()).take(n).collect()
}

/// The public list's order and featured places, the extra words, and
/// whether the host keeps the squares: out to everyone at once.
async fn set_public(State(app): State<App>, Json(p): Json<Public>) -> Response {
    {
        let mut b = app.broker.lock().unwrap_or_else(|e| e.into_inner());
        b.kept.featured = list_of(p.featured, 20, 60);
        b.kept.order = list_of(p.order, 60, 60);
        b.kept.words = list_of(p.words, 200, 40).into_iter().map(|w| w.to_lowercase()).collect();
        b.kept.host_squares = p.host_squares;
        b.say_public();
        b.say_host();
    }
    app.save();
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
struct BanOf {
    /// The connection to act on (the relay's word), over the id given.
    #[serde(default)]
    conn: Option<u64>,
    #[serde(default)]
    code: String,
    #[serde(default)]
    id: serde_json::Value,
    #[serde(default)]
    name: String,
    #[serde(default)]
    why: String,
}

/// A player's id from the console: a number, or eight hex digits.
fn id_of(v: &serde_json::Value) -> Option<u32> {
    match v {
        serde_json::Value::Number(n) => n.as_u64().and_then(|n| u32::try_from(n).ok()),
        serde_json::Value::String(s) => u32::from_str_radix(s.trim().trim_start_matches("xt"), 16).ok(),
        _ => None,
    }
}

async fn ban_village(State(app): State<App>, Json(b): Json<BanOf>) -> Response {
    if norm(&b.code).len() != 12 {
        return (StatusCode::BAD_REQUEST, "a village code is twelve letters and digits").into_response();
    }
    app.broker.lock().unwrap_or_else(|e| e.into_inner()).ban_village(&b.code, &b.name, &b.why);
    app.save();
    StatusCode::NO_CONTENT.into_response()
}

async fn unban_village(State(app): State<App>, Json(b): Json<BanOf>) -> Response {
    app.broker.lock().unwrap_or_else(|e| e.into_inner()).unban_village(&b.code);
    app.save();
    StatusCode::NO_CONTENT.into_response()
}

async fn ban_player(State(app): State<App>, Json(b): Json<BanOf>) -> Response {
    let Some(id) = by_conn(&app, b.conn).or(id_of(&b.id)).filter(|&i| i >= crate::broker::HOST_IDS) else { return (StatusCode::BAD_REQUEST, "not a player's id").into_response() };
    {
        let mut br = app.broker.lock().unwrap_or_else(|e| e.into_inner());
        br.ban_player(id, &b.name, &b.why);
        br.host_cmd(json!({ "kick": { "slot": "*", "id": id } }));
    }
    app.save();
    StatusCode::NO_CONTENT.into_response()
}

async fn unban_player(State(app): State<App>, Json(b): Json<BanOf>) -> Response {
    // (an id, or a key as the console shows it: "tag:…")
    let key = match &b.id {
        serde_json::Value::String(s) if s.starts_with("tag:") => s.clone(),
        v => match id_of(v) {
            Some(id) => format!("{id:08x}"),
            None => return (StatusCode::BAD_REQUEST, "not a player's id").into_response(),
        },
    };
    app.broker.lock().unwrap_or_else(|e| e.into_inner()).unban_key(&key);
    app.save();
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
struct Moderate {
    slot: String,
    id: serde_json::Value,
    action: String,
}

/// In a village the host keeps: someone sent home (kick), or kept from
/// talking (mute) or let talk again.
async fn moderate(State(app): State<App>, Json(m): Json<Moderate>) -> Response {
    let Some(id) = id_of(&m.id).filter(|&i| i >= crate::broker::HOST_IDS) else { return (StatusCode::BAD_REQUEST, "not a player's id").into_response() };
    let slot: String = m.slot.chars().take(64).collect();
    {
        let mut b = app.broker.lock().unwrap_or_else(|e| e.into_inner());
        match m.action.as_str() {
            "kick" => b.host_cmd(json!({ "kick": { "slot": slot, "id": id } })),
            "mute" => {
                let v = b.kept.muted.entry(slot).or_default();
                if !v.contains(&id) {
                    v.push(id);
                }
                b.say_host();
            }
            "unmute" => {
                if let Some(v) = b.kept.muted.get_mut(&slot) {
                    v.retain(|&i| i != id);
                }
                b.kept.muted.retain(|_, v| !v.is_empty());
                b.say_host();
            }
            _ => return (StatusCode::BAD_REQUEST, "kick, mute or unmute").into_response(),
        }
    }
    app.save();
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
struct Say {
    to: String,
    text: String,
}

/// An announcement, now: in one village the host keeps, or all ("*").
async fn say(State(app): State<App>, Json(s): Json<Say>) -> Response {
    let text: String = s.text.trim().chars().filter(|c| !c.is_control()).take(200).collect();
    if text.is_empty() {
        return (StatusCode::BAD_REQUEST, "nothing to say").into_response();
    }
    app.broker.lock().unwrap_or_else(|e| e.into_inner()).host_cmd(json!({ "say": { "to": s.to.chars().take(64).collect::<String>(), "text": text } }));
    StatusCode::NO_CONTENT.into_response()
}

/// What's said to whoever comes into each village the host keeps.
async fn set_welcome(State(app): State<App>, Json(w): Json<std::collections::BTreeMap<String, String>>) -> Response {
    let clean: std::collections::BTreeMap<String, String> = w.into_iter().take(64).map(|(k, v)| (k.chars().take(64).collect(), v.trim().chars().filter(|c| !c.is_control()).take(200).collect::<String>())).filter(|(_, v): &(String, String)| !v.is_empty()).collect();
    {
        let mut b = app.broker.lock().unwrap_or_else(|e| e.into_inner());
        b.kept.welcome = clean;
        b.say_host();
    }
    app.save();
    StatusCode::NO_CONTENT.into_response()
}

/// The announcements to come (each numbered; a past one-off dropped).
async fn set_schedules(State(app): State<App>, Json(list): Json<Vec<Schedule>>) -> Response {
    let now = crate::broker::now_secs();
    let out: Vec<Schedule> = list
        .into_iter()
        .take(100)
        .enumerate()
        .map(|(i, s)| Schedule { id: i as u32 + 1, text: s.text.trim().chars().filter(|c| !c.is_control()).take(200).collect(), to: s.to.chars().take(64).collect(), at: s.at, every: if s.every == 0 { 0 } else { s.every.clamp(60, 30 * 86400) }, on: s.on })
        .filter(|s| !s.text.is_empty() && (s.every > 0 || s.at + 120 > now))
        .collect();
    app.broker.lock().unwrap_or_else(|e| e.into_inner()).kept.schedules = out;
    app.save();
    StatusCode::NO_CONTENT.into_response()
}

const PAGE: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Village relay</title>
<style>
  :root { --gold: #ffd36b; --panel: #0f1428; --text: #f4ecd8; --dim: #8a90a8; --line: #2a3150; --bad: #ff7a7a; }
  * { box-sizing: border-box; }
  body { margin: 0; background: #05070f; color: var(--text); font: 14px/1.45 ui-monospace, "DejaVu Sans Mono", Menlo, Consolas, monospace; }
  header { display: flex; align-items: baseline; gap: 12px; padding: 14px 18px; border-bottom: 1px solid var(--line); }
  header b { color: var(--gold); font-size: 17px; font-weight: normal; }
  header span { color: var(--dim); }
  main { max-width: 1100px; margin: 0 auto; padding: 16px; display: grid; gap: 14px; }
  section { background: var(--panel); border: 1px solid #c9a25a55; border-radius: 3px; padding: 12px 14px; }
  h2 { margin: 0 0 8px; font-size: 13px; color: var(--gold); font-weight: normal; }
  h2::before { content: "❄ "; }
  .cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 10px; }
  .card { border: 1px solid var(--line); padding: 8px 10px; }
  .card i { display: block; color: var(--dim); font-style: normal; font-size: 12px; }
  .card b { font-size: 20px; font-weight: normal; color: var(--gold); }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  th, td { text-align: left; padding: 4px 6px; border-bottom: 1px solid var(--line); white-space: nowrap; }
  th { color: var(--dim); font-weight: normal; }
  .wrap { overflow-x: auto; }
  button { font: inherit; cursor: pointer; background: #24305a; color: var(--text); border: 1px solid var(--gold); padding: 2px 10px; }
  button.bad { border-color: var(--bad); }
  input { font: inherit; background: #0b1020; color: var(--text); border: 1px solid #4a5480; padding: 2px 6px; width: 110px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 6px 18px; }
  .grid label { display: flex; justify-content: space-between; align-items: center; gap: 10px; }
  .note { color: var(--dim); font-size: 12px; }
  #msg { color: var(--gold); min-height: 1.2em; }
  .ok { color: #9be08a; } .warn { color: var(--bad); }
  .chat { max-height: 220px; overflow-y: auto; border: 1px solid var(--line); padding: 4px 6px; font-size: 12px; }
  .chat div { white-space: pre-wrap; }
  .slot { border: 1px solid var(--line); padding: 8px 10px; margin: 8px 0; }
  .slot h3 { margin: 0 0 6px; font-size: 13px; font-weight: normal; color: var(--text); }
  textarea { font: inherit; background: #0b1020; color: var(--text); border: 1px solid #4a5480; padding: 4px 6px; width: 100%; }
  select { font: inherit; background: #0b1020; color: var(--text); border: 1px solid #4a5480; }
  button.small { padding: 0 6px; font-size: 12px; }
</style></head><body>
<header><b>❄ Village relay</b><span id="ver"></span><span id="up"></span></header>
<main>
  <section><h2>NOW</h2><div class="cards" id="cards"></div><div class="note" id="kinds"></div></section>
  <section><h2>HOST</h2>
    <p id="host"></p>
    <p><label><input type="checkbox" id="hostsq" style="width:auto"> the host keeps the public squares (else the first player in hosts each, as before)</label></p>
    <p class="note">The relay's own host keeps the squares and the server villages open, the same for everyone, all the time. It runs beside the relay (see DEPLOY.md) and signs in with HOST_TOKEN.</p>
  </section>
  <section><h2>IN THE VILLAGES THE HOST KEEPS</h2>
    <p class="note">Who's in each (with the address and the version they're connected with), and what's been said there lately (kept in memory only, the last 50 lines). Mute: their talk isn't shown to others there. Kick: sent home from that village (they can come back). Ban here: kept out of that village only. Ban: kept off the relay. Block address: nothing connects from that address again (the one that stops a bot, which can make itself a new id each time).</p>
    <div id="slots"></div>
  </section>
  <section><h2>ANNOUNCE</h2>
    <p><select id="sayto"></select> <input id="saytext" placeholder="what to say (200 letters at most)" style="width:min(520px,70%)"> <button id="saynow">say it now</button> <span id="saymsg"></span></p>
    <h2 style="margin-top:12px">SCHEDULED</h2>
    <p class="note">Said by the host at the time given (this browser's time), once or again every so many minutes. One missed while the relay was down isn't said late.</p>
    <div class="wrap"><table><thead><tr><th>on</th><th>what</th><th>where</th><th>first at</th><th>every (min, 0: once)</th><th></th></tr></thead><tbody id="sched"></tbody></table></div>
    <p><button id="addsched">+ add</button> <button id="savesched">save announcements</button> <span id="schedmsg"></span></p>
  </section>
  <section><h2>SERVER VILLAGES</h2>
    <p class="note">Villages the host keeps open, not a player's game. Private: only those given the code can come in. Listed: in everyone's Public villages. A code left blank is made for it.</p>
    <div class="wrap"><table><thead><tr><th>name</th><th>code</th><th>listed</th><th>most at once</th><th>time of day</th><th>folk &amp; animals</th><th>surprises</th><th>welcome</th><th></th></tr></thead><tbody id="servers"></tbody></table></div>
    <p><button id="addsrv">+ a village</button> <button id="savesrv">save server villages</button> <span id="srvmsg"></span></p>
  </section>
  <section><h2>PUBLIC LIST</h2>
    <p class="note">Featured first, then in this order (names or codes, one a line), then the rest, busiest first. Extra words are starred out in every public village.</p>
    <div class="grid"><label>featured <textarea id="featured" rows="3"></textarea></label><label>order <textarea id="order" rows="3"></textarea></label><label>extra words <textarea id="words" rows="3"></textarea></label></div>
    <p><button id="savepub">save the public list</button> <span id="pubmsg"></span></p>
    <div id="listings"></div>
  </section>
  <section><h2>VILLAGES OPEN RIGHT NOW</h2>
    <p class="note">Every village in use on the relay, the fullest first: the host's own and the listed ones by name; a private one only as a number, with how many are in it and from where (what's said in it is sealed: the relay can't read it, or tell who's hosting).</p>
    <div class="wrap"><table><thead><tr><th>village</th><th>in it</th><th>from</th></tr></thead><tbody id="villages"></tbody></table></div>
  </section>
  <section><h2>CONNECTED</h2><div class="wrap"><table><thead><tr><th>#</th><th>who (where the host knows)</th><th>address (country)</th><th>version</th><th>client</th><th>for</th><th>subs</th><th>sent</th><th>got</th><th>dropped</th><th></th></tr></thead><tbody id="conns"></tbody></table></div>
    <p class="note" id="versions"></p>
  </section>
  <section><h2>VERSIONS</h2>
    <p>oldest game let in: <input id="minver" placeholder="any" style="width:110px"> <button id="saveminver">save</button> <span class="note" id="minvernote"></span></p>
    <p class="note">Older games are turned away with a word to update (from 1.25.4 the game says so and checks for an update; older ones just can't connect). Raising it lets go of anyone connected with an older one. Empty: any version. The relay's own host is never turned away.</p>
  </section>
  <section><h2>UPDATE</h2>
    <p id="updnow"></p>
    <p><button id="update">update the relay and its host now</button> <span class="note">pulls the newest images and starts them again; players are dropped for a few seconds and come straight back</span></p>
    <p class="note" id="updst"></p>
  </section>
  <section><h2>LIMITS</h2>
    <div class="grid" id="limits"></div>
    <p><button id="save">save limits</button> <span id="msg"></span></p>
    <p class="note">Limits take hold at once (new connections, and each message from now on). "closed" turns new connections away.</p>
  </section>
  <section><h2>PUBLIC SQUARES</h2>
    <p class="note">Each square's own settings: everyone in it has these (their snow, wind and glow stay their own). Left blank: the game's usual. Saved settings reach everyone in a square at once.</p>
    <div class="wrap"><table><thead><tr><th>square</th><th>time of day</th><th>folk &amp; animals</th><th>most at once</th><th>surprises</th><th>sleigh every (min)</th><th>chattiness</th><th>berry bushes</th></tr></thead><tbody id="squares"></tbody></table></div>
    <p><button id="savesq">save squares</button> <span id="sqmsg"></span></p>
  </section>
  <section><h2>BANS</h2>
    <p>addresses: <input id="banip" placeholder="address" style="width:220px"> <button id="ban" class="bad">ban</button></p>
    <table><tbody id="bans"></tbody></table>
    <p>villages: <input id="banvcode" placeholder="its code" style="width:160px"> <input id="banvwhy" placeholder="why (for you)" style="width:220px"> <button id="banv" class="bad">ban</button></p>
    <table><tbody id="vbans"></tbody></table>
    <p>players: <input id="banpid" placeholder="id (8 hex digits)" style="width:160px"> <input id="banpwhy" placeholder="why (for you)" style="width:220px"> <button id="banp" class="bad">ban</button></p>
    <table><tbody id="pbans"></tbody></table>
    <p>kept out of one village only (ban here, beside a player in a village the host keeps):</p>
    <table><tbody id="kouts"></tbody></table>
    <p class="note">A blocked IPv6 address stands for its whole block (/64: the range one home or machine is given, and can hop about in), and the limit on connections from one address counts the block too.</p>
    <p class="note">A banned village can't be used on the relay and leaves the public list; a banned player (by the id their game has) can't connect. Someone can always start afresh with a new id or address: bans slow them down, they don't make them impossible.</p>
  </section>
  <section><h2>KEPT MESSAGES</h2>
    <p class="note">Messages the relay keeps for whoever comes next ("here I am", a shared village's state). Forget all under a prefix, e.g. xmas-tui/1/here/</p>
    <p><input id="prefix" value="xmas-tui/1/here/" style="width:260px"> <button id="clear" class="bad">forget them</button></p>
  </section>
</main>
<script>
const NAMES = { max_conns: "connections, in all", max_conns_per_ip: "connections from one address", max_payload: "biggest message (bytes)", msgs_per_sec: "messages a second, each", burst: "messages in a burst", max_subs: "subscriptions, each", max_retained: "kept messages", max_retained_payload: "biggest kept message (bytes)", max_retained_bytes: "kept, in all (bytes)", max_queue: "queue before a slow one's dropped", closed: "closed to new connections" };
let limits = null, editing = false;
const $ = (id) => document.getElementById(id);
const call = (path, body) => fetch(path, { method: "POST", headers: { "content-type": "application/json", "x-relay-admin": "1" }, body: JSON.stringify(body) });
const ago = (s) => s < 60 ? s + "s" : s < 3600 ? Math.floor(s / 60) + "m" : Math.floor(s / 3600) + "h" + String(Math.floor(s % 3600 / 60)).padStart(2, "0");
const size = (b) => b < 1024 ? b + " B" : b < 1048576 ? (b / 1024).toFixed(1) + " KB" : (b / 1048576).toFixed(1) + " MB";
const ICONS = { linux: "🐧", windows: "🪟", android: "🤖", macos: "🍎", web: "🌐" };
const icon = (p) => `<span title="${esc(p || "older app (doesn't say)")}">${ICONS[p] || "❔"}</span>`;
function esc(s) { return String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c])); }
async function refresh() {
  try {
    const r = await (await fetch("api/stats")).json();
    const s = r.stats;
    $("ver").textContent = "v" + r.version;
    $("up").textContent = "up " + ago(s.uptime_s);
    const plats = Object.entries(s.platforms || {}).sort((a, b) => b[1] - a[1]).map(([k, n]) => `${ICONS[k] || "❔"} ${n}`).join("  ");
    const cards = [["players online", Number(s.players) || 0], ["on", plats || "–"], ["connected", s.conns], ["by long polling", r.polling], ["addresses", s.ips], ["messages/s", s.rate.toFixed(1)], ["in", s.msgs_in + " · " + size(s.bytes_in)], ["out", s.msgs_out + " · " + size(s.bytes_out)], ["kept", s.retained + " · " + size(s.retained_bytes)], ["waiting to go out", size(Number(s.queued_bytes) || 0)], ["refused", s.refused], ["dropped", s.dropped], ["let go", s.kicked]];
    $("cards").innerHTML = cards.map(([k, v]) => `<div class="card"><i>${k}</i><b>${esc(v)}</b></div>`).join("");
    $("kinds").textContent = "kept by kind: " + (Object.entries(s.kinds).map(([k, n]) => k + " " + n).join(" · ") || "none");
    $("bans").innerHTML = r.bans.map((ip) => `<tr><td>${esc(ip)}</td><td><button data-unban="${esc(ip)}">unban</button></td></tr>`).join("") || `<tr><td class="note">nobody</td></tr>`;
    if (!editing) { limits = r.limits; drawLimits(); }
    const cs = await (await fetch("api/conns")).json();
    const what = (c) => c.host ? "the relay's host" + (c.agent ? " · " + c.agent : "") : c.agent || "(doesn't say)";
    $("conns").innerHTML = cs.map((c) => `<tr><td>${Number(c.id)}</td><td>${c.host ? "🏠" : icon(c.platform)} ${c.name ? `<b>${esc(c.name)}</b> <span class="note">in ${esc(c.village)}</span>` : ""}</td><td>${esc(c.ip)}${c.country ? ` <span class="note">${esc(c.country)}</span>` : ""}</td><td>${esc(what(c))}</td><td>${esc(c.client)}</td><td>${ago(Number(c.secs))}</td><td>${Number(c.subs)}</td><td>${Number(c.sent)}</td><td>${Number(c.got)}</td><td>${Number(c.dropped)}</td><td><button data-kick="${Number(c.id)}">kick</button> ${c.host ? "" : `<button class="bad" data-ban="${esc(c.ip)}">block address</button>`}</td></tr>`).join("") || `<tr><td colspan="11" class="note">nobody right now</td></tr>`;
    const tally = {};
    cs.filter((c) => !c.host).forEach((c) => { const k = c.agent || "(doesn't say)"; tally[k] = (tally[k] || 0) + 1; });
    $("versions").textContent = Object.keys(tally).length ? "by version: " + Object.entries(tally).sort((a, b) => b[1] - a[1]).map(([k, n]) => k + " × " + n).join(" · ") : "";
  } catch (e) { $("msg").textContent = "can't reach the relay: " + e; }
}
function drawLimits() {
  $("limits").innerHTML = Object.keys(NAMES).map((k) => k === "closed"
    ? `<label>${NAMES[k]} <input type="checkbox" data-k="${k}" ${limits[k] ? "checked" : ""} style="width:auto"></label>`
    : `<label>${NAMES[k]} <input data-k="${k}" value="${limits[k]}"></label>`).join("");
  document.querySelectorAll("#limits input").forEach((i) => i.oninput = () => editing = true);
}
$("save").onclick = async () => {
  const l = {};
  document.querySelectorAll("#limits input").forEach((i) => { l[i.dataset.k] = i.type === "checkbox" ? i.checked : Number(i.value); });
  const r = await call("api/limits", l);
  $("msg").textContent = r.ok ? "saved" : "couldn't save (" + r.status + ")";
  editing = false; refresh();
};
$("ban").onclick = async () => { const ip = $("banip").value.trim(); if (ip) { const r = await call("api/ban", { ip }); if (r.ok) $("banip").value = ""; else alert(await r.text()); refresh(); } };
$("clear").onclick = async () => { const p = $("prefix").value.trim(); if (p && confirm("Forget every kept message under " + p + "?")) { const r = await (await call("api/clear", { prefix: p })).json(); $("msg").textContent = "forgot " + r.cleared; refresh(); } };
document.addEventListener("click", async (e) => {
  const t = e.target;
  if (t.dataset.kick) { await call("api/kick", { id: Number(t.dataset.kick) }); refresh(); }
  else if (t.dataset.ban && confirm("Block " + t.dataset.ban + "? Nothing connects from that address again until it's unblocked.")) { const r = await call("api/ban", { ip: t.dataset.ban }); if (!r.ok) alert(await r.text()); refresh(); }
  else if (t.dataset.unban) { await call("api/unban", { ip: t.dataset.unban }); refresh(); }
});
const TIMES = ["", "auto", "night", "dawn", "morning", "noon", "afternoon", "dusk"];
async function loadSquares() {
  const r = await (await fetch("api/squares")).json();
  $("squares").innerHTML = r.names.map((n) => {
    const s = r.squares[n] || {};
    const num = (k, step) => `<input data-sq="${esc(n)}" data-k="${k}" type="number" step="${step}" min="0" value="${s[k] ?? ""}" style="width:80px">`;
    const time = `<select data-sq="${esc(n)}" data-k="time">${TIMES.map((t) => `<option ${s.time === t ? "selected" : ""} value="${t}">${t || "(usual)"}</option>`).join("")}</select>`;
    const bush = `<select data-sq="${esc(n)}" data-k="bushes"><option value="">(usual)</option><option ${s.bushes === true ? "selected" : ""} value="true">yes</option><option ${s.bushes === false ? "selected" : ""} value="false">no</option></select>`;
    return `<tr><td>${esc(n)}</td><td>${time}</td><td>${num("critters", 0.25)}</td><td>${num("most_at_once", 1)}</td><td>${num("events", 0.25)}</td><td>${num("sleigh_minutes", 1)}</td><td>${num("chattiness", 0.25)}</td><td>${bush}</td></tr>`;
  }).join("");
}
$("savesq").onclick = async () => {
  const out = {};
  document.querySelectorAll("#squares [data-sq]").forEach((i) => {
    const v = i.value.trim();
    if (v === "") return;
    const o = out[i.dataset.sq] = out[i.dataset.sq] || {};
    o[i.dataset.k] = i.dataset.k === "time" ? v : i.dataset.k === "bushes" ? v === "true" : Number(v);
  });
  const r = await call("api/squares", out);
  $("sqmsg").textContent = r.ok ? "saved: everyone in the squares has them now" : "couldn't save (" + r.status + ")";
  loadSquares();
};
// ---- the host, its villages, the public list, announcements, bans
let st = null, srvs = [], scheds = [], editSrv = false, editSched = false;
const when = (t) => { const d = new Date(t * 1000); return d.toLocaleString(); };
const local = (t) => { const d = new Date(t * 1000 - new Date().getTimezoneOffset() * 60000); return d.toISOString().slice(0, 16); };
const fromLocal = (s) => Math.floor(new Date(s).getTime() / 1000);
async function loadState() {
  try { st = await (await fetch("api/state")).json(); } catch (e) { return; }
  $("host").innerHTML = st.host_here ? `<span class="ok">● the host is connected</span>` : st.host_token ? `<span class="warn">○ the host isn't connected</span> (squares fall back to their players)` : `<span class="warn">○ no HOST_TOKEN set</span>: no host can sign in`;
  $("hostsq").checked = st.host_squares;
  const slots = Object.entries(st.reports).sort((a, b) => a[0].localeCompare(b[0]));
  $("slots").innerHTML = slots.filter(([k]) => k !== "_public").map(([slot, r]) => {
    const muted = st.muted[slot] || [];
    const ps = (r.players || []).map((p) => {
      const pid = Number(p.id) >>> 0, conn = p.conn == null ? "" : Number(p.conn), sl = esc(slot);
      const from = p.unverified ? `<span class="warn" title="the relay can't say which connection this is">not matched to a connection</span>` : `${esc(p.ip || "")}${p.country ? " " + esc(p.country) : ""}${p.claimed ? ` <span class="note" title="an older host: matched by the id the player gave, which a modified game can fake">(by id)</span>` : ""}`;
      return `<tr><td>${icon(p.platform)} ${esc(p.name)}</td><td class="note">${pid.toString(16).padStart(8, "0")}</td><td class="note">${from}</td><td class="note">${esc(p.agent || "")}</td><td>${muted.includes(pid) ? `<button class="small" data-act="unmute" data-slot="${sl}" data-id="${pid}">unmute</button>` : `<button class="small" data-act="mute" data-slot="${sl}" data-id="${pid}">mute</button>`} <button class="small" data-act="kick" data-slot="${sl}" data-id="${pid}">kick</button> <button class="small bad" data-kout="${pid}" data-conn="${conn}" data-code="${esc(r.code || "")}" data-slot="${sl}" data-name="${esc(p.name)}">ban here</button> <button class="small bad" data-pban="${pid}" data-conn="${conn}" data-name="${esc(p.name)}">ban</button>${p.ip && !p.unverified ? ` <button class="small bad" data-ban="${esc(p.ip)}">block address</button>` : ""}</td></tr>`;
    }).join("") || `<tr><td class="note">nobody there</td></tr>`;
    const chat = (r.chat || []).slice(-50).map((c) => `<div><span class="note">${new Date(c.at * 1000).toLocaleTimeString()}</span> <b>${esc(c.name)}</b>: ${esc(c.text)} <button class="small" data-act="mute" data-slot="${esc(slot)}" data-id="${Number(c.id) >>> 0}">mute</button></div>`).join("") || `<div class="note">nothing said lately</div>`;
    const stale = r.age > 30 ? ` <span class="warn">(not heard from for ${ago(r.age)})</span>` : "";
    return `<div class="slot"><h3>${esc(r.name || slot)} · ${esc(r.kind || "")}${r.public === false ? " · private" : ""} · ${Number(r.n) || 0} of ${Number(r.limit) || "?"}${stale}</h3><table><tbody>${ps}</tbody></table><div class="chat">${chat}</div></div>`;
  }).join("") || `<p class="note">nothing yet: the host reports every few seconds once it's connected</p>`;
  const to = ["*"].concat(slots.filter(([k]) => k !== "_public").map(([k]) => k));
  const keep = $("sayto").value;
  $("sayto").innerHTML = to.map((t) => `<option value="${esc(t)}">${t === "*" ? "every village the host keeps" : esc(t)}</option>`).join("");
  if (keep) $("sayto").value = keep;
  const pub = (st.reports._public || {}).listings || [];
  $("listings").innerHTML = pub.length ? `<table><thead><tr><th>listed now</th><th>code</th><th>here</th><th></th></tr></thead><tbody>${pub.map((l) => `<tr><td>${esc(l.name)}</td><td class="note">${esc(l.code)}</td><td>${Number(l.n) || 0} of ${Number(l.limit) || 0}</td><td><button class="small" data-feature="${esc(l.code)}">feature</button> <button class="small bad" data-vban="${esc(l.code)}" data-name="${esc(l.name)}">ban</button></td></tr>`).join("")}</tbody></table>` : `<p class="note">(the villages listed right now show here once the host's connected)</p>`;
  if (document.activeElement.tagName !== "TEXTAREA") { $("featured").value = st.featured.join("\n"); $("order").value = st.order.join("\n"); $("words").value = st.words.join("\n"); }
  $("vbans").innerHTML = st.village_bans.map((b) => `<tr><td>${esc(b.name || "(a village)")}</td><td class="note">${esc(b.key)}</td><td class="note">${esc(b.why)}</td><td class="note">${when(b.at)}</td><td><button class="small" data-vunban="${esc(b.key)}">unban</button></td></tr>`).join("") || `<tr><td class="note">no villages</td></tr>`;
  $("pbans").innerHTML = st.player_bans.map((b) => `<tr><td>${esc(b.name || "(a player)")}</td><td class="note">${esc(b.key)}</td><td class="note">${esc(b.why)}</td><td class="note">${when(b.at)}</td><td><button class="small" data-punban="${esc(b.key)}">unban</button></td></tr>`).join("") || `<tr><td class="note">no players</td></tr>`;
  const vname = (code) => { const e = Object.entries(st.reports).find(([, r]) => r.code && r.code.replace(/[^A-Za-z0-9]/g, "").toUpperCase() === code); return e ? (e[1].name || e[0]) : code; };
  $("kouts").innerHTML = Object.entries(st.keepouts || {}).flatMap(([code, list]) => list.map((b) => `<tr><td>${esc(b.name || "(a player)")}</td><td class="note">${esc(b.key)}</td><td>${esc(vname(code))}</td><td class="note">${esc(b.why)}</td><td class="note">${when(b.at)}</td><td><button class="small" data-kback="${esc(b.key)}" data-code="${esc(code)}">let back</button></td></tr>`)).join("") || `<tr><td class="note">nobody</td></tr>`;
  $("villages").innerHTML = (st.villages || []).map((v) => {
    const from = v.who.map((w) => w.host ? "the relay's host" : esc(w.ip) + (w.country ? " " + esc(w.country) : "") + (w.agent ? ` <span class="note">${esc(w.agent)}</span>` : "")).join(", ");
    const name = v.name ? `${esc(v.name)} <span class="note">${esc(v.kind)}</span>` : `<span class="note">a private village · ${esc(v.topic)}</span>`;
    return `<tr><td>${name}</td><td>${v.who.filter((w) => !w.host).length}</td><td>${from}</td></tr>`;
  }).join("") || `<tr><td colspan="3" class="note">none right now</td></tr>`;
  if (document.activeElement !== $("minver")) $("minver").value = st.min_version || "";
  $("minvernote").textContent = st.min_version ? "" : "(any version is let in)";
  const u = st.update || {};
  $("updnow").textContent = "relay " + (u.relay || "?") + (st.host_here ? " · its host connected" : " · its host not connected");
  $("update").disabled = !u.alive || u.asked || (u.status && (u.status.state === "pulling" || u.status.state === "restarting"));
  $("updst").innerHTML = !u.alive ? `<span class="warn">○ no updater beside the relay</span>: updates are by hand (docker compose pull, then up -d), or turn the updater on (DEPLOY.md)` : u.asked ? "asked for: starting in a moment…" : u.status ? `${esc(u.status.state)}${u.status.at ? " · " + when(u.status.at) : ""}${u.status.detail ? " · " + esc(u.status.detail) : ""}` : "ready";
  if (!editSrv) { srvs = st.servers.map((v) => ({ ...v, welcome: st.welcome[v.name] || "" })); drawServers(); }
  if (!editSched) { scheds = st.schedules.slice(); drawSched(); }
}
const SEL = (vals, cur, attrs) => `<select ${attrs}>${vals.map((v) => `<option value="${esc(v)}" ${String(cur) === String(v) ? "selected" : ""}>${v === "" ? "(usual)" : esc(v)}</option>`).join("")}</select>`;
function drawServers() {
  $("servers").innerHTML = srvs.map((v, i) => `<tr>
    <td><input data-s="${i}" data-k="name" value="${esc(v.name)}" style="width:150px"></td>
    <td><input data-s="${i}" data-k="code" value="${esc(v.code)}" placeholder="(made for it)" style="width:130px"></td>
    <td><input data-s="${i}" data-k="public" type="checkbox" ${v.public ? "checked" : ""} style="width:auto"></td>
    <td><input data-s="${i}" data-k="limit" type="number" min="2" max="48" value="${v.limit || 8}" style="width:60px"></td>
    <td>${SEL(TIMES, (v.rules || {}).time ?? "", `data-s="${i}" data-r="time"`)}</td>
    <td><input data-s="${i}" data-r="critters" type="number" step="0.25" min="0" value="${(v.rules || {}).critters ?? ""}" style="width:60px"></td>
    <td><input data-s="${i}" data-r="events" type="number" step="0.25" min="0" value="${(v.rules || {}).events ?? ""}" style="width:60px"></td>
    <td><input data-s="${i}" data-k="welcome" value="${esc(v.welcome || "")}" style="width:200px"></td>
    <td><button class="small bad" data-srvdel="${i}">remove</button></td></tr>`).join("") || `<tr><td class="note" colspan="9">none yet</td></tr>`;
  document.querySelectorAll("#servers [data-s]").forEach((el) => el.oninput = () => {
    editSrv = true;
    const v = srvs[el.dataset.s];
    if (el.dataset.r) { v.rules = v.rules || {}; const x = el.value.trim(); if (x === "") delete v.rules[el.dataset.r]; else v.rules[el.dataset.r] = el.dataset.r === "time" ? x : Number(x); }
    else v[el.dataset.k] = el.type === "checkbox" ? el.checked : el.dataset.k === "limit" ? Number(el.value) : el.value;
  });
}
function drawSched() {
  const to = ["*"].concat(Object.keys((st && st.reports) || {}).filter((k) => k !== "_public"));
  $("sched").innerHTML = scheds.map((s, i) => `<tr>
    <td><input data-c="${i}" data-k="on" type="checkbox" ${s.on ? "checked" : ""} style="width:auto"></td>
    <td><input data-c="${i}" data-k="text" value="${esc(s.text)}" style="width:280px"></td>
    <td>${SEL(Array.from(new Set(to.concat([s.to || "*"]))), s.to || "*", `data-c="${i}" data-k="to"`)}</td>
    <td><input data-c="${i}" data-k="at" type="datetime-local" value="${local(s.at || st.now + 3600)}"></td>
    <td><input data-c="${i}" data-k="every" type="number" min="0" value="${Math.round((s.every || 0) / 60)}" style="width:70px"></td>
    <td><button class="small bad" data-scdel="${i}">remove</button></td></tr>`).join("") || `<tr><td class="note" colspan="6">none</td></tr>`;
  document.querySelectorAll("#sched [data-c]").forEach((el) => el.oninput = () => {
    editSched = true;
    const s = scheds[el.dataset.c], k = el.dataset.k;
    s[k] = k === "on" ? el.checked : k === "at" ? fromLocal(el.value) : k === "every" ? Number(el.value) * 60 : el.value;
  });
}
$("addsrv").onclick = () => { editSrv = true; srvs.push({ name: "", code: "", public: false, limit: 8, rules: {}, welcome: "" }); drawServers(); };
$("savesrv").onclick = async () => {
  const list = srvs.map((v) => ({ name: v.name, code: v.code, public: !!v.public, limit: v.limit || 8, seed: v.seed || 0, rules: v.rules || {} }));
  const r = await call("api/servers", list);
  const welcome = {};
  for (const [k, v] of Object.entries(st.welcome)) if (!srvs.some((s) => s.name === k)) welcome[k] = v;
  srvs.forEach((v) => { if (v.welcome) welcome[v.name] = v.welcome; });
  const w = await call("api/welcome", welcome);
  $("srvmsg").textContent = r.ok && w.ok ? "saved: the host opens, changes or closes them now" : "couldn't save (" + r.status + ")";
  editSrv = false; loadState();
};
$("addsched").onclick = () => { editSched = true; scheds.push({ text: "", to: "*", at: st.now + 3600, every: 0, on: true }); drawSched(); };
$("savesched").onclick = async () => { const r = await call("api/schedules", scheds); $("schedmsg").textContent = r.ok ? "saved" : "couldn't save (" + r.status + ")"; editSched = false; loadState(); };
$("saynow").onclick = async () => { const text = $("saytext").value.trim(); if (!text) return; const r = await call("api/say", { to: $("sayto").value, text }); $("saymsg").textContent = r.ok ? (st.host_here ? "said" : "sent, but the host isn't connected") : "couldn't (" + r.status + ")"; if (r.ok) $("saytext").value = ""; };
const lines = (id) => $(id).value.split("\n").map((x) => x.trim()).filter(Boolean);
$("savepub").onclick = async () => { const r = await call("api/public", { featured: lines("featured"), order: lines("order"), words: lines("words"), host_squares: $("hostsq").checked }); $("pubmsg").textContent = r.ok ? "saved: everyone's list follows it now" : "couldn't save (" + r.status + ")"; loadState(); };
$("hostsq").onchange = () => $("savepub").click();
$("banv").onclick = async () => { const code = $("banvcode").value.trim(); if (!code) return; const r = await call("api/ban-village", { code, why: $("banvwhy").value }); if (r.ok) { $("banvcode").value = ""; $("banvwhy").value = ""; } else alert(await r.text()); loadState(); };
$("banp").onclick = async () => { const id = $("banpid").value.trim(); if (!id) return; const r = await call("api/ban-player", { id, why: $("banpwhy").value }); if (r.ok) { $("banpid").value = ""; $("banpwhy").value = ""; } else alert(await r.text()); loadState(); };
document.addEventListener("click", async (e) => {
  const t = e.target, d = t.dataset;
  if (d.act) { await call("api/moderate", { slot: d.slot, id: Number(d.id), action: d.act }); loadState(); }
  else if (d.pban && confirm("Ban " + (d.name || "this player") + " from the relay?")) { const why = prompt("Why (just for you)?") || ""; await call("api/ban-player", { id: Number(d.pban), conn: d.conn ? Number(d.conn) : null, name: d.name || "", why }); loadState(); }
  else if (d.vban && confirm("Ban the village " + (d.name || d.vban) + "?")) { const why = prompt("Why (just for you)?") || ""; await call("api/ban-village", { code: d.vban, name: d.name || "", why }); loadState(); }
  else if (d.vunban) { await call("api/unban-village", { code: d.vunban }); loadState(); }
  else if (d.punban) { await call("api/unban-player", { id: d.punban }); loadState(); }
  else if (d.kout) {
    if (!d.code) { alert("The host hasn't said this village's code yet: try again in a moment."); return; }
    if (!confirm("Keep " + (d.name || "this player") + " out of " + d.slot + " (only there)?")) return;
    const ip = confirm("By their address too? (OK: yes. Cancel: just their id and tag.)");
    const why = prompt("Why (just for you)?") || "";
    const r = await call("api/keep-out", { code: d.code, slot: d.slot, id: Number(d.kout), conn: d.conn ? Number(d.conn) : null, name: d.name || "", why, ip });
    if (!r.ok) alert(await r.text());
    else if (!(await r.json()).found) alert("No connection was found under that id (they've gone, or their game isn't the usual one), so only the id was noted, and that changes each time a game starts. To stop them for good, find their connection under VILLAGES OPEN RIGHT NOW or CONNECTED and block the address.");
    loadState();
  }
  else if (d.kback) { await call("api/let-back", { code: d.code, key: d.kback }); loadState(); }
  else if (d.feature) { const f = lines("featured"); if (!f.includes(d.feature)) f.push(d.feature); $("featured").value = f.join("\n"); $("savepub").click(); }
  else if (d.srvdel && confirm("Close and remove this server village?")) { editSrv = true; srvs.splice(Number(d.srvdel), 1); drawServers(); }
  else if (d.scdel) { editSched = true; scheds.splice(Number(d.scdel), 1); drawSched(); }
});
$("saveminver").onclick = async () => {
  const version = $("minver").value.trim();
  const r = await call("api/min-version", { version });
  if (!r.ok) { alert(await r.text()); return; }
  const j = await r.json();
  $("minvernote").textContent = j.let_go ? `saved: ${j.let_go} older connected game(s) let go` : "saved";
  loadState(); refresh();
};
$("update").onclick = async () => {
  if (!confirm("Update the relay and its host now? Everyone's dropped for a few seconds and comes straight back.")) return;
  const r = await call("api/update", {});
  if (!r.ok) alert(await r.text());
  loadState();
};
loadSquares();
loadState(); setInterval(loadState, 4000);
refresh(); setInterval(refresh, 3000);
</script></body></html>"##;


#[cfg(test)]
mod tests {
    use super::*;

    /// A key made for this test only (not used anywhere else).
    const TEST_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQCt0ARPW2ufp8HJ\nzK/j2fBNCmbnyR8XKF6WHyXtgos+vamQ6eUN0XRinCkm3xUzuDG+s/psuFGEBFhc\n3he2hECHABmSKuitPG0ZeRgrPMIBGu68wbuNf9DsGrrB+Wr1Do/a3ZHNOtby8ygl\nZFx+vO9AieYpdSFYQhd/9FW5iPy7ovcyTN1wC9oNbtdbKPo/9yudXv3skfl1EEaG\nrsdpGt42DIIm/uRRsBtWLMHH8w61On7YEu1SB8eW4j82JZN0v69c+0FuQPxdyCFs\nWPBqGPyAN5GYCCPujHyNkW8lCLPvxyPPYQYpc6n8BeQb8uohwazz0jtHCgPcq/Hv\nS9B1ChPJAgMBAAECggEABzuYmlF0ao85Y142/t/NqJ8OBjIv2YUECUeK4AB72ToH\nlDCvfUI3OiKZHHV8X+EX14GMzQ5kfKkOS5sFrezV9JLeqDRMkhI+0Khh3EFp1KWc\nGVMPLMW3WhkzUVQW4n4JzXKamR7hh/hHYOWgFbljVAezrLZOPhUjU4nwMUl6iYzz\nyXMbumPTIoDwK8oDanepmiFoVcZ0VNVSo7JacLONquLhnU3HKq6tz/5BGgwLUDYq\nLXCh6pbn7P7g8ziGoG0pzn221FvwxS0HWPfsCqKeK3yJaoQd8ebC3Jl1qrbtwzXI\ni0WGZUiyP+gWmDY+8jAEtAqDYlXfQU/0RwRUfEV20wKBgQDazmMI1tNdFYSvwGpf\nJbwxMjvlDIXLYVwedssSizpswy+AO5iQpb6O31F3DRV3E9I/AXGh3apSMkG4o1ek\n+TA9fP9POecKYMkSx8vHghBg3tyqHboAtVl548yyTsMwXDn7DwIg8ZPCt1UH+MNC\nuD9Yz1zYwbBqNmTFC5vBKUMYuwKBgQDLW62klYIAfzDKlsTgPWChGsyBSev4813Y\nZUfmdNYhNfvcBoXOtURPvs1L3AD2lm0sHaUoK3XNA71tXCSSkowHKUN9/HUfzTw2\nJZUHP52FK3tEb4nPvV1uRGvFQNow6RggQEM0HllSTVeW0TyX3gFodbLYe7BH6wi2\nPXLvLs6vSwKBgGOyKDOt1o6SMGv4Cz+hLPdpgRrjMs8xZEzJKcwgPhWydaFDBkfa\nDVKB9Fd/Ual5EZH9sW3kVyeumH1th5C2rcFGg/nwUxJKeTOib3pPC/FnejnU45xR\nBRzhq7IO6ybMzupSs1CZYg8Vg5vnP+jVlJ99lDt7PUuXzyBtIqHHkDfjAoGAeH2e\nmikXuRGjqDXC+xaC3ZISX/ybu4r/LMt92ZealFz3GPwoZjzS9+kWnP8C9xhUIVKD\nXBS8HLg9kcGJzerU2x1g5xIhl7E0HuUPsxbHT3Ne+0KXRiZuaHpT3LE7VsWVzO4m\no4fj5rb60dqx603YSbuLN2e6IDKrm8c3BW8XeZcCgYAswF7qBrWk4bBlj+p5qVZO\nLtpcOMBHRgl1Q05MqO756gOqa7s95wDxEAmUguFN4RybcuBF4MkEOauX2BeFaCXc\n9sq8EJb9aga/uSymNBzvLtcIRv7CU+vEBOpgl9O+vJC/zgQipPISk3qK+qc5DdT+\nbTcgVftfWbiHoGHOc5FMgw==\n-----END PRIVATE KEY-----";

    fn access() -> Access {
        let set = serde_json::json!({ "keys": [{ "kty": "RSA", "kid": "k1", "alg": "RS256", "use": "sig", "n": "rdAET1trn6fBycyv49nwTQpm58kfFyhelh8l7YKLPr2pkOnlDdF0YpwpJt8VM7gxvrP6bLhRhARYXN4XtoRAhwAZkirorTxtGXkYKzzCARruvMG7jX_Q7Bq6wflq9Q6P2t2RzTrW8vMoJWRcfrzvQInmKXUhWEIXf_RVuYj8u6L3MkzdcAvaDW7XWyj6P_crnV797JH5dRBGhq7HaRreNgyCJv7kUbAbVizBx_MOtTp-2BLtUgfHluI_NiWTdL-vXPtBbkD8XcghbFjwahj8gDeRmAgj7ox8jZFvJQiz78cjz2EGKXOp_AXkG_LqIcGs89I7RwoD3Kvx70vQdQoTyQ", "e": "AQAB" }] });
        Access { team: "team.cloudflareaccess.com".into(), aud: "the-aud".into(), keys: std::sync::RwLock::new(serde_json::from_value(set).ok()) }
    }

    fn token(aud: &str, iss: &str, email: &str, exp: i64) -> String {
        let mut h = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        h.kid = Some("k1".into());
        let claims = serde_json::json!({ "aud": [aud], "iss": iss, "email": email, "exp": exp, "iat": exp - 3600 });
        jsonwebtoken::encode(&h, &claims, &jsonwebtoken::EncodingKey::from_rsa_pem(TEST_KEY.as_bytes()).unwrap()).unwrap()
    }

    /// Access's token is believed only when it's signed by the team's key,
    /// for this console, from this team, and not out of date.
    #[test]
    fn only_a_real_access_token_gets_in() {
        let a = access();
        let soon = crate::broker::now_secs() as i64 + 600;
        let iss = "https://team.cloudflareaccess.com";
        assert_eq!(a.who(&token("the-aud", iss, "Admin@Example.org", soon)).as_deref(), Some("admin@example.org"));
        assert_eq!(a.who(&token("another-app", iss, "admin@example.org", soon)), None, "another application's token");
        assert_eq!(a.who(&token("the-aud", "https://other.cloudflareaccess.com", "admin@example.org", soon)), None, "another team's");
        assert_eq!(a.who(&token("the-aud", iss, "admin@example.org", soon - 7200)), None, "out of date");
        assert_eq!(a.who("not.a.token"), None);
        // (a token signed with no key at all)
        let mut forged = token("the-aud", iss, "admin@example.org", soon);
        forged.truncate(forged.rfind('.').unwrap() + 1);
        assert_eq!(a.who(&forged), None, "unsigned");
    }

    /// The host a request was sent to, and where its token is.
    #[test]
    fn hosts_and_tokens_are_read() {
        let mut h = HeaderMap::new();
        h.insert("host", "LocalHost:8081".parse().unwrap());
        assert_eq!(host_of(&h), "localhost");
        h.insert("host", "[::1]:8081".parse().unwrap());
        assert_eq!(host_of(&h), "[::1]");
        h.insert("cookie", "a=b; CF_Authorization=tok; c=d".parse().unwrap());
        assert_eq!(token_of(&h).as_deref(), Some("tok"));
        h.insert("cf-access-jwt-assertion", "hdr".parse().unwrap());
        assert_eq!(token_of(&h).as_deref(), Some("hdr"));
    }
}
