// The village in a web page: xmas.wasm (the desktop's own village, built for
// the web) draws each frame into its memory, and this page puts it on the
// canvas, and hands it the keyboard, the mouse and fingers. On a phone, the
// joystick and buttons are drawn in pixel art, as in the Android app.
"use strict";

const BTN = { E: 0, JUMP: 1, SNOWBALL: 2, POCKETS: 3, INVITE: 4, HELD: 5, BACK: 6, CENTRE: 7, JUMP_UP: 8, REST: 9, NUM: 10 };
const KEY = "xmas-tui-save", OPTS = "xmas-tui-opts";
const canvas = document.getElementById("village");
const ctx = canvas.getContext("2d", { alpha: false });
const pad = document.getElementById("pad");
const pctx = pad.getContext("2d");
const menuBox = document.getElementById("menu");
const touchy = matchMedia("(pointer: coarse)").matches;
if (touchy) document.body.classList.add("touch");

let wasm, mem, game = 0, W = 0, H = 0, last = 0, img = null;

function opts() {
  try { return Object.assign({ name: "", pet: "cat", petname: "", auto: false, size: 1 }, JSON.parse(localStorage.getItem(OPTS) || "{}")); }
  catch (e) { return { name: "", pet: "cat", petname: "", auto: false, size: 1 }; }
}
function q(s) { return '"' + String(s).replace(/\\/g, "\\\\").replace(/"/g, '\\"') + '"'; }
function config(o) {
  return `[scene]\nname = ${q(o.name)}\nautopilot = ${!!o.auto}\n[scene.pet]\nkind = ${q(o.pet)}\nname = ${q(o.petname)}\n[sound]\nenabled = false\n`;
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

function size() {
  // the canvas in device pixels (capped: big screens get bigger pixels)
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  let w = Math.round(innerWidth * dpr), h = Math.round(innerHeight * dpr);
  const k = Math.max(1, Math.max(w / 1920, h / 1200));
  // (the village is never smaller than 64 pixels a side)
  return [Math.max(64, Math.round(w / k) || 0), Math.max(64, Math.round(h / k) || 0)];
}

function build() {
  if (game) { save(); wasm.destroy(game); game = 0; }
  [W, H] = size();
  canvas.width = W; canvas.height = H;
  pad.width = innerWidth; pad.height = innerHeight;
  const o = opts();
  const n = putText(config(o) + "\u0000" + (localStorage.getItem(KEY) || ""));
  game = wasm.create(W, H, o.size || 1, (Math.random() * 4294967295) >>> 0, localMs(), n);
  img = null;
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
  }
  if (touchy) drawPad();
  requestAnimationFrame(frame);
}
let imgPtr = 0;

// ------------------------------------------------------------ menus

let menuItems = [], menuHot = 0;
function showList(n) {
  const text = getText(n);
  if (!text) { menuBox.style.display = "none"; return; }
  const [title, ...items] = text.split("\n");
  menuItems = items; menuHot = 0;
  menuBox.innerHTML = "";
  const h = document.createElement("h2"); h.textContent = title; menuBox.appendChild(h);
  items.forEach((it, i) => {
    const d = document.createElement("div");
    d.innerHTML = `<b>${i < 9 ? i + 1 : " "}</b>`;
    d.appendChild(document.createTextNode(it));
    d.onclick = (e) => { e.stopPropagation(); choose(i); };
    menuBox.appendChild(d);
  });
  menuBox.style.display = "block";
  hot(0);
}
function hot(i) {
  menuHot = (i + menuItems.length) % Math.max(1, menuItems.length);
  [...menuBox.querySelectorAll("div")].forEach((d, k) => d.classList.toggle("hot", k === menuHot));
}
function choose(i) {
  menuBox.style.display = "none";
  showList(wasm.pick(game, i));
}
function menuOpen() { return menuBox.style.display === "block"; }

// ------------------------------------------------------------ keyboard

const held = new Set();
function steer() {
  const k = (a) => a.some((c) => held.has(c));
  const mx = (k(["KeyD", "ArrowRight"]) ? 1 : 0) - (k(["KeyA", "ArrowLeft"]) ? 1 : 0);
  const my = (k(["KeyS", "ArrowDown"]) ? 1 : 0) - (k(["KeyW", "ArrowUp"]) ? 1 : 0);
  wasm.steer(game, mx, my, held.has("ShiftLeft") || held.has("ShiftRight") ? 1 : 0);
}
addEventListener("keydown", (e) => {
  if (!game || e.target.tagName === "INPUT") return;
  if (menuOpen()) {
    if (e.code === "ArrowDown" || e.code === "KeyS") hot(menuHot + 1);
    else if (e.code === "ArrowUp" || e.code === "KeyW") hot(menuHot - 1);
    else if (e.code === "Enter" || e.code === "Space" || e.code === "KeyE") choose(menuHot);
    else if (/^Digit[1-9]$/.test(e.code)) { const i = +e.code.slice(5) - 1; if (i < menuItems.length) choose(i); }
    else if (e.code === "Escape" || e.code === "KeyT") menuBox.style.display = "none";
    e.preventDefault();
    return;
  }
  if (e.repeat && !/^(Key[WASD]|Arrow)/.test(e.code)) { e.preventDefault(); return; }
  held.add(e.code);
  switch (e.code) {
    case "Space": wasm.button(game, BTN.JUMP); break;
    case "KeyE": case "Enter": wasm.button(game, BTN.E); break;
    case "KeyR": wasm.button(game, BTN.SNOWBALL); break;
    case "KeyF": wasm.button(game, BTN.INVITE); break;
    case "KeyI": wasm.button(game, BTN.POCKETS); break;
    case "KeyQ": wasm.button(game, BTN.HELD); break;
    case "KeyC": wasm.button(game, BTN.CENTRE); break;
    case "KeyZ": wasm.button(game, BTN.REST); break;
    case "KeyT": showList(wasm.todo(game)); break;
    case "Escape": wasm.button(game, BTN.BACK); break;
    default:
      if (/^Digit[1-5]$/.test(e.code)) wasm.button(game, BTN.NUM + +e.code.slice(5));
      else if (e.key === "?") toggle("help");
  }
  steer();
  if (/^(Key[WASDEFRZTQIC]|Arrow|Space|Enter|Digit)/.test(e.code)) e.preventDefault();
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
  if (menuOpen()) { menuBox.style.display = "none"; return; }
  canvas.setPointerCapture(e.pointerId);
  const [x, y] = at(e);
  down = { x, y, id: e.pointerId };
  wasm.touch(game, 0, x, y);
  clearTimeout(longT);
  longT = setTimeout(() => { if (down) { wasm.touch(game, 3, down.x, down.y); showList(wasm.menu(game, down.x, down.y)); down = null; } }, 550);
});
canvas.addEventListener("pointermove", (e) => {
  if (!game || !down || e.pointerId !== down.id) return;
  const [x, y] = at(e);
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
canvas.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  if (!game) return;
  const [x, y] = at(e);
  showList(wasm.menu(game, x, y));
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
function px() { return Math.max(2, Math.round(3 * (devicePixelRatio || 1) / (devicePixelRatio || 1) * 1.5)); }
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
  const L = layout(), x = e.clientX * pad.width / innerWidth, y = e.clientY * pad.height / innerHeight;
  if (Math.hypot(x - L.stick.x, y - L.stick.y) < L.stick.r * 1.5 && !stick) {
    stick = e.pointerId; knob = { x, y }; padSteer(x, y); pad.setPointerCapture(e.pointerId); return;
  }
  const i = L.btn.findIndex((b) => Math.hypot(x - b.x, y - b.y) < b.r * 1.25);
  if (i >= 0) {
    pressed.set(e.pointerId, i);
    const b = L.btn[i].b;
    if (b === "MORE") toggle("opts");
    else if (b === "TODO") showList(wasm.todo(game));
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

// ------------------------------------------------------------ the bar, settings, help

function toggle(id) {
  const el = document.getElementById(id);
  el.style.display = el.style.display === "block" ? "none" : "block";
  if (id === "opts" && el.style.display === "block") {
    const o = opts();
    document.getElementById("o-name").value = o.name;
    document.getElementById("o-pet").value = o.pet;
    document.getElementById("o-petname").value = o.petname;
    document.getElementById("o-auto").checked = !!o.auto;
    document.getElementById("o-size").value = String(o.size || 1);
  }
}
document.getElementById("b-todo").onclick = () => game && showList(wasm.todo(game));
document.getElementById("b-centre").onclick = () => game && wasm.button(game, BTN.CENTRE);
document.getElementById("b-help").onclick = () => toggle("help");
document.getElementById("b-opts").onclick = () => toggle("opts");
document.getElementById("help").onclick = () => toggle("help");
document.getElementById("o-apply").onclick = () => {
  localStorage.setItem(OPTS, JSON.stringify({
    name: document.getElementById("o-name").value, pet: document.getElementById("o-pet").value,
    petname: document.getElementById("o-petname").value, auto: document.getElementById("o-auto").checked,
    size: +document.getElementById("o-size").value,
  }));
  build();
  toggle("opts");
};
document.getElementById("o-export").onclick = () => {
  save();
  const blob = new Blob([localStorage.getItem(KEY) || ""], { type: "text/plain" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = `xmas-tui-save-${new Date().toISOString().slice(0, 10)}.toml`;
  a.click();
  URL.revokeObjectURL(a.href);
};
document.getElementById("o-import").onclick = () => document.getElementById("o-file").click();
document.getElementById("o-file").onchange = async (e) => {
  const f = e.target.files[0];
  if (!f) return;
  const text = await f.text();
  if (!/^\s*(#.*\n\s*)*[a-z_]+\s*=/m.test(text) || !/seed\s*=/.test(text)) { alert("that isn't an xmas-tui save"); return; }
  if (game) { wasm.destroy(game); game = 0; }
  localStorage.setItem(KEY, text);
  build();
  toggle("opts");
};
document.getElementById("o-new").onclick = () => {
  if (!confirm("A new village? (this one's pockets and scorecard go with it)")) return;
  if (game) { wasm.destroy(game); game = 0; }
  localStorage.removeItem(KEY);
  build();
  toggle("opts");
};

// ------------------------------------------------------------ start

addEventListener("resize", () => { clearTimeout(window._rs); window._rs = setTimeout(build, 300); });
document.addEventListener("visibilitychange", () => { if (document.hidden) save(); });
addEventListener("pagehide", save);
setInterval(save, 30000);

fetch("xmas.wasm")
  .then((r) => r.arrayBuffer())
  .then((b) => WebAssembly.instantiate(b, {}))
  .then(({ instance }) => {
    wasm = instance.exports;
    document.getElementById("loading").remove();
    build();
    requestAnimationFrame(frame);
  })
  .catch((e) => { document.getElementById("loading").textContent = "couldn't start the village: " + e; });
