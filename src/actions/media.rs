use dashmap::DashMap;
use openaction::*;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

pub const UUID: ActionUuid = "chs.deck.media";

pub struct Media;

/// "toggle" | "next" | "prev" | "stop"
#[derive(Default, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub action: String,
}

static INSTANCES: LazyLock<DashMap<String, Settings>> = LazyLock::new(DashMap::new);

#[async_trait]
impl Action for Media {
    const UUID: ActionUuid = UUID;
    type Settings = Settings;

    async fn will_appear(&self, instance: &Instance, settings: &Settings) -> OpenActionResult<()> {
        INSTANCES.insert(instance.instance_id.clone(), settings.clone());
        refresh_instance(instance).await
    }

    async fn will_disappear(&self, instance: &Instance, _: &Settings) -> OpenActionResult<()> {
        INSTANCES.remove(&instance.instance_id);
        Ok(())
    }

    async fn did_receive_settings(&self, instance: &Instance, settings: &Settings) -> OpenActionResult<()> {
        INSTANCES.insert(instance.instance_id.clone(), settings.clone());
        Ok(())
    }

    async fn key_up(&self, instance: &Instance, settings: &Settings) -> OpenActionResult<()> {
        let cmd = match settings.action.as_str() {
            "next" => "next",
            "prev" => "previous",
            "stop" => "stop",
            _      => "play-pause",
        };
        let _ = tokio::process::Command::new("playerctl").arg(cmd).output().await;
        // небольшая пауза чтоб статус обновился
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        refresh_instance(instance).await
    }
}

// ─── helpers ──────────────────────────────────────────────────────────────────

async fn refresh_instance(instance: &Instance) -> OpenActionResult<()> {
    let title = build_title().await;
    instance.set_title(Some(title), None).await
}

async fn build_title() -> String {
    let status = playerctl(&["status"]).await;
    let title  = playerctl(&["metadata", "title"]).await;
    let artist = playerctl(&["metadata", "artist"]).await;

    let icon = match status.trim() {
        "Playing" => "▶",
        "Paused"  => "⏸",
        _         => "⏹",
    };

    // обрезаем до 10 символов
    let trunc = |s: &str, n: usize| -> String {
        let s = s.trim();
        if s.chars().count() > n {
            format!("{}…", s.chars().take(n).collect::<String>())
        } else {
            s.to_string()
        }
    };

    let t = trunc(&title, 10);
    let a = trunc(&artist, 10);

    if t.is_empty() && a.is_empty() {
        format!("{icon}\n—")
    } else if a.is_empty() {
        format!("{icon}\n{t}")
    } else {
        format!("{icon}\n{t}\n{a}")
    }
}

async fn playerctl(args: &[&str]) -> String {
    tokio::process::Command::new("playerctl")
        .args(args)
        .output()
        .await
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

// ─── polling ──────────────────────────────────────────────────────────────────

pub async fn polling_loop() {
    let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(3));
    loop {
        ticker.tick().await;
        if INSTANCES.is_empty() {
            continue;
        }
        let title = build_title().await;
        for instance in visible_instances(UUID).await {
            let _ = instance.set_title(Some(title.clone()), None).await;
        }
    }
}
