# xmas-relay

The village's own relay for online play. The desktop app, the phone app and the web page
(xmas.novalabs.app) all reach it at `wss://xmas-relay.novalabs.app/mqtt`. It runs on the
web's own port, 443, which school and work networks let through. The free public relays stay
as a fallback.

It speaks just enough MQTT 3.1.1 over a WebSocket for the village:
- connect (with a last will), subscribe and unsubscribe (`+` and `#` wildcards), publish at
  QoS 0 (retained or not), ping and disconnect;
- only topics under `xmas-tui/`.

Everything the village sends is sealed end to end, so the relay passes bytes along without
being able to read them.

## Long polling, where WebSockets don't get through

Some networks' proxies and filters break WebSockets but let ordinary web requests through. For
those, the same MQTT goes over plain HTTPS requests at `https://xmas-relay.novalabs.app/poll`
(since 1.1.0), and the apps and the web page fall back to it when the WebSocket fails:
- `POST /poll` opens a session and answers its id (32 hex digits, the session's only key);
- `POST /poll/<id>/up` carries MQTT's bytes to the relay, in order, one request after another;
- `POST /poll/<id>/down` waits up to 20 seconds and answers with whatever's come for the
  client (answered the moment there's something), and with 410 once the session's over;
- `POST /poll/<id>/close` ends it.

Each session counts against the same connection limits, and one nobody's asked after for 45
seconds is closed. Any page may use it (`Access-Control-Allow-Origin: *`); the web page sends
`text/plain`, so there's no CORS preflight before each request.

## Limits

Everything has a limit, kept within bounds and adjustable from the console:
- connections in all, and from one address;
- the biggest message;
- messages a second per connection (a token bucket with a burst);
- subscriptions per connection;
- kept (retained) messages: how many, how big each, how much in all, with the oldest let go
  first, and anything over a week old forgotten;
- the queue a slow client may build up before it's dropped (it isn't kept in memory).

The WebSocket upgrade refuses frames over the size limit, and a client must say hello within
10 seconds. One that falls silent past one and a half times its keep-alive is let go.

## Running it

```bash
cd relay
docker compose up -d --build
```

It listens on two ports, and neither should be published on the host. A Cloudflare Tunnel
reaches them:

| hostname | service | who |
|---|---|---|
| `xmas-relay.novalabs.app` | `http://xmas-relay:8080` | everyone (WebSocket at `/mqtt`, long polling at `/poll`, `/health`) |
| `xmas-admin.novalabs.app` | `http://xmas-relay:8081` | a Cloudflare Access app allowing only the admin |

Environment:
- `ADMIN_EMAIL`: the admin's Cloudflare Access address. The console answers only requests
  Access signed in as it. Unset, it wants any Access sign-in.
- `TRUST_CF` (default 1): take each client's address from `CF-Connecting-IP`. Behind the
  tunnel, every connection comes from the tunnel itself.
- `RELAY_PORT` (8080), `ADMIN_PORT` (8081), `RELAY_DATA` (`/data`: the limits and bans).
- `ADMIN_OPEN=1` opens the console without Access. Use it only for testing on one's own
  machine.

## The console

`xmas-admin.<domain>`:
- what's happening now (connections, how many by long polling, addresses, messages a second,
  in and out, kept messages by kind);
- who's connected (kick, ban);
- every limit (edit and save; they take hold at once, and "closed" turns new connections
  away);
- bans;
- forgetting kept messages under a prefix.

## Testing on one's own machine

```bash
cargo run --release
```

Then point the game at it: `XMAS_RELAY=ws://127.0.0.1:8080/mqtt xmas-tui`. The game's
`cargo test --release through_the_real_relay -- --ignored` with `XMAS_RELAY` set runs two
villagers meeting through it.
