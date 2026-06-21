//! Canonical statement encoding for tier-2 aggregation (Poseidon preimage).

use fractal_crypto::hash::Hash256;
use fractal_shard::ProofSubmissionV1;

/// Max tier-1 submissions hashed in one Plonky2 aggregation step.
pub const MAX_AGG_PROOFS: usize = 16;

const TAG: u64 = 0x4d43_4147; // "MCAG"

/// Number of Goldilocks field elements in a fixed-size aggregation statement.
#[must_use]
pub const fn statement_field_len() -> usize {
    // tag, version, height, gsr[4], count, MAX * (shard, start, end, digest[4])
    8 + MAX_AGG_PROOFS * 7
}

/// Number of Goldilocks field elements in the verified-STWO aggregation statement.
#[must_use]
pub const fn verified_statement_field_len() -> usize {
    // Base fields plus, per proof:
    // proof key/digest[7], chain_id, height, header[4], parent[4], state[4], tx[4], gas.
    8 + MAX_AGG_PROOFS * 26
}

/// Public STWO checkpoint statement verified before Plonky2 aggregation and bound in-circuit.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct VerifiedStwoStatementV1 {
    pub shard_id: u32,
    pub chain_id: u64,
    pub start_block: u64,
    pub end_block: u64,
    pub height: u64,
    pub header_hash: Hash256,
    pub parent_hash: Hash256,
    pub state_root: Hash256,
    pub tx_root: Hash256,
    pub gas_used: u64,
    pub proof_digest: Hash256,
}

impl VerifiedStwoStatementV1 {
    #[must_use]
    pub fn from_checkpoint_parts(
        shard_id: u32,
        chain_id: u64,
        start_block: u64,
        end_block: u64,
        height: u64,
        header_hash: Hash256,
        parent_hash: Hash256,
        state_root: Hash256,
        tx_root: Hash256,
        gas_used: u64,
        proof_digest: Hash256,
    ) -> Self {
        Self {
            shard_id,
            chain_id,
            start_block,
            end_block,
            height,
            header_hash,
            parent_hash,
            state_root,
            tx_root,
            gas_used,
            proof_digest,
        }
    }

    #[must_use]
    pub fn matches_submission(&self, p: &ProofSubmissionV1) -> bool {
        self.shard_id == p.shard_id
            && self.start_block == p.start_block
            && self.end_block == p.end_block
            && self.proof_digest == p.proof_digest
    }
}

/// Pack `global_state_root` into four canonical u64 limbs (little-endian bytes).
#[must_use]
pub fn hash256_to_u64_limbs(h: &Hash256) -> [u64; 4] {
    let mut out = [0u64; 4];
    for (i, limb) in out.iter_mut().enumerate() {
        let start = i * 8;
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&h[start..start + 8]);
        *limb = u64::from_le_bytes(buf);
    }
    out
}

/// Encode a masterchain aggregation statement (sorted proofs, fixed padding).
#[must_use]
pub fn encode_statement_u64(
    version: u8,
    masterchain_height: u64,
    global_state_root: &Hash256,
    proofs: &[ProofSubmissionV1],
) -> Vec<u64> {
    let mut sorted: Vec<&ProofSubmissionV1> = proofs.iter().collect();
    sorted.sort_by(|a, b| {
        (a.shard_id, a.start_block, a.end_block).cmp(&(b.shard_id, b.start_block, b.end_block))
    });
    let gsr = hash256_to_u64_limbs(global_state_root);
    let mut v = Vec::with_capacity(statement_field_len());
    v.push(TAG);
    v.push(u64::from(version));
    v.push(masterchain_height);
    v.extend_from_slice(&gsr);
    v.push(sorted.len() as u64);
    for p in &sorted {
        v.push(u64::from(p.shard_id));
        v.push(p.start_block);
        v.push(p.end_block);
        v.extend_from_slice(&hash256_to_u64_limbs(&p.proof_digest));
    }
    let target_len = statement_field_len();
    if v.len() > target_len {
        v.truncate(target_len);
    } else {
        v.resize(target_len, 0);
    }
    v
}

/// Encode a masterchain aggregation statement with verified STWO public inputs.
#[must_use]
pub fn encode_verified_statement_u64(
    version: u8,
    masterchain_height: u64,
    global_state_root: &Hash256,
    proofs: &[ProofSubmissionV1],
    verified: &[VerifiedStwoStatementV1],
) -> Vec<u64> {
    let mut pairs: Vec<(&ProofSubmissionV1, &VerifiedStwoStatementV1)> = proofs
        .iter()
        .filter_map(|p| {
            verified
                .iter()
                .find(|s| s.matches_submission(p))
                .map(|s| (p, s))
        })
        .collect();
    pairs.sort_by(|(a, _), (b, _)| {
        (a.shard_id, a.start_block, a.end_block).cmp(&(b.shard_id, b.start_block, b.end_block))
    });
    let gsr = hash256_to_u64_limbs(global_state_root);
    let mut v = Vec::with_capacity(verified_statement_field_len());
    v.push(TAG);
    v.push(u64::from(version));
    v.push(masterchain_height);
    v.extend_from_slice(&gsr);
    v.push(pairs.len() as u64);
    for (p, s) in &pairs {
        v.push(u64::from(p.shard_id));
        v.push(p.start_block);
        v.push(p.end_block);
        v.extend_from_slice(&hash256_to_u64_limbs(&p.proof_digest));
        v.push(s.chain_id);
        v.push(s.height);
        v.extend_from_slice(&hash256_to_u64_limbs(&s.header_hash));
        v.extend_from_slice(&hash256_to_u64_limbs(&s.parent_hash));
        v.extend_from_slice(&hash256_to_u64_limbs(&s.state_root));
        v.extend_from_slice(&hash256_to_u64_limbs(&s.tx_root));
        v.push(s.gas_used);
    }
    let target_len = verified_statement_field_len();
    if v.len() > target_len {
        v.truncate(target_len);
    } else {
        v.resize(target_len, 0);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_shard::ProofSubmissionV1;

    #[test]
    fn fixed_statement_len() {
        let proofs = vec![ProofSubmissionV1 {
            shard_id: 1,
            start_block: 2,
            end_block: 3,
            prover: [0; 20],
            lag_seconds: 0,
            proof_digest: [9u8; 32],
        }];
        let enc = encode_statement_u64(2, 5, &[7u8; 32], &proofs);
        assert_eq!(enc.len(), statement_field_len());
    }

    #[test]
    fn fixed_verified_statement_len() {
        let proof = ProofSubmissionV1 {
            shard_id: 1,
            start_block: 2,
            end_block: 3,
            prover: [0; 20],
            lag_seconds: 0,
            proof_digest: [9u8; 32],
        };
        let stmt = VerifiedStwoStatementV1 {
            shard_id: 1,
            chain_id: 41,
            start_block: 2,
            end_block: 3,
            height: 3,
            header_hash: [1u8; 32],
            parent_hash: [2u8; 32],
            state_root: [3u8; 32],
            tx_root: [4u8; 32],
            gas_used: 21_000,
            proof_digest: [9u8; 32],
        };
        let enc = encode_verified_statement_u64(2, 5, &[7u8; 32], &[proof], &[stmt]);
        assert_eq!(enc.len(), verified_statement_field_len());
    }
}
