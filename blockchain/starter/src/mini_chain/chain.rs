use async_trait::async_trait;
use std::collections::BTreeMap;
use std::collections::{HashMap, HashSet};
use super::block::{Transaction, Block};
use super::metadata::{BlockchainMetadata, BlockchainMetadataOperations, self};
use std::sync::Arc;
use rand::{Rng, distributions::Standard};
use colored::*;
use futures::stream::{BoxStream, Stream, StreamExt};
use futures::SinkExt;
use tokio_stream::wrappers::ReceiverStream;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use tracing::{instrument, info};

fn compute_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}


#[async_trait]
pub trait BlockchainOperations {

    /// Gets a clone of a transaction from the blockchain going back a number of slots
    async fn get_transaction (
        &self, 
        transaction: &Transaction
    ) -> Result<Option<Transaction>, String>;

     /// Checks whether a transaction has been accepted into the blockchain going back a number of slots
     async fn has_transaction (
        &self, 
        transaction: &Transaction
    ) -> Result<bool, String> {
        Ok(self.get_transaction(transaction).await?.is_some())
    }

    async fn add_block (
        &mut self, 
        block: Block
    ) -> Result<(), String>;

    async fn get_block (
        &self, 
        hash : String
    ) -> Result<Option<Block>, String>;

    async fn has_block (
        &self, 
        hash : String
    ) -> Result<bool, String>;

    async fn get_main_chain(
        &self
    ) -> Result<Vec<Block>, String>;

    async fn resolve_chain (
        &mut self, 
    ) -> Result<(), String>;

    async fn is_resolved (
        &self, 
    ) -> bool;


    async fn get_chain_tip (
        &self
    ) -> Result<Option<Block>, String>;

    async fn get_leaves (
        &self
    ) -> Result<Vec<Block>, String>;

    async fn get_leaf_hashes (
        &self
    ) -> Result<HashSet<String>, String>;

    async fn compute_genesis_block(&self) -> Result<Option<&(Block, HashSet<String>)>, String>;
    async fn pretty_print_tree(&self) -> Result<(), String>;
    async fn pretty_print_branch(&self, block_hash: &str, depth: usize, prefix: &str, is_last: bool, main_chain_set: &HashSet<String>) -> Result<(), String>;
    
}

#[derive(Debug, Default, Clone)] // TODO: why is it dangerous to derive clone?
pub struct Blockchain {
    blocks: HashMap<String, (Block, HashSet<String>)>,
    leaves: HashSet<String>,
    slots: BTreeMap<u64, HashSet<String>>,
    metadata : Arc<BlockchainMetadata>, // blockchain does not mutate its own metadata
}


impl Blockchain {
    pub fn new(metadata : Arc<BlockchainMetadata>) -> Self {
        Self {
            blocks : HashMap::new(),
            leaves : HashSet::new(),
            slots : BTreeMap::new(),
            metadata : metadata
        }

    }

    pub async fn genesis(&mut self) -> Result<(), String> {
        self.blocks.clear();
        self.leaves.clear();
        self.slots.clear();
        let genesis_block = Block::genesis(
            self.metadata.get_slot_secs().await.unwrap()
        );
        self.add_block(genesis_block).await?;
        Ok(())
    }

}

#[async_trait]
impl BlockchainOperations for Blockchain {

    async fn add_block(&mut self, block: Block) -> Result<(), String> {

        // assume block has been verified at this point
        let block_hash = block.hash.clone();
        
        // If block is not an orphan (allow genesis block to be added)
        if self.blocks.contains_key(&block.previous_hash) || self.blocks.is_empty() {

            // Add block to blocks without any children yet
            self.blocks.insert(block_hash.clone(), (block.clone(), HashSet::new()));

            // Add to its slot; assume correct one has been given
            self.slots.entry(block.slot_number).or_insert(HashSet::new()).insert(block_hash.clone());

            // Update parent's children
            if let Some((_, children)) = self.blocks.get_mut(&block.previous_hash) {
                children.insert(block_hash.clone());
            }
            
            // Update leaves: remove parent and add the new block
            self.leaves.remove(&block.previous_hash);
            self.leaves.insert(block_hash);

            // println!("Added block: {} {}", block.hash.clone(), block.previous_hash.clone());
            
            Ok(())
        } else {
            // Err("Previous hash not found. Orphan block.".to_string())
            Ok(())
        }
    }

    async fn get_block(&self, hash: String) -> Result<Option<Block>, String> {
        Ok(self.blocks.get(&hash).map(|(block, _)| block.clone()))
    }

    async fn has_block(&self, hash: String) -> Result<bool, String> {
        Ok(self.blocks.contains_key(&hash))
    }

    async fn get_main_chain(&self) -> Result<Vec<Block>, String> {

        let mut longest_chain = Vec::new();
        
        for leaf in &self.leaves {
            let mut current_hash = leaf.clone();
            let mut current_chain = Vec::new();
            
            while let Some((block, _)) = self.blocks.get(&current_hash) {
                current_chain.push(block.clone());
                current_hash = block.previous_hash.clone();
            }

            if current_chain.len() > longest_chain.len() {
                longest_chain = current_chain;
            }
        }

        Ok(longest_chain)

    }

    async fn resolve_chain(&mut self) -> Result<(), String> {

        // Get the longest chain 
        let longest_chain = self.get_main_chain().await?;  

        if longest_chain.len() < 1 {
            return Ok(());
        }

        // clear everything out
        self.blocks.clear();
        self.slots.clear();
        self.leaves.clear();
        self.leaves.insert(longest_chain.last().unwrap().hash.clone());

        // the blocks are in reverse order, so we need to reverse them
        // thankfully we can just add them back in because they are already a valid chain
        for block in longest_chain.into_iter().rev() {
            self.add_block(block).await?;
        }

        Ok(())

    }

    async fn is_resolved(&self) -> bool {
        // Check if there's only a single leaf
        self.leaves.len() == 1
    }

    async fn get_transaction(&self, transaction: &Transaction) -> Result<Option<Transaction>, String> {

        // Get relevant metadata
        let slot_lower_bound = self.metadata.get_query_slot_lower_bound().await?;

        // Check the slots back to the lower bound
        for (slot_ts, block_hashes) in self.slots.iter().rev() {
    
            if *slot_ts < slot_lower_bound {
                return Ok(None);
            }

            for block_hash in block_hashes {
                if let Some((block, _)) = self.blocks.get(block_hash) {
                    if block.transactions.contains(transaction) {
                        return Ok(Some(transaction.clone()))
                    }
                }
            }

        }

        Ok(None)

    }

    async fn get_chain_tip(&self) -> Result<Option<Block>, String> {
        let chain = self.get_main_chain().await?;
        Ok(chain.first().cloned())
    }

    async fn compute_genesis_block(&self) -> Result<Option<&(Block, HashSet<String>)>, String> {
        Ok(self.blocks.iter().find(|&(_, (block, _))| block.previous_hash.is_empty()).map(|(_, value)| value))
    }

    async fn get_leaves(&self) -> Result<Vec<Block>, String> {
        Ok(self.leaves.iter().filter_map(|hash| self.blocks.get(hash).map(|(block, _)| block.clone())).collect())
    }

    async fn get_leaf_hashes(&self) -> Result<HashSet<String>, String> {
        Ok(self.leaves.clone())
    }
    
    async fn pretty_print_tree(&self) -> Result<(), String> {
        
        let main_chain_vec = self.get_main_chain().await?; // assuming this function is available
        // let main_chain_vec: Vec<Block> = vec![];
        let main_chain_set: HashSet<String> = main_chain_vec.iter().map(|block| block.hash.clone()).collect();

        if let Some(genesis_block) = main_chain_vec.last() {
            self.pretty_print_branch(genesis_block.hash.as_str(), 0, "", true, &main_chain_set).await?;
        }
    
        Ok(())
    }
    
    async fn pretty_print_branch(&self, block_hash: &str, depth: usize, prefix: &str, is_last: bool, main_chain_set: &HashSet<String>) -> Result<(), String> {
        if let Some((block, children)) = self.blocks.get(block_hash) {
            let display_hash = &block.hash[0..16]; // slice the first 16 characters
            
            // Decide on characters to use for this branch
            let branch = if is_last { "└──" } else { "├──" };
    
            let is_leaf = children.is_empty();
            let color = if is_leaf { "green" } else { "gray" };
    
            // Check if the block is in the main chain
            let in_main_chain = main_chain_set.contains(block_hash);
    
            let display_str = if in_main_chain {
                format!(
                    "Block: {} {}", 
                    display_hash,
                    if is_leaf { "< TIP".white().on_purple() } else { "".white() }
                ).underline().on_cyan()
            } else {
                format!("Block: {}", display_hash).color(color)
            };
            
            println!(
                "{}{} ({}) {}",
                prefix,
                branch.yellow(),
                depth.to_string().yellow(),
                display_str
            );
    
            // Generate the next prefix for the children
            let next_prefix = if is_last { 
                format!("{}    ", prefix).yellow()
            } else { 
                format!("{}│   ", prefix).yellow() 
            };
            
            // recursively print children
            let num_children = children.len();
            for (index, child_hash) in children.iter().enumerate() {
                let is_last_child = index == num_children - 1;
                self.pretty_print_branch(child_hash, depth + 1, &next_prefix, is_last_child, main_chain_set).await?;
            }
        }
    
        Ok(())
    }

}

#[cfg(test)]
mod test {

    use super::*;
    use tokio::sync::RwLock;
    use rand::SeedableRng;

    #[tokio::test]
    async fn test_add_block() -> Result<(), String> {
        let metadata = Arc::new(BlockchainMetadata::default());
        let mut blockchain = Blockchain::new(metadata.clone());
        let block = Block::random(
            &mut rand::thread_rng(), 
            metadata.clone().get_slot_secs().await.unwrap()
        );
        blockchain.add_block(block).await.unwrap();
        assert_eq!(blockchain.blocks.len(), 1);
        assert_eq!(blockchain.leaves.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_chain_tip() -> Result<(), String> {
        let metadata = Arc::new(BlockchainMetadata::default());
        let mut blockchain = Blockchain::new(metadata.clone());
        let slot_secs = metadata.clone().get_slot_secs().await.unwrap();

        // chain tip should be none
        let tip_zero = blockchain.get_chain_tip().await.unwrap();
        assert!(tip_zero.is_none());

        // add the first block
        let first_block = Block::random(
            &mut rand::thread_rng(), 
            slot_secs
        );
        blockchain.add_block(first_block.clone()).await.unwrap();
        let tip_one = blockchain.get_chain_tip().await.unwrap().unwrap();
        assert_eq!(tip_one.calculate_hash(), first_block.calculate_hash());

        // add the second block
        let mut second_block = Block::random(
            &mut rand::thread_rng(), 
            slot_secs
        );
        second_block.previous_hash = blockchain.get_chain_tip().await.unwrap().unwrap().hash.clone();
        blockchain.add_block(second_block.clone()).await.unwrap();
        let tip_two = blockchain.get_chain_tip().await.unwrap().unwrap();
        assert_eq!(tip_two.calculate_hash(), second_block.calculate_hash());
       
        Ok(())
    }

    #[async_recursion::async_recursion]
    async fn create_fork(
        blockchain: Arc<RwLock<Blockchain>>,
        rng: Arc<RwLock<rand::rngs::StdRng>>,
        slot_secs: u64,
        previous_hash: String,
        depth: usize
    ) -> Result<usize, String> {
        
        if depth == 0 {
            return Ok(0);
        }

        let mut block = {
            let mut rng_guard = rng.write().await;
            Block::random(&mut *rng_guard, slot_secs)
        };

        block.previous_hash = previous_hash;
        blockchain.write().await.add_block(block.clone()).await?;
        let current_hash = block.hash.clone();

        let should_fork_left: bool = {
            let mut rng_guard = rng.write().await;
            rng_guard.gen::<f32>() < 0.7
        };
        
        let should_fork_right: bool = {
            let mut rng_guard = rng.write().await;
            rng_guard.gen::<f32>() < 0.7
        };
        

        let mut left_length = 0;
        let mut right_length = 0;

        if should_fork_left {
            let left_future = Box::pin(create_fork(blockchain.clone(), rng.clone(), slot_secs, current_hash.clone(), depth - 1));
            left_length = left_future.await?;
        }

        if should_fork_right {
            let right_future = Box::pin(create_fork(blockchain.clone(), rng.clone(), slot_secs, current_hash, depth - 1));
            right_length = right_future.await?;
        }

        Ok(1 + std::cmp::max(left_length, right_length))
    }

    
    #[tokio::test]
    async fn test_fork() -> Result<(), String> {
        let rng = Arc::new(RwLock::new(rand::rngs::StdRng::from_entropy()));
    
        let metadata = Arc::new(BlockchainMetadata::default());
        let blockchain = Arc::new(RwLock::new(Blockchain::new(metadata.clone())));
        let slot_secs = metadata.clone().get_slot_secs().await.unwrap();
    
        // Add the genesis block
        let genesis_block = {
            let mut rng_guard = rng.write().await;
            Block::random(&mut *rng_guard, slot_secs)
        };
        blockchain.write().await.add_block(genesis_block.clone()).await.unwrap();
    
        // Build the trunk
        let trunk_size = {
            let mut rng_guard = rng.write().await;
            rng_guard.gen_range(1..4)
        };
    
        let mut last_hash = genesis_block.hash.clone();
        for _ in 0..trunk_size {
            let mut block = {
                let mut rng_guard = rng.write().await;
                Block::random(&mut *rng_guard, slot_secs)
            };
            block.previous_hash = last_hash;
            last_hash = block.hash.clone();
            blockchain.write().await.add_block(block).await.unwrap();
        }
    
        // Build the forks
        let fork_depth = 6; // For simplicity
        let fork_length = create_fork(blockchain.clone(), rng.clone(), slot_secs, last_hash, fork_depth).await?;
    
        blockchain.read().await.pretty_print_tree().await?;

        // Check the longest chain
        let longest_chain = blockchain.read().await.get_main_chain().await.unwrap();
        assert_eq!(longest_chain.len(), 1 + trunk_size + fork_length);
    
        Ok(())
    }
    
    
    #[tokio::test]
    async fn test_fork_and_resolve() -> Result<(), String> {
        let rng = Arc::new(RwLock::new(rand::rngs::StdRng::from_entropy()));
    
        let metadata = Arc::new(BlockchainMetadata::default());
        let blockchain = Arc::new(RwLock::new(Blockchain::new(metadata.clone())));
        let slot_secs = metadata.clone().get_slot_secs().await.unwrap();
    
        // Add the genesis block
        let genesis_block = {
            let mut rng_guard = rng.write().await;
            Block::random(&mut *rng_guard, slot_secs)
        };
        blockchain.write().await.add_block(genesis_block.clone()).await.unwrap();
    
        // Build the trunk
        let trunk_size = {
            let mut rng_guard = rng.write().await;
            rng_guard.gen_range(1..4)
        };
    
        let mut last_hash = genesis_block.hash.clone();
        for _ in 0..trunk_size {
            let mut block = {
                let mut rng_guard = rng.write().await;
                Block::random(&mut *rng_guard, slot_secs)
            };
            block.previous_hash = last_hash;
            last_hash = block.hash.clone();
            blockchain.write().await.add_block(block).await.unwrap();
        }
    
        // Build the forks
        let fork_depth = 6; // For simplicity
        let fork_length = create_fork(blockchain.clone(), rng.clone(), slot_secs, last_hash, fork_depth).await?;
    
        blockchain.read().await.pretty_print_tree().await?;

        // Check the longest chain
        let longest_chain = blockchain.read().await.get_main_chain().await.unwrap();
        assert_eq!(longest_chain.len(), 1 + trunk_size + fork_length);

        blockchain.write().await.resolve_chain().await?;
        blockchain.read().await.pretty_print_tree().await?;

        let longest_chain_hashes = longest_chain.iter().map(|block| block.hash.clone()).collect::<HashSet<String>>();
        
        for (hash, _) in blockchain.read().await.blocks.iter() {
            assert!(longest_chain_hashes.contains(hash));
        }

        Ok(())


    }

    #[tokio::test]
    async fn test_get_transaction(){
            
            let metadata = Arc::new(BlockchainMetadata::default());
            let mut blockchain = Blockchain::new(metadata.clone());
            let slot_secs = metadata.clone().get_slot_secs().await.unwrap();
    
            let mut block = Block::random(
                &mut rand::thread_rng(), 
                slot_secs
            );
    
            let transaction = Transaction::random(&mut rand::thread_rng());
            block.transactions.insert(transaction.clone());
    
            blockchain.add_block(block).await.unwrap();
    
            let transaction_copy = blockchain.get_transaction(&transaction).await.unwrap().unwrap();
            assert_eq!(transaction, transaction_copy);
    
    }

}


#[async_trait]
pub trait BlockchainAnaylsisOperations {

    async fn get_trunk(&self) -> Result<Vec<Block>, String>;

    async fn is_trunk_tall(&self) -> Result<bool, String>;

    async fn compute_transaction_success_rate(&self, transactions :  &Vec<Transaction>) -> Result<f64, String>;

    async fn stream_transactions(&self) -> Result<BoxStream<'_, Result<Transaction, String>>, String>;

    fn chains_edit_score(main_chains : Vec<Vec<String>>) -> f64;

}

pub fn levenshtein_distance_vec(a: &Vec<String>, b: &Vec<String>) -> usize {
    let (len_a, len_b) = (a.len(), b.len());
    if len_a == 0 {
        return len_b;
    }
    if len_b == 0 {
        return len_a;
    }

    // Create a table to store results of subproblems
    let mut dp = vec![vec![0; len_b + 1]; len_a + 1];

    // Initialization
    for i in 0..=len_a {
        for j in 0..=len_b {
            if i == 0 {
                dp[i][j] = j;
            } else if j == 0 {
                dp[i][j] = i;
            } else if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            } else {
                dp[i][j] = 1 + std::cmp::min(
                    dp[i - 1][j - 1],   // Substitution
                    std::cmp::min(
                        dp[i][j - 1],   // Insertion
                        dp[i - 1][j],   // Deletion
                    )
                );
            }
        }
    }

    dp[len_a][len_b]
}



#[async_trait]
impl BlockchainAnaylsisOperations for Blockchain {

    async fn get_trunk(&self) -> Result<Vec<Block>, String> {
        let mut trunk = Vec::new();
        let (mut block, mut children) = self.compute_genesis_block().await?.unwrap().clone();
        while children.len() == 1 {
            trunk.push(block.clone());
            (block, children) = self.blocks.get(children.iter().next().unwrap()).unwrap().clone();
        }
        Ok(trunk)
    }

    async fn is_trunk_tall(&self) -> Result<bool, String> {
        let trunk = self.get_trunk().await?;
        let main_chain = self.get_main_chain().await?;
        let tall = {
            trunk.len() >= 
            (
                main_chain.len() -  
                (2 * ((main_chain.len() as f64).log2() as usize))
            )
        };
        Ok(tall)
    }


    async fn stream_transactions(&self) -> Result<BoxStream<'_, Result<Transaction, String>>, String> {
        let (mut tx, rx) = tokio::sync::mpsc::channel::<Result<Transaction, String>>(32);
        let main_chain = self.get_main_chain().await?;
        
        tokio::spawn(async move {
            for block in main_chain.iter() {
                for transaction in block.transactions.iter() {
                    let _ = tx.send(Ok(transaction.clone())).await;
                }
            }
        });
    
        // Wrap the Receiver into a Stream
        let stream = ReceiverStream::new(rx).boxed();

        Ok(stream)
    } 

    async fn compute_transaction_success_rate(&self, transactions : &Vec<Transaction>) -> Result<f64, String> {
        // Collect transactions from the stream into a HashSet
        let mut txn_stream = self.stream_transactions().await?;
        let mut txn_set = HashSet::new();
        while let Some(txn_result) = txn_stream.next().await {
            match txn_result {
                Ok(transaction) => {
                    txn_set.insert(transaction);
                },
                Err(e) => {
                    eprintln!("Error fetching transaction: {}", e);
                }
            }
        }

        // Iterate over the provided transactions vec and count the successes
        let transaction_count = transactions.iter().filter(|txn| txn_set.contains(txn)).count();

        Ok((transaction_count as f64) / (transactions.len() as f64))
    }

    fn chains_edit_score(main_chains : Vec<Vec<String>>) -> f64 {

        let mut score = 0.0;
        let mut max_score = 0.0;

        for (i, chain_a) in main_chains.iter().enumerate() {
            let length_a = chain_a.len();
            for (j, chain_b) in main_chains.iter().enumerate() {
                let length_b = chain_b.len();
                if i != j {
                    score += levenshtein_distance_vec(chain_a, chain_b) as f64;
                    max_score += (length_a + length_b) as f64;
                }
            }
        }

        1.0 - (score/max_score)

    }

}