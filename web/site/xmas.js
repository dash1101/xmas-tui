// The village in a web page: xmas.wasm (the desktop's own village, built for
// the web) draws each frame into its memory, and this page puts it on the
// canvas, plays its sound, and hands it the keyboard, the mouse and fingers.
// Menus and the controls card are drawn in the village itself, as on the
// desktop; on a phone the joystick and buttons are drawn in pixel art, as in
// the Android app.
"use strict";

const BTN = { E: 0, JUMP: 1, SNOWBALL: 2, POCKETS: 3, INVITE: 4, HELD: 5, BACK: 6, CENTRE: 7, JUMP_UP: 8, REST: 9, NUM: 10 };
const KEY = "xmas-tui-save", OPTS = "xmas-tui-opts", PLACE = "xmas-tui-opts-at";
const canvas = document.getElementById("village");
const ctx = canvas.getContext("2d", { alpha: false });
const pad = document.getElementById("pad");
const pctx = pad.getContext("2d");
const touchy = matchMedia("(pointer: coarse)").matches;
if (touchy) document.body.classList.add("touch");

let wasm, game = 0, W = 0, H = 0, K = 1, last = 0, img = null, imgPtr = 0;

// ------------------------------------------------------------ settings

const DEFAULTS = {
  name: "", pet: "cat", petcoat: "", petname: "", auto: false, walk: 100,
  friends: [], only_friends: false,
  time: "auto", snow: 100, wind: 100, aurora: true, glow: 100, size: 100, zoom: 100,
  critters: 100, most: 24, bushes: true, events: 100, sleigh: 4, chattiness: 100, talk_time: 100,
  sound: true, master: 50, music: 60, sfx: 70, ambience: 50, track: "shuffle",
  look: "classic",
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
  s += `chattiness = ${o.chattiness / 100}\ntalk_time = ${o.talk_time / 100}\nonly_friends = ${!!o.only_friends}\n`;
  s += `[scene.pet]\nkind = ${q(o.pet)}\ncoat = ${q(o.petcoat)}\nname = ${q(o.petname)}\n`;
  for (const f of o.friends) {
    if (!String(f.name || "").trim()) continue;
    s += `[[scene.friends]]\nname = ${q(f.name.trim())}\nkid = ${!!f.kid}\ncoat = ${q(f.coat)}\nhat = ${q(f.hat)}\nhair = ${q(f.hair)}\nskin = ${q(f.skin)}\nscarf = ${f.scarf !== false}\n`;
  }
  s += `[ui]\nlook = ${q(o.look)}\n[sound]\nenabled = ${!!o.sound}\nmaster = ${o.master / 100}\nmusic = ${o.music / 100}\nsfx = ${o.sfx / 100}\nambience = ${o.ambience / 100}\ntrack = ${q(o.track)}\n`;
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

function build() {
  if (game) { save(); netBye(); wasm.destroy(game); game = 0; }
  let dpr;
  [W, H, K, dpr] = size();
  canvas.width = W; canvas.height = H;
  canvas.style.width = (W * K / dpr) + "px";
  canvas.style.height = (H * K / dpr) + "px";
  pad.width = innerWidth; pad.height = innerHeight;
  const o = opts();
  const n = putText(config(o) + "\u0000" + (localStorage.getItem(KEY) || ""));
  game = wasm.create(W, H, o.size / 100, (Math.random() * 4294967295) >>> 0, localMs(), n);
  img = null;
  sound();
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

// what the village asks of the page: the settings, a line typed (a
// friend's code, a message), a code onto the clipboard
function asked(a) {
  if (!a) return;
  if (a === "settings") openOpts(true);
  else if (a.startsWith("text:")) {
    held.clear(); steer();
    setTimeout(() => { const v = prompt(a.slice(5)); if (game) wasm.give_text(game, putText(v || "")); }, 0);
  } else if (a.startsWith("copy:")) {
    const t = a.slice(5);
    try { navigator.clipboard.writeText(t).then(() => toast("copied: " + t), () => toast(t)); } catch (e) { toast(t); }
  }
}

// ------------------------------------------------------------ online
// The relay (a free public MQTT broker) through a WebSocket; the village
// seals and unseals what goes through it, the page only carries bytes.

const RELAYS = ["wss://broker.emqx.io:8084/mqtt", "wss://broker.hivemq.com:8884/mqtt", "wss://test.mosquitto.org:8081/mqtt"];
let ws = null, wsTry = 0, wsWait = 0, relays = null;
fetch("relays.json").then((r) => r.json()).then((j) => { if (Array.isArray(j.ws) && j.ws.length) relays = j.ws; }).catch(() => {});
function netPump(now) {
  if (!wasm.net_want(game)) {
    if (ws) { flushNet(); ws.onclose = null; ws.close(); ws = null; }
    return;
  }
  if (!ws && now > wsWait) {
    const list = relays || RELAYS;
    const url = list[wsTry % list.length];
    wsTry++;
    try { ws = new WebSocket(url, "mqtt"); } catch (e) { ws = null; wsWait = now + 3000; return; }
    ws.binaryType = "arraybuffer";
    ws.onopen = () => { wsTry--; if (game) wasm.net_link(game, 1); };
    ws.onmessage = (e) => {
      if (!game) return;
      const b = new Uint8Array(e.data);
      const p = wasm.text_buf(0, b.length);
      new Uint8Array(wasm.memory.buffer, p, b.length).set(b);
      wasm.net_in(game, b.length);
    };
    ws.onclose = () => { if (game) wasm.net_link(game, 0); ws = null; wsWait = performance.now() + 4000; };
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
  const o = opts();
  wasm.sound(game, o.sound ? 1 : 0, o.master / 100, o.music / 100, o.sfx / 100, o.ambience / 100);
  document.getElementById("b-sound").style.opacity = o.sound ? 1 : 0.4;
}
// (a browser lets sound start only after a key or a tap)
function wake() {
  if (ac) { if (ac.state === "suspended") ac.resume(); return; }
  if (!opts().sound) return;
  try { ac = new (window.AudioContext || window.webkitAudioContext)({ sampleRate: 48000 }); } catch (e) { ac = null; }
}
function feedAudio() {
  if (!ac || ac.state !== "running" || !opts().sound) return;
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
    case "KeyT": if (menu) wasm.back(game); else wasm.todo(game); break;
    case "KeyO": if (menu) wasm.back(game); else wasm.online(game); break;
    case "Escape": if (box.classList.contains("open")) openOpts(false); else wasm.back(game); break;
    default:
      if (/^Digit[1-9]$/.test(e.code)) wasm.button(game, BTN.NUM + +e.code.slice(5));
      else if (e.key === "?") wasm.help(game, 1);
  }
  steer();
  if (/^(Key[WASDEFRZTQICO]|Arrow|Space|Enter|Digit|Escape)/.test(e.code)) e.preventDefault();
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
  const w = pad.width, h = pad.height, s = Math.min(w, h) / 360;
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
// a change: kept, and the village made anew with it (a moment later, so a
// slider being dragged doesn't rebuild it every step); the sound's live
function change(k, v, live) {
  const o = opts();
  o[k] = v;
  setOpts(o);
  if (live) { if (k === "sound" && v) wake(); sound(); return; }
  clearTimeout(rebuildT);
  rebuildT = setTimeout(build, 700);
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
  const put = (key, v) => { const p = opts(); p.friends[k][key] = v; setOpts(p); clearTimeout(rebuildT); rebuildT = setTimeout(build, 700); };
  const name = el("input", { type: "text", value: f.name || "", maxLength: 16 });
  name.oninput = () => put("name", name.value);
  const kid = el("input", { type: "checkbox", checked: !!f.kid });
  kid.onchange = () => put("kid", kid.checked);
  const del = button("✕", () => { const p = opts(); p.friends.splice(k, 1); setOpts(p); show(1); build(); }, true);
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
    pick("Pet", "pet", ["cat", "dog", "bunny", "none"], () => show(0));
    if (o.pet !== "none") {
      pick("Pet's coat", "petcoat", COATS[o.pet] || [""]);
      text("Pet's name", "petname", "(it asks)");
    }
    slide("Walking speed", "walk", 50, 300, "%");
    check("Autopilot (left alone: off doing their own thing)", "auto");
  } else if (i === 1) {
    h3("FRIENDS & FAMILY");
    note("Some of the folk who come by take these names, know the villager, and wear what's chosen for them (\"any\": picked from the name).");
    check("Only them (no strangers; the animals still come)", "only_friends");
    o.friends.forEach((f, k) => pane.append(friendBox(f, k)));
    pane.append(button("+ add someone", () => { const p = opts(); p.friends.push({ name: "Friend " + (p.friends.length + 1), kid: false, scarf: true }); setOpts(p); show(1); build(); }));
  } else if (i === 2) {
    h3("THE VILLAGE");
    pick("Time of day", "time", ["auto", "night", "dawn", "morning", "noon", "afternoon", "dusk"]);
    slide("Snowfall", "snow", 0, 300, "%");
    slide("Wind", "wind", 0, 300, "%");
    check("Northern lights", "aurora");
    slide("Size", "size", 50, 200, "%");
    slide("Zoom", "zoom", 100, 300, "%");
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
    o.name = you.name || "";
    if (you.pet) { o.pet = you.pet.kind || "cat"; o.petcoat = you.pet.coat || ""; o.petname = you.pet.name || ""; }
    o.friends = (you.friends || []).map((fr) => ({ name: fr.name, kid: !!fr.kid, coat: fr.coat || "", hat: fr.hat || "", hair: fr.hair || "", skin: fr.skin || "", scarf: fr.scarf !== false }));
    o.only_friends = !!you.only_friends;
    if (you.look) o.look = you.look;
    setOpts(o);
  }
  if (game) { wasm.destroy(game); game = 0; }
  localStorage.setItem(KEY, text);
  build();
  show(tab);
  toast(o.name ? `imported ${o.name}'s village` : "imported the village");
};
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
document.getElementById("b-help").onclick = () => game && wasm.help(game, touchy ? 0 : 1);
document.getElementById("b-opts").onclick = () => openOpts();
document.getElementById("b-sound").onclick = () => {
  const o = opts();
  o.sound = !o.sound;
  setOpts(o);
  wake();
  sound();
  toast(o.sound ? "♪ sound on" : "sound off");
  if (box.classList.contains("open") && tab === 3) show(3);
};

// ------------------------------------------------------------ start

addEventListener("resize", () => { clearTimeout(window._rs); window._rs = setTimeout(build, 300); });
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
