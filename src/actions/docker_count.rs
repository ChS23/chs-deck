use bollard::{Docker, query_parameters::ListContainersOptions};
use openaction::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::time::{Duration, interval};

pub const UUID: ActionUuid = "chs.deck.docker.count";

pub struct DockerCount;

#[derive(Default, Serialize, Deserialize)]
pub struct Settings {}

#[async_trait]
impl Action for DockerCount {
    const UUID: ActionUuid = UUID;
    type Settings = Settings;

    async fn will_appear(&self, instance: &Instance, _settings: &Settings) -> OpenActionResult<()> {
        if let Ok(count) = running_count().await {
            instance.set_title(Some(count.to_string()), None).await?;
        }
        Ok(())
    }
}

pub async fn running_count() -> Result<usize, bollard::errors::Error> {
    let docker = Docker::connect_with_local_defaults()?;
    let mut filters: HashMap<String, Vec<String>> = HashMap::new();
    filters.insert("status".into(), vec!["running".into()]);
    let containers = docker
        .list_containers(Some(ListContainersOptions {
            filters: Some(filters),
            ..Default::default()
        }))
        .await?;
    Ok(containers.len())
}

#[allow(dead_code)]
pub async fn polling_loop() {
    let mut ticker = interval(Duration::from_secs(5));
    loop {
        ticker.tick().await;
        match running_count().await {
            Ok(count) => {
                for instance in visible_instances(UUID).await {
                    let _ = instance.set_title(Some(count.to_string()), None).await;
                }
            }
            Err(e) => log::error!("Docker count failed: {e}"),
        }
    }
}
