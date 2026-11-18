// The village as an app on a phone (or a computer): what it's made of is
// kept here, so it opens with no connection at all; with one, it's always
// the newest (each file checked with the site as it's asked for, and kept).
// Only the site's own files: the relay and the weather go straight through.
const CACHE = "village-1";
const SHELL = ["./", "index.html", "xmas.js", "xmas.wasm", "manifest.webmanifest", "relays.json", "icon-64.png", "icon-180.png", "icon-192.png", "icon-512.png", "icon-maskable-512.png"];

self.addEventListener("install", (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(SHELL)).then(() => self.skipWaiting()));
});

self.addEventListener("activate", (e) => {
  e.waitUntil(caches.keys().then((ks) => Promise.all(ks.filter((k) => k !== CACHE).map((k) => caches.delete(k)))).then(() => self.clients.claim()));
});

self.addEventListener("fetch", (e) => {
  const req = e.request;
  if (req.method !== "GET" || new URL(req.url).origin !== self.location.origin) return;
  e.respondWith((async () => {
    try {
      // (asked of the site every time: unchanged, it says so and nothing
      // more comes down)
      const res = await fetch(req, { cache: "no-cache" });
      if (res.ok && res.type === "basic") {
        const c = await caches.open(CACHE);
        c.put(req, res.clone());
      }
      return res;
    } catch (err) {
      const hit = await caches.match(req, { ignoreSearch: true });
      if (hit) return hit;
      throw err;
    }
  })());
});
