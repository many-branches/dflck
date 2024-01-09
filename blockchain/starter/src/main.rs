use mini_chain::simulation::simulation;
use tracing_subscriber;
use tracing::Instrument;

#[tokio::main]
async fn main() -> Result<(), String> {

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .init();
    // console_subscriber::init();

    // long simulation
    simulation(120, 25, 5).await?;

    // high TPS
    simulation(60, 5, 2).await?;

    Ok(())

}