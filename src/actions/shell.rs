use openaction::*;
use serde::{Deserialize, Serialize};

pub const UUID: ActionUuid = "chs.deck.shell";

pub struct Shell;

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub cmd: String,
}

#[async_trait]
impl Action for Shell {
    const UUID: ActionUuid = UUID;
    type Settings = Settings;

    async fn key_up(&self, instance: &Instance, settings: &Settings) -> OpenActionResult<()> {
        if settings.cmd.is_empty() {
            return instance.show_alert().await;
        }

        let cmd = settings.cmd.clone();
        tokio::spawn(async move {
            let result = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .status()
                .await;

            if let Ok(status) = result {
                log::info!("shell cmd exited: {status}");
            }
        });

        instance.show_ok().await
    }
}
