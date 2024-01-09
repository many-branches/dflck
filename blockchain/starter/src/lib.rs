pub mod mini_chain;
pub mod network;

pub mod simulation {

    use crate::mini_chain::block::Transaction;
    use crate::mini_chain::node::{ChannelFullNode, FullNode};
    use crate::mini_chain::metadata::{BlockchainMetadata, BlockchainMetadataOperations};
    use crate::mini_chain::client::{ChannelClient, ClientOperations};
    use crate::mini_chain::chain::{Blockchain, BlockchainOperations, BlockchainAnaylsisOperations};
    use crate::network::Network;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tokio::sync::RwLock;
    use colored::*;
    use std::io::{self, Write};
    use tokio::runtime::Builder;

    /// Runs the simulation
    /// Duration in seconds
    /// Client delay in milliseconds
    pub async fn simulation(duration : u64, client_delay : u64, node_count : u64) -> Result<(), String> {

        let rng = &mut rand::thread_rng();
        let timeout_duration = std::time::Duration::from_secs(duration);

        let mut network = Network::common_network_quality();
        let mut client = ChannelClient::new(network.channels.mempool_transaction_ingress_sender.clone());
        let mut full_nodes = Vec::new();
        let common_metadata = Arc::new(RwLock::new(BlockchainMetadata::fast_chain()));

        for _ in 0..node_count {
            let mut full_node = ChannelFullNode::default();
            full_node.chain_metadata = common_metadata.clone();
            network.pipe_channel_full_node(&mut full_node);
            full_nodes.push(full_node);
        }

        // store some full_nodes for analysis
        let analysis_full_nodes = full_nodes.clone(); 

        let _ = tokio::time::timeout(
            timeout_duration,
            async {
                tokio::try_join!(
                    async {
                      
                        let mut runs = Vec::new();
                            for full_node in &full_nodes {
                                let cloned_node = full_node.clone();
                                runs.push(async move {
                                    cloned_node.run_full_node().await?;
                                    Ok::<(), String>(())
                                });
                            }
                        futures::future::try_join_all(
                            runs
                        ).await?;
                        Ok::<(), String>(())
                    
                    },
                    async {
                        loop {
                            let transaction = Transaction::random_now(rng);
                            client.submit_transaction(transaction).await?;
                            tokio::time::sleep(Duration::from_millis(client_delay)).await;
                        }
                        Ok::<(), String>(())
                    },
                    async {
                        loop {
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            // println!("{}", "Health check! If you don't see this you may have a livelock/overutilization".green());
                            // println!("{}", "If you see the health check above, but don't have block-building activity; you may have a deadlock.".purple());
                        }
                        Ok::<(), String>(())
                    },
                    async {
                        loop {
                            {
                                let now = SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .expect("Time went backwards")
                                    .as_secs();
                                let slot = common_metadata.read().await.get_slot_start_time(now).await?;
                                let display_slot = format!("{}", slot).underline().bold().blue();
                                println!("\n{}", display_slot);
                                let mut i = 0;
                                for full_node in &full_nodes {
                                    let full_node_display = format!("Full Node #{}", i).underline().bold().magenta();
                                    println!("{}", full_node_display);
                                    let chain = full_node.chain.read().await;
                                    chain.pretty_print_tree().await?;
                                    io::stdout().flush().unwrap();
                                    i += 1;
                                }
                            }
                            tokio::time::sleep(Duration::from_secs(
                                common_metadata.read().await.get_slot_secs().await?
                            )).await; 
                        }
                        Ok::<(), String>(())
                    },
                    async {
                    
                        network.run_network().await?;
                        
                        Ok::<(), String>(())
        
                    }
                ).expect("Simulation failed");
            }
        ).await;

        // analysis
        let mut main_chains = Vec::new();
        for full_node in analysis_full_nodes {
            let chain = full_node.chain.read().await;
            chain.pretty_print_tree().await?;
            main_chains.push(
                chain.get_main_chain().await?
            );
        }
        let edit_score = Blockchain::chains_edit_score(
            main_chains.iter().map(|blocks| {
                blocks.iter().map(|block| block.hash.clone()).collect::<Vec<String>>()
            }).collect::<Vec<Vec<String>>>()
        );
    
        println!("{} {}", "Edit score:".blue().underline(), edit_score);

        assert!(edit_score > 0.9);

        Ok(())

    }


    /// TODO [S2]
    #[tokio::test]
    async fn test_simulation() -> Result<(), String> {

        simulation(60, 50, 5).await?;
        Ok(())

    }
}
