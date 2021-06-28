use {anyhow::Context, async_trait::async_trait, rand::rngs::StdRng};

use zkabacus_crypto::{
    customer::{LockMessage, StartMessage},
    ClosingSignature, Context as ProofContext, PayToken, PaymentAmount,
};

use zeekoe::{
    abort,
    customer::{
        cli::{Pay, Refund},
        client::SessionKey,
        database::{QueryCustomer, QueryCustomerExt, State},
        Chan, ChannelName, Config,
    },
    offer_abort, proceed,
    protocol::{pay, Party::Customer},
};

use super::{connect, database, Command};

#[async_trait]
impl Command for Pay {
    async fn run(self, rng: StdRng, config: self::Config) -> Result<(), anyhow::Error> {
        // Convert the payment amount appropriately
        let minor_units: i64 = self.pay.try_into_minor_units().ok_or_else(|| {
            anyhow::anyhow!("Payment amount invalid for currency or out of range for channel")
        })?;
        let payment_amount = (if minor_units < 0 {
            PaymentAmount::pay_customer
        } else {
            PaymentAmount::pay_merchant
        })(minor_units.abs() as u64)
        .context("Payment amount out of range")?;

        let database = database(&config)
            .await
            .context("Failed to connect to local database")?;

        // Look up the address and current local customer state for this merchant in the database
        let address = match database
            .channel_address(&self.label)
            .await
            .context("Failed to look up channel address in local database")?
        {
            None => return Err(anyhow::anyhow!("Unknown channel label: {}", self.label)),
            Some(address) => address,
        };

        // Connect and select the Pay session
        let (session_key, chan) = connect(&config, &address)
            .await
            .context("Failed to connect to merchant")?;
        let chan = chan
            .choose::<2>()
            .await
            .context("Failed selecting pay session with merchant")?;

        // Read the contents of the note, if any
        let note = self
            .note
            .unwrap_or_else(|| zeekoe::customer::cli::Note::String(String::from("")))
            .read(config.max_note_length)
            .context("Failed to read payment note from standard input or command line")?;

        // Send the payment amount and note to the merchant
        let chan = chan
            .send(payment_amount)
            .await
            .context("Failed to send payment amount")?
            .send(note)
            .await
            .context("Failed to send payment note")?;

        // Allow the merchant to accept or reject the payment and note
        offer_abort!(in chan as Customer);

        // Run the core zkAbacus.Pay protocol
        let chan = zkabacus_pay(
            rng,
            database.as_ref(),
            &self.label,
            session_key,
            chan,
            payment_amount,
        )
        .await
        .context("Failed to complete pay protocol")?;

        // Receive the response note (i.e. the fulfillment of the service)
        let (response_note, chan) = chan
            .recv()
            .await
            .context("Failed to receive response note")?;

        // Close the communication channel: we are done communicating with the merchant
        chan.close();

        // Print the response note on standard out
        if let Some(response_note) = response_note {
            println!("{}", response_note);
        }

        Ok(())
    }
}

/// The core zkAbacus.Pay protocol.
async fn zkabacus_pay(
    mut rng: StdRng,
    database: &dyn QueryCustomer,
    label: &ChannelName,
    session_key: SessionKey,
    chan: Chan<pay::CustomerStartPayment>,
    payment_amount: PaymentAmount,
) -> Result<Chan<pay::MerchantProvideService>, anyhow::Error> {
    // Generate the shared context for proofs
    let context = ProofContext::new(&session_key.to_bytes());

    // Start the zkAbacus core payment and get fresh proofs and commitments
    let start_message = start_payment(&mut rng, database, label, payment_amount, context).await?;

    // Send the initial proofs and commitments to the merchant
    let chan = chan
        .send(start_message.nonce)
        .await
        .context("Failed to send nonce")?
        .send(start_message.pay_proof)
        .await
        .context("Failed to send payment proof")?;

    // Allow the merchant to cancel the session at this point, and throw an error if so
    offer_abort!(in chan as Customer);

    // Receive a closing signature from the merchant
    let (closing_signature, chan) = chan
        .recv()
        .await
        .context("Failed to receive closing signature")?;

    // Verify the closing signature and transition into a locked state
    let chan = if let Some(lock_message) = lock_payment(database, label, closing_signature).await? {
        proceed!(in chan);

        // If the closing signature verifies, reveal our lock, secret, and blinding factor
        let chan = chan
            .send(lock_message.revocation_lock)
            .await
            .context("Failed to send revocation lock")?
            .send(lock_message.revocation_secret)
            .await
            .context("Failed to send revocation secret")?
            .send(lock_message.revocation_lock_blinding_factor)
            .await
            .context("Failed to send revocation lock blinding factor")?;

        // Allow the merchant to cancel the session at this point, and throw an error if so
        offer_abort!(in chan as Customer);
        chan
    } else {
        // If the closing signature does not verify, inform the merchant we are aborting
        abort!(in chan return pay::Error::InvalidClosingSignature);
    };

    // Receive a pay token from the merchant, which allows us to pay again
    let (pay_token, chan) = chan
        .recv()
        .await
        .context("Failed to receive payment token")?;

    // Unlock the payment channel using the pay token
    unlock_payment(database, label, pay_token).await?;

    Ok(chan)
}

/// Attempt to start the payment for the channel of the given label, using the given
/// [`PaymentAmount`] and [`ProofContext`].
///
/// Returns the [`StartMessage`] for broadcast to the merchant if successful.
async fn start_payment(
    rng: &mut StdRng,
    database: &dyn QueryCustomer,
    label: &ChannelName,
    payment_amount: PaymentAmount,
    context: ProofContext,
) -> Result<StartMessage, anyhow::Error> {
    database
        .with_channel_state(label, |state| {
            // Make sure channel is in ready state
            let ready = state.ready().map_err(|(e, _)| e)?;

            // Attempt to start the payment using the payment amount and proof context
            match ready.start(rng, payment_amount, &context) {
                Ok((started, start_message)) => {
                    // Return the start message and new state
                    Ok((start_message, State::Started(started)))
                }
                Err((_, error)) => {
                    // Return an error describing the failure
                    Err(error).context("Failed to generate nonce and pay proof")
                }
            }
        })
        .await
        .context("Database error while fetching initial pay state")??
}

/// Attempt to lock a started payment for the channel of the given label, using the given
/// [`ClosingSignature`].
///
/// Returns the [`LockMessage`] for broadcast to the merchant if successful, or `None` if the
/// database operations succeeded but the closing signature was invalid.
async fn lock_payment(
    database: &dyn QueryCustomer,
    label: &ChannelName,
    closing_signature: ClosingSignature,
) -> Result<Option<LockMessage>, anyhow::Error> {
    let result = database
        .with_channel_state(label, |state| {
            // Ensure channel is in the started state
            let started = state.started().map_err(|(e, _)| e)?;

            // Attempt to lock the state using the closing signature
            match started.lock(closing_signature) {
                Err(_) => {
                    // Return no start message, since we failed
                    Err(())
                }
                Ok((locked, lock_message)) => {
                    // Return the start message and set the new state
                    Ok((lock_message, State::Locked(locked)))
                }
            }
        })
        .await
        .context("Database error while fetching started pay state")??;

    match result {
        Ok(lock_message) => Ok(Some(lock_message)),
        Err(()) => Ok(None),
    }
}

/// Attempt to unlock a locked payment for a channel of the given label, using the given
/// [`PayToken`].
///
/// If successful, this updates the state in the database for the channel so that it is ready for
/// the next payment.
async fn unlock_payment(
    database: &dyn QueryCustomer,
    label: &ChannelName,
    pay_token: PayToken,
) -> Result<(), anyhow::Error> {
    database
        .with_channel_state(label, |state| {
            // Ensure the channel is in locked state
            let locked = take_state(State::locked, state).with_context(|| {
                format!(
                    "Expecting the channel \"{}\" to be in a different state",
                    label
                )
            })?;

            // Attempt to unlock the state using the pay token
            match locked.unlock(pay_token) {
                Err(locked) => {
                    // Restore the state in the database to the original locked state
                    *state = Some(State::Locked(locked));

                    // Return an error since the state could not be unlocked
                    Err(pay::Error::InvalidPayToken.into())
                }
                Ok(ready) => {
                    // Set the new ready state in the database
                    *state = Some(State::Ready(ready));

                    // Success
                    Ok(())
                }
            }
        })
        .await
        .context("Database error while fetching locked pay state")??
}

#[async_trait]
impl Command for Refund {
    async fn run(self, rng: StdRng, config: Config) -> Result<(), anyhow::Error> {
        // A refund is merely a negative payment
        self.into_negative_pay().run(rng, config).await
    }
}
