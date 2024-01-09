use async_trait::async_trait;
use std::collections::BTreeSet;
use super::block::Transaction;
use super::chain::{
    Blockchain,
    BlockchainOperations,
};
use super::metadata::{
    BlockchainMetadata,
    BlockchainMetadataOperations
};
use std::sync::Arc;


#[async_trait]
pub trait MempoolOperations {

    /// Pushes a transaction to the mempool
    async fn push_transaction(
        &mut self, transaction: Transaction
    ) -> Result<(), String>;

    /// Pops a transaction from the mempool
    async fn pop_transaction(
        &mut self
    ) -> Result<Option<Transaction>, String>;

    /// Ticks the mempool over once
    async fn tick_mempool(
        &mut self,
    ) -> Result<(), String>;

    /// Checks if a transaction is valid entering the queue
    async fn is_transaction_acceptable(
        &self, transaction : &Transaction
    ) -> Result<bool, String>;

    /// Checks if a transaction is valid exiting the queue
    async fn is_transaction_deliverable(
        &self, transaction : &Transaction
    ) -> Result<bool, String>;

}

#[async_trait]
pub trait QueueInFlightMempool {

    /// Pushes a transaction to the mempool queue
    async fn add_to_queue(
        &mut self, transaction: Transaction
    ) -> Result<(), String>;

    /// Peeks a transaction from the mempool queue
    async fn peek_from_queue(
        &self
    ) -> Result<Option<&Transaction>, String>;

    /// Pops a transaction from the mempool queue
    async fn pop_from_queue(
        &mut self
    ) -> Result<Option<Transaction>, String>;

    /// Pops a transaction from in-flight
    async fn pop_from_in_flight(
        &mut self
    ) -> Result<Option<Transaction>, String>;

    /// Moves a transaction from the mempool queue to the mempool in-flight
    async fn mark_in_flight(
        &mut self, transaction : Transaction
    ) -> Result<(), String>;

    /// Moves a transaction from the mempool in-flight to the mempool queue
    async fn reenter(
        &mut self, transaction : Transaction
    ) -> Result<(), String>;

    /// Checks if transaction is in flight
    async fn is_in_flight(
        &self, transaction : &Transaction
    ) -> Result<bool, String>;

}

#[async_trait]
pub trait MempoolBlockchainOperations {

    // Checks the blockchain for a transaction going back a number of epochs
    async fn blockchain_has_transaction(
        &self, transaction : &Transaction
    ) -> Result<bool, String>;

}

#[async_trait]
pub trait MempoolMetadataOperations  {

    /// Checks whether the transaction has expired
    async fn transaction_has_expired(
        &self, transaction : &Transaction
    ) -> Result<bool, String>;

}

#[async_trait]
impl <T> MempoolOperations for T 
    where T: QueueInFlightMempool + MempoolBlockchainOperations + MempoolMetadataOperations + Send + Sync + 'static 
{

    async fn is_transaction_acceptable(
        &self, transaction : &Transaction
    ) -> Result<bool, String> {
        Ok(
            ! self.is_in_flight(transaction).await? 
            && ! self.blockchain_has_transaction(transaction).await?
        )
    }

    async fn is_transaction_deliverable(
        &self, transaction : &Transaction
    ) -> Result<bool, String> {
        self.is_transaction_acceptable(transaction).await
    }

    async fn push_transaction(
        &mut self, transaction: Transaction
    ) -> Result<(), String> {

        if ! self.is_transaction_acceptable(&transaction).await? {
            return Ok(())
        }
            
        self.add_to_queue(transaction).await?;
        Ok(())
    }

    async fn pop_transaction(&mut self) -> Result<Option<Transaction>, String> {
    
        let transaction_opt = {
            let peeked_transaction = self.pop_from_queue().await?;
            match peeked_transaction {
                Some(transaction) => {
                    if !self.is_transaction_deliverable(&transaction).await? {
                        None // it means we want to continue looking for a valid transaction.
                    } else {
                        Some(transaction.clone())
                    }
                },
                None => None,
            }
        };
    
        match transaction_opt {
            Some(valid_transaction) => {
                self.mark_in_flight(valid_transaction.clone()).await?;
                Ok(Some(valid_transaction))
            },
            None => Ok(None),
        }
    }
    

    async fn tick_mempool(&mut self) -> Result<(), String> {
       
        while let Some(transaction) = self.pop_from_in_flight().await? {
            self.reenter(transaction).await?;
        }

        Ok(())
        
    }

}

#[derive(Debug, Clone)]
pub struct Mempool {
    pub queue: Vec<Transaction>,
    pub in_flight: BTreeSet<Transaction>,
    pub blockchain : Arc<Blockchain>, // mempool should not mutate blockchain
    pub blockchain_metadata: Arc<BlockchainMetadata>, // mempool should not mutate blockchain metadata
}

#[async_trait]
impl MempoolBlockchainOperations for Mempool {

    async fn blockchain_has_transaction(
        &self, transaction : &Transaction
    ) -> Result<bool, String> {
        self.blockchain.has_transaction(transaction).await
    }

}

#[async_trait]
impl MempoolMetadataOperations for Mempool {
    async fn transaction_has_expired(
        &self, transaction : &Transaction
    ) -> Result<bool, String> {
        self.blockchain_metadata.has_transaction_expired(transaction.timestamp).await
    }
}

impl Default for Mempool {
    fn default() -> Self {
        Mempool {
            queue: Vec::new(),
            in_flight: BTreeSet::new(),
            blockchain: Arc::new(Blockchain::default()),
            blockchain_metadata: Arc::new(BlockchainMetadata::default()),
        }
    }
}

#[async_trait]
impl QueueInFlightMempool for Mempool {

    async fn add_to_queue(
        &mut self, transaction: Transaction
    ) -> Result<(), String> {
        self.queue.push(transaction);
        Ok(())
    }

    async fn peek_from_queue(
        &self
    ) -> Result<Option<&Transaction>, String> {
        Ok(self.queue.first())
    }

    async fn pop_from_queue(
        &mut self
    ) -> Result<Option<Transaction>, String> {
        Ok(self.queue.pop())
    }

    async fn pop_from_in_flight(
        &mut self
    ) -> Result<Option<Transaction>, String> {
        Ok(self.in_flight.iter().next().cloned())
    }

    async fn mark_in_flight(
        &mut self, transaction : Transaction
    ) -> Result<(), String> {
        self.in_flight.insert(transaction);
        Ok(())
    }

    async fn reenter(
        &mut self, transaction : Transaction
    ) -> Result<(), String> {

        if !self.in_flight.contains(&transaction) {
            return Err("Transaction not in flight".to_string())
        }

        self.in_flight.remove(&transaction);

        self.queue.push(transaction);

        Ok(())

    }

    async fn is_in_flight(
        &self, transaction : &Transaction
    ) -> Result<bool, String> {
        Ok(self.in_flight.contains(transaction))
    }

}