//! Video playback across button screens.
//!
//! Pre-render approach: ffmpeg extracts ALL frames once → tiles are cropped,
//! JPEG-encoded and base64'd → stored in memory. Playback loop just sends
//! pre-built strings on a timer. Runtime CPU ≈ 0%.
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
    #[serde(default)]
    pub start: usize,
    #[serde(default = "default_cols")]
    pub cols: usize,
    #[serde(default = "default_rows")]
    pub rows: usize,
    #[serde(default = "default_fps")]
    pub fps: f32,
    #[serde(default)]
    pub gap_x: u32,
    #[serde(default)]
    pub gap_y: u32,
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

    async fn key_down(&self, instance: &Instance, settings: &Settings) -> OpenActionResult<()> {
        log::info!("video key_down: id={:?} path={}", instance.instance_id, settings.path);
        if settings.path.is_empty() { return Ok(()); }

        let mut old_paths: Vec<String> = Vec::new();
        for mut entry in INSTANCES.iter_mut() {
            if entry.value().path != settings.path
                || entry.value().fps != settings.fps
                || entry.value().gap_x != settings.gap_x
                || entry.value().gap_y != settings.gap_y
            {
                old_paths.push(entry.value().path.clone());
                entry.value_mut().path  = settings.path.clone();
                entry.value_mut().fps   = settings.fps;
                entry.value_mut().gap_x = settings.gap_x;
                entry.value_mut().gap_y = settings.gap_y;
            }
        }

        if !old_paths.is_empty() {
            for p in &old_paths {
                if let Some((_, handle)) = TASKS.remove(p) { handle.abort(); }
            }
            if let Some((_, handle)) = TASKS.remove(&settings.path) { handle.abort(); }
            ensure_running(settings);

            if let Some(profile_name) = parse_profile(&instance.instance_id) {
                update_profile_toml(&profile_name, &settings.path, settings.fps, settings.gap_x, settings.gap_y);
            }
            log::info!("video: propagated from key_down");
        }
        Ok(())
    }
}

fn ensure_running(settings: &Settings) {
    if TASKS.contains_key(&settings.path) { return; }
    let task = tokio::spawn(playback_loop(settings.clone()));
    TASKS.insert(settings.path.clone(), task.abort_handle());
}

// ─── Pre-render + playback ───────────────────────────────────────────────────

/// One frame = Vec of (tile_col, tile_row, base64_data_url)
type Frame = Vec<(u32, u32, String)>;

async fn playback_loop(s: Settings) {
    let is_url = s.path.starts_with("http://") || s.path.starts_with("https://");
    if is_url {
        playback_realtime(s).await;
    } else {
        playback_cached(s).await;
    }
}

/// Streaming / live: ffmpeg runs continuously, frames sent as decoded
async fn playback_realtime(s: Settings) {
    let cols  = s.cols.max(1) as u32;
    let rows  = s.rows.max(1) as u32;
    let fps   = s.fps.clamp(1.0, 30.0);
    let gap_x = s.gap_x;
    let gap_y = s.gap_y;
    let w = cols * BTN + (cols - 1) * gap_x;
    let h = rows * BTN + (rows - 1) * gap_y;
    let frame_bytes = (w * h * 3) as usize;
    let frame_delay = tokio::time::Duration::from_secs_f32(1.0 / fps);
    let start_col = (s.start % 6) as u32;
    let start_row = (s.start / 6) as u32;

    loop {
        let stream_url = match resolve_stream_url(&s.path).await {
            Some(u) => u,
            None => {
                log::error!("video: yt-dlp failed for {}", s.path);
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                continue;
            }
        };

        log::info!("video: streaming {}", s.path);
        let mut cmd = tokio::process::Command::new("ffmpeg");
        cmd.args([
            "-threads", "1",
            "-hwaccel", "cuda", "-hwaccel_output_format", "cuda",
            "-c:v", "h264_cuvid",
            "-resize", &format!("{w}x{h}"),
            "-i", &stream_url,
            "-vf", &format!("fps={fps},hwdownload,format=nv12"),
            "-map", "0:v", "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1",
            "-map", "0:a?", "-f", "pulse", "deck-video",
        ]);
        let mut child = match cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                log::error!("video: ffmpeg spawn failed: {e}");
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        let mut stdout = child.stdout.take().unwrap();
        let mut buf = vec![0u8; frame_bytes];

        loop {
            if stdout.read_exact(&mut buf).await.is_err() { break; }
            let img = match image::RgbImage::from_raw(w, h, buf.clone()) {
                Some(i) => i,
                None => continue,
            };
            send_frame(&s, &img, cols, rows, gap_x, gap_y, start_col, start_row).await;
            tokio::time::sleep(frame_delay).await;
        }

        let _ = child.wait().await;
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }
}

/// Local file: pre-render ALL frames, then loop from memory (0% CPU)
async fn playback_cached(s: Settings) {
    let cols  = s.cols.max(1) as u32;
    let rows  = s.rows.max(1) as u32;
    let fps   = s.fps.clamp(1.0, 30.0);
    let gap_x = s.gap_x;
    let gap_y = s.gap_y;
    let w = cols * BTN + (cols - 1) * gap_x;
    let h = rows * BTN + (rows - 1) * gap_y;
    let frame_bytes = (w * h * 3) as usize;
    let frame_delay = tokio::time::Duration::from_secs_f32(1.0 / fps);
    let start_col = (s.start % 6) as u32;
    let start_row = (s.start / 6) as u32;

    log::info!("video: pre-rendering {} ...", s.path);

    let mut cmd = tokio::process::Command::new("ffmpeg");
    cmd.args([
        "-threads", "2", "-hwaccel", "auto",
        "-i", &s.path,
        "-vf", &format!("fps={fps},scale={w}x{h}:flags=fast_bilinear"),
        "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1",
    ]);
    let mut child = match cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("video: ffmpeg spawn failed: {e}");
            return;
        }
    };

    let mut stdout = child.stdout.take().unwrap();
    let mut buf = vec![0u8; frame_bytes];
    let mut frames: Vec<Frame> = Vec::new();

    while stdout.read_exact(&mut buf).await.is_ok() {
        let img = match image::RgbImage::from_raw(w, h, buf.clone()) {
            Some(i) => i,
            None => continue,
        };
        let mut frame: Frame = Vec::with_capacity((cols * rows) as usize);
        for tc in 0..cols {
            for tr in 0..rows {
                let x = tc * (BTN + gap_x);
                let y = tr * (BTN + gap_y);
                let tile = image::imageops::crop_imm(&img, x, y, BTN, BTN).to_image();
                frame.push((tc, tr, encode_tile(&tile)));
            }
        }
        frames.push(frame);
    }
    let _ = child.wait().await;

    if frames.is_empty() {
        log::error!("video: no frames decoded from {}", s.path);
        return;
    }

    let mem_kb: usize = frames.iter().map(|f| f.iter().map(|(_, _, d)| d.len()).sum::<usize>()).sum::<usize>() / 1024;
    log::info!("video: cached {} frames ({} KB) — 0% CPU playback", frames.len(), mem_kb);

    loop {
        for frame in &frames {
            let mut sends: Vec<(std::sync::Arc<Instance>, &str)> = Vec::new();
            for instance in visible_instances(UUID).await {
                match INSTANCES.get(&instance.instance_id) {
                    Some(c) if c.path == s.path => {}
                    _ => continue,
                };
                let slot = parse_slot(&instance.instance_id);
                let sc = (slot % 6) as u32;
                let sr = (slot / 6) as u32;
                if sc < start_col || sc >= start_col + cols { continue; }
                if sr < start_row || sr >= start_row + rows { continue; }
                let tc = sc - start_col;
                let tr = sr - start_row;
                if let Some((_, _, data)) = frame.iter().find(|(c, r, _)| *c == tc && *r == tr) {
                    sends.push((instance, data.as_str()));
                }
            }
            let futs = sends.into_iter().map(|(inst, data)| {
                let d = data.to_string();
                async move { let _ = inst.set_image(Some(d), None).await; }
            });
            futures_util::future::join_all(futs).await;
            tokio::time::sleep(frame_delay).await;
        }
    }
}

/// Encode and send one frame to all matching instances
async fn send_frame(s: &Settings, img: &image::RgbImage, cols: u32, rows: u32, gap_x: u32, gap_y: u32, start_col: u32, start_row: u32) {
    let mut sends: Vec<(std::sync::Arc<Instance>, String)> = Vec::new();
    for instance in visible_instances(UUID).await {
        match INSTANCES.get(&instance.instance_id) {
            Some(c) if c.path == s.path => {}
            _ => continue,
        };
        let slot = parse_slot(&instance.instance_id);
        let sc = (slot % 6) as u32;
        let sr = (slot / 6) as u32;
        if sc < start_col || sc >= start_col + cols { continue; }
        if sr < start_row || sr >= start_row + rows { continue; }
        let tile_col = sc - start_col;
        let tile_row = sr - start_row;
        let x = tile_col * (BTN + gap_x);
        let y = tile_row * (BTN + gap_y);
        let tile = image::imageops::crop_imm(img, x, y, BTN, BTN).to_image();
        sends.push((instance, encode_tile(&tile)));
    }
    let futs = sends.into_iter().map(|(inst, data)| async move {
        let _ = inst.set_image(Some(data), None).await;
    });
    futures_util::future::join_all(futs).await;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

async fn resolve_stream_url(url: &str) -> Option<String> {
    let out = tokio::process::Command::new("yt-dlp")
        .args(["-f", "worst[height>=240][vcodec^=avc1][ext=mp4]/worst[height>=240][ext=mp4]/worst",
               "-g", "--no-playlist", url])
        .output()
        .await
        .ok()?;
    if !out.status.success() { return None; }
    let resolved = String::from_utf8(out.stdout).ok()?;
    Some(resolved.trim().to_string())
}

fn parse_profile(instance_id: &str) -> Option<String> {
    let parts: Vec<&str> = instance_id.split('.').collect();
    if parts.len() >= 2 { Some(parts[1].to_string()) } else { None }
}

fn update_profile_toml(profile_name: &str, path: &str, fps: f32, gap_x: u32, gap_y: u32) {
    let toml_path = format!("profiles/{profile_name}.toml");
    let content = match std::fs::read_to_string(&toml_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut doc = match content.parse::<toml_edit::DocumentMut>() {
        Ok(d) => d,
        Err(_) => return,
    };
    if doc.contains_key("video_path") {
        doc["video_path"] = toml_edit::value(path);
    }
    if let Some(slots) = doc.get_mut("slot").and_then(|v| v.as_array_of_tables_mut()) {
        for slot in slots.iter_mut() {
            if slot.get("action").and_then(|v| v.as_str()) == Some("chs.deck.video") {
                slot["video_path"] = toml_edit::value(path);
                slot["video_fps"] = toml_edit::value(fps as f64);
                slot["video_gap_x"] = toml_edit::value(gap_x as i64);
                slot["video_gap_y"] = toml_edit::value(gap_y as i64);
            }
        }
    }
    let _ = std::fs::write(&toml_path, doc.to_string());
    log::info!("video: updated {toml_path}");
}

fn parse_slot(context: &str) -> usize {
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
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 50)
        .encode_image(tile)
        .expect("jpeg encode failed");
    format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&jpg)
    )
}
