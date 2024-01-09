
use std::collections::{BTreeSet, HashSet, HashMap};
use std::sync::Arc;
use std::vec;
use tokio::sync::RwLock;
use tokio::time::error::Elapsed;

use super::metadata::{
    BlockchainMetadata,
    BlockchainMetadataOperations
};
use super::block::{
    Block,
    Transaction, self
};
use super::chain::{
    Blockchain, 
    BlockchainOperations
};
use super::mempool::{Mempool, MempoolOperations};
use async_trait::async_trait;
use async_channel::{Sender, Receiver};
use tokio::try_join;
use tokio::time::{sleep, Duration, timeout};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

#[async_trait]
pub trait Proposer {

    // Builds a block from the mempool
    async fn build_block(&self) -> Result<Block, String>;

    // Sends a block to the network for mining
    async fn send_proposed_block(&self, block : Block)->Result<(), String>;

    // Propose a block to the network and return the block proposed
    async fn propose_next_block(&self) -> Result<(), String> {
        // println!("Proposing block!");
        let block = self.build_block().await?;
        self.send_proposed_block(block.clone()).await?;
        // println!("Sent proposed block!");
        Ok(())
    }

}

#[async_trait]
pub trait ProposerController : Proposer {

    // Runs the proposer loop
    async fn run_proposer(&self) -> Result<(), String> {
        loop {
            self.propose_next_block().await?;
            tokio::task::yield_now().await;
        }
    }

}

#[async_trait]
pub trait Miner {

    // Receives a block over the network and attempts to mine
    async fn receive_proposed_block(&self) -> Result<Block, String>;

    // Sends a block to the network for validation
    async fn send_mined_block(&self, block : Block) -> Result<(), String>;

    // Mines a block and sends it to the network
    async fn mine_next_block(&self) -> Result<(), String> {
        let block = self.receive_proposed_block().await?;
        // default implementation does no mining
        self.send_mined_block(block.clone()).await?;
        Ok(())
    }

}


#[async_trait]
pub trait MinerController : Miner {

    // Runs the miner loop
    async fn run_miner(&self) -> Result<(), String> {
        loop {
            self.mine_next_block().await?;

            // go idea to yield the processor here
            tokio::task::yield_now().await;

        }
    }

}

// TODO [R3]: Identify the pattern that is forced by this trait--assuming the Verifier would in fact mutate state.
#[async_trait]
pub trait Verifier {

    // Receives a block over the network and attempts to verify
    async fn receive_mined_block(&self) -> Result<Block, String>;

    // Verifies a block
    async fn verify_block(&self, block: &Block) -> Result<bool, String>;

    // Synchronizes the chain
    async fn synchronize_chain(&self, block: Block) -> Result<(), String>;

    // Verifies the next block
    async fn verify_next_block(&self) -> Result<(), String> {
        let block = self.receive_mined_block().await?;
        let valid = self.verify_block(&block).await?;
        if valid {
            self.synchronize_chain(block).await?;
        }
        Ok(())
    }

}

#[async_trait]
pub trait Synchronizer {

    async fn receive_block_request(&self) -> Result<String, String>;

    async fn check_for_block(&self, block_hash : String) -> Result<Option<Block>, String>;

    async fn send_block_response(&self, block : Option<Block>) -> Result<(), String>;

    async fn check_for_next_block(&self) -> Result<(), String> {
        let block_hash = self.receive_block_request().await?;
        let block = self.check_for_block(block_hash.clone()).await?;
        self.send_block_response(block).await?;
        Ok(())
    }

    async fn request_block(&self, block_hash : String) -> Result<Option<Block>, String>;

}

// TODO [R2]: Compare an contrast the conditional trait implementations.
#[async_trait]
pub trait SynchronizerSerivce : Synchronizer {
    
    async fn run_synchronizer_service(&self) -> Result<(), String> {
        loop {
            self.check_for_next_block().await?;
            // go idea to yield the processor here
            tokio::task::yield_now().await;
        }
    }
    
    
}


#[async_trait]
pub trait VerifierController : Verifier {

    // Runs the verifier loop
    async fn run_verifier(&self) -> Result<(), String> {
        loop {
            self.verify_next_block().await?;
            // good idea to yield the processor here
            tokio::task::yield_now().await;
        }
    }

}

#[async_trait]
pub trait MempoolService {

    async fn receive_mempool_transaction(&self) -> Result<Transaction, String>;

    async fn add_transaction_to_mempool(&self, transaction : Transaction) -> Result<(), String>;

}


#[async_trait]
pub trait MempoolServiceController : MempoolService {
    
    async fn run_mempool_service(&self) -> Result<(), String> {
        loop {
            let transaction = self.receive_mempool_transaction().await?;
            self.add_transaction_to_mempool(transaction).await?;
            // good idea to yield the processor here
            tokio::task::yield_now().await;
        }
    }

}

#[async_trait]
pub trait MempoolController {

    async fn run_mempool(&self) -> Result<(), String>;

}

#[async_trait]
pub trait BlockchainController {

    async fn run_blockchain(&self) -> Result<(), String>;

}

#[async_trait]
pub trait Initializer {
    
    async fn initialize(&self) -> Result<(), String>;
    
}

#[async_trait]
pub trait FullNode 
    : Initializer + 
    ProposerController + MinerController + VerifierController + 
    MempoolController + MempoolServiceController +
    BlockchainController + Synchronizer + SynchronizerSerivce {

    /// Runs the full node loop
    async fn run_full_node(&self) -> Result<(), String> {
        self.initialize().await?;
        loop {
            let error = try_join!(
                self.run_proposer(),
                self.run_miner(),
                self.run_verifier(),
                self.run_mempool(),
                self.run_blockchain(),
                self.run_mempool_service(),
                self.run_synchronizer_service()
            ).expect_err("One of the controllers stopped without error.");

            println!("Error: {}", error);
            // go idea to yield the processor here
            tokio::task::yield_now().await;
        }

    }

}

#[derive(Clone, Debug)]
pub struct ChannelFullNode {

    /// The channel for sending proposed blocks
    pub proposed_block_sender: Sender<Block>,
    pub proposed_block_receiver: Receiver<Block>, // TODO: what is the danger of placing this in an Arc<RwLock<...>>?

    pub mined_block_sender: Sender<Block>,
    pub mined_block_receiver: Receiver<Block>,
   
    pub chain : Arc<RwLock<Blockchain>>,
    pub chain_metadata : Arc<RwLock<BlockchainMetadata>>,

    pub mempool : Arc<RwLock<Mempool>>,
    pub mempool_transaction_receiver : Receiver<Transaction>,
    pub mempool_transaction_sender : Sender<Transaction>, // helps make lifetime management easier

    pub synchronizer_request_sender : Sender<String>,
    pub synchronizer_request_receiver : Receiver<String>,

    pub synchronizer_response_sender : Sender<Option<Block>>,
    pub synchronizer_response_receiver : Receiver<Option<Block>>,

    pub synchronizer_response_set : Arc<RwLock<HashMap<String, (u64, Option<Block>)>>>


}

impl ChannelFullNode {


}

impl Default for ChannelFullNode {
    fn default() -> Self {
        let (proposed_block_sender, proposed_block_receiver_temp) =  async_channel::unbounded();
        let (mined_block_sender, mined_block_receiver_temp) =  async_channel::unbounded();
        let (mempool_transaction_sender, mempool_transaction_receiver_temp) =  async_channel::unbounded();
        let (synchronizer_request_sender, synchronizer_request_receiver_temp) =  async_channel::unbounded();
        let (synchronizer_response_sender, synchronizer_response_receiver_temp) =  async_channel::unbounded();

        Self {
            proposed_block_sender,
            proposed_block_receiver: proposed_block_receiver_temp,
            mined_block_sender,
            mined_block_receiver: mined_block_receiver_temp,
            chain: Arc::new(RwLock::new(Blockchain::default())), // Assuming Blockchain has a Default impl
            chain_metadata: Arc::new(RwLock::new(BlockchainMetadata::default())), // Assuming BlockchainMetadata has a Default impl
            mempool: Arc::new(RwLock::new(Mempool::default())), // Assuming Mempool has a Default impl
            mempool_transaction_receiver: mempool_transaction_receiver_temp,
            mempool_transaction_sender,
            synchronizer_request_sender,
            synchronizer_request_receiver: synchronizer_request_receiver_temp,
            synchronizer_response_sender,
            synchronizer_response_receiver: synchronizer_response_receiver_temp,
            synchronizer_response_set : Arc::new(RwLock::new(HashMap::new()))
        }
    }
}


#[async_trait]
impl Proposer for ChannelFullNode {

    async fn build_block(&self) -> Result<Block, String> {

        // Get the neded metadata
        let (
            slot_secs,
            block_size,
            proposer_time,
            proposer_wait_time
        )= {
            let chain_metadata = self.chain_metadata.read().await;
            (
                chain_metadata.get_slot_secs().await?,
                chain_metadata.get_block_size().await?,
                chain_metadata.get_maximum_proposer_time().await?,
                chain_metadata.get_proposer_wait_ms().await?
            )
        };
    
        // Initialize the block
        let mut block = Block::new(BTreeSet::new(), "", slot_secs);

        // Set a time bound for building the block
        let time_bound = SystemTime::now() + Duration::from_secs(proposer_time);
    
        let task_mempool = self.mempool.clone();
        // let block = tokio::task::spawn(async move {

            for _ in 0..block_size {

                let next = {
                    let mut mempool = task_mempool.write().await;
                    mempool.pop_transaction().await?
                };
        
                if let Some(transaction) = next {
                    block.add_transaction(transaction);
                } else {
                    sleep(Duration::from_millis(proposer_wait_time)).await;
                }

                // Break if we've run out of time
                if SystemTime::now() > time_bound {
                    break;
                }

            }

            // Ok::<Block, String>(block)

        // }).await.map_err(|e| e.to_string())??;
    
        Ok(block)
        
    }

    async fn send_proposed_block(&self, block : Block) -> Result<(), String> {
        /*let chain = self.chain.read().await;
        let leaves = chain.get_leaf_hashes().await?;
        for leaf in leaves {
            let mut cloned_block = block.clone();
            cloned_block.previous_hash = leaf;
            self.proposed_block_sender.send(cloned_block).await.map_err(|e| e.to_string())?;
            tokio::task::yield_now().await;
        }
        // let chain_tip = chain.get_chain_tip().await?.ok_or("No chain tip".to_string())?;
        // block.previous_hash = chain_tip.hash;
        // self.proposed_block_sender.send(block).await.map_err(|e| e.to_string())?;*/
        self.proposed_block_sender.send(block).await.map_err(|e| e.to_string())?;
        Ok(())
    }

}

impl ProposerController for ChannelFullNode {}

#[async_trait]
impl Miner for ChannelFullNode {

    async fn send_mined_block(&self, block : Block) -> Result<(), String> {
        let slot_secs = {
            let chain_metadata = self.chain_metadata.read().await;
            chain_metadata.get_slot_secs().await?
        };

        if block == Block::genesis(slot_secs) {
            println!("Resending genesis block!");
            return Ok(())
        }

        /*{ // make sure we have the block, so that we can keep building the chaine
            let mut chain = self.chain.write().await;
            chain.add_block(block.clone()).await?;
        }*/

        self.mined_block_sender.send(block).await.map_err(|e| e.to_string())
    }

    async fn receive_proposed_block(&self) -> Result<Block, String> {
        
        let mut rx = self.proposed_block_receiver.clone();
        let blocks = Arc::new(RwLock::new(BTreeSet::new()));
        let timeout_blocks = blocks.clone();
        let slot_secs = {
            let metadata = self.chain_metadata.read().await;
            metadata.get_slot_secs().await?
        };

        let _ : Result<Result<(), String>, Elapsed>= timeout(Duration::from_secs(
            slot_secs
        ), async {
            let mut blocks =timeout_blocks.write().await;
            for _ in 0..5 {
                let block = rx.recv().await;
                match block {
                    Ok(block) => {
                        blocks.insert(block);
                    },
                    Err(_) => {}
                }
            }
            Ok(())
        }).await;

        let blocks = blocks.read().await;
        let genesis = Block::genesis(slot_secs);
        let block = blocks.iter().next().unwrap_or(&genesis);

        Ok(block.clone())

    }

    async fn mine_next_block(&self) -> Result<(), String> {

        // println!("Mining block!");
        // get the needed metadata
        let (difficulty, slot_secs) = {
            let chain_metadata = self.chain_metadata.read().await;
            (
                chain_metadata.get_difficulty().await?,
                chain_metadata.get_slot_secs().await?
            )
        };

        let chain_tip = {
            let chain = self.chain.read().await;
            chain.get_chain_tip().await?.unwrap_or(Block::genesis(slot_secs))
        };

        // TODO: The code below clones each transaction during selection
        // TODO: Propose and implement a better model. 
        // TODO: You may modify the Block implementation.
        // * Select transactions from the block.
        // receive the block
        let mut block = self.receive_proposed_block().await?;

        let mut selected_transactions: Vec<Transaction> = vec![];

        // collect transactions to be dropped (clone if necessary)
        {
            let chain = self.chain.read().await;
            for txn in &block.transactions {
                if chain.has_transaction(&txn).await? {
                    selected_transactions.push(txn.clone())
                }
            }
            tokio::task::yield_now().await;
        }

        // drop transactions that are already in the chain
        for txn in &selected_transactions {
            block.drop_transaction(txn); // This method may need to change based on how it's defined
        }

        let leaves = {
            let chain = self.chain.read().await;
            chain.get_leaf_hashes().await?
        };

        // prepare blocks for each of the leafs
        let mut blocks = Vec::new();
        for leaf in leaves {
            let mut cloned_block = block.clone();
            cloned_block.previous_hash = leaf;
            blocks.push(cloned_block);
        }

        // mine the blocks
        let mut mine_handlers = Vec::new();

        for mut block in blocks {
            mine_handlers.push(tokio::task::spawn_blocking(move || {
                block.mine_block(difficulty);
                Ok::<Block, String>(block)
            }));
        }

        let res = futures::future::try_join_all(
            mine_handlers
        ).await.map_err(|e| e.to_string())?;

        for block in res {
            match block {
                Ok(block) => {
                    // introduce bias towards chain tip
                    {
                        let mut chain = self.chain.write().await;
                        chain.add_block(block.clone()).await?;
                        if (block.previous_hash == chain_tip.hash) 
                            && !chain.has_block(block.hash.clone()).await? {
                                chain.add_block(block.clone()).await?;
                            }
                    }
                    self.send_mined_block(block).await?;
                },
                Err(e)=> {
                    return Err(e);
                } 
            }
        }

        Ok(())


    }

}

impl MinerController for ChannelFullNode {}

#[async_trait]
impl Verifier for ChannelFullNode {

    // Receives a block over the network and attempts to verify
    async fn receive_mined_block(&self) -> Result<Block, String> {
        let mut rx = self.mined_block_receiver.clone();
        let block = rx.recv().await.map_err(|e| e.to_string())?;
        Ok(block)
    }

    // Verifies a block
    async fn verify_block(&self, block: &Block) -> Result<bool, String> {

        let (
            slot_secs,
            difficulty
        ) = {
            let chain_metadata = self.chain_metadata.read().await;
            (
                chain_metadata.get_slot_secs().await?,
                chain_metadata.get_difficulty().await?
            )
        };
        Ok(block.verify_self(slot_secs, difficulty))
    }

    // Update chain
    async fn synchronize_chain(&self, block: Block) -> Result<(), String> {

        {

            let has_block = {
                let chain = self.chain.read().await;
                chain.has_block(block.hash.clone()).await?
            };
            if !has_block {
                // 
                let parent_block = self.request_block(block.previous_hash.clone()).await?;
                match parent_block {
                    Some(parent_block) => {
                        tokio::task::yield_now().await;
                        self.synchronize_chain(parent_block).await?;
                    },
                    None => {}
                }
                
            }

        }

        {
            let mut chain = self.chain.write().await;
            chain.add_block(block).await?;
        }

        Ok(())

    }


}

impl VerifierController for ChannelFullNode {}


/// Synchronizer response helpers
impl ChannelFullNode {

    async fn add_synchronizer_request(&self, block_hash : String) -> Result<Option<Block>, String> {

        // no one else should write while we're doing this
        let mut synchronizer_response_set = self.synchronizer_response_set.write().await;

        // check if the block is already in the set
        let (count, block) = {
            let val = synchronizer_response_set.get(&block_hash);
            match val {
                Some((count, block)) => (*count, block.clone()),
                None => (0, None)
            }
        };

        if let Some(block) = block {
            return Ok(Some(block));
        }

        // increment the request count
        synchronizer_response_set.insert(block_hash, (count + 1, block));
        Ok(None)

    }

    async fn add_synchronizer_response(&self, block : Block) -> Result<(), String> {

        // no one else should write while we're doing this
        let mut synchronizer_response_set = self.synchronizer_response_set.write().await;

        let count = {
            let val = synchronizer_response_set.get(&block.hash);
            match val {
                Some((count, _)) => *count,
                None => 0
            }
        };

        if count == 0 {
            return Ok(());
        }

        synchronizer_response_set.insert(block.hash.clone(), (count, Some(block)));
        Ok(())
    }

    async fn get_synchronizer_response(&self, block_hash : String, last_chance : bool) -> Result<Option<Block>, String> {

        // no-one else should write while we're doing this
        let mut synchronizer_response_set = self.synchronizer_response_set.write().await;
        
        let (count, block) = {
            let val = synchronizer_response_set.get(&block_hash);
            match val {
                Some((count, block)) => (*count, block.clone()),
                None => (0, None)
            }
        };

        let new_count = if count < 1 { 0 } else { count - 1};

        let res = if let Some(block) = block {
            // decrement the count
            synchronizer_response_set.insert(block.hash.clone(), (new_count, Some(block.clone())));
            Ok(Some(block))
        } else if last_chance {
            // decrement the count
            synchronizer_response_set.insert(block_hash.clone(), (new_count, None));
            Ok(None)
        } else {
            Ok(None)
        };

        if new_count < 1 {
            synchronizer_response_set.remove(&block_hash);
        }

        res

    }


}

#[async_trait]
impl Synchronizer for ChannelFullNode {

    async fn receive_block_request(&self) -> Result<String, String> {
        let mut rx = self.synchronizer_request_receiver.clone();
        rx.recv().await.map_err(|e| e.to_string())
    }

    async fn check_for_block(&self, block_hash : String) -> Result<Option<Block>, String> {

        // check if you have it
        let chain = self.chain.read().await;
        chain.get_block(block_hash.clone()).await
      
    }

    async fn send_block_response(&self, block : Option<Block>) -> Result<(), String> {
        self.synchronizer_response_sender.send(block).await.map_err(|e| e.to_string())
    }

    async fn request_block(&self, block_hash : String) -> Result<Option<Block>, String> {
        
        // return Ok(None);

        // send the request--adding it to the request set
        {
            let request_sender = self.synchronizer_request_sender.clone();
            self.add_synchronizer_request(block_hash.clone()).await?;
            request_sender.send(block_hash.clone()).await.map_err(|e| e.to_string())?;
        }


        // run a timeout loop
        let res : Result<Option<Block>, String> = timeout(Duration::from_millis(200), async {

            loop {

                // receive one response
                let block = {
                    let response_receiver = self.synchronizer_response_receiver.clone();
                    response_receiver.recv().await.unwrap_or(None)
                };
            
                // if it's yours, return it
                if let Some(block) = block {
                    if block.hash == block_hash {
                        return Ok(Some(block));
                    } else {
                        // otherwise add it in to the set
                        self.add_synchronizer_response(block).await?;
                    }
                }

                // check if you're in the set
                let res = self.get_synchronizer_response(block_hash.clone(), false).await?;

                // if you are, return it
                if let Some(block) = res {
                    return Ok(Some(block));
                }

                // otherwise sleep for a bit
                sleep(Duration::from_millis(4)).await;

            }

        }).await.unwrap_or(Ok(None));

        // sort out the final result
        match res {
            Ok(res)=>{
                match res {
                    Some(block) => {
                        Ok(Some(block))
                    },
                    None => {
                        // last chance saloon
                        Ok(self.get_synchronizer_response(block_hash.clone(), true).await?)
                    }
                }
            },
            Err(e) => Err(e)
        }

    }

}

impl SynchronizerSerivce for ChannelFullNode {}

#[async_trait]
impl MempoolController for ChannelFullNode {

    async fn run_mempool(&self) -> Result<(), String> {

        // get metadata
        let mempool_reentrancy_secs = {
            let chain_metadata = self.chain_metadata.read().await;
            chain_metadata.get_mempool_reentrancy_secs().await?
        };

        loop {
            sleep(Duration::from_secs(mempool_reentrancy_secs)).await;
            let mut mempool = self.mempool.write().await;
            mempool.tick_mempool().await?;
            tokio::task::yield_now().await;
        }

    }    

}

#[async_trait]
impl MempoolService for ChannelFullNode {

    async fn receive_mempool_transaction(&self) -> Result<Transaction, String> {
        let mut rx = self.mempool_transaction_receiver.clone();
        rx.recv().await.map_err(|e| e.to_string())
    }

    async fn add_transaction_to_mempool(&self, transaction : Transaction) -> Result<(), String> {
        let mut mempool = self.mempool.write().await;
        mempool.push_transaction(transaction).await
    }

}

impl MempoolServiceController for ChannelFullNode {}

#[async_trait]
impl BlockchainController for ChannelFullNode {

    async fn run_blockchain(&self) -> Result<(), String> {

        // get metadata
        let (
            fork_resolution_secs,
            slot_secs
         ) = {
            let chain_metadata = self.chain_metadata.read().await;
            (
                chain_metadata.get_fork_resolution_secs().await?,
                chain_metadata.get_slot_secs().await?
            )
        };

        let mut last = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();

        loop {

            // big nap
            sleep(Duration::from_secs(fork_resolution_secs)).await;
            let mut chain = self.chain.write().await;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs();

            // little naps
            while now < (last + fork_resolution_secs) {
                sleep(Duration::from_secs(slot_secs)).await;
            }

            // time for work
            chain.resolve_chain().await?;
            last = now;
            tokio::task::yield_now().await;

        }


    }

}

#[async_trait]
impl Initializer for ChannelFullNode {

    async fn initialize(&self) -> Result<(), String> {
        let mut chain = self.chain.write().await;
        chain.genesis().await?;
        Ok(())
    }

}

       
// TODO [R2]: Compare an contrast the conditional trait implementations.
impl FullNode for ChannelFullNode {}

#[cfg(test)]
mod test {

    use super::*;
    use std::sync::Arc;
    use tokio::sync::mpsc::{self, Sender, Receiver};
    use tokio::sync::RwLock;
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::time::Duration;
    use super::super::client::{ClientOperations, MempoolAccessClient};
    use colored::*;
    use super::super::chain::BlockchainAnaylsisOperations;

    async fn run_full_node(duration : u64, client_delay : u64) -> Result<(), String> {

        let rng = &mut rand::thread_rng();

        let mut full_node = ChannelFullNode::default();
        full_node.chain_metadata = Arc::new(RwLock::new(BlockchainMetadata::fast_chain()));

        // drop the synchronizer response sender so that it always sends none
        //  full_node.synchronizer_response_sender = mpsc::channel(16).0; // you may want to uncomment this to see the effect

        let mut client = MempoolAccessClient {
            mempool : full_node.mempool.clone(),
        };

        let logger_chain = full_node.chain.clone();
        let logger_metadata = full_node.chain_metadata.clone();

        let timeout_duration = Duration::from_secs(duration); // two seconds after last fork resolution

        let txns = Arc::new(RwLock::new(vec![]));

        let _ = tokio::time::timeout(
            timeout_duration,
            async {tokio::join!(
                async {
                    full_node.run_full_node().await.expect("Full node failed");
                },
                async {
                    loop {
                        let transaction = Transaction::random_now(rng);
                        txns.write().await.push(transaction.clone());
                        client.submit_transaction(transaction).await.expect("Client failed");
                        tokio::time::sleep(Duration::from_millis(client_delay)).await;
                        tokio::task::yield_now().await;
                    }
                },
                async {
                    loop {
                        {
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .expect("Time went backwards")
                                .as_secs();
                            let slot = logger_metadata.read().await.get_slot_start_time(now).await.expect("Failed to get slot");
                            let display_slot = format!("{}", slot).underline().bold().blue();
                            println!("\n{}", display_slot);
                            let blockchain = logger_chain.read().await;
                            blockchain.pretty_print_tree().await.expect("Failed to print tree");
                        }
                        tokio::time::sleep(Duration::from_secs(
                            logger_metadata.read().await.get_slot_secs().await.expect("Failed to get slot secs")
                        )).await; 
                    }
                }
            )}
        ).await;

        let analysis_chain = full_node.chain.write().await;
        println!(
            "\n{}", 
            "Final Chain".blue().bold().underline()
        );
        analysis_chain.pretty_print_tree().await?;

        let analysis_main_chain = analysis_chain.get_main_chain().await?;
        let analysis_trunk = analysis_chain.get_trunk().await?;

        assert!(analysis_main_chain.len() > 0);
        assert!(analysis_trunk.len() > 0);
        assert!(analysis_chain.is_trunk_tall().await?);

        println!(
            "\n{}", 
            "Chain has a tall trunk!".blue().bold().underline(),
        );

        let txn_success_rate = analysis_chain.compute_transaction_success_rate(
            &txns.read().await.to_vec()
        ).await?;

        println!(
            "\n{}: {}%", 
            "Transaction Success Rate".blue().bold().underline(),
            txn_success_rate * 100.0
        );
        assert!(txn_success_rate > 0.75);

        Ok(())

    }

    /// TODO [S1]
    #[tokio::test]
    async fn test_run_full_node()->Result<(), String>{

        run_full_node(60, 75).await

    }

}
