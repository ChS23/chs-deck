use openaction::{global_events::*, *};

mod actions;

struct GlobalHandler;

#[async_trait]
impl GlobalEventHandler for GlobalHandler {
    async fn plugin_ready(&self) -> OpenActionResult<()> {
        // Single polling loop handles CPU/RAM/Docker + updates all display slots
        tokio::spawn(actions::stats::polling_loop());
        tokio::spawn(actions::media::polling_loop());
        log::info!("Plugin ready");
        Ok(())
    }
}

static GLOBAL_HANDLER: GlobalHandler = GlobalHandler;

#[tokio::main]
async fn main() -> OpenActionResult<()> {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Info,
        simplelog::Config::default(),
        simplelog::TerminalMode::Stdout,
        simplelog::ColorChoice::Never,
    )
    .unwrap();

    set_global_event_handler(&GLOBAL_HANDLER);
    register_action(actions::docker_count::DockerCount).await;
    register_action(actions::stats::Stats).await;
    register_action(actions::media::Media).await;
    register_action(actions::shell::Shell).await;
    register_action(actions::video::VideoPlayer).await;

    run(std::env::args().collect()).await
}
