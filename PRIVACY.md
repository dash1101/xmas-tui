# Privacy

*Last updated: 17 November 2026. Applies to xmas-tui 1.25.0 and later: the desktop app (Linux and Windows), the Android app, the web page at [xmas.novalabs.app](https://xmas.novalabs.app), and the village's own relay at xmas-relay.novalabs.app.*

xmas-tui is a small, free, source-available hobby project: a Christmas village that lives on the desktop, on a phone, or in a browser. This page sets out what it keeps, what it sends, who it sends it to, and the choices there are about all of it. Everything here can be checked against the source code in this repository.

## In short

- **No accounts, no ads, no analytics, no tracking, no cookies.** Nothing is sold or shared for advertising. There is no crash reporting and no usage telemetry.
- **Almost everything stays on the device**: the settings, the save (the village, the villager, the pockets, friends and chats), and the log.
- **Going online is optional, and off until it's turned on.** Once on, it stays on until it's turned off again.
- **What goes through the relay is sealed** (encrypted). The relay passes it along without being able to read it, apart from the public squares and listed villages, which are public by design.
- **The public squares are looked after.** The relay's own host keeps them open, and the project can see what's said aloud there, to keep them friendly: someone can be quietened, sent home, or kept out.
- **A few outside services are used**, only for the features that need them: Cloudflare (in front of the relay), three public message brokers (only if the relay can't be reached), Open-Meteo (the weather widget), and GitHub (updates, downloads and hosting the web page).

## Who runs it

xmas-tui is made and run by its maintainer, **dash1101** on GitHub ("the project" below). For the relay, the project is the one deciding what's done with the data that passes through it (the "controller", in GDPR terms).

Contact: open an issue at [github.com/dash1101/xmas-tui/issues](https://github.com/dash1101/xmas-tui/issues). Issues are public, so leave personal details out of it. For anything private, ask in the issue for a private way to talk, and one will be arranged.

## What stays on the device

| Where | What | Where it's kept |
|---|---|---|
| Desktop (Linux) | Settings | `~/.config/xmas-tui/config.toml` |
| | The save: the village, the villager, pockets, friends, chats, linked devices | `~/.local/share/xmas-tui/save.toml` |
| | The log (what the wallpaper did, for troubleshooting) | `~/.local/state/xmas-tui/wallpaper.log` |
| Desktop (Windows) | Settings and the save | `%APPDATA%\xmas-tui\` |
| | The log | `%LOCALAPPDATA%\xmas-tui\wallpaper.log` |
| Android | Settings and the save | The app's own private storage |
| Web page | Settings, the save, other villagers made in that browser | The browser's local storage for xmas.novalabs.app |

None of this is sent anywhere by xmas-tui, except as described under "Going online" (and then sealed).

Two notes:

- **Android backups.** The app allows Android's own backup. If the device backs up apps to a Google account, the system may include the app's files (settings and save) in that backup. That happens between the device and Google, under Google's terms, and can be turned off in Android's settings.
- **Sharing a save.** "Export save" in the menu writes the save to a file, and a save can be imported. Whatever is done with an exported file is up to whoever exported it.

### What it looks at on the device, and never sends

Some widgets and effects read things on the device to show them. None of this leaves the device:

- **The system widget**: processor, memory, disk and network use.
- **The music widget**: what's playing in other apps (the title and artist, through the desktop's media controls).
- **The visualizer**: the sound the computer is playing, to draw bars from it.
- **Window snow and lights**: where open windows are on the screen (on KDE Plasma, through a small script that xmas-tui loads into KWin and unloads again).
- **Shelves**: the names and icons of the files in the folders chosen for them.
- **Resting for games**: which programs are running (to spot a game), and how busy the processors and graphics card are.
- **The clipboard**: only when copying or pasting a code.

## Going online

Online play is off until it's turned on in the menu (Friends & online). Once on, the app connects to the village's relay and uses it for friends, chats, shared villages and the public squares.

### What's sent, and who can read it

Everything goes through the relay sealed with ChaCha20-Poly1305. The key comes from a code: a friend code, a village code, or a link code between one person's own devices. Only someone with the code can read what's sealed with it, or even tell which conversation it belongs to. The relay itself never gets the codes.

| What | Sent when | Who can read it |
|---|---|---|
| The villager's name, look, profile card (the line on it, the pet, games won, best ski score and so on), and whether they're online | Online, now and then | Friends (those with the friend code) |
| Direct messages to a friend | When written | The friend. **Note:** a message is sealed with the receiver's friend code, which the receiver's other friends also have. A modified app in one of their hands could read it. Nothing secret belongs in a chat. |
| Where the villager is and what they're doing (walking, jumping, paused, their pets), what's said aloud, waves, games, snowballs, shooting stars, and the host's village settings, clock and music | In a shared village | Everyone in that village (those with its code) |
| "Just to …" messages inside a village | When written | Meant for one person, but sealed with the village's code: a modified app in the same village could read them |
| Public squares, and villages listed in Public villages: everything above that happens in them, the listing itself (for example "Robin's village · 3 of 8 here") | While in one, or while listed | **Anyone.** Their codes are public by design. |
| A villager's progress, name, look, pet, family, and the device's name (for example "dash-pc") | With linked devices | Only the linked devices (those with the link code) |

Names are whatever the player types in. A first name or a nickname is plenty, and a full name isn't needed anywhere.

Strong language from others is starred out by default (a setting turns this off), and links in what others write are always hidden until asked for.

### What the relay can see

The relay runs on a server the project looks after, behind Cloudflare. Like any server, it sees:

- **The IP address** of each connection. It's used to limit how many connections one address can make, and, if the relay is being abused, to block that address.
- **When** a connection comes and goes, how much it sends, and **which topics** it uses. Topics are long codes made from hashes, which don't say who is talking to whom.
- **A player tag** (from 1.25.0): a number worked out from the player's friend code by a one-way hash, sent as part of the connection's name. It's the same each time the same player connects, so that if a player is kept out, it holds when they come back. The friend code can't be worked out from it.

The relay doesn't log connections or messages. Its own log has only lines like "started" and "stopping". The project's admin console shows the connections open right now (address, how long connected, how much sent) and totals, and what the relay's own host reports (below). It's protected by a sign-in, and none of it is written down, apart from what's listed under "What the relay keeps".

### The relay's own host

Alongside the relay runs its own host: a copy of the game with no window, signed in to the relay with a secret only it has. It keeps the public squares open (so they're the same for everyone, whoever's there), and any "server villages" the project sets up: villages hosted by the server rather than by a player, either listed in Public villages or private (reached only with their code).

In the villages it keeps, the host is there like any other player, so it sees what everyone there sees. It doesn't keep what it sees, except for these, which it sends to the admin console every few seconds:

- who's in each of those villages right now (the id the game picked for them, and the name they gave);
- the last 60 lines said aloud in each (who said it, and when).

These are held in memory only, replaced as they come, and gone within two minutes once the host stops sending them. They're never written to disk. The host doesn't see friends' messages, and it doesn't keep "just to …" messages.

From the console, the project can quieten someone in those villages (what they say isn't passed on), send someone home, make announcements (now, or at set times), greet newcomers, and keep villages or players out of the relay.

### What the relay keeps, and for how long

| What | How long |
|---|---|
| The last "here I am" from each player (sealed) | Until replaced; forgotten after 7 days without an update |
| The last word from each linked device (sealed) | Forgotten after 63 days without an update |
| Public village listings and the public squares' status | Forgotten after 7 days without an update |
| The public squares' settings, set by the project | Until changed |
| Blocked IP addresses | Until unblocked |
| Server villages: their names, codes and settings | Until the project removes them |
| Villages kept out: their codes, the name they were listed under, why, and when | Until lifted |
| Players kept out: their id and player tag, the name they went by, why, and when | Until lifted |
| Players quietened in a village the host keeps: their id | Until lifted |
| The welcome said in each village the host keeps, announcements still to come, extra words starred out in public villages, and the public list's order | Until changed |

These are kept in the relay's memory and saved to a file on its server so they survive a restart. **Messages and chats aren't kept on the relay at all** (apart from the host's last lines in its own villages, above, which are only ever in memory). They're passed on to whoever is online and listening right then, and a friend who is offline doesn't get them later.

### Another relay

The game can be pointed at a different relay (Menu → Online → Relay), run by someone else: a club, a school, a group of friends. Everything sent then goes through that relay instead, and whoever runs it is responsible for it, under their own privacy notice. The project has no part in it and no access to it. The sealing works the same way there.

## Other services

These are other companies' services, each under its own privacy policy:

- **Cloudflare** carries the connections to the relay (xmas-relay.novalabs.app), so it sees the IP address and the sealed traffic, like any network in between. [Cloudflare's privacy policy](https://www.cloudflare.com/privacypolicy/).
- **Public MQTT brokers**: EMQX (broker.emqx.io), HiveMQ (broker.hivemq.com) and the Eclipse Mosquitto test server (test.mosquitto.org). These are used **only if the village's own relay can't be reached**. They see the IP address and the sealed traffic, but not what's in it. Some of these connections don't encrypt the traffic itself in transit, but what's inside stays sealed.
- **Open-Meteo** ([open-meteo.com](https://open-meteo.com)) provides the weather widget, only once a city is set for it. It gets the city's name (to find it), then its coordinates rounded to about 100 m, along with the IP address. This happens from the desktop app and the web page; the phone app doesn't fetch the weather.
- **GitHub**:
  - The desktop app checks GitHub for a newer version about every 6 hours (this can be turned off in the menu: Upkeep → Check for updates).
  - Updates and the installers are downloaded from GitHub.
  - On Linux, setting up the wallpaper may look up the latest kitty terminal there.
  - The web page is hosted on GitHub Pages.

  In each case GitHub sees the IP address, as for any web request. [GitHub's privacy statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).
- **Google Play / Android**: only the backup noted above. The Android app is distributed as a file from GitHub, and its only permission is internet access.

## Why (legal bases)

For people in the EU, the UK and similar places: the only personal data the project itself handles is what passes through the relay (IP addresses, and the sealed, mostly unreadable messages). It's handled because players ask for the online features by turning them on, and on the basis of the project's legitimate interest in running those features and keeping the relay safe from abuse. It isn't used for anything else: no profiling, no advertising, nothing sold or shared beyond the services above.

## Choices and rights

- **Go offline**: Menu → Online → go offline. Nothing more is sent, and what the relay kept expires as described above.
- **Friends and chats**: the ⋯ by each friend in Friends & chats. It can clear the chat with them, rename them, quiet them, or unfriend them (which clears the chat too). Chats are only ever kept on the device.
- **Everything else**:
  - Reset the settings, or start a new village (Menu → Upkeep).
  - Uninstall (Menu → Upkeep → uninstall). On a computer, uninstalling keeps the settings and the save, so the village is there if it comes back; deleting the folders listed above removes them too. On Android, uninstalling the app removes its storage.
  - On the web, clear the site's data for xmas.novalabs.app in the browser.
- **Unlink a device**: Menu → Online → devices → unlink.
- **Ask the project**: to say what it holds about a connection, to delete what the relay keeps, to unblock an address or lift a ban, or anything else. Open an issue as described under "Who runs it". In practice the relay holds very little that can be tied to a person, and most of it can't be read even by the project, but the request will be looked at either way.

Depending on where someone lives, they may also have the right to access, correct, delete or move their data, to restrict or object to how it's handled, and to complain to their data protection authority. Those rights are respected here, and the steps above are the quickest way to use them.

The project doesn't sell personal information or share it for advertising, as "sell" and "share" are meant in the California Consumer Privacy Act and similar laws.

## Children

xmas-tui is made for a general audience, families included. It doesn't ask for anyone's age, real name, email address, phone number or location, and it doesn't knowingly collect personal information from children. Online play is off until it's turned on. Strong language is starred out and links are hidden by default, only friends (who've exchanged codes) can send direct messages, and the public squares are looked after (see "The relay's own host").

If a child has shared something they shouldn't have, a parent or guardian can contact the project (see "Who runs it") to have what the relay keeps removed.

## Security

- What goes through the relay is sealed end to end with ChaCha20-Poly1305.
- The connections to the relay use TLS (wss:// and https://).
- The relay limits connections and message sizes, and the topics it keeps are restricted to the village's own.
- Only the relay's own host can speak for the relay (its settings, its public list, its announcements): it signs in with a secret, the relay refuses a wrong one, and what anyone else sends on the relay's own topics is dropped.
- No system is perfectly secure, and this is a hobby project. Anything that must stay secret belongs somewhere else.
- Security problems can be reported as described under "Who runs it".

## Across borders

The relay's server, Cloudflare, GitHub, Open-Meteo and the public brokers may be in other countries than the person using xmas-tui, so data sent online may cross borders on its way.

## Changes

When this page changes, the date at the top changes, and the release notes say so. The history of every change is in this repository.
