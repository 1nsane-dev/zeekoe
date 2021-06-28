use {
    async_trait::async_trait,
    futures::stream::StreamExt,
    serde::{Deserialize, Serialize},
    sqlx::SqlitePool,
    std::any::Any,
    thiserror::Error,
};

use zkabacus_crypto::customer::Inactive;

use crate::customer::{client::ZkChannelAddress, ChannelName};

mod state;
pub use state::{take_state, NameState, State, StateName, UnexpectedState};

type Result<T> = std::result::Result<T, Error>;

/// Extension trait augmenting the customer database [`QueryCustomer`] with extra methods.
///
/// These are implemented automatically for any database handle which implements
/// [`ErasedQueryCustomer`]; when passing a trait object, use that trait instead, but prefer to call
/// the methods of this trait.
#[async_trait]
pub trait QueryCustomerExt {
    /// Given a channel's unique label, mutate its state in the database using a provided closure,
    /// that is given the current state and a flag indicating whether the state is dirty or clean.
    /// Returns `Ok(None)` if the label did not exist in the database, otherwise the result of the
    /// closure.
    ///
    /// **Important:** The given closure should be idempotent on the state of the world aside from
    /// the single side effect of modifying their given `&mut Option<State>`. In particular, the
    /// closure **should not result in communication with the merchant**.
    async fn with_channel_state<'a, T: Send + 'static>(
        &'a self,
        label: &ChannelName,
        with_state: impl for<'s> FnOnce(&'s mut Option<State>) -> T + Send + 'a,
    ) -> Result<T>;
}

/// Trait-object safe version of [`QueryCustomer`]: use this type in trait objects and implement it
/// for database backends, but prefer to call the methods from [`QueryCustomer`], since all
/// [`ErasedQueryCustomer`] are [`QueryCustomer`].
#[async_trait]
pub trait QueryCustomer: Send + Sync {
    /// Insert a newly initialized [`Requested`] channel into the customer database, associated with
    /// a unique label and [`ZkChannelAddress`].
    ///
    /// If the [`Requested`] could not be inserted, it is returned along with the error that
    /// prevented its insertion.
    async fn new_channel(
        &self,
        label: &ChannelName,
        address: &ZkChannelAddress,
        inactive: Inactive,
    ) -> std::result::Result<(), (Inactive, Error)>;

    /// Get the address of a given channel, or `None` if the label does not exist in the database.
    async fn channel_address(&self, label: &ChannelName) -> Result<Option<ZkChannelAddress>>;

    /// Relabel an existing channel from a given label to a new one.
    ///
    /// Returns `true` if the label existed and `false` if it did not.
    async fn relabel_channel(&self, label: &ChannelName, new_label: &ChannelName) -> Result<bool>;

    /// Assign a new [`ZkChannelAddress`] to an existing channel.
    ///
    /// Returns `true` if the label existed and `false` if it did not.
    async fn readdress_channel(
        &self,
        label: &ChannelName,
        new_address: &ZkChannelAddress,
    ) -> Result<bool>;

    /// **Don't call this function directly:** instead call [`QueryCustomer::with_channel_state`].
    /// Note that this method uses `Box<dyn Any + Send>` to avoid the use of generic parameters,
    /// which is what allows the trait to be object safe.
    ///
    /// # Panics
    ///
    /// The corresponding method [`QueryCustomer::with_channel_state`] will panic if the boxed
    /// [`Any`] type returned by `with_clean_state` does not match that of the `Ok` case of the
    /// function's result, and similarly if the boxed [`Any`] type returned by `with_dirty_state`
    /// does not match the `Err` case of the function's result. It is expected that any
    /// implementation of this function merely forwards these values to the returned `Result<Box<dyn
    /// Any>, Box<dyn Any>>`.
    async fn with_channel_state_erased<'a>(
        &'a self,
        label: &ChannelName,
        with_state: Box<
            dyn for<'s> FnOnce(&'s mut Option<State>) -> Box<dyn Any + Send> + Send + 'a,
        >,
    ) -> Result<Box<dyn Any>>;
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    UnexpectedState(UnexpectedState),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("There is no channel by the name of \"{0}\"")]
    NoSuchChannel(ChannelName),
    #[error("There is already a channel by the name of \"{0}\"")]
    ChannelExists(ChannelName),
}

#[async_trait]
impl QueryCustomer for SqlitePool {
    async fn new_channel(
        &self,
        label: &ChannelName,
        address: &ZkChannelAddress,
        inactive: Inactive,
    ) -> std::result::Result<(), (Inactive, Error)> {
        let state = State::Inactive(inactive);
        (|| async {
            let mut transaction = self.begin().await?;

            // Determine if the channel already exists
            let already_exists =
                match sqlx::query!("SELECT label FROM customer_channels WHERE label = ?", label)
                    .fetch(&mut transaction)
                    .next()
                    .await
                    .transpose()?
                {
                    Some(_) => true,
                    _ => false,
                };

            // Return an error if it does exist
            if already_exists {
                return Err(Error::ChannelExists(label.clone()));
            }

            let result = sqlx::query!(
                "INSERT INTO customer_channels (label, address, state) VALUES (?, ?, ?)",
                label,
                address,
                state,
            )
            .execute(&mut transaction)
            .await
            .map(|_| ());

            transaction.commit().await?;

            Ok(result?)
        })()
        .await
        .map_err(|e| (state.inactive().unwrap(), e))
    }

    async fn channel_address(&self, label: &ChannelName) -> Result<Option<ZkChannelAddress>> {
        Ok(sqlx::query!(
            r#"
            SELECT address AS "address: ZkChannelAddress"
            FROM customer_channels
            WHERE label = ?"#,
            label,
        )
        .fetch(self)
        .next()
        .await
        .transpose()
        .map(|option| option.map(|r| r.address))?)
    }

    async fn relabel_channel(&self, label: &ChannelName, new_label: &ChannelName) -> Result<bool> {
        Ok(sqlx::query!(
            "UPDATE customer_channels SET label = ? WHERE label = ?",
            new_label,
            label,
        )
        .execute(self)
        .await
        .map(|r| r.rows_affected() == 1)?)
    }

    async fn readdress_channel(
        &self,
        label: &ChannelName,
        new_address: &ZkChannelAddress,
    ) -> Result<bool> {
        sqlx::query!(
            "UPDATE customer_channels SET address = ? WHERE label = ?",
            new_address,
            label,
        )
        .execute(self)
        .await
        .map(|r| r.rows_affected() == 1)
        .map_err(|e| e.into())
    }

    async fn with_channel_state_erased<'a>(
        &'a self,
        label: &ChannelName,
        with_state: Box<
            dyn for<'s> FnOnce(&'s mut Option<State>) -> Box<dyn Any + Send> + Send + 'a,
        >,
    ) -> Result<Box<dyn Any>> {
        let mut transaction = self.begin().await?;

        // Retrieve the state so that we can modify it
        let mut state: Option<State> = sqlx::query!(
            r#"SELECT state AS "state: State" FROM customer_channels WHERE label = ?"#,
            label,
        )
        .fetch_one(&mut transaction)
        .await?
        .state;

        // Perform the operation with the state fetched from the database
        let output = with_state(&mut state);

        // Store the new state to the database and set it to clean again
        sqlx::query!(
            "UPDATE customer_channels SET state = ? WHERE label = ?",
            state,
            label
        )
        .execute(&mut transaction)
        .await?;

        // Commit the transaction
        transaction.commit().await?;

        Ok(output)
    }
}

// Blanket implementation of [`QueryCustomerExt`] for all [`QueryCustomer`]
#[async_trait]
impl<Q: QueryCustomer + ?Sized> QueryCustomerExt for Q {
    async fn with_channel_state<'a, T: Send + 'static>(
        &'a self,
        label: &ChannelName,
        with_state: impl for<'s> FnOnce(&'s mut Option<State>) -> T + Send + 'a,
    ) -> Result<T> {
        <Self as QueryCustomer>::with_channel_state_erased(
            self,
            label,
            Box::new(|state| Box::new(with_state(state))),
        )
        .await
        .map(|t| *t.downcast().unwrap())
    }
}
