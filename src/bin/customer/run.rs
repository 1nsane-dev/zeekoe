use std::time::Duration;

use dialectic::offer;
use {
    async_trait::async_trait,
    rand::rngs::StdRng,
    std::net::{IpAddr, Ipv4Addr},
    std::sync::Arc,
    tokio::sync::broadcast,
};

use zeekoe::{
    customer::database::zkchannels_state::{self, ZkChannelState},
    customer::server::{Chan, Server},
    customer::{cli::Run, Config},
    escrow::{tezos, types::ContractStatus},
    protocol::daemon::Daemon,
};

use super::{close, database, Command};
use anyhow::Context;

#[async_trait]
impl Command for Run {
    async fn run(self, rng: StdRng, config: Config) -> Result<(), anyhow::Error> {
        let database = database(&config)
            .await
            .context("Customer chain-watching daemon failed to connect to local database")?;

        let config = Arc::new(config);

        // Retrieve Tezos keys from disk
        let customer_key_material = config
            .load_tezos_key_material()
            .await
            .context("Customer chain-watching daemon failed to load Tezos key material")?;

        // Sender and receiver to indicate graceful shutdown should occur
        let (terminate, _) = broadcast::channel(1);
        let mut wait_terminate = terminate.subscribe();

        // Initialize a new `Server` with parameters taken from the configuration
        let server: Server<Daemon> = Server::new();

        // Serve on this address
        let localhost_v4 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let address = (localhost_v4, config.daemon_port);

        // There is no meaningful initialization necessary per request
        let initialize = || async { Some(()) };

        // For each request, dispatch to the appropriate method, defined elsewhere
        let interact = move |_session_key, (), _chan: Chan<Daemon>| {
            // Clone `Arc`s for the various resources we need in this request
            //let _database = database.clone();

            async move {
                offer!(in _chan {
                    // Refresh
                    0 => {
                        println!("refreshed");
                        Ok::<_, anyhow::Error>(())
                    }
                })?
            }
        };

        // Set the polling service interval to run every 60 seconds
        let mut interval = tokio::time::interval(Duration::from_secs(60));

        // Run the polling service
        let polling_service_thread_handle = tokio::spawn(async move {
            // Clone resources
            let database = database.clone();
            let customer_key_material = customer_key_material.clone();

            loop {

                // Retrieve list of channels from database
                let channels = match database
                    .get_channels()
                    .await
                    .context("Failed to retrieve contract IDs")
                {
                    Ok(channels) => channels,
                    Err(e) => return Err::<(), anyhow::Error>(e),
                };

                // Query each contract ID and dispatch on the result
                for channel in channels {
                    // Retrieve on-chain contract status
                    let contract_state = match channel.contract_details.contract_id {
                        Some(contract_id) => tezos::get_contract_state(&contract_id),
                        None => continue,
                    };

                    // The channel has not reacted to an expiry transaction being posted
                    if contract_state.status() == ContractStatus::Expiry
                        && zkchannels_state::PendingClose.matches(&channel.state)
                    {
                        // call unilateral close, which will decide whether to respond to expiry.
                        // if it posts a custClaim, will wait for it to be finalized.
                        // should this go in a separate thread/block that is awaited elsewhere?
                        close::unilateral_close(
                            &channel.label,
                            self.off_chain,
                            &mut rng,
                            database.as_ref(),
                            &customer_key_material,
                        )
                        .await
                        .unwrap_or_else(|e| eprintln!("Error: {}", e));
                    }

                    // The channel has not claimed funds after custClose timeout expired
                    if contract_state.status() == ContractStatus::CustomerClose
                        && contract_state.timeout_expired().is_some()
                        && contract_state.timeout_expired().unwrap()
                        && !zkchannels_state::PendingCustomerClaim.matches(&channel.state)
                    {
                        // call custClaim and wait for it to be finalized (should this go in a
                        // separate thread that is awaited elsewhere?)
                        close::claim_funds(
                            database.as_ref(),
                            &channel.label,
                            &customer_key_material,
                        )
                        .await
                        .unwrap_or_else(|e| eprintln!("Error: {}", e));
                    }

                    // The channel has not reacted to a merchDispute transaction being posted
                    if contract_state.status() == ContractStatus::Closed
                        && zkchannels_state::PendingClose.matches(&channel.state)
                        && channel.closing_balances.merchant_balance.is_some()
                    {
                        // call merchDispute
                        close::process_dispute(database.as_ref(), &channel.label)
                            .await
                            .unwrap_or_else(|e| eprintln!("Error: {}", e));

                        // TODO: also call finalize_dispute??
                    }
                }
                interval.tick().await;
            }
        });

        // Future that completes on graceful shutdown
        let wait_terminate = async move { wait_terminate.recv().await.unwrap_or(()) };

        server
            .serve_while(address, None, initialize, interact, wait_terminate)
            .await?;

        polling_service_thread_handle.await?;

        Ok::<_, anyhow::Error>(())
    }
}
