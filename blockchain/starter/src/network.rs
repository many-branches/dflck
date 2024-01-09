use futures::stream::FuturesUnordered;
use async_channel::{Sender, Receiver};
use tokio::time::sleep;
use tracing::info;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock};
use crate::mini_chain::block::{
    Block,
    Transaction
};
use crate::mini_chain::metadata;
use crate::mini_chain::node::{
    ChannelFullNode, FullNode
};
use tokio::sync::mpsc;
use rand::Rng;
use std::fmt::Debug;

#[derive(Clone)]
pub struct Channels {

    pub proposed_block_egress_senders: Vec<Sender<Block>>,
    pub proposed_block_ingress_sender: Sender<Block>,
    pub proposed_block_ingress_receiver: Receiver<Block>,

    pub mined_block_egress_senders: Vec<Sender<Block>>,
    pub mined_block_ingress_sender: Sender<Block>,
    pub mined_block_ingress_receiver: Receiver<Block>,

    pub transaction_request_egress_senders: Vec<Sender<String>>,
    pub transaction_request_ingress_sender: Sender<String>,
    pub transaction_request_ingress_receiver : Receiver<String>,

    pub transaction_response_egress_senders: Vec<Sender<Option<Block>>>,
    pub transaction_response_ingress_sender: Sender<Option<Block>>,
    pub transaction_response_ingress_receiver: Receiver<Option<Block>>,

    pub mempool_transaction_egress_senders: Vec<Sender<Transaction>>,
    pub mempool_transaction_ingress_sender: Sender<Transaction>,
    pub mempool_transcation_ingress_receivers: Receiver<Transaction>,

}

impl Default for Channels {
    fn default() -> Self {
        let (proposed_block_ingress_sender, proposed_block_ingress_receiver_temp) = async_channel::unbounded();
        let (mined_block_ingress_sender, mined_block_ingress_receiver_temp) =  async_channel::unbounded();
        let (transaction_request_ingress_sender, transaction_request_ingress_receiver_temp) =  async_channel::unbounded();
        let (transaction_response_ingress_sender, transaction_response_ingress_receiver_temp) = async_channel::unbounded();
        let (mempool_transaction_ingress_sender, mempool_transaction_ingress_receiver_temp) =  async_channel::unbounded();

        Self {
            proposed_block_egress_senders: Vec::new(),
            proposed_block_ingress_sender,
            proposed_block_ingress_receiver: proposed_block_ingress_receiver_temp,

            mined_block_egress_senders: Vec::new(),
            mined_block_ingress_sender: mined_block_ingress_sender,  // minor typo: should it be "mined" instead of "mined"?
            mined_block_ingress_receiver: mined_block_ingress_receiver_temp,

            transaction_request_egress_senders: Vec::new(),
            transaction_request_ingress_sender,
            transaction_request_ingress_receiver: transaction_request_ingress_receiver_temp,

            transaction_response_egress_senders: Vec::new(),
            transaction_response_ingress_sender,
            transaction_response_ingress_receiver: transaction_response_ingress_receiver_temp,

            mempool_transaction_egress_senders: Vec::new(),
            mempool_transaction_ingress_sender,
            mempool_transcation_ingress_receivers: mempool_transaction_ingress_receiver_temp,
        }
    }
}

impl Channels {

    pub fn pipe_channel_full_node(&mut self, full_node : &mut ChannelFullNode) {

        // For proposed block channel
        self.proposed_block_egress_senders.push(full_node.proposed_block_sender.clone());
        full_node.proposed_block_sender = self.proposed_block_ingress_sender.clone();

        // For mined block channel
        self.mined_block_egress_senders.push(full_node.mined_block_sender.clone());
        full_node.mined_block_sender = self.mined_block_ingress_sender.clone();

        // For mempool transaction channel
        self.mempool_transaction_egress_senders.push(full_node.mempool_transaction_sender.clone());
        full_node.mempool_transaction_sender = self.mempool_transaction_ingress_sender.clone();

        // For synchronizer request channel
        self.transaction_request_egress_senders.push(full_node.synchronizer_request_sender.clone());
        full_node.synchronizer_request_sender = self.transaction_request_ingress_sender.clone();

        // For synchronizer response channel
        self.transaction_response_egress_senders.push(full_node.synchronizer_response_sender.clone());
        full_node.synchronizer_response_sender = self.transaction_response_ingress_sender.clone();


    }

}


#[derive(Clone)]
pub struct NetworkMetadata {
    pub drop_rate: f64,
    pub latency_range_ms: (u64, u64),
}

impl Default for NetworkMetadata {
    fn default() -> Self {
        NetworkMetadata {
            drop_rate: 0.0,
            latency_range_ms: (0, 0),
        }
    }
}

impl NetworkMetadata {

    pub fn common_network_quality() -> Self {
        NetworkMetadata {
            drop_rate: 0.005,
            latency_range_ms: (12, 70),
        }
    }

    pub fn should_drop(&self) -> bool {
        rand::thread_rng().gen_bool(self.drop_rate)
    }

    pub async fn introduce_latency(&self) {
        let (min, max) = self.latency_range_ms;
        let latency = if min == max { min}  else { rand::thread_rng().gen_range(min..max) };
        tokio::time::sleep(tokio::time::Duration::from_millis(latency)).await;
    }

}

#[derive(Clone)]
pub struct Network {
    pub channels: Channels,
    pub metadata: Arc<RwLock<NetworkMetadata>>,
}

impl Default for Network {
    fn default() -> Self {
        Network {
            channels: Channels::default(),
            metadata: Arc::new(RwLock::new(NetworkMetadata::default())),
        }
    }
}

impl Network {

    pub fn common_network_quality() -> Self {
        Self {
            channels: Channels::default(),
            metadata: Arc::new(RwLock::new(NetworkMetadata::common_network_quality())),
        }
    }

    pub fn pipe_channel_full_node(&mut self, full_node : &mut ChannelFullNode) {
        self.channels.pipe_channel_full_node(full_node);
    }

    // TODO [R4]: Explain how this broadcast_message function works
    pub async fn broadcast_message<T: Clone + Send + Debug + 'static>(
        &self,
        ingress_receiver: Receiver<T>,
        egress_senders: &[Sender<T>],
    ) -> Result<(), String> {

        loop {
            let message = ingress_receiver.recv().await.unwrap();
    
            let mut broadcasts = Vec::new();
            for sender in egress_senders {
                let cloned_message = message.clone();
                let metadata = self.metadata.clone();
                let sender = sender.clone();
                broadcasts.push(async move {
                    let metadata = metadata.read().await;
                    if !metadata.should_drop() {
                        metadata.introduce_latency().await;
                        sender.send(cloned_message).await.unwrap_or(());
                    }
                });
            }

            // futures::future::join_all(broadcasts).await;

            // TODO: want to ensure better completion of broadcast
            // will separately run spawned tasks check
            tokio::spawn(futures::future::join_all(broadcasts));
            // go idea to yield the processor here
            tokio::task::yield_now().await;
        }
    }

    pub async fn run_network(&mut self) -> Result<(), String> {
        let proposed_block_broadcast = 
            self.broadcast_message(self.channels.proposed_block_ingress_receiver.clone(), &self.channels.proposed_block_egress_senders);
        let mined_block_broadcast = 
            self.broadcast_message(self.channels.mined_block_ingress_receiver.clone(), &self.channels.mined_block_egress_senders);
        let transaction_request_broadcast = 
            self.broadcast_message(self.channels.transaction_request_ingress_receiver.clone(), &self.channels.transaction_request_egress_senders);
        let transaction_response_broadcast = 
            self.broadcast_message(self.channels.transaction_response_ingress_receiver.clone(), &self.channels.transaction_response_egress_senders);
        let mempool_transaction_broadcast = 
            self.broadcast_message(self.channels.mempool_transcation_ingress_receivers.clone(), &self.channels.mempool_transaction_egress_senders);

        tokio::try_join!(
            proposed_block_broadcast,
            mined_block_broadcast,
            transaction_request_broadcast,
            transaction_response_broadcast,
            mempool_transaction_broadcast
        )?;

        Ok(())
    }

}

#[cfg(test)]
pub mod test {

    use std::time::Duration;

    use tokio::time::timeout;

    use super::*;

    #[tokio::test]
    async fn test_delivers() -> Result<(), String> {

        let rng = &mut rand::thread_rng();
        let slot_secs = 8;

        let mut node_1 = ChannelFullNode::default();
        let mut node_2 = ChannelFullNode::default();

        let mut network = Network::default();
        network.pipe_channel_full_node(&mut node_1);
        network.pipe_channel_full_node(&mut node_2);

        let sent_block = Block::random(
            rng, 
            slot_secs
        );

        node_1.proposed_block_sender.send(sent_block.clone()).await.unwrap();

        let _ = timeout(Duration::from_millis(1), async {
            network.run_network().await.unwrap();
        }).await;

        let received_block = node_2.proposed_block_receiver.recv().await.unwrap();
        assert_eq!(sent_block.hash, received_block.hash);

        Ok(())

    }

}
