<div align="center">

# <img src="assets/icon/xmas-tui-128.png" width="48" height="48" alt="" align="top"> xmas-tui

**A cozy, glowing Christmas village that lives on the desktop.**

A real terminal UI rendered as an interactive live wallpaper for Linux (KDE Plasma and other Wayland
compositors) and Windows. Snow drifts down, the big tree twinkles, Santa flies past the moon, and a little
villager can wander the streets, chat with the neighbours and feed the deer.

[![Latest release](https://img.shields.io/github/v/release/dash1101/xmas-tui?color=c8202a&label=release)](https://github.com/dash1101/xmas-tui/releases/latest)
[![CI](https://github.com/dash1101/xmas-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/dash1101/xmas-tui/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-2f9a52)](LICENSE)
![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20Windows%20%7C%20Android-3a5ad8)

![The village at night: a giant decorated tree, snow-covered houses, a church, a snowman, Santa's sleigh crossing the aurora, the clock and a weather widget](docs/hero.png)

</div>

## ✨ Highlights

- **A whole village to explore**: about 3½ screens wide, with parallax mountains, lit cabins on the slopes, a
  cobbled sidewalk, two-lane traffic, a railway with a station, and a sky above the sky.
- **A villager to walk around**: say hello, pet and feed animals, pick berries, throw snowballs, bring friends
  along, step inside houses, and spend "cheer" at the cocoa cabin.
- **Townsfolk and wildlife**: named neighbours who hold real back-and-forth conversations about what's going
  on, react to shooting stars, Santa, fireworks and power cuts, gather for the light shows and step around
  each other; kids who build snowmen and igloos (and pile inside) and start snowball fights; deer, foxes and
  rabbits too.
- **Talk like a story game**: a dialogue box with choices. Everyone has their own way of talking (warm, chatty,
  shy, cheeky, dreamy or gruff-but-kind) and remembers the villager; every grown-up has a job and a mood, every kid
  a wish, and the talk follows whatever's going on, including what they just saw the villager do.
- **Homes worth visiting**: every house is its own household (the baker's, the toymaker's, the bookworm's…) with a
  family that potters about, a cat or dog, chairs to sit in, an oven, books, music, little jobs to help with, and
  windows that look out on the real village.
- **A home and a pet of the villager's own**: a house with a parlour, a kitchen, a bedroom upstairs and an attic with
  a telescope (walk up the stairs and the view follows), a sofa to sit on, a bed to sleep in and an oven that bakes
  gingerbread for the pockets; and a dog, cat or bunny (named, coloured, and remembered) that follows everywhere,
  fetches, does tricks, naps, and plays with other animals.
- **Play it like a game**: pop the village out into its own window, or take the wallpaper itself full screen and
  back (nothing restarts); zoom in on the villager with Ctrl +/-.
- **Sharp at any size**: Resolution (Ctrl Shift +/-) adds detail without shrinking anyone. Rooms, people and
  animals keep their size on a finer grid.
- **Christmas Eve and New Year's Eve**: the village goes to bed while Santa does his rounds roof by roof, leaving
  presents under the tree; on New Year's Eve there's a ball drop, a big countdown and a midnight party.
- **Day and night**: follows the real sunrise and sunset at the weather city, or stays at a chosen time.
- **Widgets**: a big clock with a Christmas countdown, system stats, now playing, weather, a village map and an
  audio visualizer.
- **Synthesized music and sound**: no audio files, all generated live, with live level meters.
- **Snow on open windows**: it piles up on window tops and the taskbar, with string lights.
- **Installers and updates**: a per-user Windows setup, a `.deb`, a no-sudo Linux installer, and built-in updates.

<p align="center"><img src="docs/village.gif" width="100%" alt="Snow falling on the village: the cocoa cabin, the station with a train, the snowman and townsfolk with lanterns"></p>

## 📸 A look around

<table>
<tr>
<td width="50%"><img src="docs/interior.png" alt="Inside a house: a crackling fire with stockings, a bookshelf, the front door with a wreath, the host and the villager"><br><sub><b>Inside a house</b>: walk up to a front door and press E.</sub></td>
<td width="50%"><img src="docs/church.png" alt="Inside the church: stained-glass windows, candles on the altar, a tree and the organ"><br><sub><b>The church</b>: stained glass, candles and an organ to play.</sub></td>
</tr>
<tr>
<td><img src="docs/shop.png" alt="The Cocoa and Gifts cabin with its menu open, next to the railway station"><br><sub><b>Cocoa & Gifts</b>: spend cheer on cocoa, gingerbread, carrots and a lantern.</sub></td>
<td><img src="docs/high-sky.png" alt="The high sky: the Milky Way, constellations, a hot-air balloon and a shooting star"><br><sub><b>Above the clouds</b>: Ctrl+wheel looks up to constellations and a balloon.</sub></td>
</tr>
<tr>
<td><img src="docs/christmas-eve.png" alt="Christmas Eve: the village asleep with dark windows while Santa's sleigh hovers by a roof and he calls ho ho ho down the chimney"><br><sub><b>Christmas Eve</b>: everyone's in bed, carrots are out, and Santa goes roof to roof.</sub></td>
<td><img src="docs/new-year.png" alt="Midnight on New Year's: HAPPY NEW YEAR in big letters over the big tree, with the ball-drop mast beside it"><br><sub><b>New Year's Eve</b>: the ball drops, the sky counts down, and midnight is a party.</sub></td>
</tr>
<tr>
<td><img src="docs/talk.png" alt="Talking with a kid called Ned: a dialogue box with what he said and three numbered answers"><br><sub><b>Conversations</b>: pick what to say with 1-5 or a click.</sub></td>
<td><img src="docs/snow-play.png" alt="Dusk: kids in a glowing igloo they just built, saying this is our house now"><br><sub><b>Kids at play</b>: snowmen, igloos (with them inside) and snowball fights.</sub></td>
</tr>
<tr>
<td><img src="docs/stairs.png" alt="Halfway up the stairs in the villager's home: the parlour below with its tree, the bedroom floor coming into view above"><br><sub><b>Upstairs</b>: the view follows the villager up into the bedroom.</sub></td>
<td><img src="docs/hires.png" alt="At a higher resolution: villagers with bobble hats and lanterns by the big tree, the villager and their dog by the snowman"><br><sub><b>More resolution, same size</b>: everyone drawn to scale on a finer grid.</sub></td>
</tr>
<tr>
<td><img src="docs/noon.png" alt="The village at noon under a blue sky"><br><sub><b>Noon</b>, with the time of day set to follow the real sun…</sub></td>
<td><img src="docs/dusk.png" alt="The village at dusk with an orange sunset behind the mountains"><br><sub>…and <b>dusk</b>, as the lights come on.</sub></td>
</tr>
</table>

## 📦 Install

| Platform | Download | Notes |
|---|---|---|
| **Windows** 10/11 | `xmas-tui-setup-x86_64.exe` | Per-user install (no admin needed), Start menu entries, optional run at startup, uninstaller. |
| | `xmas-tui-windows.zip` | Portable: unzip anywhere and run `xmas-tui.exe`. |
| **Linux** (no sudo) | `xmas-tui-linux-installer.sh` | Installs to `~/.local/share/xmas-tui` with a systemd user service; updates itself. |
| **Debian / Ubuntu** | `xmas-tui_*_amd64.deb` | System-wide (dpkg always needs sudo); then `systemctl --user enable --now xmas-tui`. Updated through the package manager. |
| **Android** 7+ | `xmas-tui-android.apk` | A live wallpaper: a cut-down village that lives its own life on the home screen. See [Android](#-android-live-wallpaper). |

All of these are on the [latest release](https://github.com/dash1101/xmas-tui/releases/latest). Both platforms also
have a one-line installer straight from the repo. On **Linux** (no sudo):
```bash
curl -fsSL https://raw.githubusercontent.com/dash1101/xmas-tui/main/scripts/get.sh | bash
```
On **Windows**, in PowerShell (per-user, no admin; installs the latest release with run at startup, then starts it):
```powershell
irm https://raw.githubusercontent.com/dash1101/xmas-tui/main/scripts/get.ps1 | iex
```
Linux needs [kitty](https://sw.kovidgoyal.net/kitty/) ≥ 0.36 and a layer-shell compositor such as KDE Plasma 6, Sway
or Hyprland (GNOME isn't supported). Settings live in `~/.config/xmas-tui/config.toml` (Linux) or
`%APPDATA%\xmas-tui\config.toml` (Windows). Remove the Linux install with the same command plus `-s -- --remove`.

**Updates:** when a new version is out, a small banner asks whether to install it. **Install automatically** under
Settings → Display → Updates skips the question. Downloads are checked against the release's `SHA256SUMS` before
anything is replaced, and `xmas-tui --update` does the same from a terminal. **Run at startup** sits in the same tab.

## 🎮 Controls

Click the bare desktop first so the wallpaper has keyboard focus.

| Input | What it does |
|---|---|
| **WASD** / **arrow keys** | walk the villager; the view follows (hold **Shift** to run) |
| **E** / **Space** / **Enter** | interact with whatever the `[E]` prompt points at (people, animals, bushes, doors, the cabin…); with nothing else around, the villager's pet |
| **F** | invite someone along (animals want a berry); with nobody in front, say goodbye to everyone |
| **R** | scoop a snowball, press again to throw it |
| **Q** | what's in hand: berries or something from the pockets (E then gives it, or uses it) |
| **I** | show or hide the pockets |
| **1**–**5** | answer in a conversation, use an item, or buy one while the cabin's menu is open |
| **Esc** | close a menu, conversation or window view; with nothing open, the settings |
| **?** / **F1** | the controls card (also the **? controls** button, top right) |
| **A** / **D** at a window | lean to look further along the street |
| **mouse wheel** / **Ctrl+wheel** | look along the village / up into the sky |
| **right-click**, or **press and hold** | options for whatever's under the pointer: drop in from the sky, walk over, say hello, give a gift, visit a house, the villager's stats… (settings at the bottom) |
| **drag** anyone (the villager too) | carry them about, then drop them in the snow |
| **click** things | presents, trees, the snowman, houses, lamps, the station, the moon, the sky… |
| **⚙ settings** (top right) | the settings panel; drag its title bar to move it |
| **F11** | full screen and back: the wallpaper itself, or a popped-out window |
| **Ctrl +** / **Ctrl -** / **Ctrl 0** | zoom in, out, back to 1× |
| **Ctrl Shift +** / **Ctrl Shift -** | resolution: finer or coarser detail, everything the same size |

## 🌟 What's in the scene
- A big tiered tree with light strings that cycle through twinkle, chase, wave, breathe and sparkle patterns,
  plus tinsel, glinting ornaments and a pulsing star.
- Smaller trees with warm, icy and multicolour lights, and presents with ribbons and bows.
- Houses with firelit windows, roofline lights and chimney smoke, plus a church with stained glass. Every one
  can be visited.
- Lamp posts, fences, berry bushes, a snowman, the Cocoa & Gifts cabin and the railway station.
- Three depths of snow in gusty wind, and glinting snowfields.
- A moon with a halo, an aurora, shooting stars, and Santa's sleigh flying past every few minutes. Higher up:
  the Milky Way, constellations, the North Star, a hot-air balloon and the odd satellite.
- A subtle powerline along the hills. (Click it enough times and see what happens.)

**Widgets** (each can be dragged anywhere, toggled, and locked in place):
- A big glowing clock with a Christmas countdown.
- A system panel: CPU, GPU, VRAM, RAM, disk and network, with candy-cane bars and sparklines.
- Now playing, from any MPRIS player (Linux) or media session (Windows).
- Weather, from Open-Meteo, which needs no API key.
- A map of the whole village: where the villager is, where they started (⚑), what's on screen, trains and
  townsfolk. Click it to look somewhere; drag its ◢ corner to resize it.
- A visualizer (off until switched on): bars in Christmas colours dancing to whatever's playing, each with a cap
  of snow that settles back down, standing on a strip of ice that reflects them. **kHz labels** write each bar's
  frequency on the ice. It listens to the default audio output (the sink's monitor through `pw-record` or `parec`
  on Linux, WASAPI loopback on Windows), only while it's on; nothing is recorded or sent anywhere.

## 🖱️ Interacting in detail
**Walking around** (click the desktop first so it has keyboard focus)
- **WASD or the arrow keys** walk the little villager (gold coat, green bobble hat) along the snowfield.
  The view follows along; the **mouse wheel** looks around independently. **Walk speed** is in the Scene tab.
- **E, Space or Enter** interacts with whatever the `[E]` prompt points at:
  - **people:** a conversation opens in a dialogue box: they say something, and **1**–**5** (or a click, or E for
    the first answer) picks what to say back. Grown-ups talk about how they are, their job, tips for a newcomer
    and what's going on (the snow, the train, the light show…), and can be asked to walk along; kids talk about
    what they're up to, what they want for Christmas and Santa, and are always up for a snowball fight. A cocoa
    in the pockets can be handed over to someone who'd like one. People notice what's around them, too: kids
    building a snowman ask for a hand (and get one), kids mid-snowball-fight can be joined, and anyone nearby
    mentions what they just saw the villager do: feeding or petting an animal, throwing snowballs, handing out
    presents, carrying someone about, dropping in from the sky, coming out of a house. The box can be dragged
    anywhere and stays there. Walking away or Esc ends it;
  - **animals:** a pat, or (with berries in hand) a snack, after which they tag along for a while;
  - **berry bushes:** pick a few berries (they grow back);
  - **front doors:** step inside. Every house is its own: a household with a theme (the baker's oven and cookie
    jar, the toymaker's workbench and a toy train round the tree, the bookworm's shelves, the musician's piano and
    gramophone, the sailor's telescope, the candle-maker's glow, the farmer's rocking chair and radio), its own
    wallpaper, floor and furniture, one to three people who move about (warming their hands, sitting down, looking
    out of the window), and often a cat or dog. In there:
    - **talk** to anyone at home in the dialogue box: about their home, their family, how long they've lived there;
      they'll offer a seat or a fresh cookie, play the piano, or ask for a hand (put the star back on the tree, a log
      on the fire, find the cat, play a tune, hang the last stocking) — and are grateful when it's done;
    - **sit** in the armchairs and on the sofas, **bake** (the oven dings when the cookies are done; E again takes
      the tray of gingerbread for the pockets), **read** a book, **play** the piano, gramophone or radio,
      **decorate** the tree, peek in a **stocking**, play with the **pet** (the dog fetches, the cat chases string);
    - **look out of a window**: the real, live village outside, from where that window is (A/D to lean, E or Esc to
      step back). Walk out the door or press Esc to leave;
  - **the villager's own house** (the one with the flag by the door and lights up under the roof): a parlour with
    a sofa, a kitchen next door with an oven and a table, stairs up to a bedroom with a quilted bed, and a ladder up
    to the attic with a round window and a telescope. E at the stairs walks the villager up them while the view
    pans up into the room above (and back down the same way). E on the sofa sits down on it; E on the bed tucks the
    villager in (the room dims, the pet curls up at the foot of the bed) until a direction key gets them up. Each
    window looks out from its own height — the attic's up into the stars;
  - **the pet:** the villager has a dog, cat or bunny that follows them everywhere, indoors too. Turn to face it
    and press E (it comes before anything else in reach), or click it, for its box. The first time it asks for a name; after that it can be fussed over, played with (a ball or a stick for a dog, a ball of yarn or
    the scarf for a cat, a carrot for a bunny, tricks, "chase me!"), renamed, given a new look, or asked to stay
    home. How a game goes varies: a dog might bring the ball straight back, take a victory lap first, or come back
    with a stick; a cat bats the yarn about, unravels it, or curls up round it. Left standing about a while, the pet
    naps. Out in the village a dog chases rabbits and foxes, a cat stalks them, a bunny hops along with rabbits, and
    pets meeting other people's pets have a happy bounce together. With a carrot or gingerbread in hand (**Q**), E
    gives it a treat. Its kind, colour and name live in the Scene tab (and are remembered);
  - **the cocoa cabin:** opens its menu;
  - **presents, trees, lamps, houses, the station and the snowman:** the same as clicking them.
- **F** invites the person in front to come along (animals need a berry). Companions follow the villager around.
  Stopping near someone else gets the companions talking with them, or with each other. **F** with nobody in
  front says goodbye to everyone.
- **R** packs a snowball, and **R** again throws it at whoever's ahead. People react, and kids often throw one back.
- **Cheer (✦)** is earned by being neighbourly: first hellos, feeding animals, snowball fights, watching a light
  show by the big tree, finding constellations. It buys things at the **Cocoa & Gifts** cabin: hot cocoa (cozy
  glow), gingerbread and candy canes (to give away), carrots (a deer or rabbit becomes a friend) and a lantern.
  **I** shows the pockets; **1**–**5** use what's in them. **Q** picks what's in hand (berries, or anything in the
  pockets; it shows at the bottom left): E then gives it to whoever's in front, or uses it with nobody there.
- **People know the villager's name** once one is set (Scene tab → Your name): they say it when they see the
  villager again, and word gets round.
- **Right-click** (or **press and hold** the left button for a moment, which works everywhere, including the Windows
  desktop) shows what can be done with whatever's under the pointer, and the villager walks over to do it:
  - **open ground:** *drop in here* (the villager shimmers down from the sky and lands with a puff) or *walk over*;
  - **the villager:** the stats card and Christmas list, or the pockets;
  - **people:** say hello, invite along or say goodbye, give a gingerbread or candy cane, throw a snowball;
  - **animals:** pet, feed a berry, give a carrot;
  - **houses and the church:** visit, or switch the lights / ring the bells;
  - **the cocoa cabin, the station, the sky:** shop, ask about the next train, wish on a star.
  The settings are always at the bottom of the menu (and on the **⚙ settings** button).
- **The Christmas list** (on the stats card) has ten little goals: hellos, chats, feeding animals, visiting homes,
  giving a gift, landing snowballs, watching a light show, finding constellations, walking the village end to end,
  and waving down Santa's sleigh.
  Each pays cheer when it's done.
- The villager can be turned off with **Villager (WASD)** in the Scene tab; **Walk speed** goes up to 6×.

**Mouse**
- **Drag any widget** to move it. Positions are saved, and can be locked or reset in settings.
- **The ⚙ settings button (top right)** opens settings; drag the panel's title bar to put it anywhere. Click ◂ ▸ or scroll on a row
  to adjust a value. Click the city field and type a new city. Every change is saved straight to the config.
- **The cursor** leaves a twinkling trail with a soft glow and pushes falling snow aside. Sweep it
  through the snowfield to kick snow up.
- **Clicking things in the scene:**
  - **Presents** wiggle and pop sparkles, and sometimes reveal what's inside.
  - **Trees** blaze. Click a tree's **star** for a starburst.
  - The **snowman** talks back. Try clicking him a few times in a row.
  - **Houses** switch their lights off and on. The **church** rings its bells.
  - **Lamps** sputter. The **station** says when the next train is due.
  - The **moon** summons Santa. **Click the sleigh** as it flies past: Santa pulls up for a word with whoever's
    below, or just gives a hearty "ho ho ho".
  - **Empty sky** launches a shooting star, and **snow** puffs up. **Click a shooting star** to catch it (and make a
    wish).
  - **Cars** honk and flash their high beams; **trains** whistle and blaze with light; the **airplane** blinks its
    landing lights; the **owl** hoots; a **flock** of birds scatters; the little **cabins up on the mountains** turn
    their lamps up.
  - Up in the high sky: **constellations** light up and name themselves, the **balloon** fires its burner, and
    the **North Star** grants a wish.

**Typed easter eggs** (also need keyboard focus)
- Type `hohoho`, `snow`, `joy`, `aurora` or `lights` to trigger an effect.
- There's also one classic cheat code (↑↑↓↓←→←→BA).

**Things that happen on their own**, timed by the "Surprises" setting:
- **Wildlife:** deer, foxes and rabbits wander through and leave footprints.
- **People and travel:** walkers carrying lanterns, often in twos and threes chatting as they go; cars in both
  lanes of the distant road (keeping right, or left where the weather city drives on the left); trains that stop
  at the station and let passengers off.
- **Sky and weather:** wind gusts, short blizzards, meteor showers, aurora flares and a rare comet.
- **Lights:** village-wide rainbow light shows (announced first, so everyone gathers round the big tree), and
  houses switching their lights.
- **Sky life:** clouds drifting past (moonlit at night), a blinking airplane, flocks of birds by day, an owl
  across the moon, and paper lanterns floating up from the village. The sky drifts a little as the view scrolls.
- **On a schedule:** church bells on the hour, and on December nights Santa sometimes drops in on a house or two:
  the sleigh hovers by the roof, he climbs over to the chimney and goes down, and another present appears under
  the big tree.

**Special nights** (going by the clock):
- **Christmas Eve** (from 6 pm): the kids put carrots out on the doorsteps in the afternoon. At night the village
  goes to bed: windows go dark (bar the odd night-light), the traffic and trains stop, the cocoa cabin shuts, and the
  trees and presents glow softly. Santa works his way round every house, roof by roof, all night. The reindeer
  eat the carrots, and the pile under the big tree grows. In the morning everyone's up at once, the bells ring
  and there's a light show.
- **New Year's Eve** (from 6 pm): a ball-drop mast goes up beside the big tree and party blowers come out. Ten minutes
  before midnight everyone gathers round. The ball slides down over the last minute, the last ten seconds count
  down huge across the sky, and at midnight there are fireworks, confetti, cheering and HAPPY NEW YEAR.

## 🔔 Tray icon and settings window
While it runs as the wallpaper, a little glowing tree sits in the system tray (KDE Plasma, or any tray that
speaks StatusNotifierItem; the notification area on Windows).
- **Left click** opens a small **settings window**: the same panel, on its own over a snowy backdrop. Typing
  works here, so the weather city can be set without Notepad. The wallpaper picks up every change within
  about a second, through its normal config live-reload. Only one settings window opens at a time.
- **Right click** gives quick toggles:
  - **Linux:** Settings…, Window snow, Sound, Pause effects, Quit wallpaper.
    **Pause effects** freezes the animation and hides window snow and lights until it's unticked. It isn't saved.
  - **Windows:** Settings…, Window snow, Sound, Run at startup, Quit wallpaper.
- **Quit wallpaper:** under the systemd service, this runs `systemctl --user stop xmas-tui` (a plain exit would
  just be restarted). Otherwise it exits cleanly.
- The settings window also runs on its own: `xmas-tui --settings-window [--config PATH]`.
## 🎶 Sound
Every sound is synthesized live in code. There are no audio files and nothing copyrighted.
- **Ambience:** wind that swells with gusts and blizzards, a soft snowfall hiss, a gently crackling
  fire and very faint distant wind chimes. An aurora flare adds a quiet shimmer.
- **Effects** for what happens in the scene:
  - present shakes and jingles, the snowman's little "hm-hm?" and sleigh bells that pan across with Santa
  - church bells on the hour (and a peal when the church is clicked), fireworks, shooting stars
  - lamp sputters, light-switch clicks, the distant train's chuff and whistle, snow crunching
  - a small fanfare for the typed easter eggs, and an optional airy sparkle when the cursor is flicked
- **Music:** gentle generated pieces in four styles: music box, celesta with a warm pad, soft piano and
  an upbeat sleigh ride. Half are original tunes composed on the spot (different every time); half are
  new arrangements of public-domain carols (Silent Night, Jingle Bells, Deck the Halls, O Christmas Tree,
  We Wish You a Merry Christmas, Good King Wenceslas, Joy to the World).

**Settings → SOUND** has an on/off switch, master, music, effects and ambience volumes, the music style
("shuffle" or one style) and the cursor sparkle. These are also under `[sound]` in the config.

**What's playing:** the Sound tab shows live level meters for the music, the ambience and whichever effects are
sounding right now (loudest first, in dB), and the Effects tab puts a small live bar beside each effect's volume.
That makes it easy to tell which sound is which, and which slider turns it down.

**Output:**
- **Linux:** streams to PipeWire through `pw-cat`, or to PulseAudio through `paplay`. If the player dies,
  it is restarted. No ALSA headers or audio libraries are needed.
- **Windows:** uses `winmm`, which ships with Windows.
- **Latency:** about 50–80 ms.
- **Pausing:** when the wallpaper pauses (for example during a fullscreen game on Windows), the sound
  fades out and the device is released.

**Developer tools:**
```bash
xmas-tui --export-audio DIR && python3 scripts/check-audio.py DIR   # render every sound to WAV, check levels
XMAS_AUDIO_DEV_VOLUME=0 xmas-tui --audio-test 8                      # drive the real output path, silently
```

## ❄️ Snow on open windows
Snow also falls in front of open windows. It settles on the top edge of each window and on the panel, up to
`pile_rows` high (1.5 by default), stacked in eighth-block clumps.
- **Moving a window** carries its pile along with it.
- **Shaking or flinging a window** throws the snow off.
- **Sweeping the cursor** through a pile brushes the snow away.
- **Closing or minimizing a window** drops its pile.
- **Lights:** twinkling bulbs hang on a wire around the six topmost windows, and droop along the top edge.

**Settings → Windows** tunes it all: pile height, snowfall, settling speed, how fast the flakes fall, the wind,
how easily shaking a window throws its snow off (or never), slow melting, and whether the cursor sweeps piles away.
The **taskbar** has its own on/off and pile height.

Both effects can be switched on or off in settings ("Snow on windows" / "Lights around windows") or in `[windows]` in
the config. Everything is hidden while a fullscreen app is in front.

**How it works on KDE:** Wayland doesn't let apps see other windows. So xmas-tui loads a tiny KWin script
(`kwin/xmas-snow.js`) that reports window positions over D-Bus, and it unloads the script again when xmas-tui stops.
The snow and lights are drawn on a transparent, click-through overlay layer, so it never blocks clicks. Damage is
merged into tiles; that keeps KWin's own cost at about 0.4% of a core.

**How it works on Windows:** a click-through, always-on-top layered window covers the wallpaper's monitor. Window
positions come from `EnumWindows` plus DWM's visible frame bounds; the taskbar counts as a panel. It polls about 30
times a second, about 60 while a window is being dragged, and 4 times a second (window hidden) when there's nothing to
draw or a fullscreen app is running. Sizes follow the monitor's DPI scaling.

Debugging: running with `XMAS_SNOW_DEBUG=1` writes `snow-debug.txt` and `snow-canvas.png` to
`$XDG_RUNTIME_DIR/xmas-tui/` (KDE) or `%TEMP%\xmas-tui\` (Windows; a directory path as the value is used instead).
On Windows the status line also has per-phase timings. `XMAS_WINFX=1 xmas-tui --windowed` starts the Windows overlay
next to a normal window, for testing (e.g. under Wine).

## 📱 Android live wallpaper

<img src="docs/android.png" align="right" width="320" alt="The phone wallpaper at night and at dusk: cottages, the big tree, villagers in bobble hats, and a snowy shelf at the bottom with snow piled on it">

A cut-down village for the phone's home screen, made to be watched rather than played:
- **The village lives its own life.** Folk wander, stop to chat in speech bubbles, pop into each other's
  houses and gather by the tree. The villager (green bobble hat) is out among them: greeting everyone by name,
  playing with their pet, feeding the deer. Deer and rabbits wander through, kids end a chat with a snowball,
  and someone always spots the shooting stars.
- **A snowy dock.** A shelf sits under the launcher's dock, and the snow lands on it and piles up, with icicles
  underneath. Its position, height, width, style (frosted glass, wood, candy cane or stone) and how high the snow
  piles are all in the settings, so it can be lined up with any launcher.
- **Taps** (on empty home-screen space): someone waves and says hi, the tree blazes, a house switches its
  lights, the sky gets a shooting star, and a tap on the dock knocks its snow off.
- **Day and night** follow the phone's clock (or stay at a chosen time), and the village pans gently as the home
  screens are swiped.
- **Settings** (the app's own screen, or the gear in the wallpaper picker): the frame rate (24 fps by default; it
  only runs while the home screen is showing), snowfall, detail, the villager's name, the pet (cat, dog, bunny or
  none) and its name, speech bubbles, time of day, and the dock.

**Install:** download `xmas-tui-android.apk` from the [latest release](https://github.com/dash1101/xmas-tui/releases/latest)
on the phone, open it (allowing installs from the browser or file manager when asked), then open **xmas-tui
village** and tap **Set as live wallpaper**. Android 7 or newer. [Orbit](https://github.com/dash1101/orbit) on Android
installs it too, and keeps it updated.

**Build:** `android/build.sh` (a JDK 17 and the Android SDK; no Gradle). The village itself is plain Java in
`android/src/.../core`, and `android/preview/Preview.java` renders it to PNGs on the desktop.

**Signing:** releases are signed with xmas-tui's own release key, and Android only installs an update signed with
the same key. Its certificate's SHA-256 is in `android/release-cert.sha256`. The release workflow signs the APK with
the key from the repository's secrets (`XMAS_KEYSTORE_B64`, the keystore in base64, and `XMAS_KEYSTORE_PASSWORD`), and
CI checks those secrets on every push to main. Locally, `build.sh` signs with the key in
`~/.local/share/xmas-tui-signing/`; `XMAS_NEW_KEY=1` makes a new key for a fork or a test build. Without the secrets, a
release has no APK, and one built with `build.sh` is uploaded by hand (and added to `SHA256SUMS`).

<br clear="right">

## 🪟 Windows
The release has a per-user setup (`xmas-tui-setup-x86_64.exe`, no admin needed) and a portable zip with a standalone
`xmas-tui.exe`; its README explains how to use it.
- **How it runs:** it draws the same grid itself, with fonts bundled inside the `.exe`, and attaches behind the
  desktop icons on the main monitor. It pauses during fullscreen games.
- **Controls:** the **⚙ settings** button (top right) opens settings, which include **Run at startup** and **Quit**.
  Right-click on the desktop belongs to Windows, so **press and hold** the left button instead: the same menus open.
  Walking works too: click the bare desktop, then use WASD / the arrows, E, F, R, I and 1–5 (the wheel can't
  scroll the view there, because the icon layer sits on top; the map widget can be clicked instead).
- **Weather city:** clicking the city field opens the config in Notepad, because a wallpaper behind the icons
  can't receive typing.
- **Window snow and lights:** see above; they're on by default and run in front of all windows.
- **Fallback:** `--windowed` runs it as a normal window. That's useful with Lively Wallpaper if the
  desktop attach doesn't work on some Windows build.

Releases are built on Windows by CI. To cross-compile from Linux instead, run `./scripts/build-windows.sh`; it
needs the `x86_64-pc-windows-gnullvm` Rust target plus [llvm-mingw](https://github.com/mstorsjo/llvm-mingw) in
`~/.local/llvm-mingw` (or set `LLVM_MINGW`).

**What has been tested:**
- It cross-compiles cleanly, and the `.exe` needs only DLLs that ship with Windows.
- Under Wine 11.22 it renders, and CPU, RAM, disk, network and weather all work.
- Under Wine, the window snow overlay finds the windows and a taskbar, piles snow on them, draws the lights, drops a
  closed window's pile, and shuts down cleanly when the app closes.

**Not yet tested on real Windows:** the desktop attach (classic and 24H2), clicks and keys through the icon layer,
GPU %, now-playing, the fullscreen pause, run at startup, the setup program, self-updating, and the window snow
overlay's real CPU/DWM cost. (CI builds the `.exe` and the installer on Windows for every release.)

**Desktop attach:** on Windows 10 through 11 23H2 the wallpaper becomes a child of the WorkerW that Explorer creates
behind the icons. On Windows 11 24H2's "raised desktop" (Progman has `WS_EX_NOREDIRECTIONBITMAP`, and both the icon
view and the WorkerW are Progman's children), it becomes a layered child of Progman, placed between the icons and
the WorkerW, as Lively Wallpaper does. Layered child windows need the app manifest
(`packaging/windows/xmas-tui.manifest`, embedded by `build.rs`) to declare Windows 8+ support.

The same native window also works on Linux with `--windowed`.

## 🦌 Animals, people and berry bushes
Deer, foxes, rabbits, people carrying lanterns (often in small groups) and pairs of kids wander through. Animals often
stick around for a few minutes. They also interact:
- **Petting and greeting:** passers-by stop to pet animals (hearts float up), greet each other and say hi to the snowman.
- **Conversations:** everyone has a name, and conversations are written exchanges where each line answers the last.
  They fit what's going on: morning, afternoon, evening or night; a blizzard; a train that just came in; a light
  show; the power cut; Santa overhead; a shooting star; how many days are left until Christmas; and where they're
  standing (the big tree, the church, the cabin, the station). People walking together stop to chat now and then,
  and strangers who meet sometimes do too. Drop two people next to each other, or lead companions over to someone,
  to get one going. A passer-by may stop and chime in, and the group answers them.
- **Reactions:** when something happens (a shooting star, a comet, Santa, fireworks, the light show, the aurora
  flaring, a blizzard, a train pulling in, the power going out or coming back, a typed easter egg) the people in
  view stop, look and say something about it.
- **Pets:** some folk walk their dog on a lead; others have a cat or a bunny along. Click a pet for a happy bounce
  (and its person says who it is), and ask its owner about it.
- **Play:** a fox plays chase with a rabbit (the fox never catches it). Kids square off in two sides for proper
  snowball fights, build snowmen ball by ball, and raise igloos block by block, then all pile inside for a while
  (giggling) before coming back out. What they build stands for about ten minutes, then melts.
- **Manners:** nobody walks through anybody. People step a row up or down to get round each other (and the
  villager), or stop for a chat first and then step round.
- **Light shows:** when one's coming, it's announced, a few more folk come out, and everyone gathers round the
  big tree to watch.
- **Snacking:** animals stop at the snow-capped berry bushes. The berries get eaten and regrow a minute or two later.
- **The cursor:** animals startle when it rushes at them, and come over curious when it rests nearby.

**Pick anyone up:** press on them and drag to carry them around. They wiggle and comment while being carried. Let go
to drop them back in the snow; people get a little dizzy. A click without dragging is a pat or a friendly hello. Clicking
a bush rustles it, and sometimes a cardinal flies out or a rabbit pops out.

The Scene tab sets how many visitors come by ("Animals & people"), the most out at once, and turns the bushes on
or off.

## ⚙️ Settings
<img src="docs/settings.png" align="right" width="360" alt="The settings panel, Scene tab: snowfall, wind, glow, surprises, visitors, sleigh and frame rate">

The settings panel (the **⚙ settings** button, the bottom of any right-click menu, or the tray icon's standalone
window, which shows the live sound levels too) can be dragged anywhere by its title bar. It has five tabs:
- **Scene:** snow, wind, glow, surprises, visitors and the **most out at once**, sleigh, walk speed, which side of the road traffic keeps to,
  **time of day** (auto from the real sunrise and sunset, or night, dawn, morning, noon, afternoon, dusk), frame
  rate, **speech bubbles** (how long they stay up; longer lines always stay longer), **chattiness** (how often people
  stop to talk and remark on things), aurora, bushes, the villager and **their name**, and the villager's **pet**
  (dog, cat, bunny or none, its colour and its name).
- **Display:** clock, widgets (including the map, and the visualizer with its kHz labels), weather city and units, widget layout, **resolution** (0.5× to
  2× in 0.05 steps: more detail, with everything the same size; Ctrl Shift +/-), **zoom** (1× to 3×, centred on
  the villager; Ctrl +/-: the village magnified crisply with a soft glow, people and animals drawn at the bigger
  size, speech bubbles kept readable), **pop out** (the village in a window of its own, to play it like a game; the wallpaper
  rests while it's open) and **full screen** (the wallpaper itself comes out in front, with the keyboard, and **back
  to wallpaper** returns it; F11 does both). In the kitty wallpaper a new resolution applies live; if kitty can't
  change its font, a banner asks before restarting. Then updates and run at startup. A window also works from the
  command line: `xmas-tui --windowed` and `xmas-tui --fullscreen`.
- **Sound:** master, music, effects and ambience volumes, music style, a per-layer ambience mix, and **Quiet while
  other media plays**. When on, the sound fades out as soon as a browser video or music player starts, and comes
  back a few seconds after it stops.
- **Effects:** a separate volume for each kind of effect, 16 in all, each with a live level bar.
- **Windows:** the snow on open windows and the lights around them:
  - pile height, snowfall, settling speed, fall speed, wind, shake-off and melting
  - **falling snow in front of windows** on/off (piles still grow when it's off), and cursor sweeping
  - snow on the taskbar, with its own pile height
  - light colours (multicolour, warm white, icy blue, candy cane), how many windows get lights, and brightness

## 🔁 Staying on
It runs as a systemd user service (`xmas-tui.service`) that starts with the graphical session.
If it ever exits, the service starts it again within about 3 seconds. That covers the monitor deep-sleeping
and disconnecting (which removes the wallpaper surface), crashes, and so on. Before relaunching, it waits until a
monitor is actually connected again.

## 🔧 How it works
- **Rendering.** Everything is a character grid. The backdrop (sky, moon,
  mountains, houses) uses half-block `▀` pixels. Everything else is real
  glyphs: `^ ◢ ◣ ● ★ ❄ ┼ ━ ▰`.
- **Glow.** Light sources write into a light buffer. It is blurred at two
  radii and added to the colors of nearby cells, which gives halos around
  bulbs and warm pools of light on the snow.
- **Efficient output.** Only changed cells are sent, inside synchronized-output
  blocks so a half-drawn frame never shows.
- **Wallpaper.** `kitten panel --edge=background` puts a kitty terminal on
  the Wayland background layer, so it sits behind every window.

## 🛠️ Building from source
```bash
cargo build --release
./scripts/install.sh            # default config + systemd user service, start now
./scripts/install.sh --remove   # stop it and remove the service
systemctl --user restart xmas-tui   # restart it by hand (status/stop work too)
./scripts/xmas-wallpaper.sh     # start the wallpaper manually
./target/release/xmas-tui       # run inside any truecolor terminal (q or Esc quits)
```
A build from source never updates itself; pull and rebuild instead. The installers are built by CI from
`packaging/linux/build-deb.sh` (the `.deb`) and `packaging/windows/xmas-tui.iss` (the Inno Setup per-user setup). Releases are cut by pushing a `vX.Y.Z` tag
(matching `Cargo.toml`): CI builds both platforms and publishes them with `SHA256SUMS`.
Config lives at `~/.config/xmas-tui/config.toml`. See `config.example.toml`
for every option, including the weather city.

Developer tools:
```bash
xmas-tui --export DIR --size 568x75 --frames 1 --at 20 --sleigh   # render frames to PNG (--pan CELLS to look around)
xmas-tui --export X --frames 720 --fps 24 --clean --raw | ffmpeg -f rawvideo -pix_fmt rgb24 -s 5112x1425 -r 24 -i - out.mp4  # a video
xmas-tui --bench 300 --size 568x75                                 # timing + bytes per frame
xmas-tui --settings-window --export DIR                            # settings window + tray icons to PNG
xmas-tui --bench 300 --size 568x75                                 # timing + bytes per frame (+ window fx on Linux)
```

## 📋 Requirements
- kitty ≥ 0.36, for background panels.
- Rust toolchain (only to build from source).
- `busctl`, for the music widget (part of systemd).
- `pw-record` (PipeWire) or `parec` (PulseAudio), for the visualizer.

## 📊 Performance
Measured on a 5120×1440 screen (the size it was designed for; it adapts to any other).

| | |
|---|---|
| Grid at `font_size=10` | 568×75 cells, about 9×19 px per cell |
| Frame cost | ~2.6 ms of CPU (scene 0.4, compose 1.7, encode 0.5) |
| Terminal output | ~70 KB per frame, ~16% of cells change |
| Live at 30 fps | xmas-tui ~10% of one core, kitty ~10% of one core |
| Memory | xmas-tui 17 MB, kitty 174 MB |

## 📜 License
MIT, see [LICENSE](LICENSE). Bundled fonts: DejaVu (Bitstream Vera license) and Noto Sans Symbols 2 (SIL OFL);
their licenses are in `assets/fonts/`. The carol arrangements are of public-domain tunes.
