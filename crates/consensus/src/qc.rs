//! HotStuff-2 quorum certificate wire shape + block-header helpers (`docs/prd.md` §7.3).

pub use fractal_bft_wire::qc::*;

use crate::BlockHeader;
use fractal_crypto::Hash256;

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
