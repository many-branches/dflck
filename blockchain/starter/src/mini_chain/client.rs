use async_trait::async_trait;
use tokio::sync::RwLock;
use super::{block::{Transaction}, mempool::{Mempool, MempoolOperations}};
use std::sync::Arc;
use async_channel::{Sender, Receiver};

#[async_trait]
pub trait ClientOperations {

    // Submits a transaction to the mempool
    async fn submit_transaction(&mut self, transaction: Transaction) -> Result<(), String>;

}

pub struct MempoolAccessClient {
    pub mempool : Arc<RwLock<Mempool>>
}

#[async_trait]
impl ClientOperations for MempoolAccessClient {

    async fn submit_transaction(&mut self, transaction: Transaction) -> Result<(), String> {
        let mut mempool = self.mempool.write().await;
        mempool.push_transaction(transaction).await
    }

}

pub struct ChannelClient {
    pub transaction_sender : Sender<Transaction>
}

impl ChannelClient {
    pub fn new(transaction_sender : Sender<Transaction>) -> Self {
        Self {
            transaction_sender
        }
    }
}

#[async_trait]
impl ClientOperations for ChannelClient {

    async fn submit_transaction(&mut self, transaction: Transaction) -> Result<(), String> {
        self.transaction_sender.send(transaction).await.map_err(|_| "Failed to send transaction".to_string())
    }

}