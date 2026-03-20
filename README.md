# chs-deck

Personal [OpenDeck](https://github.com/ninjas-code-official/opendeck) plugin for the **Ajazz AKP153E** — a 5×3 button grid with a right display column (18 slots total). Written in Rust.

![Ajazz AKP153E](docs/deck.jpg)

---

## Device Layout

```
       COL 0     COL 1     COL 2     COL 3     COL 4   │  COL 5 (display)
     ┌─────────┬─────────┬─────────┬─────────┬─────────┼──────────────┐
ROW0 │   [0]   │   [1]   │   [2]   │   [3]   │   [4]   │     [5]      │
     ├─────────┼─────────┼─────────┼─────────┼─────────┤              │
ROW1 │   [6]   │   [7]   │   [8]   │   [9]   │  [10]   │    [11]      │
     ├─────────┼─────────┼─────────┼─────────┼─────────┤              │
ROW2 │  [12]   │  [13]   │  [14]   │  [15]   │  [16]   │    [17]      │
     └─────────┴─────────┴─────────┴─────────┴─────────┴──────────────┘
      ← 5 pressable columns (72×72 px each) →            right display
```

Each button is **72×72 px**. Slots 0–4 · 6–10 · 12–16 are pressable; slots 5 · 11 · 17 are the right display panel.

---

## Actions

### 📊 Stats Display
Shows CPU%, RAM%, or Docker container count — updates every few seconds.

```
┌─────────┐  ┌─────────┐  ┌─────────┐
│ 🖥  CPU │  │ 🧠  RAM │  │ 🐳  8   │
│   27%   │  │   53%   │  │  ctrs   │
└─────────┘  └─────────┘  └─────────┘
```

### 🎵 Media Playback
Controls any MPRIS player (Spotify, browser, mpv…) via `playerctl`. Shows track title and status.

```
┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐
│   ⏮    │  │  ▶/⏸   │  │   ⏭    │  │   ⏹    │
│  Prev   │  │ Toggle  │  │  Next   │  │  Stop   │
└─────────┘  └─────────┘  └─────────┘  └─────────┘
```

### ⚡ Shell Command
Runs any shell command on press. One-tap VPN, app launch, scripts.

```toml
[[slot]]
index   = 6
action  = "chs.deck.shell"
command = "pkill -x nekoray; sudo hiddify &"
label   = "VPN"
```

### 🎬 Video Tile
Plays a video file — or a **YouTube / Twitch live stream** via yt-dlp — tiled across multiple buttons in real-time via ffmpeg.

```
┌────┬────┬────┬────┬────┐
│    │    │    │    │    │  one video frame
│ ▶  │    │    │    │    │  split across
├────┼────┼────┼────┼────┤  5×3 = 15 buttons
│    │    │    │    │    │  at up to 10 fps
│    │    │    │    │    │
├────┼────┼────┼────┼────┤
│    │    │    │    │    │
│    │    │    │    │    │
└────┴────┴────┴────┴────┘
```

```toml
[[slot]]
index      = 0
action     = "chs.deck.video"
video_path = "https://www.youtube.com/watch?v=..."   # yt-dlp resolves stream
video_cols = 5
video_rows = 3
video_fps  = 10
```

### 🌐 Web Tile
Screenshots **any website** via Playwright (headless Chromium) and tiles it across buttons. Gap-aware: accounts for physical spacing between buttons so the image aligns correctly.

```
 [weather profile — wttr.in tiled 5×3]

 ┌──────┬──────┬──────┬──────┬──────┐
 │Прогн │погоды│: 59. │000000│,26.00│
 │      │/ Сол │нечно │      │      │
 ├──────┼──────┼──────┼──────┼──────┤
 │  —   │  ) — │ км/ч │      │      │
 │      │  0.0 │  мм  │      │      │
 ├──────┼──────┼──────┼──────┼──────┤
 │Утро  │      │Сол   │      │      │
 │  — ( │      │нечно │      │      │
 └──────┴──────┴──────┴──────┴──────┘
```

```toml
[[slot]]
index        = 0
action       = "chs.deck.webtile"
url          = "https://dzen.ru/"
web_interval = 30.0
web_cols     = 5
web_rows     = 3
web_gap_x    = 8   # physical gap between buttons in px
web_gap_y    = 8
```

### 🔀 Profile Switching
Profiles form a **ring** — arrows cycle through all `.toml` files. Buttons show the actual target profile name.

```
┌─────────┐                    ┌─────────┐
│    ◀    │                    │    ▶    │
│cinematic│                    │  work   │
└─────────┘                    └─────────┘
```

---

## Profile System

Profiles are TOML files in `profiles/`. A profile can reference shared **layers** that are merged on top.

```
profiles/
  work.toml        ← daily driver: apps, VPN, docker
  weather.toml     ← 15 webtile slots (wttr.in / any site)
  cinematic.toml   ← full-screen video across all buttons

layers/
  display.toml     ← right column: CPU%, RAM%, docker count
  nav.toml         ← profile-switch arrows (bottom right)
```

**Priority**: profile slots → layers (layers always override).

### work profile layout

```
       COL 0        COL 1        COL 2        COL 3        COL 4     │  COL 5
     ┌────────────┬────────────┬────────────┬────────────┬───────────┼────────────┐
ROW0 │[0] Cursor  │[1] Obsidian│[2] TickTick│     —      │     —     │[5]  CPU%   │
     ├────────────┼────────────┼────────────┼────────────┼───────────┤    RAM%    │
ROW1 │[6]   VPN   │     —      │     —      │     —      │     —     │[11] Docker │
     ├────────────┼────────────┼────────────┼────────────┼───────────┤            │
ROW2 │     —      │     —      │     —      │ [15]  ◀   │ [16]  ▶  │[17]   —    │
     └────────────┴────────────┴────────────┴────────────┴───────────┴────────────┘
```

---

## Quick Start

```bash
# Build & deploy plugin
just install

# Regenerate all profile JSONs from TOML sources
just gen-profiles

# Full cycle: build + deploy + reload OpenDeck
just release
```

### Requirements

| Tool | Used for |
|------|----------|
| [OpenDeck](https://github.com/ninjas-code-official/opendeck) | deck runtime |
| `ffmpeg` | video frame decoding |
| `yt-dlp` | resolving YouTube / Twitch stream URLs |
| `node` + `/usr/lib/node_modules/playwright` | web tile screenshots |
| Playwright's headless-shell | headless Chromium rendering |

---

## Project Structure

```
chs-deck/
├── src/
│   ├── main.rs                  # plugin entry point, action registry
│   ├── actions/
│   │   ├── stats.rs             # CPU / RAM display
│   │   ├── docker.rs            # container count
│   │   ├── media.rs             # playerctl control
│   │   ├── shell.rs             # shell command on press
│   │   ├── video.rs             # video + yt-dlp stream tiling
│   │   └── webtile.rs           # website screenshot tiling
│   └── bin/
│       └── gen_profile.rs       # TOML → OpenDeck profile JSON
├── profiles/                    # per-profile TOML configs
├── layers/                      # reusable slot overlays
├── assets/                      # button icons (PNG + SVG)
├── screenshot.js                # Playwright screenshot helper (Node)
├── justfile                     # build / deploy / reload tasks
└── prompt-ideas.md              # Claude prompt for use-case brainstorming
```

---

## License

Personal use only.
