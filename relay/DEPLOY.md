# Deploying xmas-relay (for the Homelab server)

The xmas-tui village's own relay, on the server: amd64, Debian 13, Docker with Compose v2,
and the existing Cloudflare tunnel (routes managed from the dashboard and Nova). The web
game is already at xmas.novalabs.app (its DNS is done).

## Image

`ghcr.io/dash1101/xmas-relay:1.0.0`, also tagged `:latest`.
- Built for linux/amd64 by the repo's `relay` workflow from `relay/`, whenever it changes.
- Runs as a non-root user (uid 10001).
- Has a healthcheck.

## Steps

```bash
sudo mkdir -p /opt/xmas-relay/data
sudo chown 10001:10001 /opt/xmas-relay/data      # the container's user writes its limits and bans there
cd /opt/xmas-relay
sudo curl -fsSLo compose.yaml https://raw.githubusercontent.com/dash1101/xmas-tui/main/relay/compose.yaml
echo "ADMIN_EMAIL=<the owner's Cloudflare Access login>" | sudo tee .env >/dev/null   # value not shared
sudo chmod 600 .env
sudo docker compose up -d
sudo docker compose logs --tail 5    # "WebSocket on :8080/mqtt, admin console on :8081"
```

`compose.yaml`:
- `restart: unless-stopped`;
- publishes `127.0.0.1:8080:8080` and `127.0.0.1:8081:8081` only;
- bind-mounts `./data` (so it's in Nova's backups);
- read-only root, all capabilities dropped, no new privileges, 512 MB and 2 CPUs at most;
- a healthcheck on `/health`.

## Tunnel routes

| hostname | service | access |
|---|---|---|
| `xmas-relay.novalabs.app` | `http://localhost:8080` | public, no login (WebSocket at `/mqtt`) |
| `xmas-admin.novalabs.app` | `http://localhost:8081` | the same Access application as Nova |

## Environment

- `ADMIN_EMAIL` (in `.env`): the owner's Access login. The console then answers only requests
  Cloudflare Access signed in as exactly that address (it reads the
  `Cf-Access-Authenticated-User-Email` header Access adds). Unset, any Access sign-in gets in.
  There's no other admin secret.
- `TRUST_CF=1` (set in the compose file): client addresses come from `CF-Connecting-IP`, for
  the per-address limits and bans.
- Optional: `RELAY_PORT`, `ADMIN_PORT`, `RELAY_DATA` (defaults 8080, 8081, `/data`).

## Updating

```bash
cd /opt/xmas-relay
sudo docker compose pull
sudo docker compose up -d
```

## Check

- `curl -s https://xmas-relay.novalabs.app/health` gives `ok`.
- https://xmas-admin.novalabs.app shows "❄ Village relay" after the Access sign-in, and is
  refused without it.
- `curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8081/` on the server gives
  `403`: no Access header means no console, even locally.

Then tell the xmas-tui session (or Parker) it's up. The game gets tested against it end to
end.
