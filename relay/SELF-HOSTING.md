# Running a relay of one's own

The village's online play goes through a relay. xmas-tui's own is at xmas-relay.novalabs.app,
but any community (a club, a school, a group of friends) can run one with the same two
containers, and its players point their game at it.

## What it takes

- A machine with Docker and Compose v2 (amd64).
- A way in from outside: a domain with TLS in front of the relay's port 8080. The web page needs
  `wss://` (TLS); the desktop and phone apps can also use plain `ws://` on a local network.
- The images: `ghcr.io/dash1101/xmas-relay` and `ghcr.io/dash1101/xmas-host` (the host is
  optional: without it, each public square is hosted by its first player).

## Steps

1. Copy [`compose.yaml`](compose.yaml), and put a `.env` beside it:
   ```
   HOST_TOKEN=<openssl rand -hex 24>
   ```
2. Without Cloudflare in front, set `TRUST_CF: "0"` for the relay, so it doesn't take addresses
   from a header anyone could send. Behind a proxy of one's own (nginx, Caddy, Traefik), every
   player would then seem to come from the proxy, so the limit on connections from one address
   would never apply, and blocking an address would block everyone. To avoid that, set
   `IP_HEADER` to the header the proxy puts the player's address in: `X-Real-IP`, or
   `X-Forwarded-For` (whose last address is taken). It's believed only from a connection on the
   same machine or network, which is where the proxy is.
3. The admin console (port 8081) answers only requests signed in through Cloudflare Access.
   Behind anything else, it needs a sign-in of its own in front of it; or it can be reached over
   an SSH tunnel, with the relay run with `ADMIN_OPEN=1` and the port left on 127.0.0.1. The
   console is never to be exposed openly.
4. `docker compose up -d`, then `docker compose logs --tail 10`.

## Updating

`docker compose pull && docker compose up -d` takes the newest images (or the versions set with
`RELAY_TAG` and `HOST_TAG` in `.env`). The console can do it too, with its **update** button,
once the updater beside the relay is turned on: [DEPLOY.md](DEPLOY.md#updating) has the steps,
and what holding the Docker socket means.

## Players

In the game: **Menu → Online → Relay**, the address (for example `wss://relay.example.org/mqtt`).
The game then goes through it first, and believes what it says as itself: its public squares'
settings, its public list, its announcements. **Use the village's own** goes back.

## Good to know

- Everything players send is sealed end to end. A relay can't read friends' messages or
  private villages; it can read only the public squares and listed villages (whose codes are
  public), and what its own host hears in the villages it keeps.
- Whoever runs a relay for other people is responsible for it: how long things are kept, who
  can see the console, and what their own privacy notice says. xmas-tui's
  [PRIVACY.md](../PRIVACY.md) describes what the relay and host keep, as a starting point.
