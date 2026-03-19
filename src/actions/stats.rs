use dashmap::DashMap;
use openaction::*;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use tokio::sync::RwLock;

pub const UUID: ActionUuid = "chs.deck.stats";

pub struct Stats;

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct Settings {
    /// "cpu" | "ram" | "docker"
    pub stat: String,
}

/// instance_id → which stat to show
static INSTANCE_STAT: LazyLock<DashMap<String, String>> = LazyLock::new(DashMap::new);

#[async_trait]
impl Action for Stats {
    const UUID: ActionUuid = UUID;
    type Settings = Settings;

    async fn will_appear(&self, instance: &Instance, settings: &Settings) -> OpenActionResult<()> {
        INSTANCE_STAT.insert(instance.instance_id.clone(), settings.stat.clone());
        let snap = SNAPSHOT.read().await.clone();
        set_stat(instance, &settings.stat, &snap).await
    }

    async fn will_disappear(&self, instance: &Instance, _settings: &Settings) -> OpenActionResult<()> {
        INSTANCE_STAT.remove(&instance.instance_id);
        Ok(())
    }

    async fn did_receive_settings(&self, instance: &Instance, settings: &Settings) -> OpenActionResult<()> {
        INSTANCE_STAT.insert(instance.instance_id.clone(), settings.stat.clone());
        let snap = SNAPSHOT.read().await.clone();
        set_stat(instance, &settings.stat, &snap).await
    }
}

// ─── Shared snapshot ──────────────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct Snapshot {
    pub cpu_pct: f32,
    pub ram_pct: f32,
    pub docker: usize,
}

pub static SNAPSHOT: LazyLock<RwLock<Snapshot>> = LazyLock::new(Default::default);

async fn set_stat(instance: &Instance, stat: &str, snap: &Snapshot) -> OpenActionResult<()> {
    let title = match stat {
        "cpu"    => format!("CPU\n{:.0}%", snap.cpu_pct),
        "ram"    => format!("RAM\n{:.0}%", snap.ram_pct),
        "docker" => format!("🐳 {}", snap.docker),
        _        => format!("CPU\n{:.0}%", snap.cpu_pct), // default
    };
    instance.set_title(Some(title), None).await
}

// ─── Polling loop ─────────────────────────────────────────────────────────────

pub async fn polling_loop() {
    let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(5));
    let mut prev_cpu: Option<CpuRaw> = None;

    loop {
        ticker.tick().await;

        let cpu_raw = read_cpu_raw();
        let cpu_pct = match (&prev_cpu, &cpu_raw) {
            (Some(prev), Some(cur)) => calc_cpu(prev, cur),
            _ => 0.0,
        };
        prev_cpu = cpu_raw;

        let snap = Snapshot {
            cpu_pct,
            ram_pct: read_ram_pct(),
            docker: crate::actions::docker_count::running_count().await.unwrap_or(0),
        };
        *SNAPSHOT.write().await = snap.clone();

        // Also update the docker.count buttons
        for instance in visible_instances(crate::actions::docker_count::UUID).await {
            let _ = instance.set_title(Some(snap.docker.to_string()), None).await;
        }

        // Update all stats display slots
        for instance in visible_instances(UUID).await {
            let stat = INSTANCE_STAT
                .get(&instance.instance_id)
                .map(|s: dashmap::mapref::one::Ref<'_, String, String>| s.clone())
                .unwrap_or_default();
            let _ = set_stat(&instance, &stat, &snap).await;
        }
    }
}

// ─── /proc readers ────────────────────────────────────────────────────────────

#[derive(Clone)]
struct CpuRaw {
    total: u64,
    idle: u64,
}

fn read_cpu_raw() -> Option<CpuRaw> {
    let text = std::fs::read_to_string("/proc/stat").ok()?;
    let line = text.lines().next()?;
    let nums: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|s| s.parse().ok())
        .collect();
    let total: u64 = nums.iter().sum();
    let idle = nums.get(3).copied().unwrap_or(0) + nums.get(4).copied().unwrap_or(0);
    Some(CpuRaw { total, idle })
}

fn calc_cpu(prev: &CpuRaw, cur: &CpuRaw) -> f32 {
    let total = cur.total.saturating_sub(prev.total) as f32;
    let idle = cur.idle.saturating_sub(prev.idle) as f32;
    if total == 0.0 { 0.0 } else { (1.0 - idle / total) * 100.0 }
}

fn read_ram_pct() -> f32 {
    let Ok(text) = std::fs::read_to_string("/proc/meminfo") else { return 0.0 };
    let mut total = None::<u64>;
    let mut available = None::<u64>;
    for line in text.lines() {
        let mut p = line.split_whitespace();
        match p.next() {
            Some("MemTotal:")     => total     = p.next().and_then(|v| v.parse().ok()),
            Some("MemAvailable:") => available = p.next().and_then(|v| v.parse().ok()),
            _ => {}
        }
        if total.is_some() && available.is_some() { break; }
    }
    match (total, available) {
        (Some(t), Some(a)) if t > 0 => (1.0 - a as f32 / t as f32) * 100.0,
        _ => 0.0,
    }
}
