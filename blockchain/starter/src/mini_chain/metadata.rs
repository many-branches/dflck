use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[async_trait]
pub trait BlockchainMetadataOperations {

    /// Gets the number of seconds in a slot
    async fn get_slot_secs(&self) -> Result<u64, String>;

    /// Gets the number of slots back the blockchain should be queried
    async fn get_query_slots_count(&self) -> Result<u64, String>;

    /// Gets the slot start time for a number of seconds since the epoch
    async fn get_slot_start_time(&self, secs : u64) -> Result<u64, String> {
        let slot_secs = self.get_slot_secs().await?;
        Ok((secs / slot_secs) * slot_secs)
    }

    /// Gets the number of slots before forks in the chain should be resolved
    async fn get_fork_resolution_slots(&self) -> Result<u64, String>;

    /// Gets the number of seconds until forks should be resolved
    async fn get_fork_resolution_secs(&self) -> Result<u64, String> {
        let slot_secs = self.get_slot_secs().await?;
        let fork_resolution_slots = self.get_fork_resolution_slots().await?;
        Ok(slot_secs * fork_resolution_slots)
    }

    /// Gets the time at which a fork should be resolved relative to provided timestamp
    async fn get_fork_resolution_time(&self, secs : u64) -> Result<u64, String> {
        let slot_time = self.get_slot_start_time(secs).await?;
        let fork_resolution_secs = self.get_fork_resolution_secs().await?;
        Ok(slot_time + fork_resolution_secs)
    }

    /// Gets the timestamp of the oldest epoch to query
    async fn get_query_slot_lower_bound(&self) -> Result<u64, String>;
    
    /// Gets the maximum block size
    async fn get_block_size(&self) -> Result<u64, String>;

    /// Gets the maximum time the proposer should spend building a block
    /// ! In a real chain, you would want to create mechanisms to coerce the proposer to build a block in a reasonable amount of time.
    /// * This is subslot timing, so it should be a time by itself.
    async fn get_maximum_proposer_time(&self) -> Result<u64, String>;

    /// Gets the amount of time a transaction should be in the mempool in-flight set before being reentered
    async fn get_mempool_reentrancy_secs(&self) -> Result<u64, String>;

    /// Gets the amount of time a transaction should be in the network before being expired
    async fn get_transaction_expiry_secs(&self) -> Result<u64, String>;

    /// Checks whether a transaction has expired
    async fn has_transaction_expired(&self, transaction_ts : u64) -> Result<bool, String>;

    /// Gets the difficulty of the chain
    async fn get_difficulty(&self) -> Result<usize, String>;

    /// Gets the time the proposer should wait after encountering an empty mempool
    async fn get_proposer_wait_ms(&self) -> Result<u64, String>;

}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainMetadata {
    pub query_slots: u64,
    pub slot_secs: u64,
    pub fork_resolution_slots: u64,
    pub block_size: u64,
    pub maximum_proposer_time: u64,  
    pub mempool_reentrancy_secs: u64,
    pub transaction_expiry_secs: u64,
    pub difficulty: usize,
    pub proposer_wait_ms: u64, 
}

impl Default for BlockchainMetadata {
    fn default() -> Self {
        BlockchainMetadata {
            query_slots: 4,
            slot_secs: 8,
            fork_resolution_slots : 8,
            block_size: 64,
            maximum_proposer_time: 2,
            mempool_reentrancy_secs: 2,
            transaction_expiry_secs: 32,
            difficulty: 8,
            proposer_wait_ms : 100
        }
    }

}

impl BlockchainMetadata {
    pub fn fast_chain() -> Self {
        BlockchainMetadata {
            query_slots: 4,
            slot_secs: 2,
            fork_resolution_slots : 8,
            block_size: 128,
            maximum_proposer_time: 1,
            mempool_reentrancy_secs: 2,
            transaction_expiry_secs: 8,
            difficulty: 2,
            proposer_wait_ms : 100
        }
    }

}

#[async_trait]
impl BlockchainMetadataOperations for BlockchainMetadata {

    async fn get_slot_secs(&self) -> Result<u64, String> {
        Ok(self.slot_secs)
    }

    async fn get_query_slots_count(&self) -> Result<u64, String> {
        Ok(self.query_slots)
    }

    async fn get_query_slot_lower_bound(&self) -> Result<u64, String> {
        let query_slots = self.get_query_slots_count().await?;
        let slot_secs = self.get_slot_secs().await?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        Ok(
            self.get_slot_start_time(now).await? -
            (query_slots * slot_secs)
        )
    }

    async fn get_fork_resolution_slots(&self) -> Result<u64, String> {
        Ok(self.fork_resolution_slots)
    }

    async fn get_block_size(&self) -> Result<u64, String> {
        Ok(self.block_size)
    }

    /// Gets the maximum time the proposer should spend building a block
    async fn get_maximum_proposer_time(&self) -> Result<u64, String> {
        Ok(self.maximum_proposer_time)
    }

    async fn get_mempool_reentrancy_secs(&self) -> Result<u64, String> {
        Ok(self.mempool_reentrancy_secs)
    }

    async fn get_transaction_expiry_secs(&self) -> Result<u64, String> {
        Ok(self.transaction_expiry_secs)
    }
    
    async fn has_transaction_expired(&self, transaction_ts : u64) -> Result<bool, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let slot_start = self.get_slot_start_time(now).await?;
        let transaction_age = slot_start - transaction_ts;
        Ok(transaction_age > self.transaction_expiry_secs)
    }

    async fn get_difficulty(&self) -> Result<usize, String> {
        Ok(self.difficulty)
    }

    async fn get_proposer_wait_ms(&self) -> Result<u64, String> {
        Ok(self.proposer_wait_ms)
    }

}
