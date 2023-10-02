use async_trait::async_trait;

pub mod basic_strategy;

#[async_trait]
pub trait Strategy {
    type Output;

    /// Get the strategy name.
    fn name(&self) -> String;

    /// Execute the strategy.
    async fn execute(&self) -> Self::Output;
}
