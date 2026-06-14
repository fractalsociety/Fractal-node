//! HotStuff-2 quorum certificate wire shape (`docs/prd.md` §7.3, §18 M7).
//!
//! Phase 1 (`n = 1`): aggregate signature bytes are zeroed; [`hash_qc`] still commits the
//! voted header identity so `parent_qc_hash` chains across blocks. Real BLS verification
//! is deferred to full M7.

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_crypto::hash::keccak256;
use fractal_crypto::{AggregateSignature, Hash256};

use crate::BlockHeader;

/// Certificate that a quorum voted for a specific block header identity.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct QuorumCertificate {
    pub view: u64,
    pub block_height: u64,
    pub block_header_hash: Hash256,
    pub aggregate_sig: AggregateSignature,
}

/// `keccak256(borsh(qc))` — used as `BlockHeader.parent_qc_hash`.
pub fn hash_qc(qc: &QuorumCertificate) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(qc)?))
}

/// QC for the synthetic genesis parent (height 0, no header yet).
pub fn genesis_parent_qc() -> QuorumCertificate {
    QuorumCertificate {
        view: 0,
        block_height: 0,
        block_header_hash: [0u8; 32],
        aggregate_sig: AggregateSignature { bytes: [0u8; 96] },
    }
}

/// Singleton placeholder: one logical vote for `block_header_hash` at (`height`, `view`).
pub fn singleton_qc_certifying(
    block_header_hash: Hash256,
    block_height: u64,
    view: u64,
) -> QuorumCertificate {
    QuorumCertificate {
        view,
        block_height,
        block_header_hash,
        aggregate_sig: AggregateSignature { bytes: [0u8; 96] },
    }
}

/// After committing `header` (with canonical `header_hash`), the next block's `parent_qc_hash`.
pub fn next_parent_qc_hash_after_commit(
    header: &BlockHeader,
    header_hash: Hash256,
) -> Result<Hash256, std::io::Error> {
    let qc = singleton_qc_certifying(header_hash, header.height, header.view);
    hash_qc(&qc)
}

/// Expected `parent_qc_hash` for the child of `parent_header` (must match `header_hash(parent_header)`).
pub fn expected_parent_qc_for_parent_header(
    parent_header: &BlockHeader,
) -> Result<Hash256, std::io::Error> {
    let ph = crate::header_hash(parent_header)?;
    let qc = singleton_qc_certifying(ph, parent_header.height, parent_header.view);
    hash_qc(&qc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genesis_qc_hash_stable() {
        let h = hash_qc(&genesis_parent_qc()).unwrap();
        assert_ne!(h, [0u8; 32]);
        let h2 = hash_qc(&genesis_parent_qc()).unwrap();
        assert_eq!(h, h2);
    }

    #[test]
    fn qc_hash_differs_when_header_hash_differs() {
        let a = singleton_qc_certifying([1u8; 32], 1, 0);
        let b = singleton_qc_certifying([2u8; 32], 1, 0);
        assert_ne!(hash_qc(&a).unwrap(), hash_qc(&b).unwrap());
    }
}
