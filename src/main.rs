use anyhow::Result;
use striem_config::StrIEMConfig;
mod app;
mod detection;
use app::App;
use log::info;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let argv: Vec<String> = std::env::args().collect();

    let config = match argv.len() {
        1 => StrIEMConfig::new()?,
        _ => StrIEMConfig::from_file(&argv[1])?,
    };
    let mut app = App::new(config).await?;
    let shutdown = app.shutdown();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        info!("StrIEM shutting down...");
        shutdown.send(()).unwrap();
    });

    println!(".:: Starting StrIEM ::.");
    app.run().await?;
    println!(".:: StrIEM Stopped. Goodbye ::.");

    Ok(())
}
