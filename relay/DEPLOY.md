# Deploying xmas-relay (for the Homelab server)

This is what's needed on the server for the xmas-tui village's own relay, and for the web
game's new address. Parker's request: build the relay, host it through the server's
Cloudflare tunnel for free, add it to the Nova app as a service, and give him an admin
console that only his Cloudflare account can open. The web game moves from
dash1101.github.io/xmas-tui to xmas.novalabs.app.

## 1. The web game's address (DNS only)

In the Cloudflare zone `novalabs.app`:
- CNAME record: name `xmas`, target `dash1101.github.io`, **proxy off** (grey cloud, "DNS
  only"). GitHub has to see the record directly to issue the HTTPS certificate.
- No other A, AAAA or CNAME records for `xmas`.

The xmas-tui side then sets the custom domain in the repo's Pages settings.

## 2. The relay container

```bash
git clone https://github.com/dash1101/xmas-tui    # or git pull
cd xmas-tui/relay
echo "ADMIN_EMAIL=<Parker's Cloudflare Access login>" > .env   # not committed
docker compose up -d --build
```

- `compose.yaml` expects an external Docker network named `tunnel`, the one cloudflared is
  on. Change it to the actual cloudflared network, or run cloudflared beside it.
- Do **not** publish ports 8080/8081 on the host.
- The log should say: `WebSocket on :8080/mqtt, admin console on :8081`.

## 3. Tunnel hostnames

| hostname | service | access |
|---|---|---|
| `xmas-relay.novalabs.app` | `http://xmas-relay:8080` | public (WebSockets on) |
| `xmas-admin.novalabs.app` | `http://xmas-relay:8081` | a Cloudflare Access self-hosted app allowing only Parker's account |

The game already tries `wss://xmas-relay.novalabs.app/mqtt` first, so please keep that
exact hostname.

## 4. Nova

Add it as a service:
- health: `http://xmas-relay:8080/health`, or `https://xmas-relay.novalabs.app/health`
  (answers `ok`);
- admin UI: `https://xmas-admin.novalabs.app`.

## 5. Check

- `curl https://xmas-relay.novalabs.app/health` gives `ok`.
- `https://xmas-admin.novalabs.app` shows "❄ Village relay" after the Access sign-in, and a
  `403` without it.

Then tell the xmas-tui session (or Parker) it's up, and the game gets tested against it end
to end.
