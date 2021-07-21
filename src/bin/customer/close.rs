//* Close functionalities for a customer.
//*
//* In the current design, the customer requires either a watchtower or a notification service
//* to finalize the channel close.
//* This architecture is flexible; we could alternately allow the customer CLI to wait (hang) until
//* it receives confirmation (e.g. call `process_mutual_close_confirmation` directly from
//* `mutual_close()`).
use {async_trait::async_trait, rand::rngs::StdRng, std::convert::Infallible};

use zeekoe::{
    customer::{
        cli::Close,
        database::{self, Closed, QueryCustomer, QueryCustomerExt},
        Chan, ChannelName, Config,
    },
    offer_abort, proceed,
    protocol::{close, Party::Customer},
};
use zkabacus_crypto::customer::{ClosingMessage, Inactive, Locked, Ready, Started};

use super::{connect, database, Command};
use anyhow::Context;

#[async_trait]
impl Command for Close {
    #[allow(unused)]
    async fn run(self, mut rng: StdRng, config: self::Config) -> Result<(), anyhow::Error> {
        if self.force {
            close(&self, rng, config)
                .await
                .context("Unilateral close failed")?;
        } else {
            mutual_close(&self, rng, config)
                .await
                .context("Mutual close failed")?;
        }

        Ok(())
    }
}

/// Closes the channel on the current balances, either unilaterally or in response to the
/// merchant posting an expiry to the contract.
///
/// **Usage**: This function can be called
/// - directly from the command line to initiate unilateral customer channel closure.
/// - in response to an on-chain event: the merchant posts an expiry operation and it is confirmed
///   on chain at any depth.
async fn close(close: &Close, rng: StdRng, config: self::Config) -> Result<(), anyhow::Error> {
    let database = database(&config)
        .await
        .context("Failed to connect to local database")?;

    // Retrieve the close state and update channel status to PENDING_CLOSE.
    let _close_message = get_close_message(rng, database.as_ref(), &close.label)
        .await
        .context("Failed to get closing information.")?;

    // TODO: Call the customer close entrypoint which will take:
    // - current channel balances
    // - contract ID
    // - revocation lock
    // Raise an error if it fails.
    //
    // This function will:
    // - Generate customer authorization EDDSA signature on the operation with the customer's
    //   Tezos public key.
    // - Send cust close operation to blockchain
    // - Send merchant payout operation to blockchain

    Ok(())
}

/// Update channel balances at first payout in unilateral close flows.
///
/// **Usage**: this function is called in response to an on-chain event. It is called after the
/// custClose operation is confirmed on chain at an appropriate depth.
#[allow(unused)]
async fn process_confirmed_customer_close() {
    // TODO: assert that the db status is PENDING_CLOSE,
    // Indicate that the merchant balance has been paid out to the merchant.
}

/// Claim final balance of the channel.
///
/// **Usage**: this function is called as a response to an on-chain event. It is only called after
/// the contract claim delay has passed *and* the custClose entrypoint is confirmed at the required
/// confirmation depth.
#[allow(unused)]
async fn claim_funds(close: &Close, config: self::Config) -> Result<(), anyhow::Error> {
    // TODO: assert that the db status is PENDING_CLOSE,
    // If it is DISPUTE, do nothing.

    // TODO: Otherwise, call the customer claim entrypoint which will take:
    // - contract ID

    Ok(())
}

/// Update channel to indicate a dispute.
///
/// **Usage**: this function is called in response to a merchDispute operation being
/// confirmed on chain (at any depth).
#[allow(unused)]
async fn process_dispute(config: self::Config) -> Result<(), anyhow::Error> {
    // TODO: update status in db from PENDING_CLOSE to DISPUTE
    Ok(())
}

/// Update channel state once a disputed unilateral close flow is finalized.
///
/// **Usage**: this function is called in response to a merchDispute operation begin confirmed
/// on chain to an appropriate depth.
#[allow(unused)]
async fn finalize_dispute(config: self::Config) -> Result<(), anyhow::Error> {
    // TODO: Update status in db from DISPUTE to CLOSED
    // Indicate that all balances are paid out to the merchant.
    Ok(())
}

/// Update channel state once close operations are finalized.
///
/// **Usage**: this function is called as response to an on-chain event, either:
/// - a custClaim operation is confirmed on chain at an appropriate depth.
/// - a merchClaim operation is confirmed on chain at an appropriate depth
///
/// Note: these functions are separate in the merchant implementation. Maybe they should also be
/// separate here.
#[allow(unused)]
async fn finalize_close(config: self::Config) -> Result<(), anyhow::Error> {
    // TODO: update status in db from PENDING_CLOSE to CLOSED with the final balances.
    // - for custClaim, indicate that the customer balance is paid out to the customer
    //   (the merchant balance was already paid out; final balances will match PENDING_CLOSE)
    //   This happens in any undisputed, unilateral close flow.
    //
    // - for merchClaim, indicate that the customer and merchant balances are paid out
    //   to the merchant.
    //   This happens in a merchant unilateral close flow when the customer does not post updated
    //   channel balances with custClose.

    Ok(())
}

async fn mutual_close(
    close: &Close,
    rng: StdRng,
    config: self::Config,
) -> Result<(), anyhow::Error> {
    let database = database(&config)
        .await
        .context("Failed to connect to local database")?;

    // Look up the address and current local customer state for this merchant in the database
    let address = database
        .channel_address(&close.label)
        .await
        .context("Failed to look up channel address in local database")?;

    // Connect and select the Close session
    let (_session_key, chan) = connect(&config, &address)
        .await
        .context("Failed to connect to merchant")?;

    let chan = chan
        .choose::<3>()
        .await
        .context("Failed selecting close session with merchant")?;

    let chan = zkabacus_close(rng, database.as_ref(), &close.label, chan)
        .await
        .context("zkAbacus close failed.")?;

    // TODO: Receive an authorization signature from merchant under the merchant's EDDSA Tezos key.
    // The signature should be over a tuple with
    // (contract id, "zkChannels mutual close", channel id, customer balance, merchant balance).
    /*
    let merchant_authorization_signature = chan
        .recv()
        .await
        .context("Failed to receive authorization signature from the merchant.")?;
    */

    // TODO: Verify that signature is a valid EDDSA signature with respect to the merchant's Tezos
    // public key on the tuple:
    // (contract id, "zkChannels mutual close", channel id, customer balance, merchant balance).
    //
    // abort!() if invalid with error InvalidMerchantAuthSignature.
    //
    // The customer has the option to retry or initiate a unilateral close.
    // We should consider having the customer automatically initiate a unilateral close after a
    // random delay.
    proceed!(in chan);

    // Close the dialectic channel - all remaining operations are with the escrow agent.
    chan.close();

    // TODO: Call the mutual close entrypoint which will take:
    // - current channel balances
    // - merchant authorization signature
    // - contract ID
    // - channel ID
    // raise error if it fails with error ArbiterRejectedMutualClose.
    //
    // This function will:
    // - Generate customer authorization EDDSA signature on the operation with the customer's
    //   Tezos public key.
    // - Send operation to blockchain
    // - Raises an error if the operation fails to post. This may include relevant information
    //   (e.g. insufficient fees) or may be more generic.

    Ok(())
}

/// Update the channel state from pending to closed.
///
/// **Usage**: This should be called when the customer receives a confirmation from the blockchain
/// that the mutual close operation has been applied and has reached required confirmation depth.
/// It will only be called after a successful execution of [`mutual_close()`].
#[allow(unused)]
async fn finalize_mutual_close(
    rng: &mut StdRng,
    config: self::Config,
    label: ChannelName,
) -> Result<(), anyhow::Error> {
    let database = database(&config)
        .await
        .context("Failed to connect to local database")?;

    // Update database channel status from PendingClose to Closed.
    database
        .with_channel_state(&label, |pending: ClosingMessage| {
            Ok((
                Closed::new(
                    *pending.channel_id(),
                    *pending.customer_balance(),
                    *pending.merchant_balance(),
                ),
                (),
            ))
        })
        .await
        .context("Database error while updating status to closed")?
}

async fn zkabacus_close(
    rng: StdRng,
    database: &dyn QueryCustomer,
    label: &ChannelName,
    chan: Chan<close::Close>,
) -> Result<Chan<close::MerchantSendAuthorization>, anyhow::Error> {
    // Generate the closing message and update state to pending-close.
    let closing_message = get_close_message(rng, database, label)
        .await
        .context("Failed to generate mutual close data.")?;

    let (close_signature, close_state) = closing_message.into_parts();

    // Send the pieces of the CloseMessage.
    let chan = chan
        .send(close_signature)
        .await
        .context("Failed to send close state signature")?
        .send(close_state)
        .await
        .context("Failed to send close state")?;

    // Let merchant reject an invalid or outdated `CloseMessage`.
    offer_abort!(in chan as Customer);

    Ok(chan)
}

/// Try to extract a close message from the database, assuming that the current channel status
/// holds type $ty. There are four types that can successfully call close.
/// If the current channel status is closeable, update the channel status to PENDING_CLOSE and
/// return the close message.
/// Otherwise, does nothing.
macro_rules! try_close {
    ($rng:expr, $database:expr, $label:expr, $ty:ty) => {{
        let result = $database
            .with_channel_state(&$label, |state: $ty| {
                let message = state.close(&mut $rng);
                Ok::<_, Infallible>((message.clone(), message))
            })
            .await;

        match result {
            Ok(message) => match message {
                Ok(message) => return Ok(message),
                Err(infallible) => match infallible {},
            },
            Err(error) => match error {
                database::Error::UnexpectedState(_) => {}
                _ => return Err(error).context("Failed to set state to pending close in database"),
            },
        }
    };};
}

/// Extract the close message from the saved channel status (including the current state
/// any stored signatures) and update the channel state to PENDING_CLOSE atomically.
async fn get_close_message(
    mut rng: StdRng,
    database: &dyn QueryCustomer,
    label: &ChannelName,
) -> Result<ClosingMessage, anyhow::Error> {
    try_close!(rng, database, label, Inactive);
    try_close!(rng, database, label, Ready);
    try_close!(rng, database, label, Started);
    try_close!(rng, database, label, Locked);

    let result = database
        .with_channel_state(&label, |message: ClosingMessage| {
            Ok::<_, Infallible>((message.clone(), message))
        })
        .await;

    match result {
        Ok(message) => match message {
            Ok(message) => return Ok(message),
            Err(infallible) => match infallible {},
        },
        Err(error) => match error {
            database::Error::UnexpectedState(_) => {}
            _ => return Err(error).context("Failed to set state to pending close in database"),
        },
    }

    return Err(anyhow::anyhow!(
        "The channel with label \"{}\" was already closed",
        label
    ));
}
