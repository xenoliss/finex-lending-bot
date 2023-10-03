use async_trait::async_trait;

pub mod simple_strategy;

#[async_trait]
pub trait Strategy {
    type Output;

    fn from_config(path: &str) -> Vec<Self>
    where
        Self: std::marker::Sized;

    async fn execute(&self) -> Self::Output;
}
