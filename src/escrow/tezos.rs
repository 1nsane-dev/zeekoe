pub mod establish {
    use crate::escrow::{notify::Level, types::*};
    use zkabacus_crypto::{ChannelId, CustomerBalance, MerchantBalance, PublicKey};

    #[allow(unused)]
    pub struct CustomerFundingInformation {
        /// Initial balance for the customer in the channel.
        pub balance: CustomerBalance,

        /// Funding source which will support the balance. This address is the hash of
        /// the `public_key`.
        pub address: TezosFundingAddress,

        /// Public key associated with the funding address. The customer must have access to the
        /// corresponding [`tezedge::PrivateKey`].
        pub public_key: TezosPublicKey,
    }

    #[allow(unused)]
    pub struct MerchantFundingInformation {
        /// Initial balance for the merchant in the channel.
        pub balance: MerchantBalance,

        /// Funding source which will support the balance. This address is the hash of
        /// the `public_key`.
        pub address: TezosFundingAddress,

        /// Public key associated with the funding address. The merchant must have access to the
        /// corresponding [`tezedge::PrivateKey`].
        pub public_key: TezosPublicKey,
    }

    /// Originate a contract on chain.
    ///
    /// This call will wait until the contract is confirmed at depth.
    /// It returns the new [`ContractId`] and the [`Level`] of the block that contains the
    /// originated contract.
    ///
    /// The `originator_key_pair` should belong to whichever party originates the contract.
    /// Currently, this must be called by the customer. Its public key must be the same as
    /// the one in the provided [`CustomerFundingInformation`].
    #[allow(unused)]
    pub async fn originate(
        merchant_funding_info: &MerchantFundingInformation,
        customer_funding_info: &CustomerFundingInformation,
        merchant_public_key: &PublicKey,
        originator_key_pair: &TezosKeyMaterial,
        channel_id: &ChannelId,
    ) -> Result<(ContractId, Level), Error> {
        todo!()
    }

    /// Call the `addFunding` entrypoint with the [`CustomerFundingInformation`].
    ///
    /// This will wait until the funding operation is confirmed at depth. It is called by
    /// the customer.
    ///
    /// The operation is invalid if:
    /// - the channel status is not AWAITING_FUNDING
    /// - the specified customer address does not match the `cust_addr` field in the contract
    /// - the specified funding information does not match the `custFunding` amount in the contract
    /// - the `addFunding` entrypoint has not been called by the customer address before.
    #[allow(unused)]
    pub async fn add_customer_funding(
        contract_id: &ContractId,
        customer_funding_info: &CustomerFundingInformation,
        customer_key_pair: &TezosKeyMaterial,
    ) -> Result<(), Error> {
        todo!()
    }

    /// Verify that the contract specified by [`ContractId`] has been correctly originated on
    /// chain with respect to the expected values.
    ///
    /// Correct origination requires that:
    /// - Contract encodes the expected zkChannels contract
    /// - Contract storage is correctly instantiated
    /// - Contract is confirmed on chain to the expected depth
    ///
    /// This function will wait until the origination operation is confirmed at depth
    /// and is called by the merchant.
    ///
    /// This function will return [`Error::InvalidZkChannelsContract`] if the contract is not a valid
    /// zkChannels contract or it does not have the expected storage.
    #[allow(unused)]
    pub async fn verify_origination(
        contract_id: &ContractId,
        merchant_funding_info: &MerchantFundingInformation,
        customer_funding_info: &CustomerFundingInformation,
        merchant_public_key: &PublicKey,
        channel_id: &ChannelId,
    ) -> Result<(), Error> {
        todo!()
    }

    /// Verify that the customer has sucessfully funded the contract via the `addFunding`
    /// entrypoint
    ///
    /// Correct funding requires that:
    /// - The `addFunding` operation is the latest operation to be applied to the contract
    /// - The `addFunding` operation is confirmed on chain to the expected depth
    ///
    /// This function will wait until the customer's funding operation is confirmed at depth
    /// and is called by the merchant.
    #[allow(unused)]
    pub async fn verify_customer_funding(
        contract_id: &ContractId,
        customer_funding_info: &CustomerFundingInformation,
    ) -> Result<(), Error> {
        todo!()
    }

    /// Add merchant funding via the `addFunding` entrypoint to the given [`ContractId`],
    /// according to the [`MerchantFundingInformation`]
    ///
    /// This should only be called if [`verify_origination()`] and [`verify_customer_funding()`]
    /// both returned successfully.
    ///
    /// This function will wait until the merchant funding operation is confirmed at depth. It
    /// is called by the merchant.
    ///
    /// If the expected merchant funding is non-zero, this operation is invalid if:
    /// - the contract status is not AWAITING_FUNDING
    /// - the specified merchant address does not match the `merch_addr` field in the contract
    /// - the specified funding information does not match the `merchFunding` amount in the contract
    /// - the `addFunding` entrypoint has already been called by the merchant address
    ///
    /// If the expected merchant funding is 0, this operation is invalid if:
    /// - the contract status is not OPEN
    #[allow(unused)]
    pub async fn add_merchant_funding(
        contract_id: &ContractId,
        merchant_funding_info: &MerchantFundingInformation,
        merchant_key_pair: &TezosKeyMaterial,
    ) -> Result<(), Error> {
        todo!()
    }

    /// Reclaim customer funding via the `reclaimFunding` entrypoint on the given [`ContractId`].
    ///
    /// This function will wait until the customer reclaim operation is confirmed at depth. It is
    /// called by the customer.
    ///
    /// The operation is invalid if:
    /// - the contract status is not AWAITING_FUNDING.
    /// - the `addFunding` entrypoint has not been called by the customer address
    #[allow(unused)]
    pub async fn reclaim_customer_funding(
        contract_id: &ContractId,
        customer_key_pair: &TezosKeyMaterial,
    ) -> Result<(), Error> {
        todo!()
    }
}

pub mod close {
    use serde::de::SeqAccess;
    use zkabacus_crypto::ChannelId;

    use crate::escrow::types::*;

    use {
        serde::{
            de::{self, Visitor},
            ser::SerializeStruct,
            Deserialize, Serialize,
        },
        std::fmt,
        tezedge::signer::OperationSignatureInfo,
        zkabacus_crypto::{
            customer::ClosingMessage, revlock::RevocationSecret, CloseState, CustomerBalance,
            MerchantBalance,
        },
    };

    #[derive(Debug, Clone)]
    pub struct MutualCloseAuthorizationSignature(OperationSignatureInfo);

    impl MutualCloseAuthorizationSignature {
        pub fn operation_hash(&self) -> &String {
            &self.0.operation_hash
        }
        pub fn operation_with_signature(&self) -> &String {
            &self.0.operation_with_signature
        }
        pub fn signature(&self) -> &String {
            &self.0.signature
        }
    }

    pub struct FinalBalances {
        merchant_balance: MerchantBalance,
        customer_balance: CustomerBalance,
    }

    impl FinalBalances {
        pub fn merchant_balance(&self) -> MerchantBalance {
            self.merchant_balance
        }

        pub fn customer_balance(&self) -> CustomerBalance {
            self.customer_balance
        }
    }

    /// Initiate expiry close flow via the `expiry` entrypoint on the given [`ContractId`].
    ///
    /// This function will wait until the expiry operation is confirmed at depth and is called
    /// by the merchant.
    ///
    /// This operation is invalid if:
    /// - the contract status is not OPEN
    /// - the [`TezosFundingAddress`] specified does not match the `merch_addr` field in the
    ///   the specified contract
    #[allow(unused)]
    pub async fn expiry(
        contract_id: &ContractId,
        merchant_key_pair: &TezosKeyMaterial,
    ) -> Result<(), Error> {
        todo!()
    }

    /// Complete expiry close flow by claiming the entire channel balance on the [`ContractId`]
    /// via the `merchClaim` entrypoint.
    ///
    /// This function will wait until the self-delay period on the `expiry` entrypoint has passed.
    /// After posting the `merchClaim` operation, it will wait until it has been confirmed at
    /// depth. It is called by the merchant.
    ///
    /// This operation is invalid if:
    /// - the contract status is not EXPIRY
    /// - the [`TezosKeyPair`] does not match the `merch_addr` field in the specified
    ///   contract
    #[allow(unused)]
    pub async fn merch_claim(
        contract_id: &ContractId,
        merchant_key_pair: &TezosKeyMaterial,
    ) -> Result<(FinalBalances), Error> {
        todo!()
    }

    /// Initiate unilateral customer close flow or correct balances from the expiry flow by
    /// posting the correct channel balances for the [`ContractId`] via the `custClose` entrypoint.
    ///
    /// This function will wait until it is confirmed at depth. It is called by the customer. If
    /// it is called in response to an `expiry` operation, it will be called by the customer's
    /// notification service.
    ///
    /// This operation is invalid if:
    /// - the contract status is neither OPEN nor EXPIRY
    /// - the [`TezosKeyPair`] does not match the `cust_addr` field in the specified contract
    /// - the signature in the [`ClosingMessage`] is not a well-formed signature
    /// - the signature in the [`ClosingMessage`] is not a valid signature under the merchant
    ///   public key on the expected tuple
    #[allow(unused)]
    pub async fn cust_close(
        contract_id: &ContractId,
        close_message: &ClosingMessage,
        customer_key_pair: &TezosKeyMaterial,
    ) -> Result<(MerchantBalance), Error> {
        // This function should:
        // - Generate customer authorization EdDSA signature on the operation with the customer's
        //   Tezos public key.
        // - Send custClose entrypoint calling operation to blockchain. This operation results in a
        //   timelock on the customer's balance and an immediate payout of the merchant balance
        todo!()
    }

    /// Dispute balances posted by a customer (via [`cust_close()`]) by posting a revocation
    /// secret that matches the posted revocation lock. On successful completion, this call
    /// will transfer the posted customer balance to the merchant.
    ///
    /// This function will wait until it is confirmed at depth. It is called by the merchant.
    ///
    /// This operation is invalid if:
    /// - the contract status is not CUST_CLOSE
    /// - the [`TezosKeyPair`] does not match the `merch_addr` field in the specified contract
    /// - the [`RevocationSecret`] does not hash to the `rev_lock` field in the specified contract
    #[allow(unused)]
    pub async fn merch_dispute(
        contract_id: &ContractId,
        revocation_secret: &RevocationSecret,
        merchant_key_pair: &TezosKeyMaterial,
    ) -> Result<(FinalBalances), Error> {
        todo!()
    }

    /// Claim customer funds (posted via [`cust_close()`]) after the timeout period has elapsed
    /// via the `custClaim` entrypoint.
    ///
    /// This function will wait until the timeout period from the `custClose` entrypoint call has
    /// elapsed, and until the `custClaim` operation is confirmed at depth. It is called by the
    /// customer.
    ///
    /// This operation is invalid if:
    /// - the contract status is not CUST_CLOSE
    /// - the [`TezosKeyPair`] does not match the `cust_addr` field in the specified contract
    #[allow(unused)]
    pub async fn cust_claim(
        contract_id: &ContractId,
        customer_key_pair: &TezosKeyMaterial,
    ) -> Result<(CustomerBalance), Error> {
        todo!()
    }

    /// Authorize the close state provided in the mutual close flow by producing a valid EdDSA
    /// signature over the tuple
    /// `(contract id, "zkChannels mutual close", channel id, customer balance, merchant balance)`
    ///
    /// This is called by the merchant.
    #[allow(unused)]
    pub async fn authorize_mutual_close(
        contract_id: &ContractId,
        close_state: &CloseState,
        merchant_key_pair: &TezosKeyMaterial,
    ) -> Result<MutualCloseAuthorizationSignature, Error> {
        todo!()
    }

    /// Execute the mutual close flow via the `mutualClose` entrypoint paying out the specified
    /// channel balances to both parties.
    ///
    /// This function will wait until the operation is confirmed at depth. It is called by the
    /// customer.
    ///
    /// This operation is invalid if:
    /// - the contract status is not OPEN
    /// - the [`TezosKeyPair`] does not match the `cust_addr` field in the specified contract
    /// - the `authorization_signature` is not a valid signature under the merchant public key
    ///   on the expected tuple
    #[allow(unused)]
    pub async fn mutual_close(
        contract_id: &ContractId,
        channel_id: &ChannelId,
        customer_balance: &CustomerBalance,
        merchant_balance: &MerchantBalance,
        authorization_signature: MutualCloseAuthorizationSignature,
        customer_key_pair: &TezosKeyMaterial,
    ) -> Result<(FinalBalances), Error> {
        todo!()
    }

    impl Serialize for MutualCloseAuthorizationSignature {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let mut mcas = serializer.serialize_struct("MutualCloseAuthorizationSignature", 3)?;
            mcas.serialize_field("operation hash", self.operation_hash())?;
            mcas.serialize_field("operation_with_signature", self.operation_with_signature())?;
            mcas.serialize_field("signature", self.signature())?;
            mcas.end()
        }
    }

    impl<'de> Deserialize<'de> for MutualCloseAuthorizationSignature {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            struct MutualCloseAuthorizationSignatureVisitor;

            impl<'de> Visitor<'de> for MutualCloseAuthorizationSignatureVisitor {
                type Value = MutualCloseAuthorizationSignature;

                fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                    formatter.write_str("struct MutualCloseAuthorizationSignature")
                }

                fn visit_seq<V>(
                    self,
                    mut seq: V,
                ) -> Result<MutualCloseAuthorizationSignature, V::Error>
                where
                    V: SeqAccess<'de>,
                {
                    let operation_hash = seq
                        .next_element()?
                        .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                    let operation_with_signature = seq
                        .next_element()?
                        .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                    let signature = seq
                        .next_element()?
                        .ok_or_else(|| de::Error::invalid_length(2, &self))?;
                    Ok(MutualCloseAuthorizationSignature(OperationSignatureInfo {
                        operation_hash,
                        operation_with_signature,
                        signature,
                    }))
                }
            }

            const FIELDS: &'static [&'static str] =
                &["operation_hash", "operation_with_signature", "signature"];
            deserializer.deserialize_struct(
                "MutualCloseAuthorizationSignature",
                FIELDS,
                MutualCloseAuthorizationSignatureVisitor,
            )
        }
    }

    #[cfg(test)]
    mod tests {
        use tezedge::signer::OperationSignatureInfo;

        use super::MutualCloseAuthorizationSignature;

        #[test]
        fn mutual_close_authorization_deserializes() {
            let mcas = MutualCloseAuthorizationSignature(OperationSignatureInfo {
                operation_hash: "here is a stupid operation hash 0x0".to_string(),
                operation_with_signature: "here is a very bad operation with signature 108"
                    .to_string(),
                signature: "0xksdja3ulkdfjklsdjfalksdhf;ls".to_string(),
            });
            let serialized_mcas = bincode::serialize(&mcas).unwrap();
            println!("{:?}", String::from_utf8(serialized_mcas.clone()));
            let de_mcas =
                bincode::deserialize::<MutualCloseAuthorizationSignature>(&serialized_mcas)
                    .unwrap();

            assert_eq!(mcas.operation_hash(), de_mcas.operation_hash());
            assert_eq!(
                mcas.operation_with_signature(),
                de_mcas.operation_with_signature()
            );
            assert_eq!(mcas.signature(), de_mcas.signature());
        }
    }
}
