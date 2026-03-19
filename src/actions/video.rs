//! Video playback across button screens.
//!
//! Each button instance represents one tile of a video grid.
//! ffmpeg decodes the video → raw RGB frames → split into BTN×BTN tiles
//! → each tile sent as PNG to the corresponding button.
//!
//! Settings (same on every instance in the grid):
//!   path  — absolute path to video file
//!   start — slot index of the top-left corner  (e.g. 0)
//!   cols  — grid width  in buttons             (e.g. 5)
//!   rows  — grid height in buttons             (e.g. 3)
//!   fps   — target fps                         (default 10)
use base64::Engine as _;
use dashmap::DashMap;
use openaction::*;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use tokio::io::AsyncReadExt;

pub const UUID: ActionUuid = "chs.deck.video";

/// Physical button screen size in pixels.
const BTN: u32 = 72;

pub struct VideoPlayer;

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub path: String,
    /// Slot index of the top-left corner of the video grid.
    #[serde(default)]
    pub start: usize,
    #[serde(default = "default_cols")]
    pub cols: usize,
    #[serde(default = "default_rows")]
    pub rows: usize,
    #[serde(default = "default_fps")]
    pub fps: f32,
}

fn default_cols() -> usize { 5 }
fn default_rows() -> usize { 3 }
fn default_fps()  -> f32   { 10.0 }

// instance_id → settings
static INSTANCES: LazyLock<DashMap<String, Settings>> = LazyLock::new(DashMap::new);
// video path → abort handle of its playback task
static TASKS: LazyLock<DashMap<String, tokio::task::AbortHandle>> = LazyLock::new(DashMap::new);

#[async_trait]
impl Action for VideoPlayer {
    const UUID: ActionUuid = UUID;
    type Settings = Settings;

    async fn will_appear(&self, instance: &Instance, settings: &Settings) -> OpenActionResult<()> {
        if settings.path.is_empty() { return Ok(()); }
        INSTANCES.insert(instance.instance_id.clone(), settings.clone());
        ensure_running(settings);
        Ok(())
    }

    async fn will_disappear(&self, instance: &Instance, settings: &Settings) -> OpenActionResult<()> {
        INSTANCES.remove(&instance.instance_id);
        if !INSTANCES.iter().any(|e| e.value().path == settings.path) {
            if let Some((_, handle)) = TASKS.remove(&settings.path) {
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
    if TASKS.contains_key(&settings.path) { return; }
    let task = tokio::spawn(playback_loop(settings.clone()));
    TASKS.insert(settings.path.clone(), task.abort_handle());
}

// ─── Playback loop ────────────────────────────────────────────────────────────

async fn playback_loop(s: Settings) {
    let cols = s.cols.max(1) as u32;
    let rows = s.rows.max(1) as u32;
    let fps  = s.fps.clamp(1.0, 60.0);
    let w = cols * BTN;
    let h = rows * BTN;
    let frame_bytes = (w * h * 3) as usize;
    let frame_delay = tokio::time::Duration::from_secs_f32(1.0 / fps);

    // columns and rows of the top-left button in the deck grid (6 cols per row)
    let start_col = (s.start % 6) as u32;
    let start_row = (s.start / 6) as u32;

    loop {
        let mut child = match tokio::process::Command::new("ffmpeg")
            .args([
                "-stream_loop", "-1",
                "-i", &s.path,
                "-vf", &format!("fps={fps},scale={w}x{h}:flags=lanczos"),
                "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(c)  => c,
            Err(e) => {
                log::error!("video: ffmpeg spawn failed for {}: {e}", s.path);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        let mut stdout = child.stdout.take().unwrap();

        loop {
            let mut buf = vec![0u8; frame_bytes];
            if stdout.read_exact(&mut buf).await.is_err() { break; }

            let img = match image::RgbImage::from_raw(w, h, buf) {
                Some(i) => i,
                None    => continue,
            };

            // Send each tile to its button instance
            for instance in visible_instances(UUID).await {
                let slot = parse_slot(&instance.instance_id);
                let sc = slot % 6;
                let sr = slot / 6;

                // Is this button inside our grid?
                if sc < start_col as usize || sc >= (start_col + cols) as usize { continue; }
                if sr < start_row as usize || sr >= (start_row + rows) as usize { continue; }

                // Does this instance belong to the same video?
                match INSTANCES.get(&instance.instance_id) {
                    Some(cfg) if cfg.path == s.path => {}
                    _ => continue,
                }

                let tile_col = (sc as u32) - start_col;
                let tile_row = (sr as u32) - start_row;
                let tile = image::imageops::crop_imm(&img, tile_col * BTN, tile_row * BTN, BTN, BTN)
                    .to_image();

                let data_url = encode_tile(&tile);
                let _ = instance.set_image(Some(data_url), None).await;
            }

            tokio::time::sleep(frame_delay).await;
        }

        let _ = child.wait().await;
        // brief pause before restarting (e.g. if file disappeared temporarily)
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn parse_slot(context: &str) -> usize {
    // "Keypad.7.0" → 7
    context.split('.').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0)
}

fn encode_tile(tile: &image::RgbImage) -> String {
    use image::ImageEncoder;
    let mut png: Vec<u8> = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(tile.as_raw(), BTN, BTN, image::ExtendedColorType::Rgb8)
        .expect("png encode failed");
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&png)
    )
}
