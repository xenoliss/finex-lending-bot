use dotenv_codegen::dotenv;
use std::time::Duration;

use bitfinex_api::bitfinex::AsyncBitfinex;

mod strategies;
use strategies::{basic_strategy::BasicStrategy, Strategy};

#[tokio::main]
async fn main() {
    let api = AsyncBitfinex::new_auth(dotenv!("API_KEY"), dotenv!("SECRET_KEY"));
    let basic_strategy = BasicStrategy::new(&api);

    let duration = Duration::from_secs(1);
    loop {
        tokio::time::sleep(duration).await;

        basic_strategy.execute().await;
    }
}
