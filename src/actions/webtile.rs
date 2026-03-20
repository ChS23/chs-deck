//! Renders a web page as a tiled image across deck buttons.
//!
//! Uses Playwright (headless Chromium via Node.js) to screenshot a URL,
//! then tiles the result across a grid of buttons — same approach as video.rs.
//!
//! Settings (same on every instance in the grid):
//!   url       — page to render
//!   interval  — refresh every N seconds   (default 30)
//!   start     — top-left slot index       (default 0)
//!   cols      — grid width in buttons     (default 5)
//!   rows      — grid height in buttons    (default 3)
use base64::Engine as _;
use dashmap::DashMap;
use openaction::*;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

pub const UUID: ActionUuid = "chs.deck.webtile";

const BTN: u32 = 72;

pub struct WebTile;

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub url: String,
    #[serde(default = "default_interval")]
    pub interval: f32,
    #[serde(default)]
    pub start: usize,
    #[serde(default = "default_cols")]
    pub cols: usize,
    #[serde(default = "default_rows")]
    pub rows: usize,
    /// Physical gap between buttons in pixels (horizontal).
    #[serde(default)]
    pub gap_x: u32,
    /// Physical gap between buttons in pixels (vertical).
    #[serde(default)]
    pub gap_y: u32,
    /// Kept for backwards compatibility but not used for tiling — we use parse_slot() instead.
    #[serde(default)]
    pub slot_index: usize,
}

fn default_interval() -> f32 { 30.0 }
fn default_cols()     -> usize { 5 }
fn default_rows()     -> usize { 3 }

static INSTANCES: LazyLock<DashMap<String, Settings>> = LazyLock::new(DashMap::new);
static TASKS:     LazyLock<DashMap<String, tokio::task::AbortHandle>> = LazyLock::new(DashMap::new);

#[async_trait]
impl Action for WebTile {
    const UUID: ActionUuid = UUID;
    type Settings = Settings;

    async fn will_appear(&self, instance: &Instance, settings: &Settings) -> OpenActionResult<()> {
        if settings.url.is_empty() { return Ok(()); }
        INSTANCES.insert(instance.instance_id.clone(), settings.clone());
        ensure_running(settings);
        Ok(())
    }

    async fn will_disappear(&self, instance: &Instance, settings: &Settings) -> OpenActionResult<()> {
        INSTANCES.remove(&instance.instance_id);
        if !INSTANCES.iter().any(|e| e.value().url == settings.url) {
            if let Some((_, handle)) = TASKS.remove(&settings.url) {
                handle.abort();
            }
        }
        Ok(())
    }

    async fn did_receive_settings(&self, instance: &Instance, settings: &Settings) -> OpenActionResult<()> {
        INSTANCES.insert(instance.instance_id.clone(), settings.clone());
        ensure_running(settings);
        Ok(())
    }
}

fn ensure_running(settings: &Settings) {
    if TASKS.contains_key(&settings.url) { return; }
    let task = tokio::spawn(refresh_loop(settings.clone()));
    TASKS.insert(settings.url.clone(), task.abort_handle());
}

/// Extract slot index from the instance context string, e.g. "...Keypad.7.0" → 7.
fn parse_slot(context: &str) -> usize {
    let parts: Vec<&str> = context.split('.').collect();
    for i in 0..parts.len().saturating_sub(1) {
        if parts[i] == "Keypad" {
            return parts[i + 1].parse().unwrap_or(0);
        }
    }
    0
}

async fn refresh_loop(s: Settings) {
    let cols = s.cols.max(1) as u32;
    let rows = s.rows.max(1) as u32;
    let gap_x = s.gap_x;
    let gap_y = s.gap_y;
    // Full canvas including physical gaps between buttons (same logic as video.rs)
    let w = cols * BTN + (cols - 1) * gap_x;
    let h = rows * BTN + (rows - 1) * gap_y;
    let start_col = (s.start % 6) as u32;
    let start_row = (s.start / 6) as u32;
    let interval = tokio::time::Duration::from_secs_f32(s.interval.max(5.0));
    let tmp = format!("/tmp/chs-deck-webtile-{}.png", url_hash(&s.url));

    // screenshot.js lives next to the plugin binary
    let script = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("screenshot.js")))
        .unwrap_or_else(|| std::path::PathBuf::from("screenshot.js"));

    loop {
        let ok = tokio::process::Command::new("node")
            .args([
                script.to_str().unwrap_or("screenshot.js"),
                &s.url,
                &w.to_string(),
                &h.to_string(),
                &tmp,
            ])
            .stderr(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .status()
            .await
            .map(|st| st.success())
            .unwrap_or(false);

        if ok {
            match image::open(&tmp) {
                Ok(img) => {
                    let rgb = img.to_rgb8();
                    let instances = visible_instances(UUID).await;
                    for instance in instances {
                        let cfg = match INSTANCES.get(&instance.instance_id) {
                            Some(c) if c.url == s.url => c,
                            _ => continue,
                        };
                        let slot = parse_slot(&instance.instance_id);
                        let sc = (slot % 6) as u32;
                        let sr = (slot / 6) as u32;
                        if sc < start_col || sc >= start_col + cols { continue; }
                        if sr < start_row || sr >= start_row + rows { continue; }
                        let _ = cfg; // settings verified, slot from context
                        let tile_col = sc - start_col;
                        let tile_row = sr - start_row;
                        let tile = image::imageops::crop_imm(
                            &rgb,
                            tile_col * (BTN + gap_x),
                            tile_row * (BTN + gap_y),
                            BTN, BTN,
                        ).to_image();
                        let data = encode_tile(&tile);
                        let _ = instance.set_title(None::<String>, None).await;
                        let _ = instance.set_image(Some(data), None).await;
                    }
                }
                Err(e) => log::error!("webtile: image open failed: {e}"),
            }
        } else {
            log::warn!("webtile: screenshot failed for {}", s.url);
        }

        tokio::time::sleep(interval).await;
    }
}

fn url_hash(url: &str) -> u64 {
    use std::hash::Hash;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut h);
    std::hash::Hasher::finish(&h)
}

fn encode_tile(tile: &image::RgbImage) -> String {
    let mut jpg: Vec<u8> = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 75)
        .encode_image(tile)
        .expect("jpeg encode");
    format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&jpg)
    )
}
