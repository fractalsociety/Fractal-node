//! HotStuff-2 quorum certificate wire shape (`docs/prd.md` §7.3, §18 M7).
//!
//! Phase 1 (`n = 1`): aggregate signature bytes may be zeroed; [`hash_qc`] still commits the
//! voted header identity so `parent_qc_hash` chains across blocks. M7-d-6: real BLS aggregates
//! are carried in [`QuorumCertificate`] and verified via [`crate::vote::verify_formed_qc`] using
//! signer indices on the [`crate::Block`].

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_crypto::hash::keccak256;
use fractal_crypto::{AggregateSignature, Hash256};

/// Certificate that a quorum voted for a specific block header identity.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct QuorumCertificate {
    pub view: u64,
    pub block_height: u64,
    pub block_header_hash: Hash256,
    pub aggregate_sig: AggregateSignature,
}

/// `true` for [`genesis_parent_qc`] (synthetic parent of height 1); skips aggregate crypto verify.
#[must_use]
pub fn is_genesis_parent_qc(qc: &QuorumCertificate) -> bool {
    qc.view == 0
        && qc.block_height == 0
        && qc.block_header_hash == [0u8; 32]
        && qc.aggregate_sig.bytes == [0u8; 96]
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
    fn is_genesis_parent_qc_detects_genesis_only() {
        assert!(is_genesis_parent_qc(&genesis_parent_qc()));
        let mut q = genesis_parent_qc();
        q.view = 1;
        assert!(!is_genesis_parent_qc(&q));
    }

    #[test]
    fn qc_hash_differs_when_header_hash_differs() {
        let a = singleton_qc_certifying([1u8; 32], 1, 0);
        let b = singleton_qc_certifying([2u8; 32], 1, 0);
        assert_ne!(hash_qc(&a).unwrap(), hash_qc(&b).unwrap());
    }
}
