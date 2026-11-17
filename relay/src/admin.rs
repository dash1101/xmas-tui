//! The admin console: a page (stats, connections, limits, bans) and the
//! calls it makes. It answers only requests Cloudflare Access has signed in
//! (and, with ADMIN_EMAIL set, only as that address); changes need a header
//! a page elsewhere can't send, so another site can't make them.

use crate::broker::Limits;
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
        .route("/", get(|| async { Html(PAGE) }))
        .route("/api/stats", get(stats))
        .route("/api/conns", get(conns))
        .route("/api/limits", post(limits))
        .route("/api/kick", post(kick))
        .route("/api/ban", post(ban))
        .route("/api/unban", post(unban))
        .route("/api/clear", post(clear))
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .with_state(app)
}

/// Who's asking: signed in through Cloudflare Access (as the admin, if one's
/// set); and for a change, the console's own header.
async fn guard(State(app): State<App>, headers: HeaderMap, req: Request, next: Next) -> Response {
    let who = headers.get("cf-access-authenticated-user-email").and_then(|v| v.to_str().ok()).map(|v| v.trim().to_lowercase());
    let open = std::env::var("ADMIN_OPEN").is_ok_and(|v| v == "1");
    let ok = open
        || match (&app.admin_email, &who) {
            (Some(want), Some(got)) => want == got,
            (None, Some(_)) => true,
            _ => false,
        };
    if !ok {
        return (StatusCode::FORBIDDEN, "sign in through Cloudflare Access (as the relay's admin)\n").into_response();
    }
    if req.method() != axum::http::Method::GET && headers.get("x-relay-admin").is_none() {
        return (StatusCode::FORBIDDEN, "changes come from the console\n").into_response();
    }
    next.run(req).await
}

async fn stats(State(app): State<App>) -> Json<serde_json::Value> {
    let b = app.broker.lock().unwrap_or_else(|e| e.into_inner());
    Json(json!({ "stats": b.stats(), "limits": b.limits(), "bans": b.kept.bans, "version": env!("CARGO_PKG_VERSION") }))
}

async fn conns(State(app): State<App>) -> Json<serde_json::Value> {
    let b = app.broker.lock().unwrap_or_else(|e| e.into_inner());
    let list: Vec<serde_json::Value> = b.conns().into_iter().map(|(id, ip, client, secs, subs, sent, got, dropped)| json!({ "id": id, "ip": ip, "client": client, "secs": secs, "subs": subs, "sent": sent, "got": got, "dropped": dropped })).collect();
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
    let n = if prefix.starts_with(crate::broker::PREFIX) { app.broker.lock().unwrap_or_else(|e| e.into_inner()).clear_retained(&prefix) } else { 0 };
    Json(json!({ "cleared": n }))
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
</style></head><body>
<header><b>❄ Village relay</b><span id="ver"></span><span id="up"></span></header>
<main>
  <section><h2>NOW</h2><div class="cards" id="cards"></div><div class="note" id="kinds"></div></section>
  <section><h2>CONNECTED</h2><div class="wrap"><table><thead><tr><th>#</th><th>address</th><th>client</th><th>for</th><th>subs</th><th>sent</th><th>got</th><th>dropped</th><th></th></tr></thead><tbody id="conns"></tbody></table></div></section>
  <section><h2>LIMITS</h2>
    <div class="grid" id="limits"></div>
    <p><button id="save">save limits</button> <span id="msg"></span></p>
    <p class="note">Limits take hold at once (new connections, and each message from now on). "closed" turns new connections away.</p>
  </section>
  <section><h2>BANS</h2>
    <p><input id="banip" placeholder="address" style="width:220px"> <button id="ban" class="bad">ban</button></p>
    <table><tbody id="bans"></tbody></table>
  </section>
  <section><h2>KEPT MESSAGES</h2>
    <p class="note">Messages the relay keeps for whoever comes next ("here I am", a shared village's state). Forget all under a prefix, e.g. xmas-tui/1/here/</p>
    <p><input id="prefix" value="xmas-tui/1/" style="width:260px"> <button id="clear" class="bad">forget them</button></p>
  </section>
</main>
<script>
const NAMES = { max_conns: "connections, in all", max_conns_per_ip: "connections from one address", max_payload: "biggest message (bytes)", msgs_per_sec: "messages a second, each", burst: "messages in a burst", max_subs: "subscriptions, each", max_retained: "kept messages", max_retained_payload: "biggest kept message (bytes)", max_retained_bytes: "kept, in all (bytes)", max_queue: "queue before a slow one's dropped", closed: "closed to new connections" };
let limits = null, editing = false;
const $ = (id) => document.getElementById(id);
const call = (path, body) => fetch(path, { method: "POST", headers: { "content-type": "application/json", "x-relay-admin": "1" }, body: JSON.stringify(body) });
const ago = (s) => s < 60 ? s + "s" : s < 3600 ? Math.floor(s / 60) + "m" : Math.floor(s / 3600) + "h" + String(Math.floor(s % 3600 / 60)).padStart(2, "0");
const size = (b) => b < 1024 ? b + " B" : b < 1048576 ? (b / 1024).toFixed(1) + " KB" : (b / 1048576).toFixed(1) + " MB";
function esc(s) { return String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c])); }
async function refresh() {
  try {
    const r = await (await fetch("api/stats")).json();
    const s = r.stats;
    $("ver").textContent = "v" + r.version;
    $("up").textContent = "up " + ago(s.uptime_s);
    const cards = [["connected", s.conns], ["addresses", s.ips], ["messages/s", s.rate.toFixed(1)], ["in", s.msgs_in + " · " + size(s.bytes_in)], ["out", s.msgs_out + " · " + size(s.bytes_out)], ["kept", s.retained + " · " + size(s.retained_bytes)], ["refused", s.refused], ["dropped", s.dropped], ["let go", s.kicked]];
    $("cards").innerHTML = cards.map(([k, v]) => `<div class="card"><i>${k}</i><b>${esc(v)}</b></div>`).join("");
    $("kinds").textContent = "kept by kind: " + (Object.entries(s.kinds).map(([k, n]) => k + " " + n).join(" · ") || "none");
    $("bans").innerHTML = r.bans.map((ip) => `<tr><td>${esc(ip)}</td><td><button data-unban="${esc(ip)}">unban</button></td></tr>`).join("") || `<tr><td class="note">nobody</td></tr>`;
    if (!editing) { limits = r.limits; drawLimits(); }
    const cs = await (await fetch("api/conns")).json();
    $("conns").innerHTML = cs.map((c) => `<tr><td>${c.id}</td><td>${esc(c.ip)}</td><td>${esc(c.client)}</td><td>${ago(c.secs)}</td><td>${c.subs}</td><td>${c.sent}</td><td>${c.got}</td><td>${c.dropped}</td><td><button data-kick="${c.id}">kick</button> <button class="bad" data-ban="${esc(c.ip)}">ban</button></td></tr>`).join("") || `<tr><td colspan="9" class="note">nobody right now</td></tr>`;
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
$("ban").onclick = async () => { const ip = $("banip").value.trim(); if (ip) { await call("api/ban", { ip }); $("banip").value = ""; refresh(); } };
$("clear").onclick = async () => { const p = $("prefix").value.trim(); if (p && confirm("Forget every kept message under " + p + "?")) { const r = await (await call("api/clear", { prefix: p })).json(); $("msg").textContent = "forgot " + r.cleared; refresh(); } };
document.addEventListener("click", async (e) => {
  const t = e.target;
  if (t.dataset.kick) { await call("api/kick", { id: Number(t.dataset.kick) }); refresh(); }
  else if (t.dataset.ban && confirm("Ban " + t.dataset.ban + "?")) { await call("api/ban", { ip: t.dataset.ban }); refresh(); }
  else if (t.dataset.unban) { await call("api/unban", { ip: t.dataset.unban }); refresh(); }
});
refresh(); setInterval(refresh, 3000);
</script></body></html>"##;
