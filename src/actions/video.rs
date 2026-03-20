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
    /// Physical gap between buttons in pixels (horizontal).
    #[serde(default)]
    pub gap_x: u32,
    /// Physical gap between buttons in pixels (vertical).
    #[serde(default)]
    pub gap_y: u32,
    /// Slot index of THIS button in the deck grid (set by gen_profile).
    #[serde(default)]
    pub slot_index: usize,
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
        let _ = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/chs-deck-video.log")
            .and_then(|mut f| { use std::io::Write;
                writeln!(f, "id={:?} slot_index={} parsed={}", instance.instance_id, settings.slot_index, parse_slot(&instance.instance_id)) });
        log::info!("video will_appear: id={:?} slot_index={}", instance.instance_id, settings.slot_index);
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
    let cols  = s.cols.max(1) as u32;
    let rows  = s.rows.max(1) as u32;
    let fps   = s.fps.clamp(1.0, 60.0);
    let gap_x = s.gap_x;
    let gap_y = s.gap_y;

    // Full canvas size including physical gaps between buttons
    let w = cols * BTN + (cols - 1) * gap_x;
    let h = rows * BTN + (rows - 1) * gap_y;

    let frame_bytes = (w * h * 3) as usize;
    let frame_delay = tokio::time::Duration::from_secs_f32(1.0 / fps);

    // columns and rows of the top-left button in the deck grid (6 cols per row)
    let start_col = (s.start % 6) as u32;
    let start_row = (s.start / 6) as u32;

    loop {
        // If path is a URL, resolve the actual stream URL via yt-dlp
        let stream_url = if s.path.starts_with("http://") || s.path.starts_with("https://") {
            match resolve_stream_url(&s.path).await {
                Some(u) => u,
                None => {
                    log::error!("video: yt-dlp failed for {}", s.path);
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                    continue;
                }
            }
        } else {
            s.path.clone()
        };

        // For live streams don't loop (-stream_loop only makes sense for files)
        let is_url = stream_url.starts_with("http");
        let mut cmd = tokio::process::Command::new("ffmpeg");
        if !is_url { cmd.args(["-stream_loop", "-1"]); }
        cmd.args([
            "-i", &stream_url,
            "-vf", &format!("fps={fps},scale={w}x{h}:flags=lanczos"),
            "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1",
        ]);
        let mut child = match cmd
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

            // 1. Encode all tiles synchronously (CPU-bound, no blocking awaits)
            let mut sends: Vec<(std::sync::Arc<openaction::Instance>, String)> = Vec::new();
            for instance in visible_instances(UUID).await {
                match INSTANCES.get(&instance.instance_id) {
                    Some(c) if c.path == s.path => {}
                    _ => continue,
                };
                let slot = parse_slot(&instance.instance_id);
                let sc = slot % 6;
                let sr = slot / 6;

                if sc < start_col as usize || sc >= (start_col + cols) as usize { continue; }
                if sr < start_row as usize || sr >= (start_row + rows) as usize { continue; }

                let tile_col = (sc as u32) - start_col;
                let tile_row = (sr as u32) - start_row;
                let x = tile_col * (BTN + gap_x);
                let y = tile_row * (BTN + gap_y);
                let tile = image::imageops::crop_imm(&img, x, y, BTN, BTN).to_image();
                sends.push((instance, encode_tile(&tile)));
            }

            // 2. Fire all set_image calls concurrently — minimise inter-tile lag
            let futs = sends.into_iter().map(|(inst, data)| async move {
                let _ = inst.set_image(Some(data), None).await;
            });
            futures_util::future::join_all(futs).await;

            tokio::time::sleep(frame_delay).await;
        }

        let _ = child.wait().await;
        // brief pause before restarting (e.g. if file disappeared temporarily)
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Resolve a YouTube/Twitch/etc URL to a direct stream URL using yt-dlp.
async fn resolve_stream_url(url: &str) -> Option<String> {
    let out = tokio::process::Command::new("yt-dlp")
        .args(["-f", "best[ext=mp4]/best", "-g", "--no-playlist", url])
        .output()
        .await
        .ok()?;
    if !out.status.success() { return None; }
    let resolved = String::from_utf8(out.stdout).ok()?;
    Some(resolved.trim().to_string())
}

fn parse_slot(context: &str) -> usize {
    // "99-4250D2781310.cinematic.Keypad.7.0" → 7
    let parts: Vec<&str> = context.split('.').collect();
    for i in 0..parts.len().saturating_sub(1) {
        if parts[i] == "Keypad" {
            return parts[i + 1].parse().unwrap_or(0);
        }
    }
    0
}

fn encode_tile(tile: &image::RgbImage) -> String {
    let mut jpg: Vec<u8> = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 75)
        .encode_image(tile)
        .expect("jpeg encode failed");
    format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&jpg)
    )
}
