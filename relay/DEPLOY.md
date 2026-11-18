# Deploying xmas-relay and its host (for the Homelab server)

The xmas-tui village's own relay, on the server: amd64, Debian 13, Docker with Compose v2,
and the existing Cloudflare tunnel (routes managed from the dashboard and Nova). The web
game is already at xmas.novalabs.app (its DNS is done).

## Image

`ghcr.io/dash1101/xmas-relay:1.3.0`, also tagged `:latest`; and from 1.3.0, its own host,
`ghcr.io/dash1101/xmas-host:1.25.0` (also `:latest`).
- 1.1.0 added long polling at `/poll`, for networks that break WebSockets.
- 1.2.0 keeps what it holds (linked devices' sides, listings) in `data/kept.bin` across restarts, and adds the public
  squares' settings to the console. 1.2.1: a client's last will can't speak on the relay's own topics.
- 1.2.2 pings each WebSocket every 30 seconds, so Cloudflare (which closes a proxied connection after 100 seconds
  without a frame) never sees one go quiet. Long polls already answer within 20 seconds.
- 1.3.0 lets the relay's own host sign in (`HOST_TOKEN`), and adds to the console: the villages the host keeps (who's
  in them, what's said, mute, kick, ban), server villages (private or listed), the public list's order and featured
  places, extra words, announcements (now and scheduled), welcomes, and bans of villages and players. **It needs one
  new setting on the server: `HOST_TOKEN` (see Steps).**
- Before 1.3.0, none of these changed anything on the server's side.
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

## The host (from relay 1.3.0)

The relay's own host is a second container, `xmas-host`, in the same compose file. It keeps the
public squares and the server villages open, the same for everyone, all the time, and does what the
console asks. It has no ports: it reaches the relay on the compose network and signs in with a
secret the two share.

```bash
cd /opt/xmas-relay
echo "HOST_TOKEN=$(openssl rand -hex 24)" | sudo tee -a .env >/dev/null   # value not shared
sudo chmod 600 .env
sudo curl -fsSLo compose.yaml https://raw.githubusercontent.com/dash1101/xmas-tui/main/relay/compose.yaml
sudo docker compose pull
sudo docker compose up -d
sudo docker compose logs --tail 8 xmas-host   # "keeping North Pole Square open" and the other three
```

- No `HOST_TOKEN`: the relay lets no host in, and the squares are hosted by their first player
  as before. A wrong one: the host is turned away (and says so in its log).
- The host uses at most 512 MB and one CPU (it's typically far less: the villages go on unseen,
  ten times a second).
- If the host stops, the squares carry on with their players hosting them, and the host takes
  them back when it returns.

## Tunnel routes

| hostname | service | access |
|---|---|---|
| `xmas-relay.novalabs.app` | `http://localhost:8080` | public, no login (WebSocket at `/mqtt`, long polling at `/poll`) |
| `xmas-admin.novalabs.app` | `http://localhost:8081` | the same Access application as Nova |

## Environment

- `ADMIN_EMAIL` (in `.env`): the owner's Access login. The console then answers only requests
  Cloudflare Access signed in as exactly that address (it reads the
  `Cf-Access-Authenticated-User-Email` header Access adds). Unset, any Access sign-in gets in.
  There's no other admin secret.
- `TRUST_CF=1` (set in the compose file): client addresses come from `CF-Connecting-IP`, for
  the per-address limits and bans.
- `HOST_TOKEN` (in `.env`, read by both containers): the secret the relay's own host signs in
  with (at least 16 letters; `openssl rand -hex 24` makes one). Keep it out of anything shared.
- Optional: `RELAY_PORT`, `ADMIN_PORT`, `RELAY_DATA` (defaults 8080, 8081, `/data`).
- The console's settings (server villages and their codes, bans, the public list, mutes,
  welcomes, announcements) are kept in `data/relay.json`, which is in Nova's backups. A private
  server village's code is in there: anyone who can read that file can get in.

## Updating

The images follow `latest` unless a version is set in `.env` (`RELAY_TAG=1.4.0`, `HOST_TAG=1.25.2`).
By hand:

```bash
cd /opt/xmas-relay
sudo docker compose pull
sudo docker compose up -d
```

Players are dropped for a few seconds and come straight back.

### From the console (the update button, from relay 1.4.0)

The console's **UPDATE** section does the same from the browser, through a small updater that
runs beside the relay. It's off until it's set up:

```bash
cd /opt/xmas-relay
sudo curl -fsSLo compose.yaml https://raw.githubusercontent.com/dash1101/xmas-tui/main/relay/compose.yaml
sudo mkdir -p updater
sudo curl -fsSLo updater/update.sh https://raw.githubusercontent.com/dash1101/xmas-tui/main/relay/updater/update.sh
# in .env, two more lines:
#   COMPOSE_PROFILES=updater
#   PROJECT_DIR=/opt/xmas-relay
sudo docker compose up -d
sudo docker compose logs --tail 5 xmas-updater   # "watching /data for requests…"
```

- The relay can't touch Docker itself. Pressing the button writes a file (`data/update.request`);
  the updater sees it within a few seconds, runs `docker compose pull` and `up -d` for
  `xmas-relay` and `xmas-host` only, and writes how it went (`data/update.status`), which the
  console shows.
- The updater holds the Docker socket, which is full control of Docker on the machine. So it
  has no ports and no network, a read-only file system, and never reads what's in the request
  file. Whoever can press the button (the console's sign-in) can restart the relay with the
  newest image, and nothing more. Left off, nothing on the machine holds the socket.
- A request more than ten minutes old is dropped, not acted on late.
- The updater doesn't update itself. `compose.yaml` and `update.sh` change by hand, as above.

## Check

- `curl -s https://xmas-relay.novalabs.app/health` gives `ok`.
- `curl -s -X POST https://xmas-relay.novalabs.app/poll` gives 32 hex digits (a long-polling
  session, closed by itself in under a minute).
- https://xmas-admin.novalabs.app shows "❄ Village relay" after the Access sign-in, and is
  refused without it.
- `curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8081/` on the server gives
  `403`: no Access header means no console, even locally.
- The console's HOST section says "● the host is connected", and IN THE VILLAGES THE HOST KEEPS
  lists the four squares.

Then tell the xmas-tui session (or Parker) it's up. The game gets tested against it end to
end.
