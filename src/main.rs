use std::time::Duration;

mod strategies;
use dotenv::dotenv;
use strategies::{simple_strategy::SimpleStrategy, Strategy};

#[tokio::main]
async fn main() {
    dotenv().ok();

    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_timestamp_secs()
        .init();

    let strategies = SimpleStrategy::from_config("./config.yaml");

    loop {
        for strategy in &strategies {
            let res = strategy.execute().await;
            if let Err(e) = res {
                log::error!("{e}")
            }
        }

        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
