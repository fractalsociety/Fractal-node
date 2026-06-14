//! HotStuff-2–oriented block types for **singleton** (`n = 1`, `f = 0`) production (`docs/prd.md` §7.3, §18 M2).
//!
//! Full vote aggregation / libp2p gossip lands in later milestones; this crate freezes the
//! on-disk / wire shape and deterministic header hashing for the execution pipeline.
//!
//! [`qc`] defines quorum certificate hashing and the Phase-1 singleton `parent_qc_hash` chain
//! (`docs/prd.md` §18 M7-a).
//!
//! [`validators`] holds static validator sets and view-based leader ids (`docs/prd.md` §18 M7-b).
//!
//! [`vote`] holds per-validator HotStuff-2 vote wire types (`docs/prd.md` §18 M7-d-3).

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_core::{state_root, ExecError, State, Transaction};
use fractal_crypto::hash::{keccak256, Hash256};
use reed_solomon_erasure::galois_8::ReedSolomon;
use thiserror::Error;

pub mod proof;
pub mod qc;
pub mod validators;
pub mod vote;

pub use fractal_core::Transaction as Tx;
pub use proof::{
    canonical_recursive_proof_fixture_v1, stwo_execution_air_adapter_digest,
    stwo_execution_air_adapter_v1, stwo_execution_air_id, stwo_plonky2_public_input_limbs,
    stwo_plonky2_verifier_id, verify_stwo_plonky2_proof, CanonicalRecursiveProofFixtureV1,
    ProductionProofVerifyError, StwoExecutionAirAdapterV1, StwoPlonky2ProofEnvelope,
};
pub use qc::{
    expected_parent_qc_for_parent_header, genesis_parent_qc, hash_qc,
    next_parent_qc_hash_after_commit, singleton_qc_certifying, QuorumCertificate,
};
pub use validators::{ValidatorEntry, ValidatorId, ValidatorSet};
pub use vote::{
    verify_formed_qc, FormedQc, RecordVoteOutcome, Vote, VoteError, VotePool, VoteSignBody,
};

#[derive(Debug, Error)]
pub enum BuildBlockError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Exec(#[from] ExecError),
    #[error("eth_signed_raw length {got} != transactions length {txs}")]
    EthRawLenMismatch { txs: usize, got: usize },
    #[error("data availability sidecar invalid")]
    DataAvailability,
}

#[derive(Debug, Error)]
pub enum ProofVerifyError {
    #[error("validity proof chain id does not match block")]
    ChainId,
    #[error("validity proof height does not match block")]
    Height,
    #[error("validity proof block hash does not match block")]
    BlockHash,
    #[error("validity proof state root does not match block")]
    StateRoot,
    #[error("validity proof tx root does not match block")]
    TxRoot,
    #[error("validity proof DA root does not match block")]
    DaRoot,
    #[error("validity proof zone namespace does not match block")]
    ZoneNamespace,
    #[error("validity proof bytes are empty")]
    EmptyProof,
    #[error("production proof verification failed: {0}")]
    Production(#[from] ProductionProofVerifyError),
    #[error("dev digest proof does not match public inputs")]
    BadDevDigest,
    #[error("data availability sidecar invalid")]
    DataAvailability,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DaVerifyError {
    #[error("data availability namespace mismatch")]
    Namespace,
    #[error("data availability original length mismatch")]
    OriginalLen,
    #[error("data availability share count mismatch")]
    ShareCount,
    #[error("data availability root mismatch")]
    Root,
    #[error("data availability share index mismatch")]
    ShareIndex,
    #[error("data availability share commitment mismatch")]
    ShareCommitment,
    #[error("data availability sampled share missing")]
    SampleMissing,
    #[error("data availability erasure coding failed")]
    ErasureCoding,
    #[error("data availability insufficient shares for reconstruction")]
    InsufficientShares,
}

pub type DaNamespace = [u8; 8];
pub type ExecutionZoneNamespace = DaNamespace;

pub const DEFAULT_DA_NAMESPACE: DaNamespace = *b"fracbase";
pub const MASTERCHAIN_ZONE_NAMESPACE: ExecutionZoneNamespace = DEFAULT_DA_NAMESPACE;
pub const DEFAULT_DA_SHARE_SIZE: u32 = 512;
pub const DEFAULT_DA_PARITY_RATIO_NUMERATOR: u32 = 1;
pub const DEFAULT_DA_PARITY_RATIO_DENOMINATOR: u32 = 1;
pub const DEFAULT_DA_GAS_PER_BYTE: u64 = 1;
pub const DEFAULT_DA_FEE_PER_GAS: u128 = 1;

/// Legacy floor gas per tx (EVM transfer); native txs use [`fractal_core::intrinsic_gas`].
pub const MIN_TX_GAS: u64 = 21_000;

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct BlockHeader {
    pub version: u16,
    pub chain_id: u64,
    pub height: u64,
    pub view: u64,
    pub parent_hash: Hash256,
    /// Parent QC hash (HotStuff-2): `keccak256(borsh(QC))` certifying the parent block header.
    /// First real block uses [`crate::genesis_parent_qc`]; see [`crate::qc`].
    pub parent_qc_hash: Hash256,
    pub proposer: [u8; 32],
    pub timestamp_ms: u64,
    pub state_root: Hash256,
    pub tx_root: Hash256,
    pub zone_namespace: ExecutionZoneNamespace,
    pub da_root: Hash256,
    pub da_bytes: u64,
    pub da_share_count: u32,
    pub da_gas_used: u64,
    pub da_fee_paid: u128,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub extra: [u8; 32],
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct DaShare {
    pub namespace: DaNamespace,
    pub index: u32,
    pub is_parity: bool,
    pub data: Vec<u8>,
    pub commitment: Hash256,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct DaSidecar {
    pub namespace: DaNamespace,
    pub original_len: u64,
    pub share_size: u32,
    pub data_share_count: u32,
    pub parity_share_count: u32,
    pub shares: Vec<DaShare>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    /// Parallel to `transactions`: optional original EIP-1559 bytes (`keccak256` = RPC tx hash).
    /// Followers replay this to populate `NodeInner::eth_signed_raw` / hash maps like the producer.
    pub eth_signed_raw: Vec<Option<Vec<u8>>>,
    /// Initial DA sidecar: chunked transaction payload committed by `header.da_root`.
    pub da_sidecar: DaSidecar,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum ValidityProofSystem {
    /// Test/dev proof receipt: `proof_bytes == validity_proof_public_input_digest(proof)`.
    DevDigest,
    /// Production target. Verification must be wired before this mode can finalize blocks.
    StwoPlonky2,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct BlockValidityProof {
    pub chain_id: u64,
    pub height: u64,
    pub block_hash: Hash256,
    pub state_root: Hash256,
    pub tx_root: Hash256,
    pub zone_namespace: ExecutionZoneNamespace,
    pub da_root: Hash256,
    pub proof_system: ValidityProofSystem,
    pub proof_bytes: Vec<u8>,
}

#[derive(BorshSerialize)]
struct ValidityProofPublicInputs {
    chain_id: u64,
    height: u64,
    block_hash: Hash256,
    state_root: Hash256,
    tx_root: Hash256,
    zone_namespace: ExecutionZoneNamespace,
    da_root: Hash256,
}

fn tx_hash(tx: &Transaction) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(tx)?))
}

fn hash_pair(left: &Hash256, right: &Hash256) -> Hash256 {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(left);
    buf[32..].copy_from_slice(right);
    keccak256(&buf)
}

fn merkle_root_from_hashes(hashes: &[Hash256]) -> Hash256 {
    if hashes.is_empty() {
        return [0u8; 32];
    }
    let mut level = hashes.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity((level.len() + 1) / 2);
        let mut i = 0;
        while i < level.len() {
            if i + 1 < level.len() {
                next.push(hash_pair(&level[i], &level[i + 1]));
                i += 2;
            } else {
                next.push(hash_pair(&level[i], &level[i]));
                i += 1;
            }
        }
        level = next;
    }
    level[0]
}

/// Ordered Merkle root over transaction hashes (matches canonical tx order in the block).
pub fn ordered_tx_root(txs: &[Transaction]) -> Result<Hash256, std::io::Error> {
    if txs.is_empty() {
        return Ok([0u8; 32]);
    }
    let hashes: Vec<Hash256> = txs.iter().map(tx_hash).collect::<Result<_, _>>()?;
    Ok(merkle_root_from_hashes(&hashes))
}

pub fn header_hash(header: &BlockHeader) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(header)?))
}

#[derive(BorshSerialize)]
struct DaShareCommitment<'a> {
    namespace: DaNamespace,
    index: u32,
    is_parity: bool,
    data: &'a [u8],
}

pub fn da_share_commitment(
    namespace: DaNamespace,
    index: u32,
    is_parity: bool,
    data: &[u8],
) -> Hash256 {
    let body = DaShareCommitment {
        namespace,
        index,
        is_parity,
        data,
    };
    keccak256(&borsh::to_vec(&body).expect("da share commitment borsh"))
}

pub fn da_root(sidecar: &DaSidecar) -> Hash256 {
    let commits: Vec<Hash256> = sidecar.shares.iter().map(|s| s.commitment).collect();
    merkle_root_from_hashes(&commits)
}

pub fn da_encoded_bytes(sidecar: &DaSidecar) -> u64 {
    sidecar.shares.iter().map(|s| s.data.len() as u64).sum()
}

pub fn da_gas_for_sidecar(sidecar: &DaSidecar) -> u64 {
    da_encoded_bytes(sidecar).saturating_mul(DEFAULT_DA_GAS_PER_BYTE)
}

pub fn da_fee_for_gas(da_gas_used: u64) -> u128 {
    u128::from(da_gas_used).saturating_mul(DEFAULT_DA_FEE_PER_GAS)
}

fn default_parity_share_count(data_share_count: u32) -> u32 {
    data_share_count
        .saturating_mul(DEFAULT_DA_PARITY_RATIO_NUMERATOR)
        .div_ceil(DEFAULT_DA_PARITY_RATIO_DENOMINATOR)
        .max(1)
}

fn required_data_share_count(original_len: u64, share_size: u32) -> Result<u32, DaVerifyError> {
    let share_size = share_size.max(1);
    if original_len == 0 {
        return Ok(0);
    }
    let count = original_len.div_ceil(u64::from(share_size));
    u32::try_from(count).map_err(|_| DaVerifyError::ShareCount)
}

pub fn build_da_sidecar(payload: &[u8], namespace: DaNamespace, share_size: u32) -> DaSidecar {
    let share_size = share_size.max(1) as usize;
    if payload.is_empty() {
        return DaSidecar {
            namespace,
            original_len: 0,
            share_size: share_size as u32,
            data_share_count: 0,
            parity_share_count: 0,
            shares: Vec::new(),
        };
    }
    let data_share_count = payload.len().div_ceil(share_size);
    let parity_share_count = default_parity_share_count(data_share_count as u32) as usize;
    let codec = ReedSolomon::new(data_share_count, parity_share_count)
        .expect("valid DA erasure coding parameters");
    let mut shards = Vec::with_capacity(data_share_count + parity_share_count);
    for i in 0..data_share_count {
        let start = i * share_size;
        let end = (start + share_size).min(payload.len());
        let mut shard = vec![0u8; share_size];
        shard[..end - start].copy_from_slice(&payload[start..end]);
        shards.push(shard);
    }
    for _ in 0..parity_share_count {
        shards.push(vec![0u8; share_size]);
    }
    codec
        .encode(&mut shards)
        .expect("DA erasure encoding should succeed");
    let mut shares = Vec::new();
    for (i, data) in shards.into_iter().enumerate() {
        let index = i as u32;
        let is_parity = i >= data_share_count;
        let commitment = da_share_commitment(namespace, index, is_parity, &data);
        shares.push(DaShare {
            namespace,
            index,
            is_parity,
            data,
            commitment,
        });
    }
    DaSidecar {
        namespace,
        original_len: payload.len() as u64,
        share_size: share_size as u32,
        data_share_count: data_share_count as u32,
        parity_share_count: parity_share_count as u32,
        shares,
    }
}

pub fn reconstruct_da_payload(sidecar: &DaSidecar) -> Result<Vec<u8>, DaVerifyError> {
    reconstruct_da_payload_from_shares(sidecar, sidecar.shares.iter().cloned())
}

pub fn reconstruct_da_payload_from_shares<I>(
    sidecar: &DaSidecar,
    shares: I,
) -> Result<Vec<u8>, DaVerifyError>
where
    I: IntoIterator<Item = DaShare>,
{
    verify_da_layout(sidecar)?;
    if sidecar.original_len == 0 {
        return Ok(Vec::new());
    }
    let data_count = sidecar.data_share_count as usize;
    let parity_count = sidecar.parity_share_count as usize;
    let total_count = data_count + parity_count;
    let share_size = sidecar.share_size as usize;
    let mut shards = vec![None; total_count];
    for share in shares {
        if share.namespace != sidecar.namespace {
            return Err(DaVerifyError::Namespace);
        }
        if share.index as usize >= total_count {
            return Err(DaVerifyError::ShareIndex);
        }
        if share.is_parity != (share.index as usize >= data_count) {
            return Err(DaVerifyError::ShareIndex);
        }
        if share.data.len() != share_size {
            return Err(DaVerifyError::OriginalLen);
        }
        let expected =
            da_share_commitment(share.namespace, share.index, share.is_parity, &share.data);
        if share.commitment != expected {
            return Err(DaVerifyError::ShareCommitment);
        }
        shards[share.index as usize] = Some(share.data);
    }
    if shards.iter().filter(|s| s.is_some()).count() < data_count {
        return Err(DaVerifyError::InsufficientShares);
    }
    let codec =
        ReedSolomon::new(data_count, parity_count).map_err(|_| DaVerifyError::ErasureCoding)?;
    codec
        .reconstruct(&mut shards)
        .map_err(|_| DaVerifyError::ErasureCoding)?;
    let mut out = Vec::new();
    for shard in shards.into_iter().take(data_count) {
        let shard = shard.ok_or(DaVerifyError::ErasureCoding)?;
        out.extend_from_slice(&shard);
    }
    out.truncate(sidecar.original_len as usize);
    Ok(out)
}

fn verify_da_layout(sidecar: &DaSidecar) -> Result<(), DaVerifyError> {
    let share_size = sidecar.share_size.max(1);
    if sidecar.share_size == 0 {
        return Err(DaVerifyError::OriginalLen);
    }
    let required_data_shares = required_data_share_count(sidecar.original_len, share_size)?;
    if sidecar.data_share_count != required_data_shares {
        return Err(DaVerifyError::ShareCount);
    }
    if sidecar.original_len == 0 {
        if sidecar.data_share_count != 0
            || sidecar.parity_share_count != 0
            || !sidecar.shares.is_empty()
        {
            return Err(DaVerifyError::ShareCount);
        }
        return Ok(());
    }
    if sidecar.parity_share_count == 0 {
        return Err(DaVerifyError::ShareCount);
    }
    let total = sidecar
        .data_share_count
        .checked_add(sidecar.parity_share_count)
        .ok_or(DaVerifyError::ShareCount)?;
    if sidecar.shares.len() != total as usize {
        return Err(DaVerifyError::ShareCount);
    }
    Ok(())
}

pub fn verify_da_sidecar(header: &BlockHeader, sidecar: &DaSidecar) -> Result<(), DaVerifyError> {
    if sidecar.namespace != header.zone_namespace {
        return Err(DaVerifyError::Namespace);
    }
    if sidecar.original_len != header.da_bytes {
        return Err(DaVerifyError::OriginalLen);
    }
    if sidecar.shares.len() != header.da_share_count as usize {
        return Err(DaVerifyError::ShareCount);
    }
    if header.da_gas_used != da_gas_for_sidecar(sidecar) {
        return Err(DaVerifyError::OriginalLen);
    }
    if header.da_fee_paid != da_fee_for_gas(header.da_gas_used) {
        return Err(DaVerifyError::OriginalLen);
    }
    verify_da_layout(sidecar)?;
    for (i, share) in sidecar.shares.iter().enumerate() {
        if share.index != i as u32 {
            return Err(DaVerifyError::ShareIndex);
        }
        if share.is_parity != (i >= sidecar.data_share_count as usize) {
            return Err(DaVerifyError::ShareIndex);
        }
        if share.namespace != header.zone_namespace || share.namespace != sidecar.namespace {
            return Err(DaVerifyError::Namespace);
        }
        if share.data.len() != sidecar.share_size as usize {
            return Err(DaVerifyError::OriginalLen);
        }
        let expected =
            da_share_commitment(share.namespace, share.index, share.is_parity, &share.data);
        if share.commitment != expected {
            return Err(DaVerifyError::ShareCommitment);
        }
    }
    if da_root(sidecar) != header.da_root {
        return Err(DaVerifyError::Root);
    }
    Ok(())
}

pub fn verify_da_samples(
    sidecar: &DaSidecar,
    expected_root: Hash256,
    expected_namespace: DaNamespace,
    seed: u64,
    sample_count: usize,
) -> Result<(), DaVerifyError> {
    if sidecar.namespace != expected_namespace {
        return Err(DaVerifyError::Namespace);
    }
    if da_root(sidecar) != expected_root {
        return Err(DaVerifyError::Root);
    }
    if sidecar.shares.is_empty() {
        return Ok(());
    }
    let mut rng = SampleRng::new(seed);
    for _ in 0..sample_count {
        let idx = (rng.next() as usize) % sidecar.shares.len();
        let share = sidecar
            .shares
            .get(idx)
            .ok_or(DaVerifyError::SampleMissing)?;
        if share.namespace != expected_namespace {
            return Err(DaVerifyError::Namespace);
        }
        let expected =
            da_share_commitment(share.namespace, share.index, share.is_parity, &share.data);
        if share.commitment != expected {
            return Err(DaVerifyError::ShareCommitment);
        }
    }
    Ok(())
}

pub fn validity_proof_public_input_digest(
    proof: &BlockValidityProof,
) -> Result<Hash256, std::io::Error> {
    let inputs = ValidityProofPublicInputs {
        chain_id: proof.chain_id,
        height: proof.height,
        block_hash: proof.block_hash,
        state_root: proof.state_root,
        tx_root: proof.tx_root,
        zone_namespace: proof.zone_namespace,
        da_root: proof.da_root,
    };
    Ok(keccak256(&borsh::to_vec(&inputs)?))
}

pub fn verify_block_validity_proof(
    block: &Block,
    proof: &BlockValidityProof,
) -> Result<(), ProofVerifyError> {
    if proof.chain_id != block.header.chain_id {
        return Err(ProofVerifyError::ChainId);
    }
    if proof.height != block.header.height {
        return Err(ProofVerifyError::Height);
    }
    if proof.block_hash != header_hash(&block.header)? {
        return Err(ProofVerifyError::BlockHash);
    }
    if proof.state_root != block.header.state_root {
        return Err(ProofVerifyError::StateRoot);
    }
    if proof.tx_root != block.header.tx_root {
        return Err(ProofVerifyError::TxRoot);
    }
    if proof.zone_namespace != block.header.zone_namespace {
        return Err(ProofVerifyError::ZoneNamespace);
    }
    if proof.da_root != block.header.da_root {
        return Err(ProofVerifyError::DaRoot);
    }
    verify_da_sidecar(&block.header, &block.da_sidecar)
        .map_err(|_| ProofVerifyError::DataAvailability)?;
    if proof.proof_bytes.is_empty() {
        return Err(ProofVerifyError::EmptyProof);
    }
    match proof.proof_system {
        ValidityProofSystem::DevDigest => {
            let expected = validity_proof_public_input_digest(proof)?;
            if proof.proof_bytes.as_slice() != expected {
                return Err(ProofVerifyError::BadDevDigest);
            }
            Ok(())
        }
        ValidityProofSystem::StwoPlonky2 => {
            let public_input_digest = validity_proof_public_input_digest(proof)?;
            verify_stwo_plonky2_proof(&proof.proof_bytes, public_input_digest)?;
            Ok(())
        }
    }
}

/// One `None` per transaction when no Ethereum signed envelope is present.
pub fn eth_signed_raws_for_txs(txs_len: usize) -> Vec<Option<Vec<u8>>> {
    vec![None; txs_len]
}

/// Execute `txs` on top of `state`, compute roots, and assemble a `Block`.
/// Caller supplies `parent_qc_hash` (see [`crate::qc`]).
pub fn execute_and_build_block(
    chain_id: u64,
    height: u64,
    view: u64,
    parent_hash: Hash256,
    parent_qc_hash: Hash256,
    proposer: [u8; 32],
    timestamp_ms: u64,
    gas_limit: u64,
    state: &mut State,
    txs: Vec<Transaction>,
    eth_signed_raw: Vec<Option<Vec<u8>>>,
) -> Result<Block, BuildBlockError> {
    execute_and_build_zone_block(
        chain_id,
        height,
        view,
        parent_hash,
        parent_qc_hash,
        proposer,
        timestamp_ms,
        gas_limit,
        state,
        txs,
        eth_signed_raw,
        MASTERCHAIN_ZONE_NAMESPACE,
    )
}

/// Execute `txs` for one execution zone, commit its canonical payload into that zone's DA namespace,
/// and assemble a `Block`.
pub fn execute_and_build_zone_block(
    chain_id: u64,
    height: u64,
    view: u64,
    parent_hash: Hash256,
    parent_qc_hash: Hash256,
    proposer: [u8; 32],
    timestamp_ms: u64,
    gas_limit: u64,
    state: &mut State,
    txs: Vec<Transaction>,
    eth_signed_raw: Vec<Option<Vec<u8>>>,
    zone_namespace: ExecutionZoneNamespace,
) -> Result<Block, BuildBlockError> {
    if eth_signed_raw.len() != txs.len() {
        return Err(BuildBlockError::EthRawLenMismatch {
            txs: txs.len(),
            got: eth_signed_raw.len(),
        });
    }
    let mut budget_sum = 0u64;
    for tx in &txs {
        let g = fractal_core::tx_gas_limit(tx)?;
        budget_sum = budget_sum.checked_add(g).ok_or(ExecError::GasOverflow)?;
    }
    if budget_sum > gas_limit {
        return Err(ExecError::GasLimitExceeded.into());
    }
    let mut evm = fractal_evm::RevmEngine::default();
    let gas_used = fractal_core::apply_block_with_evm(state, &txs, &mut evm)?;
    debug_assert!(gas_used <= budget_sum);
    let sr = state_root(state)?;
    let tx_root = ordered_tx_root(&txs)?;
    let da_payload = borsh::to_vec(&txs)?;
    let da_sidecar = build_da_sidecar(&da_payload, zone_namespace, DEFAULT_DA_SHARE_SIZE);
    let da_root = da_root(&da_sidecar);
    let da_gas_used = da_gas_for_sidecar(&da_sidecar);
    let da_fee_paid = da_fee_for_gas(da_gas_used);
    let header = BlockHeader {
        version: 1,
        chain_id,
        height,
        view,
        parent_hash,
        parent_qc_hash,
        proposer,
        timestamp_ms,
        state_root: sr,
        tx_root,
        zone_namespace,
        da_root,
        da_bytes: da_sidecar.original_len,
        da_share_count: da_sidecar.shares.len() as u32,
        da_gas_used,
        da_fee_paid,
        gas_used,
        gas_limit,
        extra: [0u8; 32],
    };
    Ok(Block {
        header,
        transactions: txs,
        eth_signed_raw,
        da_sidecar,
    })
}

struct SampleRng {
    state: u64,
}

impl SampleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_core::{Account, NativeCall, State, Transaction, TxBody, VmKind};

    #[test]
    fn tx_root_deterministic() {
        let tx = Transaction {
            signer: [1u8; 20],
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let a = ordered_tx_root(std::slice::from_ref(&tx)).unwrap();
        let b = ordered_tx_root(std::slice::from_ref(&tx)).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn build_block_updates_state_root() {
        let mut st = State::default();
        let addr = [9u8; 20];
        st.accounts.insert(
            addr,
            Account {
                nonce: 0,
                balance: 1_000_000,
            },
        );
        let tx = Transaction {
            signer: addr,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let parent = [7u8; 32];
        let block = execute_and_build_block(
            41,
            1,
            0,
            parent,
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            vec![tx],
            eth_signed_raws_for_txs(1),
        )
        .unwrap();
        assert_eq!(block.header.height, 1);
        assert_ne!(block.header.state_root, [0u8; 32]);
    }

    #[test]
    fn dev_digest_proof_verifies_against_block_public_inputs() {
        let mut st = State::default();
        let addr = [9u8; 20];
        st.accounts.insert(
            addr,
            Account {
                nonce: 0,
                balance: 1_000_000,
            },
        );
        let tx = Transaction {
            signer: addr,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            vec![tx],
            eth_signed_raws_for_txs(1),
        )
        .unwrap();
        let mut proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            proof_system: ValidityProofSystem::DevDigest,
            proof_bytes: Vec::new(),
        };
        proof.proof_bytes = validity_proof_public_input_digest(&proof).unwrap().to_vec();

        verify_block_validity_proof(&block, &proof).expect("proof verifies");
    }

    #[test]
    fn stwo_plonky2_proof_rejects_malformed_envelope() {
        let mut st = State::default();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: vec![1, 2, 3],
        };

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::Production(
                ProductionProofVerifyError::MalformedEnvelope
            ))
        ));
    }

    #[test]
    fn stwo_plonky2_proof_rejects_invalid_verifier_data() {
        let mut st = State::default();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: borsh::to_vec(&StwoPlonky2ProofEnvelope::Plonky2PoseidonGoldilocksV1 {
                verifier_circuit_data: vec![9, 9, 9],
                proof_with_public_inputs: vec![1, 2, 3],
                compressed: false,
            })
            .unwrap(),
        };

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::Production(
                ProductionProofVerifyError::Plonky2VerifierData
            ))
        ));
    }

    #[test]
    fn stwo_air_adapter_and_recursive_fixture_bind_public_inputs() {
        let mut st = State::default();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: vec![1],
        };

        let public_input_digest = validity_proof_public_input_digest(&proof).unwrap();
        let adapter = stwo_execution_air_adapter_v1(&proof).unwrap();
        assert_eq!(adapter.public_input_digest, public_input_digest);
        assert_eq!(
            adapter.public_input_limbs,
            stwo_plonky2_public_input_limbs(&public_input_digest)
        );
        let fixture = canonical_recursive_proof_fixture_v1(&adapter, [3u8; 32]).unwrap();
        assert_eq!(
            fixture.stwo_air_adapter_digest,
            stwo_execution_air_adapter_digest(&adapter).unwrap()
        );
        assert_eq!(fixture.public_input_digest, public_input_digest);
        assert_eq!(fixture.public_input_limbs, adapter.public_input_limbs);
    }

    #[test]
    fn stwo_air_fixture_path_validates_binding_then_fails_closed() {
        let mut st = State::default();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let mut proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: Vec::new(),
        };
        let adapter = stwo_execution_air_adapter_v1(&proof).unwrap();
        let fixture = canonical_recursive_proof_fixture_v1(&adapter, [3u8; 32]).unwrap();
        proof.proof_bytes = borsh::to_vec(&StwoPlonky2ProofEnvelope::StwoV1 {
            air_adapter: adapter,
            recursive_fixture: fixture,
            proof_bytes: vec![9],
        })
        .unwrap();

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::Production(
                ProductionProofVerifyError::StwoAdapterUnavailable
            ))
        ));
    }

    #[test]
    fn stwo_air_fixture_rejects_wrong_public_inputs_before_verifier() {
        let mut st = State::default();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let mut proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: Vec::new(),
        };
        let mut adapter = stwo_execution_air_adapter_v1(&proof).unwrap();
        adapter.public_input_digest = [4u8; 32];
        let fixture = canonical_recursive_proof_fixture_v1(&adapter, [3u8; 32]).unwrap();
        proof.proof_bytes = borsh::to_vec(&StwoPlonky2ProofEnvelope::StwoV1 {
            air_adapter: adapter,
            recursive_fixture: fixture,
            proof_bytes: vec![9],
        })
        .unwrap();

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::Production(
                ProductionProofVerifyError::PublicInputDigest
            ))
        ));
    }

    #[test]
    fn dev_digest_proof_rejects_wrong_state_root() {
        let mut st = State::default();
        let addr = [9u8; 20];
        st.accounts.insert(
            addr,
            Account {
                nonce: 0,
                balance: 1,
            },
        );
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            state_root: [9u8; 32],
            tx_root: block.header.tx_root,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            proof_system: ValidityProofSystem::DevDigest,
            proof_bytes: vec![1],
        };

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::StateRoot)
        ));
    }

    #[test]
    fn block_header_commits_to_da_sidecar() {
        let mut st = State::default();
        let addr = [9u8; 20];
        st.accounts.insert(
            addr,
            Account {
                nonce: 0,
                balance: 1,
            },
        );
        let tx = Transaction {
            signer: addr,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            vec![tx],
            eth_signed_raws_for_txs(1),
        )
        .unwrap();

        verify_da_sidecar(&block.header, &block.da_sidecar).expect("da sidecar verifies");
        assert_eq!(block.header.zone_namespace, DEFAULT_DA_NAMESPACE);
        assert_eq!(block.da_sidecar.namespace, block.header.zone_namespace);
        assert_eq!(block.header.da_root, da_root(&block.da_sidecar));
        assert!(block.header.da_bytes > 0);
        assert!(block.header.da_share_count > 0);
        assert_eq!(
            block.header.da_gas_used,
            da_gas_for_sidecar(&block.da_sidecar)
        );
        assert_eq!(
            block.header.da_fee_paid,
            da_fee_for_gas(block.header.da_gas_used)
        );
    }

    #[test]
    fn da_sidecar_rejects_wrong_da_fee_accounting() {
        let mut st = State::default();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let mut bad = block.header.clone();
        bad.da_fee_paid = bad.da_fee_paid.saturating_add(1);

        assert!(verify_da_sidecar(&bad, &block.da_sidecar).is_err());
    }

    #[test]
    fn da_payload_reconstructs_canonical_transaction_bytes() {
        let tx = Transaction {
            signer: [1u8; 20],
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let payload = borsh::to_vec(&vec![tx]).unwrap();
        let sidecar = build_da_sidecar(&payload, DEFAULT_DA_NAMESPACE, 7);

        let reconstructed = reconstruct_da_payload(&sidecar).expect("reconstruct");
        assert_eq!(reconstructed, payload);
    }

    #[test]
    fn da_sidecar_adds_parity_shares() {
        let sidecar = build_da_sidecar(b"abcdefghijklmnopqrstuvwxyz", DEFAULT_DA_NAMESPACE, 7);

        assert_eq!(sidecar.data_share_count, 4);
        assert_eq!(sidecar.parity_share_count, 4);
        assert_eq!(sidecar.shares.len(), 8);
        assert!(sidecar
            .shares
            .iter()
            .take(sidecar.data_share_count as usize)
            .all(|s| !s.is_parity));
        assert!(sidecar
            .shares
            .iter()
            .skip(sidecar.data_share_count as usize)
            .all(|s| s.is_parity));
    }

    #[test]
    fn da_payload_reconstructs_with_missing_data_shares() {
        let payload = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let sidecar = build_da_sidecar(payload, DEFAULT_DA_NAMESPACE, 8);
        let available = sidecar
            .shares
            .iter()
            .filter(|s| s.index != 0 && s.index != 2)
            .cloned();

        let reconstructed =
            reconstruct_da_payload_from_shares(&sidecar, available).expect("reconstruct");
        assert_eq!(reconstructed, payload);
    }

    #[test]
    fn da_payload_rejects_insufficient_reconstruction_shares() {
        let payload = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let sidecar = build_da_sidecar(payload, DEFAULT_DA_NAMESPACE, 8);
        let available = sidecar
            .shares
            .iter()
            .take((sidecar.data_share_count - 1) as usize)
            .cloned();

        assert!(matches!(
            reconstruct_da_payload_from_shares(&sidecar, available),
            Err(DaVerifyError::InsufficientShares)
        ));
    }

    #[test]
    fn da_sampling_rejects_tampered_share() {
        let payload = b"abcdefghijklmnopqrstuvwxyz";
        let mut sidecar = build_da_sidecar(payload, DEFAULT_DA_NAMESPACE, 64);
        let root = da_root(&sidecar);
        sidecar.shares[0].data[0] ^= 0xff;

        assert!(matches!(
            verify_da_samples(&sidecar, root, DEFAULT_DA_NAMESPACE, 41, 8),
            Err(DaVerifyError::Root) | Err(DaVerifyError::ShareCommitment)
        ));
    }

    #[test]
    fn da_sampling_rejects_tampered_parity_share() {
        let payload = b"abcdefghijklmnopqrstuvwxyz";
        let mut sidecar = build_da_sidecar(payload, DEFAULT_DA_NAMESPACE, 8);
        let root = da_root(&sidecar);
        let parity_idx = sidecar.data_share_count as usize;
        sidecar.shares[parity_idx].data[0] ^= 0xff;

        assert!(matches!(
            verify_da_samples(&sidecar, root, DEFAULT_DA_NAMESPACE, 41, 16),
            Err(DaVerifyError::ShareCommitment)
        ));
    }

    #[test]
    fn zone_block_uses_supplied_da_namespace() {
        let mut st = State::default();
        let namespace = *b"zone0001";
        let block = execute_and_build_zone_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
            namespace,
        )
        .unwrap();

        assert_eq!(block.header.zone_namespace, namespace);
        assert_eq!(block.da_sidecar.namespace, namespace);
        verify_da_sidecar(&block.header, &block.da_sidecar).expect("zone da sidecar verifies");
    }

    #[test]
    fn da_sidecar_rejects_namespace_mismatch() {
        let mut st = State::default();
        let mut block = execute_and_build_zone_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
            *b"zone0001",
        )
        .unwrap();
        block.da_sidecar.namespace = *b"zone0002";

        assert!(matches!(
            verify_da_sidecar(&block.header, &block.da_sidecar),
            Err(DaVerifyError::Namespace)
        ));
    }

    #[test]
    fn da_sampling_rejects_namespace_mismatch() {
        let payload = b"abcdefghijklmnopqrstuvwxyz";
        let sidecar = build_da_sidecar(payload, *b"zone0001", 64);
        let root = da_root(&sidecar);

        assert!(matches!(
            verify_da_samples(&sidecar, root, *b"zone0002", 41, 8),
            Err(DaVerifyError::Namespace)
        ));
    }

    #[test]
    fn proof_rejects_wrong_da_root() {
        let mut st = State::default();
        let addr = [9u8; 20];
        st.accounts.insert(
            addr,
            Account {
                nonce: 0,
                balance: 1,
            },
        );
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            zone_namespace: block.header.zone_namespace,
            da_root: [9u8; 32],
            proof_system: ValidityProofSystem::DevDigest,
            proof_bytes: vec![1],
        };

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::DaRoot)
        ));
    }

    #[test]
    fn proof_rejects_wrong_zone_namespace() {
        let mut st = State::default();
        let block = execute_and_build_zone_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
            *b"zone0001",
        )
        .unwrap();
        let proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            zone_namespace: *b"zone0002",
            da_root: block.header.da_root,
            proof_system: ValidityProofSystem::DevDigest,
            proof_bytes: vec![1],
        };

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::ZoneNamespace)
        ));
    }
}
