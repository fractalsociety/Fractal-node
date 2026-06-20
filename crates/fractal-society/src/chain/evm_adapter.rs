//! EVM commitment adapter for `BatchSettlement.submitBatchRoot(bytes32)`.
//!
//! The adapter is gated behind the `live-chain` feature. It keeps the
//! `CommitmentAdapter` surface synchronous while delegating the live EVM work to
//! an ethers-backed settlement client.

use std::sync::Arc;

use ethers::abi::{Abi, Token};
use ethers::contract::Contract;
use ethers::middleware::Middleware;
use ethers::types::{Address, H256};

use crate::error::Error;
use crate::pkgs::chain_commitment::CommitmentAdapter;
use crate::protocol::{ChainReference, Hash};

/// Method name on `BatchSettlement.sol`.
pub const SUBMIT_BATCH_ROOT_METHOD: &str = "submitBatchRoot";

/// Minimal ABI for the `BatchSettlement` commitment entrypoint.
pub const BATCH_SETTLEMENT_ABI: &str = r#"[{"type":"function","name":"submitBatchRoot","inputs":[{"name":"root","type":"bytes32"}],"outputs":[],"stateMutability":"nonpayable"}]"#;

/// Decoded EVM batch root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvmBatchRoot(pub [u8; 32]);

/// Receipt fields needed to build a [`ChainReference`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvmCommitmentReceipt {
    /// EVM network name or chain id label.
    pub network: String,
    /// Transaction hash returned by the EVM node.
    pub transaction_hash: String,
    /// Included block number.
    pub block_number: u64,
    /// Whether the client considers the commitment finalized.
    pub finalized: bool,
}

impl From<EvmCommitmentReceipt> for ChainReference {
    fn from(value: EvmCommitmentReceipt) -> Self {
        Self {
            network: value.network,
            transaction_hash: value.transaction_hash,
            block_number: value.block_number,
            finalized: value.finalized,
        }
    }
}

/// Client boundary for submitting roots to an EVM settlement contract.
pub trait EvmSettlementClient: Send + Sync {
    /// Submit `root` to `BatchSettlement.submitBatchRoot`.
    fn submit_batch_root(&self, root: EvmBatchRoot) -> crate::Result<EvmCommitmentReceipt>;
}

/// Commitment adapter that submits proof hashes as EVM batch roots.
#[derive(Debug, Clone)]
pub struct EvmCommitmentAdapter<C> {
    client: C,
}

impl<C> EvmCommitmentAdapter<C>
where
    C: EvmSettlementClient,
{
    /// Create an EVM commitment adapter from a settlement client.
    pub fn new(client: C) -> Self {
        Self { client }
    }
}

impl<C> CommitmentAdapter for EvmCommitmentAdapter<C>
where
    C: EvmSettlementClient,
{
    fn submit(&self, proof_hash: &Hash) -> crate::Result<ChainReference> {
        let root = batch_root_from_hash(proof_hash)?;
        Ok(self.client.submit_batch_root(root)?.into())
    }
}

/// Decode a protocol hash into the bytes32 root expected by the contract.
pub fn batch_root_from_hash(proof_hash: &Hash) -> crate::Result<EvmBatchRoot> {
    let bytes = hex::decode(&proof_hash.0).map_err(|err| {
        Error::InvalidArtifact(format!("proof hash must be hex encoded bytes32: {err}"))
    })?;
    let root: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
        Error::InvalidArtifact(format!(
            "proof hash must decode to 32 bytes, got {}",
            bytes.len()
        ))
    })?;
    Ok(EvmBatchRoot(root))
}

/// Build calldata for `submitBatchRoot(bytes32)`.
pub fn submit_batch_root_calldata(proof_hash: &Hash) -> crate::Result<Vec<u8>> {
    let root = batch_root_from_hash(proof_hash)?;
    let function = batch_settlement_abi()?
        .function(SUBMIT_BATCH_ROOT_METHOD)
        .map_err(|err| Error::ProtocolViolation(format!("missing ABI method: {err}")))?
        .clone();
    function
        .encode_input(&[Token::FixedBytes(root.0.to_vec())])
        .map_err(|err| Error::ProtocolViolation(format!("failed to encode EVM calldata: {err}")))
}

fn batch_settlement_abi() -> crate::Result<Abi> {
    serde_json::from_str(BATCH_SETTLEMENT_ABI).map_err(Error::Json)
}

/// ethers-backed client for a live `BatchSettlement` contract.
pub struct EthersBatchSettlementClient<M>
where
    M: Middleware,
{
    network: String,
    contract: Contract<M>,
    runtime: tokio::runtime::Runtime,
}

impl<M> EthersBatchSettlementClient<M>
where
    M: Middleware + 'static,
{
    /// Build a client from an ethers middleware and contract address.
    pub fn new(
        network: impl Into<String>,
        contract_address: Address,
        middleware: Arc<M>,
    ) -> crate::Result<Self> {
        let abi = batch_settlement_abi()?;
        let contract = Contract::new(contract_address, abi, middleware);
        let runtime = tokio::runtime::Runtime::new().map_err(Error::Io)?;
        Ok(Self {
            network: network.into(),
            contract,
            runtime,
        })
    }
}

impl<M> EvmSettlementClient for EthersBatchSettlementClient<M>
where
    M: Middleware + 'static,
{
    fn submit_batch_root(&self, root: EvmBatchRoot) -> crate::Result<EvmCommitmentReceipt> {
        let network = self.network.clone();
        let receipt = self.runtime.block_on(async {
            let call = self
                .contract
                .method::<_, ()>(SUBMIT_BATCH_ROOT_METHOD, H256::from(root.0))
                .map_err(|err| {
                    Error::ProtocolViolation(format!("failed to prepare EVM transaction: {err}"))
                })?;
            let pending = call.send().await.map_err(|err| {
                Error::ProtocolViolation(format!("failed to submit EVM transaction: {err}"))
            })?;
            pending
                .await
                .map_err(|err| {
                    Error::ProtocolViolation(format!("failed to fetch EVM receipt: {err}"))
                })?
                .ok_or_else(|| {
                    Error::ProtocolViolation(
                        "EVM transaction completed without receipt".to_string(),
                    )
                })
        })?;

        let block_number = receipt
            .block_number
            .ok_or_else(|| {
                Error::ProtocolViolation("EVM receipt missing block number".to_string())
            })?
            .as_u64();

        Ok(EvmCommitmentReceipt {
            network,
            transaction_hash: format!("{:#x}", receipt.transaction_hash),
            block_number,
            finalized: true,
        })
    }
}
