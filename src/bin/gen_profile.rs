/// Reads `profiles/<name>.toml` and writes an OpenDeck profile JSON to stdout.
///
/// Priority (низший → высший): [[slot]] → layers
/// layers всегда перекрывают слоты профиля.
///
/// Usage:  cargo run --bin gen_profile -- work
///         just gen-profile work
use serde::Deserialize;
use serde_json::{Value, json};

// ─── TOML schema ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ProfileDef {
    name: String,
    /// Layers applied last — always override [[slot]].
    #[serde(default)]
    layers: Vec<String>,
    /// Profile's own slots (lowest priority).
    #[serde(default)]
    slot: Vec<SlotDef>,
}

#[derive(Deserialize)]
struct LayerDef {
    #[serde(default)]
    slot: Vec<SlotDef>,
}

#[derive(Deserialize)]
struct SlotDef {
    index: usize,
    /// "chs.deck.stats"          stat = "cpu"|"ram"|"docker"
    /// "chs.deck.docker.count"
    /// "chs.deck.media"          media_action = "toggle"|"next"|"prev"|"stop"
    /// "chs.deck.shell"          command = "..."  label = "..."
    /// "chs.deck.video"          video_path = "..."  video_start = N  video_cols = N  video_rows = N  video_fps = N
    /// "switch-profile-prev"
    /// "switch-profile-next"
    action: String,
    stat: Option<String>,
    media_action: Option<String>,
    command: Option<String>,
    label: Option<String>,
    // video
    video_path: Option<String>,
    video_start: Option<usize>,
    video_cols: Option<usize>,
    video_rows: Option<usize>,
    video_fps: Option<f32>,
    video_gap_x: Option<u32>,
    video_gap_y: Option<u32>,
    // webtile
    url: Option<String>,
    web_interval: Option<f32>,
    web_start: Option<usize>,
    web_cols: Option<usize>,
    web_rows: Option<usize>,
    web_gap_x: Option<u32>,
    web_gap_y: Option<u32>,
    // desktop-entry
    app: Option<String>,
}

// ─── Constants ────────────────────────────────────────────────────────────────

const PLUGIN:              &str = "chs.deck.sdPlugin";
const PLUGIN_DIR:          &str = "plugins/chs.deck.sdPlugin";
const STARTERPACK:         &str = "com.amansprojects.starterpack.sdPlugin";
const STARTERPACK_DIR:     &str = "plugins/com.amansprojects.starterpack.sdPlugin";
const DESKTOP_ENTRY:       &str = "me.amankhanna.oadesktopentry.sdPlugin";
const DESKTOP_ENTRY_DIR:   &str = "plugins/me.amankhanna.oadesktopentry.sdPlugin";
/// Device ID from the profiles directory name.
const DEVICE_ID: &str = "99-4250D2781310";

// ─── Builders ─────────────────────────────────────────────────────────────────

fn state(image: &str, text: &str, colour: &str) -> Value {
    json!({
        "alignment": "middle",
        "background_colour": "#000000",
        "colour": colour,
        "family": "Liberation Sans",
        "image": image,
        "name": "",
        "show": true,
        "size": 28,
        "stroke_colour": "#000000",
        "stroke_size": 3,
        "style": "Regular",
        "text": text,
        "underline": false
    })
}

fn key(index: usize, uuid: &str, name: &str, plugin: &str, icon: &str, states: Vec<Value>, settings: Value) -> Value {
    json!({
        "action": {
            "controllers": ["Keypad"],
            "disable_automatic_states": false,
            "icon": icon,
            "name": name,
            "plugin": plugin,
            "property_inspector": "",
            "states": states,
            "supported_in_multi_actions": true,
            "tooltip": name,
            "uuid": uuid,
            "visible_in_action_list": true
        },
        "children": null,
        "context": format!("Keypad.{index}.0"),
        "current_state": 0,
        "settings": settings,
        "states": states
    })
}

fn stats_key(index: usize, stat: &str) -> Value {
    let icon = format!("{PLUGIN_DIR}/assets/stats-{stat}@2x.png");
    let states = vec![state(&icon, "", "#7dd3fc")];
    key(index, "chs.deck.stats", &format!("Stats: {stat}"), PLUGIN, &icon, states, json!({ "stat": stat }))
}

fn docker_count_key(index: usize) -> Value {
    let icon = format!("{PLUGIN_DIR}/assets/docker-count@2x.png");
    let states = vec![state(&icon, "--", "#7dd3fc")];
    key(index, "chs.deck.docker.count", "Docker: Container Count", PLUGIN, &icon, states, json!({}))
}

fn media_key(index: usize, media_action: &str) -> Value {
    let (icon_name, label) = match media_action {
        "prev"   => ("media-prev", "⏮ Prev"),
        "next"   => ("media-next", "⏭ Next"),
        "stop"   => ("media-stop", "⏹ Stop"),
        _        => ("media-play", "▶/⏸"),
    };
    let icon = format!("{PLUGIN_DIR}/assets/{icon_name}@2x.png");
    let states = vec![state(&icon, label, "#f0abfc")];
    key(index, "chs.deck.media", label, PLUGIN, &icon, states, json!({ "action": media_action }))
}

fn shell_key(index: usize, command: &str, label: Option<&str>) -> Value {
    let icon = format!("{PLUGIN_DIR}/assets/shell@2x.png");
    let truncated = if command.len() > 12 { &command[..12] } else { command };
    let display = label.unwrap_or(truncated);
    let states = vec![state(&icon, display, "#22c55e")];
    key(index, "chs.deck.shell", display, PLUGIN, &icon, states, json!({ "cmd": command }))
}

fn desktop_entry_key(index: usize, app: &str) -> Value {
    let icon = format!("{DESKTOP_ENTRY_DIR}/icon.png");
    let s = json!({
        "alignment": "middle",
        "background_colour": "#000000",
        "colour": "#FFFFFF",
        "family": "Liberation Sans",
        "image": icon,
        "name": "",
        "show": true,
        "size": 16,
        "stroke_colour": "#000000",
        "stroke_size": 3,
        "style": "Regular",
        "text": "",
        "underline": false
    });
    json!({
        "action": {
            "controllers": ["Keypad", "Encoder"],
            "disable_automatic_states": false,
            "icon": icon,
            "name": "Launch App",
            "plugin": DESKTOP_ENTRY,
            "property_inspector": format!("{DESKTOP_ENTRY_DIR}/pi/launchapp.html"),
            "states": [s],
            "supported_in_multi_actions": true,
            "tooltip": "Launch an application",
            "uuid": "me.amankhanna.oadesktopentry.launchapp",
            "visible_in_action_list": true
        },
        "children": null,
        "context": format!("Keypad.{index}.0"),
        "current_state": 0,
        "settings": { "app": app },
        "states": [s]
    })
}

fn switch_profile_key(index: usize, target: &str, icon_file: &str) -> Value {
    let icon = format!("{PLUGIN_DIR}/assets/{icon_file}");
    let s = json!({
        "alignment": "bottom",
        "background_colour": "#000000",
        "colour": "#94a3b8",
        "family": "Liberation Sans",
        "image": icon,
        "name": "",
        "show": true,
        "size": 14,
        "stroke_colour": "#000000",
        "stroke_size": 2,
        "style": "Regular",
        "text": target,
        "underline": false
    });
    json!({
        "action": {
            "controllers": ["Keypad", "Encoder"],
            "disable_automatic_states": false,
            "icon": icon,
            "name": "Switch Profile",
            "plugin": STARTERPACK,
            "property_inspector": format!("{STARTERPACK_DIR}/propertyInspector/switchProfile.html"),
            "states": [s],
            "supported_in_multi_actions": true,
            "tooltip": format!("→ {target}"),
            "uuid": "com.amansprojects.starterpack.switchprofile",
            "visible_in_action_list": true
        },
        "children": null,
        "context": format!("Keypad.{index}.0"),
        "current_state": 0,
        "settings": { "device": DEVICE_ID, "profile": target },
        "states": [s]
    })
}

// ─── Slot dispatcher ──────────────────────────────────────────────────────────

fn build_slot(slot: &SlotDef, prev: &str, next: &str) -> Value {
    let i = slot.index;
    assert!(i < 18, "slot index {i} out of range 0..17");
    match slot.action.as_str() {
        "chs.deck.stats"        => stats_key(i, slot.stat.as_deref().unwrap_or("cpu")),
        "chs.deck.docker.count" => docker_count_key(i),
        "chs.deck.media"        => media_key(i, slot.media_action.as_deref().unwrap_or("toggle")),
        "chs.deck.shell"        => shell_key(i, slot.command.as_deref().unwrap_or(""), slot.label.as_deref()),
        "chs.deck.video" => {
            let path  = slot.video_path.as_deref().unwrap_or("");
            let start = slot.video_start.unwrap_or(0);
            let cols  = slot.video_cols.unwrap_or(5);
            let rows  = slot.video_rows.unwrap_or(3);
            let fps   = slot.video_fps.unwrap_or(10.0);
            let gap_x = slot.video_gap_x.unwrap_or(0);
            let gap_y = slot.video_gap_y.unwrap_or(0);
            video_key(i, path, start, cols, rows, fps, gap_x, gap_y)
        }
        "chs.deck.webtile" => {
            let url      = slot.url.as_deref().unwrap_or("");
            let interval = slot.web_interval.unwrap_or(30.0);
            let start    = slot.web_start.unwrap_or(0);
            let cols     = slot.web_cols.unwrap_or(5);
            let rows     = slot.web_rows.unwrap_or(3);
            let gap_x    = slot.web_gap_x.unwrap_or(0);
            let gap_y    = slot.web_gap_y.unwrap_or(0);
            webtile_key(i, url, interval, start, cols, rows, gap_x, gap_y)
        }
        "desktop-entry"       => desktop_entry_key(i, slot.app.as_deref().unwrap_or("")),
        "switch-profile-prev" => switch_profile_key(i, prev, "switch-prev@2x.png"),
        "switch-profile-next" => switch_profile_key(i, next, "switch-next@2x.png"),
        other => panic!("Unknown action \"{other}\" at slot {i}"),
    }
}

fn video_key(index: usize, path: &str, start: usize, cols: usize, rows: usize, fps: f32, gap_x: u32, gap_y: u32) -> Value {
    let icon = format!("{PLUGIN_DIR}/assets/media-play@2x.png");
    let states = vec![state(&icon, "", "#f0abfc")];
    key(index, "chs.deck.video", "Video Tile", PLUGIN, &icon, states,
        json!({ "path": path, "start": start, "cols": cols, "rows": rows, "fps": fps,
                "gap_x": gap_x, "gap_y": gap_y, "slot_index": index }))
}

fn webtile_key(index: usize, url: &str, interval: f32, start: usize, cols: usize, rows: usize, gap_x: u32, gap_y: u32) -> Value {
    let icon = format!("{PLUGIN_DIR}/assets/shell@2x.png");
    let states = vec![state(&icon, "", "#7dd3fc")];
    key(index, "chs.deck.webtile", "Web Tile", PLUGIN, &icon, states,
        json!({ "url": url, "interval": interval, "start": start, "cols": cols, "rows": rows,
                "gap_x": gap_x, "gap_y": gap_y, "slot_index": index }))
}

fn apply_layer(name: &str, keys: &mut [Value], prev: &str, next: &str) {
    let path = format!("layers/{name}.toml");
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("Cannot read layer {path}"));
    let layer: LayerDef = toml::from_str(&src)
        .unwrap_or_else(|e| panic!("Parse error in {path}: {e}"));
    for slot in &layer.slot {
        keys[slot.index] = build_slot(slot, prev, next);
    }
}

// ─── Profile ring ─────────────────────────────────────────────────────────────

fn scan_profiles() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir("profiles")
        .expect("Cannot read profiles/")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.ends_with(".toml").then(|| n[..n.len() - 5].to_string())
        })
        .collect();
    names.sort();
    names
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let profile_name = std::env::args().nth(1).unwrap_or_else(|| "work".to_string());

    // Build ring from all profiles
    let all = scan_profiles();
    let n   = all.len().max(1);
    let idx = all.iter().position(|p| p == &profile_name).unwrap_or(0);
    let prev = &all[(idx + n - 1) % n];
    let next = &all[(idx + 1)     % n];

    eprintln!("Ring: {} ← [{}] → {}  ({} total)", prev, profile_name, next, n);

    // Load profile TOML
    let path = format!("profiles/{profile_name}.toml");
    let src  = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("Cannot read {path}"));
    let def: ProfileDef = toml::from_str(&src)
        .unwrap_or_else(|e| panic!("Parse error in {path}: {e}"));

    let mut keys: Vec<Value> = vec![Value::Null; 18];

    // 1. Profile slots — lowest priority
    for slot in &def.slot {
        keys[slot.index] = build_slot(slot, prev, next);
    }

    // 2. Layers — always on top
    for layer_name in &def.layers {
        apply_layer(layer_name, &mut keys, prev, next);
    }

    let total = keys.iter().filter(|k| !k.is_null()).count();
    eprintln!("Profile \"{}\" — {total} slots  [{} profile + {} layers]",
        def.name, def.slot.len(), def.layers.len());
    println!("{}", serde_json::to_string_pretty(&json!({ "keys": keys, "sliders": [] })).unwrap());
}
