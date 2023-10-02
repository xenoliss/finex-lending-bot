use async_trait::async_trait;

use bitfinex_api::{
    api::{
        authenticated::wallets::{Wallets, WalletsResp},
        query::AsyncQuery,
    },
    bitfinex::AsyncBitfinex,
};

use super::Strategy;

pub struct BasicStrategy<'a> {
    client: &'a AsyncBitfinex,
    wallets_endpoint: Wallets,
}

impl<'a> BasicStrategy<'a> {
    pub fn new(client: &'a AsyncBitfinex) -> Self {
        Self {
            client,
            wallets_endpoint: Wallets::builder().build().unwrap(),
        }
    }
}

#[async_trait]
impl<'a> Strategy for BasicStrategy<'a> {
    type Output = ();

    /// Get the strategy name.
    fn name(&self) -> String {
        String::from("BasicStrategy")
    }

    /// Execute the strategy.
    async fn execute(&self) -> Self::Output {
        println!("Execting {}", self.name());
        let r: WalletsResp = self
            .wallets_endpoint
            .query_async(self.client)
            .await
            .unwrap();
        println!("{r:?}")
    }
}
