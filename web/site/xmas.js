// The village in a web page: xmas.wasm (the desktop's own village, built for
// the web) draws each frame into its memory, and this page puts it on the
// canvas, plays its sound, and hands it the keyboard, the mouse and fingers.
// Menus and the controls card are drawn in the village itself, as on the
// desktop; on a phone the joystick and buttons are drawn in pixel art, as in
// the Android app.
"use strict";

const BTN = { E: 0, JUMP: 1, SNOWBALL: 2, POCKETS: 3, INVITE: 4, HELD: 5, BACK: 6, CENTRE: 7, JUMP_UP: 8, REST: 9, NUM: 10 };
const KEY = "xmas-tui-save", OPTS = "xmas-tui-opts", PLACE = "xmas-tui-opts-at", FOLK = "xmas-tui-villagers", CFG = "xmas-tui-config";
// the settings (the menu's, the same as the desktop app's): kept as the
// config text the game itself writes; the first time, made from the page's
// old settings
function cfgText() {
  try { const c = localStorage.getItem(CFG); if (c) return c; } catch (e) {}
  return config(opts());
}
const canvas = document.getElementById("village");
const ctx = canvas.getContext("2d", { alpha: false });
const pad = document.getElementById("pad");
const pctx = pad.getContext("2d");
const touchy = matchMedia("(pointer: coarse)").matches;
if (touchy) document.body.classList.add("touch");

let wasm, game = 0, W = 0, H = 0, K = 1, last = 0, img = null, imgPtr = 0;

// ------------------------------------------------------------ settings

const DEFAULTS = {
  name: "", pet: "cat", petcoat: "", petname: "", auto: false, walk: 100, motto: "",
  wcoat: "", what: "", whair: "", wskin: "", wscarf: true,
  friends: [], only_friends: false,
  time: "auto", snow: 100, wind: 100, aurora: true, glow: 100, size: 100, zoom: 100,
  critters: 100, most: 24, bushes: true, events: 100, sleigh: 4, chattiness: 100, talk_time: 100,
  sound: true, master: 50, music: 60, sfx: 70, ambience: 50, track: "shuffle",
  look: "classic", uiscale: 100, btnscale: 100,
  autoafter: "20", autoarea: "anywhere", autoreach: "1", vlimit: "8", cleanwords: true,
};
function opts() {
  let o = {};
  try { o = JSON.parse(localStorage.getItem(OPTS) || "{}"); } catch (e) {}
  // (the first web version kept the size as a factor)
  if (typeof o.size === "number" && o.size < 5) o.size = Math.round(o.size * 100);
  return Object.assign({}, DEFAULTS, o, { friends: Array.isArray(o.friends) ? o.friends : [] });
}
function setOpts(o) { try { localStorage.setItem(OPTS, JSON.stringify(o)); } catch (e) {} }
function q(s) { return '"' + String(s == null ? "" : s).replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/\n/g, " ") + '"'; }
function config(o) {
  let s = `zoom = ${o.zoom / 100}\n[scene]\nname = ${q(o.name)}\nautopilot = ${!!o.auto}\nwalk_speed = ${o.walk / 100}\n`;
  s += `time = ${q(o.time)}\nsnow = ${o.snow / 100}\nwind = ${o.wind / 100}\naurora = ${!!o.aurora}\nglow = ${o.glow / 100}\n`;
  s += `critters = ${o.critters / 100}\nmost_at_once = ${o.most | 0}\nbushes = ${!!o.bushes}\nevents = ${o.events / 100}\nsleigh_minutes = ${o.sleigh}\n`;
  s += `chattiness = ${o.chattiness / 100}\ntalk_time = ${o.talk_time / 100}\nonly_friends = ${!!o.only_friends}\nmotto = ${q(o.motto)}\n`;
  s += `autopilot_after = ${+o.autoafter || 20}\nautopilot_area = ${q(o.autoarea)}\nautopilot_reach = ${+o.autoreach || 1}\n`;
  s += `village_limit = ${Math.max(2, Math.min(48, +o.vlimit || 8))}\nclean_words = ${o.cleanwords !== false}\n`;
  s += `[scene.wear]\ncoat = ${q(o.wcoat)}\nhat = ${q(o.what)}\nhair = ${q(o.whair)}\nskin = ${q(o.wskin)}\nscarf = ${o.wscarf !== false}\n`;
  s += `[scene.pet]\nkind = ${q(o.pet)}\ncoat = ${q(o.petcoat)}\nname = ${q(o.petname)}\n`;
  for (const f of o.friends) {
    if (!String(f.name || "").trim()) continue;
    s += `[[scene.friends]]\nname = ${q(f.name.trim())}\nkid = ${!!f.kid}\ncoat = ${q(f.coat)}\nhat = ${q(f.hat)}\nhair = ${q(f.hair)}\nskin = ${q(f.skin)}\nscarf = ${f.scarf !== false}\n`;
  }
  s += `[ui]\nlook = ${q(o.look)}\nscale = ${(o.uiscale || 100) / 100}\n[sound]\nenabled = ${!!o.sound}\nmaster = ${o.master / 100}\nmusic = ${o.music / 100}\nsfx = ${o.sfx / 100}\nambience = ${o.ambience / 100}\ntrack = ${q(o.track)}\n`;
  return s;
}
function localMs() { const d = new Date(); return d.getTime() - d.getTimezoneOffset() * 60000; }

function putText(s) {
  const bytes = new TextEncoder().encode(s);
  const p = wasm.text_buf(0, bytes.length);
  new Uint8Array(wasm.memory.buffer, p, bytes.length).set(bytes);
  return bytes.length;
}
function getText(n) {
  if (!n) return "";
  return new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, wasm.text_ptr(game), n));
}

// ------------------------------------------------------------ the village

function size() {
  // the canvas at the screen's own pixels; a very big screen shows each of
  // the village's pixels as a whole number of the screen's (never blurred)
  const dpr = window.devicePixelRatio || 1;
  const w = Math.round(innerWidth * dpr), h = Math.round(innerHeight * dpr);
  const k = Math.max(1, Math.ceil(Math.max(w / 2560, h / 1600)));
  return [Math.max(64, Math.floor(w / k)), Math.max(64, Math.floor(h / k)), k, dpr];
}

// the size of the menus' and talk's letters, against 18 of the canvas's
// pixels: the screen's own density (about 18 css px a row on a computer,
// 13 on a phone, where room's short); the settings' sizes go on top
function cellSize(dpr) {
  return Math.max(0.5, Math.min(4, (dpr || 1) * (touchy ? 0.72 : 1) / K));
}

function build() {
  if (game) { save(); netBye(); wasm.destroy(game); game = 0; }
  let dpr;
  [W, H, K, dpr] = size();
  canvas.width = W; canvas.height = H;
  canvas.style.width = (W * K / dpr) + "px";
  canvas.style.height = (H * K / dpr) + "px";
  pad.width = innerWidth; pad.height = innerHeight;
  const n = putText(cfgText() + "\u0000" + (localStorage.getItem(KEY) || ""));
  game = wasm.create(W, H, cellSize(dpr), (Math.random() * 4294967295) >>> 0, localMs(), n);
  img = null;
  sound();
  sizeButtons();
  // the other villagers kept in this browser (for the menu)
  wasm.set_villagers(game, putText(JSON.stringify(folk().map((v) => v.name || "a villager (no name)"))));
}

// other villagers kept in this browser: their saves and settings
function folk() {
  try { const a = JSON.parse(localStorage.getItem(FOLK) || "[]"); return Array.isArray(a) ? a : []; } catch (e) { return []; }
}
function park() {
  save();
  const all = folk();
  const name = (cfgText().match(/^name = "(.*)"$/m) || [])[1] || "";
  all.push({ name, save: localStorage.getItem(KEY) || "", cfg: cfgText() });
  try { localStorage.setItem(FOLK, JSON.stringify(all)); } catch (e) { toast("no room to keep another villager"); return false; }
  return true;
}
function switchTo(i) {
  const all = folk();
  const v = all[i];
  if (!v || !park()) return;
  const now = folk();
  now.splice(i, 1);
  localStorage.setItem(FOLK, JSON.stringify(now));
  if (game) { netBye(); wasm.destroy(game); game = 0; }
  if (v.save) localStorage.setItem(KEY, v.save); else localStorage.removeItem(KEY);
  if (v.cfg) localStorage.setItem(CFG, v.cfg);
  else if (v.opts) localStorage.setItem(CFG, config(Object.assign(opts(), v.opts)));
  build();
  toast("☺ " + (v.name || "another villager"));
}
function newVillager(name) {
  if (!park()) return;
  if (game) { netBye(); wasm.destroy(game); game = 0; }
  localStorage.removeItem(KEY);
  // a villager of their own: the settings as they first are, their name
  localStorage.setItem(CFG, config(Object.assign({}, DEFAULTS, { name, friends: [] })));
  build();
  toast("☺ " + name + ", in a village of their own");
}
// a save's villager (an import, or changed on a linked device) into the settings
function takeYou(you, o) {
  o.name = you.name || "";
  if (you.pet) { o.pet = you.pet.kind || "cat"; o.petcoat = you.pet.coat || ""; o.petname = you.pet.name || ""; }
  o.friends = (you.friends || []).map((fr) => ({ name: fr.name, kid: !!fr.kid, coat: fr.coat || "", hat: fr.hat || "", hair: fr.hair || "", skin: fr.skin || "", scarf: fr.scarf !== false }));
  o.only_friends = !!you.only_friends;
  if (you.look) o.look = you.look;
  o.motto = you.motto || "";
  if (you.wear) { o.wcoat = you.wear.coat || ""; o.what = you.wear.hat || ""; o.whair = you.wear.hair || ""; o.wskin = you.wear.skin || ""; o.wscarf = you.wear.scarf !== false; }
}

function save() {
  if (!game) return;
  try { localStorage.setItem(KEY, getText(wasm.save(game))); } catch (e) {}
}

function frame(now) {
  const dt = last ? Math.min(0.1, (now - last) / 1000) : 1 / 30;
  last = now;
  if (game) {
    const p = wasm.frame(game, dt, localMs());
    if (!img || img.data.buffer !== wasm.memory.buffer || imgPtr !== p) {
      img = new ImageData(new Uint8ClampedArray(wasm.memory.buffer, p, W * H * 4), W, H);
      imgPtr = p;
    }
    ctx.putImageData(img, 0, 0);
    // what the village's menus ask of the page
    asked(getText(wasm.ask(game)));
    netPump(now);
    feedAudio();
  }
  if (touchy) drawPad();
  requestAnimationFrame(frame);
}

// a tab out of sight gets no animation frames: a slow tick instead, so the
// link to the relay stays up and friends' messages are taken in. (Not only
// when the page says it's hidden: a window covered by others can stop
// getting frames while it still says it's visible.)
setInterval(() => {
  if (!game || (!document.hidden && performance.now() - last < 900)) return;
  wasm.frame(game, 0.1, localMs());
  asked(getText(wasm.ask(game)));
  netPump(performance.now());
}, 1000);
// the weather widget's: fetched here (Open-Meteo answers a page), for the
// city in the settings, every half hour or when the city changes
let wxWant = "", wxAt = 0;
async function weather() {
  if (!game) return;
  const want = getText(wasm.weather_want(game));
  if (!want || (want === wxWant && Date.now() - wxAt < 30 * 60 * 1000)) return;
  wxWant = want; wxAt = Date.now();
  const [city, unit] = want.split("|");
  const give = (place, json) => game && wasm.weather_in(game, putText(place + "\u0000" + json));
  if (!city) { give("", "the weather: set its city in the menu (Widgets)"); return; }
  try {
    const g = await (await fetch("https://geocoding-api.open-meteo.com/v1/search?count=1&language=en&format=json&name=" + encodeURIComponent(city))).json();
    const r = g.results && g.results[0];
    if (!r) { give("", "city not found: " + city); return; }
    const f = unit === "f" ? "&temperature_unit=fahrenheit&wind_speed_unit=mph" : "";
    const url = `https://api.open-meteo.com/v1/forecast?latitude=${r.latitude.toFixed(3)}&longitude=${r.longitude.toFixed(3)}&current=temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m,is_day&daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset&forecast_days=4&timezone=auto${f}`;
    const text = await (await fetch(url)).text();
    give(r.name || city, text);
  } catch (e) { wxAt = Date.now() - 25 * 60 * 1000; give("", "the weather couldn't be fetched"); }
}
setInterval(weather, 5000);
// the page's title: the village, and where in it
setInterval(() => {
  if (!game) return;
  const p = getText(wasm.place(game));
  const t = p ? "Village · " + p : "Village";
  if (document.title !== t) document.title = t;
  // (paused from the ☰ menu or the home page too: the bar's button follows)
  const b = document.getElementById("b-pause"), ch = wasm.paused(game) ? "▶" : "⏸";
  if (b.textContent !== ch) b.textContent = ch;
}, 1500);

// what the village asks of the page: the settings, a line typed (a
// friend's code, a message), a code onto the clipboard
function asked(a) {
  if (!a) return;
  if (a === "settings") wasm.open_panel(game);
  else if (a.startsWith("config:")) {
    // the settings changed in the menu: kept
    try { localStorage.setItem(CFG, a.slice(7)); } catch (e) {}
    sizeButtons();
    sound();
  } else if (a.startsWith("edit:")) {
    // one of the menu's fields: its own box (a phone's keyboard comes up)
    held.clear(); steer();
    const [q, v] = a.slice(5).split("\u0001");
    askFor(q, v || "");
  } else if (a === "restart") { save(); location.reload(); }
  else if (a === "export") exportSave();
  else if (a === "import") document.getElementById("o-file").click();
  else if (a === "fresh") fresh();
  else if (a === "uninstall") forget(true);
  else if (a.startsWith("text:")) {
    held.clear(); steer();
    askFor(a.slice(5));
  } else if (a.startsWith("rules:") && false) {
    // the village's settings, changed by a guest (said yes to): kept
    try {
      const r = JSON.parse(a.slice(6)), o = opts();
      const pc = (v) => Math.round(v * 100);
      Object.assign(o, { snow: pc(r.snow), wind: pc(r.wind), glow: pc(r.glow), events: pc(r.events), critters: pc(r.critters), chattiness: pc(r.chattiness), most: r.most_at_once | 0, sleigh: r.sleigh_minutes, aurora: !!r.aurora, bushes: !!r.bushes, time: r.time || "auto" });
      setOpts(o);
      if (box.classList.contains("open")) show(tab);
    } catch (e) {}
  } else if (a.startsWith("you:") && false) {
    try { const o = opts(); takeYou(JSON.parse(a.slice(4)), o); setOpts(o); if (box.classList.contains("open")) show(tab); } catch (e) {}
  } else if (a.startsWith("villager:")) {
    setTimeout(() => switchTo(+a.slice(9)), 0);
  } else if (a.startsWith("newvillager:")) {
    const n = a.slice(12);
    setTimeout(() => newVillager(n), 0);
  } else if (a.startsWith("copy:")) {
    const t = a.slice(5);
    try { navigator.clipboard.writeText(t).then(() => toast("copied: " + t), () => toast(t)); } catch (e) { toast(t); }
  }
}

// a line to type (a friend's code, a message): a box in the page, not the
// browser's own prompt
const askBox = document.getElementById("ask"), askIn = document.getElementById("ask-in");
// a code asked for: capitals and a dash between each four, as it's typed
// (a group's: then a name after a space, kept as it is)
let askCode = null;
function typingCode(v, more) {
  const sp = more ? v.indexOf(" ") : -1;
  const code = sp >= 0 ? v.slice(0, sp) : v, rest = sp >= 0 ? v.slice(sp) : "";
  const n = code.replace(/[^0-9a-z]/gi, "").toUpperCase().slice(0, 12);
  return n.replace(/(.{4})(?=.)/g, "$1-") + rest;
}
askIn.addEventListener("input", () => {
  if (askCode === null) return;
  const v = typingCode(askIn.value, askCode);
  if (v !== askIn.value) { askIn.value = v; askIn.setSelectionRange(v.length, v.length); }
});
function askFor(q, value) {
  document.getElementById("ask-q").textContent = q;
  askCode = q.includes("XXXX-XXXX-XXXX") ? q.includes("name") : null;
  askIn.autocapitalize = askCode === null ? "sentences" : "characters";
  askIn.value = value || "";
  askBox.classList.add("open");
  setTimeout(() => askIn.focus(), 0);
}
function askDone(v) {
  askBox.classList.remove("open");
  askIn.blur();
  if (game) wasm.give_text(game, putText(v || ""));
}
askBox.onsubmit = (e) => { e.preventDefault(); askDone(askIn.value); };
document.getElementById("ask-no").onclick = () => askDone("");
askIn.addEventListener("keydown", (e) => { if (e.key === "Escape") { e.preventDefault(); askDone(""); } e.stopPropagation(); });

// ------------------------------------------------------------ online
// The relay (the village's own, else a free public MQTT broker) through a
// WebSocket, or plain HTTPS requests where a network breaks WebSockets; the
// village seals and unseals what goes through it, the page only carries
// bytes.

const RELAYS = ["wss://xmas-relay.novalabs.app/mqtt", "https://xmas-relay.novalabs.app/poll", "wss://broker.emqx.io:8084/mqtt", "wss://broker.hivemq.com:8884/mqtt", "wss://test.mosquitto.org:8081/mqtt"];

// The same MQTT over plain HTTPS requests ("long polling", the relay's
// /poll): one request always waiting for what's coming (answered the moment
// there's something), and what's sent in requests of its own, in order.
// Shaped like a WebSocket, so the rest doesn't mind which it has.
class PollSocket {
  constructor(url) {
    this.url = url.replace(/\/$/, "");
    this.readyState = 0;
    this.onopen = this.onmessage = this.onclose = this.onerror = null;
    this.queue = [];
    this.sending = false;
    this.id = null;
    this.start();
  }
  async post(path, body, ms, keep) {
    const ctl = new AbortController();
    const t = setTimeout(() => ctl.abort(), ms);
    try {
      // (text/plain: a "simple" request, so no CORS preflight before each
      // one; the relay takes the bytes as they are)
      return await fetch(this.url + path, { method: "POST", body: body || new Uint8Array(0), headers: { "content-type": "text/plain" }, signal: ctl.signal, cache: "no-store", keepalive: !!keep });
    } finally { clearTimeout(t); }
  }
  async start() {
    try {
      const r = await this.post("", null, 8000);
      const id = (await r.text()).trim();
      if (!r.ok || !/^[0-9a-f]{32}$/.test(id)) throw new Error("no session");
      if (this.readyState !== 0) return;
      this.id = id;
      this.readyState = 1;
      if (this.onopen) this.onopen({});
      this.down();
      this.flush();
    } catch (e) { this.end(); }
  }
  async down() {
    while (this.readyState === 1) {
      let b;
      try {
        const r = await this.post(`/${this.id}/down`, null, 35000);
        if (r.status !== 200) throw new Error("over");
        b = await r.arrayBuffer();
      } catch (e) { this.end(); return; }
      if (b.byteLength && this.readyState === 1 && this.onmessage) this.onmessage({ data: b });
    }
  }
  send(b) {
    if (this.readyState !== 1) return;
    this.queue.push(new Uint8Array(b));
    this.flush();
  }
  take() {
    const parts = this.queue.splice(0), body = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
    let at = 0;
    for (const p of parts) { body.set(p, at); at += p.length; }
    return body;
  }
  async flush() {
    if (this.sending || !this.queue.length || this.readyState !== 1) return;
    this.sending = true;
    try {
      const r = await this.post(`/${this.id}/up`, this.take(), 15000, true);
      if (r.status !== 204) throw new Error("over");
    } catch (e) { this.sending = false; this.end(); return; }
    this.sending = false;
    this.flush();
  }
  // what's queued (a goodbye) still goes, then the session's closed
  async close() {
    if (this.readyState >= 2) return;
    const was = this.readyState;
    this.readyState = 2;
    if (was === 1) {
      while (this.sending) await new Promise((r) => setTimeout(r, 20));
      try {
        if (this.queue.length) await this.post(`/${this.id}/up`, this.take(), 5000, true);
        await this.post(`/${this.id}/close`, null, 5000, true);
      } catch (e) {}
    }
    this.end();
  }
  end() {
    if (this.readyState === 3) return;
    this.readyState = 3;
    if (this.onclose) this.onclose({});
  }
}
let ws = null, wsTry = 0, wsWait = 0, wsOpenAt = 0, relays = null;
fetch("relays.json").then((r) => r.json()).then((j) => { if (Array.isArray(j.ws) && j.ws.length) relays = j.ws; }).catch(() => {});
function netPump(now) {
  const want = wasm.net_want(game);
  // (another friend code, from a linked device: the link made again)
  if (want === 2 && ws) { flushNet(); ws.onclose = null; ws.close(); ws = null; wasm.net_link(game, 0); }
  if (!want) {
    if (ws) { flushNet(); ws.onclose = null; ws.close(); ws = null; }
    return;
  }
  // a relay that took the connection but goes nowhere: on to the next
  if (ws && ws.readyState === 1 && wasm.net_stuck && wasm.net_stuck(game)) {
    ws.onclose = null; ws.close(); ws = null; wasm.net_link(game, 0);
    wsTry++; wsWait = now + 500;
  }
  if (!ws && now > wsWait) {
    const list = relays || RELAYS;
    const url = list[wsTry % list.length];
    wsTry++;
    try { ws = /^https?:/.test(url) ? new PollSocket(url) : new WebSocket(url, "mqtt"); } catch (e) { ws = null; wsWait = now + 3000; return; }
    ws.binaryType = "arraybuffer";
    // (the village's own relay: what it says as itself is believed)
    const own = /^(wss|https):\/\/xmas-relay\.novalabs\.app\//.test(url);
    ws.onopen = () => { wsTry--; wsOpenAt = performance.now(); if (game) wasm.net_link(game, own ? 2 : 1); };
    ws.onmessage = (e) => {
      if (!game) return;
      const b = new Uint8Array(e.data);
      const p = wasm.text_buf(0, b.length);
      new Uint8Array(wasm.memory.buffer, p, b.length).set(b);
      wasm.net_in(game, b.length);
    };
    // (a link that had been up a while: straight back, it was a blip;
    // one that wasn't: a few seconds first)
    ws.onclose = () => { if (game) wasm.net_link(game, 0); ws = null; const now = performance.now(); wsWait = now + (wsOpenAt && now - wsOpenAt > 15000 ? 150 : 4000); wsOpenAt = 0; };
    ws.onerror = () => {};
  }
  flushNet();
}
function flushNet() {
  const n = wasm.net_out(game);
  if (n && ws && ws.readyState === 1) ws.send(new Uint8Array(wasm.memory.buffer, wasm.text_ptr(game), n).slice());
}
function netBye() {
  if (!ws) return;
  const n = wasm.net_bye(game);
  if (n && ws.readyState === 1) ws.send(new Uint8Array(wasm.memory.buffer, wasm.text_ptr(game), n).slice());
  ws.onclose = null;
  ws.close();
  ws = null;
}

// ------------------------------------------------------------ sound

let ac = null, until = 0;
const AHEAD = 0.14, CHUNK = 2048;
function sound() {
  if (!game) return;
  document.getElementById("b-sound").style.opacity = wasm.sound_on(game) ? 1 : 0.4;
}
// (a browser lets sound start only after a key or a tap)
function wake() {
  if (ac) { if (ac.state === "suspended") ac.resume(); return; }
  if (!game || !wasm.sound_on(game)) return;
  try { ac = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 48000 }); } catch (e) { ac = null; }
}
function feedAudio() {
  if (!ac || ac.state !== "running" || !wasm.sound_on(game)) return;
  if (until < ac.currentTime) until = ac.currentTime + 0.03;
  while (until - ac.currentTime < AHEAD) {
    const p = wasm.audio(game, CHUNK);
    const f = new Float32Array(wasm.memory.buffer, p, CHUNK * 2);
    const buf = ac.createBuffer(2, CHUNK, 48000);
    const l = buf.getChannelData(0), r = buf.getChannelData(1);
    for (let i = 0; i < CHUNK; i++) { l[i] = f[i * 2]; r[i] = f[i * 2 + 1]; }
    const src = ac.createBufferSource();
    src.buffer = buf;
    src.connect(ac.destination);
    src.start(until);
    until += CHUNK / 48000;
  }
}

// ------------------------------------------------------------ keyboard

const held = new Set();
function steer() {
  const k = (a) => a.some((c) => held.has(c));
  const mx = (k(["KeyD", "ArrowRight"]) ? 1 : 0) - (k(["KeyA", "ArrowLeft"]) ? 1 : 0);
  const my = (k(["KeyS", "ArrowDown"]) ? 1 : 0) - (k(["KeyW", "ArrowUp"]) ? 1 : 0);
  wasm.steer(game, mx, my, held.has("ShiftLeft") || held.has("ShiftRight") ? 1 : 0);
}
addEventListener("keydown", (e) => {
  if (!game || /INPUT|SELECT|TEXTAREA/.test(e.target.tagName)) return;
  wake();
  if (e.repeat && !/^(Key[WASD]|Arrow)/.test(e.code)) { e.preventDefault(); return; }
  held.add(e.code);
  const menu = wasm.menu_open(game);
  switch (e.code) {
    case "Space": wasm.button(game, BTN.JUMP); break;
    case "KeyE": case "Enter": wasm.button(game, BTN.E); break;
    case "KeyR": wasm.button(game, BTN.SNOWBALL); break;
    case "KeyF": wasm.button(game, BTN.INVITE); break;
    case "KeyI": wasm.button(game, BTN.POCKETS); break;
    case "KeyQ": wasm.button(game, BTN.HELD); break;
    case "KeyC": wasm.button(game, BTN.CENTRE); break;
    case "KeyZ": wasm.button(game, BTN.REST); break;
    case "KeyP": togglePause(); break;
    case "KeyT": if (menu) wasm.back(game); else wasm.todo(game); break;
    case "KeyO": if (menu) wasm.back(game); else wasm.online(game); break;
    case "Slash": wasm.chat(game); break;
    case "KeyY": case "KeyN": wasm.answer(game, e.code === "KeyY" ? 1 : 0); break;
    case "Escape": wasm.back(game); break;
    default:
      if (/^Digit[1-9]$/.test(e.code)) wasm.button(game, BTN.NUM + +e.code.slice(5));
      else if (e.key === "?") wasm.help(game, 1);
      else if (e.key === "/") { wasm.chat(game); e.preventDefault(); }
  }
  steer();
  if (/^(Key[WASDEFRZTQICOYNP]|Arrow|Space|Enter|Digit|Escape|Slash)/.test(e.code)) e.preventDefault();
});
addEventListener("keyup", (e) => {
  if (!game) return;
  held.delete(e.code);
  if (e.code === "Space") wasm.button(game, BTN.JUMP_UP);
  steer();
});
addEventListener("blur", () => { held.clear(); if (game) steer(); });

// ------------------------------------------------------------ mouse & fingers on the village

function at(e) {
  const r = canvas.getBoundingClientRect();
  return [(e.clientX - r.left) * W / r.width, (e.clientY - r.top) * H / r.height];
}
let down = null, longT = 0;
canvas.addEventListener("pointerdown", (e) => {
  if (!game || e.button === 2) return;
  wake();
  canvas.setPointerCapture(e.pointerId);
  const [x, y] = at(e);
  down = { x, y, id: e.pointerId };
  wasm.touch(game, 0, x, y);
  clearTimeout(longT);
  // held still: what can be done there (over a menu, the menu has it)
  if (!wasm.menu_open(game)) {
    longT = setTimeout(() => { if (down) { wasm.touch(game, 3, down.x, down.y); wasm.menu(game, down.x, down.y); down = null; } }, 550);
  }
});
canvas.addEventListener("pointermove", (e) => {
  if (!game) return;
  const [x, y] = at(e);
  if (!down || e.pointerId !== down.id) { wasm.hover(game, x, y); return; }
  if (Math.abs(x - down.x) + Math.abs(y - down.y) > 8) clearTimeout(longT);
  wasm.touch(game, 1, x, y);
});
canvas.addEventListener("pointerup", (e) => {
  clearTimeout(longT);
  if (!game || !down || e.pointerId !== down.id) return;
  const [x, y] = at(e);
  wasm.touch(game, 2, x, y);
  down = null;
});
canvas.addEventListener("pointercancel", () => { clearTimeout(longT); if (game && down) wasm.touch(game, 3, down.x, down.y); down = null; });
canvas.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  if (!game) return;
  const [x, y] = at(e);
  wasm.menu(game, x, y);
});
canvas.addEventListener("wheel", (e) => {
  if (!game) return;
  e.preventDefault();
  // over the menu: it scrolls (or steps the value under the pointer)
  const [x, y] = at(e);
  if (e.deltaY && wasm.panel_wheel(game, x, y, e.deltaY < 0 ? 1 : 0)) return;
  wasm.pan(game, -(e.deltaX || e.deltaY) * W / canvas.getBoundingClientRect().width);
}, { passive: false });

// ------------------------------------------------------------ phone controls (pixel art)

const ICONS = [
  ["#####..", "##.....", "##.....", "####...", "##.....", "##.....", "#####.."],
  ["...#...", "..###..", ".#####.", "#######", "..###..", "..###..", "..###.."],
  ["..###..", ".#####.", "#######", "#######", "#######", ".#####.", "..###.."],
  [".......", "#######", ".......", "#######", ".......", "#######", "......."],
  ["...#...", "...#...", ".#####.", "#######", ".#####.", "...#...", "...#..."],
];
let stick = null, knob = null, pressed = new Map();
function layout() {
  const w = pad.width, h = pad.height, s = Math.min(w, h) / 360 * btnScale();
  return {
    stick: { x: 40 * s + 58 * s, y: h - 36 * s - 58 * s, r: 58 * s },
    btn: [
      { x: w - 70 * s, y: h - 82 * s, r: 40 * s, b: "E", icon: 0, col: "#c83a46" },
      { x: w - 170 * s, y: h - 52 * s, r: 32 * s, b: "JUMP", icon: 1, col: "#1e2850" },
      { x: w - 62 * s, y: h - 182 * s, r: 28 * s, b: "SNOWBALL", icon: 2, col: "#1e2850" },
      { x: w - 150 * s, y: h - 150 * s, r: 26 * s, b: "MORE", icon: 3, col: "#1e2850" },
      { x: w - 230 * s, y: h - 120 * s, r: 26 * s, b: "TODO", icon: 4, col: "#28503c" },
    ],
  };
}
function disc(c, cx, cy, r, body, edge, P) {
  const n = Math.ceil(r / P), ox = Math.round(cx / P) * P, oy = Math.round(cy / P) * P;
  for (let j = -n; j <= n; j++) for (let i = -n; i <= n; i++) {
    const d = Math.hypot(i, j) * P;
    if (d > r) continue;
    c.fillStyle = d > r - P ? edge : body;
    c.fillRect(ox + i * P, oy + j * P, P, P);
  }
}
function drawPad() {
  const c = pctx, L = layout(), P = Math.max(3, Math.round(Math.min(pad.width, pad.height) / 140));
  c.clearRect(0, 0, pad.width, pad.height);
  // (the menu open: the joystick and buttons out of its way)
  if (game && wasm.panel_open(game)) return;
  disc(c, L.stick.x, L.stick.y, L.stick.r, "rgba(200,220,255,0.22)", "rgba(255,211,107,0.7)", P);
  const k = knob || { x: L.stick.x, y: L.stick.y };
  disc(c, k.x, k.y, L.stick.r * 0.42, "rgba(255,236,200,0.75)", "rgba(255,211,107,0.9)", P);
  L.btn.forEach((b, i) => {
    const on = [...pressed.values()].includes(i);
    disc(c, b.x, b.y + (on ? P : 0), b.r, b.col + (on ? "ee" : "aa"), "rgba(255,211,107,0.9)", P);
    const art = ICONS[b.icon], s = P * (b.r > 30 ? 2 : 1);
    const ox = Math.round(b.x / P) * P - s * 3.5, oy = Math.round((b.y + (on ? P : 0)) / P) * P - s * 3.5;
    c.fillStyle = b.b === "TODO" ? "#ffdc78" : "#faf6ec";
    art.forEach((row, j) => [...row].forEach((ch, i2) => { if (ch === "#") c.fillRect(ox + i2 * s, oy + j * s, s, s); }));
  });
}
function padSteer(x, y) {
  const L = layout(), r = L.stick.r;
  let dx = (x - L.stick.x) / r, dy = (y - L.stick.y) / r;
  const len = Math.hypot(dx, dy);
  if (len > 1) { dx /= len; dy /= len; }
  wasm.steer(game, Math.abs(dx) < 0.3 ? 0 : Math.sign(dx), Math.abs(dy) < 0.4 ? 0 : Math.sign(dy), len > 0.95 ? 1 : 0);
}
pad.addEventListener("pointerdown", (e) => {
  if (!game) return;
  wake();
  // the menu open: every touch is the menu's
  if (wasm.panel_open(game)) { canvas.dispatchEvent(new PointerEvent("pointerdown", e)); return; }
  const L = layout(), x = e.clientX * pad.width / innerWidth, y = e.clientY * pad.height / innerHeight;
  if (Math.hypot(x - L.stick.x, y - L.stick.y) < L.stick.r * 1.5 && !stick) {
    stick = e.pointerId; knob = { x, y }; padSteer(x, y); pad.setPointerCapture(e.pointerId); return;
  }
  const i = L.btn.findIndex((b) => Math.hypot(x - b.x, y - b.y) < b.r * 1.25);
  if (i >= 0) {
    pressed.set(e.pointerId, i);
    const b = L.btn[i].b;
    if (b === "MORE") { if (wasm.menu_open(game)) wasm.back(game); else wasm.more(game); }
    else if (b === "TODO") { if (wasm.menu_open(game)) wasm.back(game); else wasm.todo(game); }
    else wasm.button(game, BTN[b]);
    return;
  }
  // anywhere else: the village itself
  canvas.dispatchEvent(new PointerEvent("pointerdown", e));
});
pad.addEventListener("pointermove", (e) => {
  if (e.pointerId === stick) { const x = e.clientX * pad.width / innerWidth, y = e.clientY * pad.height / innerHeight; knob = { x, y }; padSteer(x, y); }
  else canvas.dispatchEvent(new PointerEvent("pointermove", e));
});
function padUp(e) {
  if (e.pointerId === stick) { stick = null; knob = null; wasm.steer(game, 0, 0, 0); return; }
  if (pressed.has(e.pointerId)) {
    if (pressed.get(e.pointerId) === 1) wasm.button(game, BTN.JUMP_UP);
    pressed.delete(e.pointerId);
    return;
  }
  canvas.dispatchEvent(new PointerEvent("pointerup", e));
}
pad.addEventListener("pointerup", padUp);
pad.addEventListener("pointercancel", padUp);

// ------------------------------------------------------------ the settings panel

const box = document.getElementById("opts"), pane = document.getElementById("pane"), tabsEl = document.getElementById("tabs");
const TABS = ["Villager", "Friends", "Village", "Sound", "Looks", "Save"];
let tab = 0, rebuildT = 0;

function toast(s) {
  const t = document.getElementById("toast");
  t.textContent = s;
  t.style.display = "block";
  clearTimeout(t._t);
  t._t = setTimeout(() => (t.style.display = "none"), 2600);
}
// a change: kept, and taken in by the village where it stands (still in a
// friend's village, still online); a slider being dragged, a moment after
// it stops
function change(k, v, live) {
  const o = opts();
  o[k] = v;
  setOpts(o);
  if (k === "btnscale") { sizeButtons(); return; }
  if (live) { if (k === "sound" && v) wake(); sound(); return; }
  reconfigure();
}
function reconfigure() {
  clearTimeout(rebuildT);
  rebuildT = setTimeout(() => {
    if (!game) return;
    const o = opts();
    wasm.configure(game, putText(config(o)), o.size / 100);
  }, 120);
}
function el(tag, attrs, ...kids) {
  const e = document.createElement(tag);
  Object.entries(attrs || {}).forEach(([k, v]) => (k in e ? (e[k] = v) : e.setAttribute(k, v)));
  kids.forEach((c) => e.append(c));
  return e;
}
function h3(s) { pane.append(el("h3", { textContent: s })); }
function note(s) { pane.append(el("div", { className: "note", textContent: s })); }
function row(label, input, out) { const r = el("div", { className: "row" }, el("label", { textContent: label }), input); if (out) r.append(out); pane.append(r); return r; }
function check(label, k, live) { const c = el("input", { type: "checkbox", checked: !!opts()[k] }); c.onchange = () => change(k, c.checked, live); row(label, c); }
function text(label, k, ph) { const t = el("input", { type: "text", value: opts()[k] || "", placeholder: ph, maxLength: 20 }); t.oninput = () => change(k, t.value.trim()); row(label, t); }
function pick(label, k, items, after) {
  const s = el("select", {}, ...items.map((i) => el("option", { value: i, textContent: i || "any" })));
  const v = opts()[k];
  s.value = v == null || !items.includes(v) ? items[0] : v;
  s.onchange = () => { change(k, s.value); if (after) after(); };
  row(label, s);
}
function pickWords(label, k, values, words) {
  const s = el("select", {}, ...values.map((v, i) => el("option", { value: v, textContent: words[i] })));
  const v = String(opts()[k]);
  s.value = values.includes(v) ? v : values[0];
  s.onchange = () => change(k, s.value);
  row(label, s);
}
// the buttons' size (the bar up top, the joystick and buttons on a touch screen)
function btnScale() { return game ? Math.max(0.6, Math.min(2.5, wasm.buttons_k(game))) : 1; }
function sizeButtons() {
  document.documentElement.style.setProperty("--btn", btnScale());
  if (touchy) drawPad();
}
function slide(label, k, min, max, unit, live) {
  const v = opts()[k];
  const out = el("output", { textContent: v + unit });
  const r = el("input", { type: "range", min, max, value: v });
  r.oninput = () => { out.textContent = r.value + unit; change(k, +r.value, live); };
  row(label, r, out);
}
function button(label, fn, quiet) { const b = el("button", { className: "btn" + (quiet ? " quiet" : ""), textContent: label }); b.onclick = fn; return b; }

const COATS = { dog: ["golden", "chocolate", "black", "snowy", "spotty", "grey", "red"], cat: ["ginger", "black", "snowy", "tabby", "calico", "cream", "tuxedo"], bunny: ["snowy", "brown", "grey", "black", "cream"] };

function friendBox(f, k) {
  const fr = el("div", { className: "friend" });
  const put = (key, v) => { const p = opts(); p.friends[k][key] = v; setOpts(p); reconfigure(); };
  const name = el("input", { type: "text", value: f.name || "", maxLength: 16 });
  name.oninput = () => put("name", name.value);
  const kid = el("input", { type: "checkbox", checked: !!f.kid });
  kid.onchange = () => put("kid", kid.checked);
  const del = button("✕", () => { const p = opts(); p.friends.splice(k, 1); setOpts(p); show(1); reconfigure(); }, true);
  const sel = (key, items) => {
    const s = el("select", {}, ...items.map((v) => el("option", { value: v, textContent: v || "any" })));
    s.value = items.includes(f[key] || "") ? f[key] || "" : "";
    s.onchange = () => put(key, s.value);
    return s;
  };
  fr.append(el("div", { className: "row" }, name, el("label", { textContent: "kid" }), kid, del));
  fr.append(el("div", { className: "row" }, "coat", sel("coat", ["", "red", "green", "blue", "purple", "gold", "pink", "grey", "white"]), "hat", sel("hat", ["", "red", "green", "blue", "gold", "pink", "white", "none"])));
  fr.append(el("div", { className: "row" }, "hair", sel("hair", ["", "brown", "dark", "blonde", "red", "grey"]), "skin", sel("skin", ["", "light", "fair", "tan", "deep"])));
  return fr;
}

function show(i) {
  tab = i;
  tabsEl.innerHTML = "";
  TABS.forEach((t, k) => { const s = el("span", { textContent: t, className: k === i ? "on" : "" }); s.onclick = () => show(k); tabsEl.append(s); });
  pane.innerHTML = "";
  const o = opts();
  if (i === 0) {
    h3("THE VILLAGER");
    text("Name", "name", "neighbour");
    const line = el("input", { type: "text", value: o.motto || "", placeholder: "(friends online see it)", maxLength: 60 });
    line.oninput = () => change("motto", line.value.trim());
    row("Card's line", line);
    pick("Your coat", "wcoat", ["", "red", "green", "blue", "purple", "gold", "pink", "grey", "white"]);
    pick("Your hat", "what", ["", "red", "green", "blue", "gold", "pink", "white", "none"]);
    pick("Your hair", "whair", ["", "brown", "dark", "blonde", "red", "grey"]);
    pick("Your skin", "wskin", ["", "light", "fair", "tan", "deep"]);
    check("A scarf", "wscarf");
    note("(\"any\" here is the classic look; friends online see you this way, and on your profile card)");
    pick("Pet", "pet", ["cat", "dog", "bunny", "none"], () => show(0));
    if (o.pet !== "none") {
      pick("Pet's coat", "petcoat", COATS[o.pet] || [""]);
      text("Pet's name", "petname", "(it asks)");
    }
    slide("Walking speed", "walk", 50, 300, "%");
    check("Autopilot (left alone: off doing their own thing)", "auto");
    pickWords("Takes over after", "autoafter", ["5", "10", "20", "30", "45", "60", "120", "300", "600", "1800", "3600"],
      ["5 s alone", "10 s alone", "20 s alone", "30 s alone", "45 s alone", "1 min alone", "2 min alone", "5 min alone", "10 min alone", "30 min alone", "1 h alone"]);
    pickWords("Keeps to", "autoarea", ["anywhere", "home", "tree", "here"], ["anywhere in the village", "near home", "by the big tree", "where it took over"]);
    pickWords("That stretch is", "autoreach", ["0.5", "0.75", "1", "1.5", "2", "3", "4"], ["½ screen wide", "¾ screen wide", "1 screen wide", "1½ screens wide", "2 screens wide", "3 screens wide", "4 screens wide"]);
  } else if (i === 1) {
    h3("ONLINE");
    pickWords("Most visitors", "vlimit", ["2", "3", "4", "6", "8", "10", "12", "16", "20", "24", "32", "48"], ["2 at once", "3 at once", "4 at once", "6 at once", "8 at once", "10 at once", "12 at once", "16 at once", "20 at once", "24 at once", "32 at once", "48 at once"]);
    note("The most in your village when it's open to friends (or listed in Public villages). The public squares take 16.");
    check("Star out strong language (links are always hidden)", "cleanwords");
    h3("FRIENDS & FAMILY");
    note("Some of the folk who come by take these names, know the villager, and wear what's chosen for them (\"any\": picked from the name).");
    check("Only them (no strangers; the animals still come)", "only_friends");
    o.friends.forEach((f, k) => pane.append(friendBox(f, k)));
    pane.append(button("+ add someone", () => { const p = opts(); p.friends.push({ name: "Friend " + (p.friends.length + 1), kid: false, scarf: true }); setOpts(p); show(1); reconfigure(); }));
  } else if (i === 2) {
    h3("THE VILLAGE");
    pick("Time of day", "time", ["auto", "night", "dawn", "morning", "noon", "afternoon", "dusk"]);
    slide("Snowfall", "snow", 0, 300, "%");
    slide("Wind", "wind", 0, 300, "%");
    check("Northern lights", "aurora");
    slide("Size", "size", 50, 200, "%");
    slide("Zoom", "zoom", 100, 400, "%");
    h3("SIZES");
    slide("Menus, windows & dialogs", "uiscale", 100, 250, "%");
    slide("Buttons", "btnscale", 60, 250, "%");
    h3("FOLK & HAPPENINGS");
    slide("Animals & people coming by", "critters", 0, 300, "%");
    slide("The most out at once", "most", 2, 40, "");
    check("Berry bushes (and birds)", "bushes");
    slide("Surprises", "events", 0, 300, "%");
    slide("Santa's sleigh: every", "sleigh", 0, 20, " min");
    slide("Chattiness", "chattiness", 0, 300, "%");
    slide("Speech bubbles stay up", "talk_time", 50, 300, "%");
  } else if (i === 3) {
    h3("SOUND");
    check("Sound", "sound", true);
    slide("Volume", "master", 0, 100, "%", true);
    slide("Music", "music", 0, 100, "%", true);
    slide("Effects", "sfx", 0, 100, "%", true);
    slide("Wind, fire & chimes", "ambience", 0, 100, "%", true);
    pick("Music", "track", ["shuffle", "music-box", "celesta", "piano", "sleigh-ride"]);
    note("A browser lets a page make sound only after a key or a click.");
  } else if (i === 4) {
    h3("PANELS & MENUS");
    pick("Their look", "look", ["classic", "frosted", "wood", "gift"]);
    note("The menus, conversations, the games' table and the cards in the village: classic, frosted with snow on top, a wooden frame, or a gift box tied up with a gold ribbon.");
    slide("Glow of the lights", "glow", 20, 200, "%");
  } else {
    h3("THE SAVE");
    note("The village (the same houses, home where it was), the villager's pockets, stats and scorecard, and the villager themself: their name, the pet, friends & family. Kept in this browser as it goes. The desktop app and the Android app read the same save files.");
    pane.append(button("⇩ export", exportSave), button("⇧ import", () => document.getElementById("o-file").click()), button("✦ a new village", fresh, true));
    pane.append(el("div", { className: "note", innerHTML: 'The apps: <a style="color:var(--gold)" href="https://github.com/dash1101/xmas-tui">github.com/dash1101/xmas-tui</a>' }));
    h3("UPKEEP");
    pane.append(button("⟲ reset settings", resetOpts), button("⌫ forget this browser's village", forget, true));
    note("Forgetting takes the village, the villager and the settings out of this browser (export first to keep them).");
  }
}
function openOpts(on) {
  const open = on === undefined ? !box.classList.contains("open") : on;
  box.classList.toggle("open", open);
  if (!open) return;
  show(tab);
  // where it was left (kept on the screen)
  let p = null;
  try { p = JSON.parse(localStorage.getItem(PLACE) || "null"); } catch (e) {}
  place(p ? p[0] : innerWidth - box.offsetWidth - 12, p ? p[1] : 44);
}
function place(x, y) {
  box.style.left = Math.max(0, Math.min(innerWidth - box.offsetWidth, x)) + "px";
  box.style.top = Math.max(0, Math.min(innerHeight - 40, y)) + "px";
}
// moved by its title bar
let grip = null;
const gripEl = document.getElementById("o-grip");
gripEl.addEventListener("pointerdown", (e) => {
  if (e.target.id === "o-close") return;
  const r = box.getBoundingClientRect();
  grip = [e.clientX - r.left, e.clientY - r.top];
  gripEl.setPointerCapture(e.pointerId);
});
gripEl.addEventListener("pointermove", (e) => { if (grip) place(e.clientX - grip[0], e.clientY - grip[1]); });
gripEl.addEventListener("pointerup", () => {
  if (!grip) return;
  grip = null;
  try { localStorage.setItem(PLACE, JSON.stringify([box.offsetLeft, box.offsetTop])); } catch (e) {}
});
document.getElementById("o-close").onclick = () => openOpts(false);

function exportSave() {
  save();
  const blob = new Blob([localStorage.getItem(KEY) || ""], { type: "text/plain" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = `xmas-tui-save-${new Date().toISOString().slice(0, 10)}.toml`;
  a.click();
  URL.revokeObjectURL(a.href);
}
document.getElementById("o-file").onchange = async (e) => {
  const f = e.target.files[0];
  e.target.value = "";
  if (!f) return;
  const text = await f.text();
  if (!/seed\s*=\s*[1-9]/.test(text)) { toast("that isn't an xmas-tui save"); return; }
  // the villager comes too: their name, pet, friends & family, the look
  let you = null;
  try { you = JSON.parse(getText(wasm.you_json(game, putText(text))) || "null"); } catch (err) {}
  const o = opts();
  if (you) {
    takeYou(you, o);
    setOpts(o);
  }
  if (game) { wasm.destroy(game); game = 0; }
  localStorage.setItem(KEY, text);
  build();
  show(tab);
  toast(o.name ? `imported ${o.name}'s village` : "imported the village");
};
// every setting back to how it starts, keeping the villager's own: name,
// motto, look, the pet, friends & family
function resetOpts() {
  if (!confirm("Every setting back as it first was? (the villager, their look, the pet, friends & family and the village stay)")) return;
  const o = opts(), keep = {};
  for (const k of ["name", "motto", "pet", "petcoat", "petname", "wcoat", "what", "whair", "wskin", "wscarf", "friends"]) keep[k] = o[k];
  setOpts(keep);
  try { localStorage.removeItem(PLACE); } catch (e) {}
  if (game) { wasm.destroy(game); game = 0; }
  build();
  show(tab);
  toast("settings reset");
}
// everything xmas-tui kept in this browser, gone
function forget(asked) {
  if (!asked && !confirm("Forget this browser's village? The village, the villager and the settings go (an exported save brings them back).")) return;
  if (game) { wasm.destroy(game); game = 0; }
  try {
    for (const k of Object.keys(localStorage)) if (k.startsWith("xmas-tui")) localStorage.removeItem(k);
  } catch (e) {}
  location.reload();
}
function fresh() {
  if (!confirm("A new village? (this one's pockets and scorecard go with it)")) return;
  if (game) { wasm.destroy(game); game = 0; }
  localStorage.removeItem(KEY);
  build();
  toast("a new village");
}

// ------------------------------------------------------------ the bar

document.getElementById("b-todo").onclick = () => { if (!game) return; if (wasm.menu_open(game)) wasm.back(game); else wasm.todo(game); };
document.getElementById("b-more").onclick = () => { if (!game) return; if (wasm.menu_open(game)) wasm.back(game); else wasm.more(game); };
document.getElementById("b-centre").onclick = () => game && wasm.button(game, BTN.CENTRE);
// pause (the village stands still and quiet; online goes on)
function togglePause() {
  if (!game) return;
  wasm.toggle_pause(game);
  const p = !!wasm.paused(game);
  const b = document.getElementById("b-pause");
  b.textContent = p ? "▶" : "⏸";
  b.title = p ? "carry on (P)" : "pause the village (P)";
  toast(p ? "⏸ paused" : "▶ carrying on");
}
document.getElementById("b-pause").onclick = togglePause;
document.getElementById("b-online").onclick = () => { if (!game) return; if (wasm.menu_open(game)) wasm.back(game); else wasm.online(game); };
document.getElementById("b-help").onclick = () => game && wasm.help(game, touchy ? 0 : 1);
sizeButtons();
document.getElementById("b-opts").onclick = () => game && wasm.open_panel(game);
document.getElementById("b-sound").onclick = () => {
  if (!game) return;
  wasm.toggle_sound(game);
  wake();
  sound();
  toast(wasm.sound_on(game) ? "♪ sound on" : "sound off");
};

// ------------------------------------------------------------ start

// resized (or a phone turned): the village keeps going at the new size
function resized() {
  if (!game) return build();
  let dpr;
  [W, H, K, dpr] = size();
  canvas.width = W; canvas.height = H;
  canvas.style.width = (W * K / dpr) + "px";
  canvas.style.height = (H * K / dpr) + "px";
  pad.width = innerWidth; pad.height = innerHeight;
  wasm.resize(game, W, H, cellSize(dpr));
  img = null;
}
addEventListener("resize", () => { clearTimeout(window._rs); window._rs = setTimeout(resized, 250); });
document.addEventListener("visibilitychange", () => { if (document.hidden) save(); });
addEventListener("pagehide", () => { save(); if (game) netBye(); });
setInterval(save, 30000);

fetch("xmas.wasm")
  .then((r) => r.arrayBuffer())
  .then((b) => WebAssembly.instantiate(b, {}))
  .then(({ instance }) => {
    wasm = instance.exports;
    // randomness for friend codes and sealing
    try { const r = crypto.getRandomValues(new Uint32Array(4)); wasm.entropy(r[0], r[1], r[2], r[3]); } catch (e) {}
    document.getElementById("loading").remove();
    build();
    requestAnimationFrame(frame);
  })
  .catch((e) => { document.getElementById("loading").textContent = "couldn't wake the village: " + e; });
