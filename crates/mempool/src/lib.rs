//! In-memory mempool + simplified EIP-1559 base-fee update (`docs/prd.md` §9.4, §18 M2).

use std::collections::BTreeSet;

pub mod certificate_pool;
pub mod proof_pool;

use fractal_core::{
    OwnedObjectCertificate, OwnedObjectId, OwnedObjectVersion, Transaction, TxExecutionScope,
};

pub use certificate_pool::{
    CertificateConflictRecord, CertificateFinalityRecord, CertificatePool, CertificatePoolError,
};
pub use proof_pool::{
    PooledProofUpdate, ProofPool, ProofPoolConflictPolicy, ProofPoolError, ProofPoolMetrics,
    ProofUpdateConflict, ProofUpdateKey,
};

#[derive(Clone, Debug)]
pub struct PooledTx {
    pub tx: Transaction,
    pub max_priority_fee_per_gas: u128,
    pub max_fee_per_gas: u128,
    /// Original signed EIP-1559 bytes (`keccak256(raw)` is the canonical tx hash for RPC).
    pub eth_signed_raw: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default)]
pub struct Mempool {
    pending: Vec<PooledTx>,
}

#[derive(Clone, Debug)]
pub struct PooledCertificate {
    pub certificate: OwnedObjectCertificate,
    pub max_priority_fee_per_gas: u128,
}

#[derive(Clone, Debug, Default)]
pub struct CertificateMempool {
    pending: Vec<PooledCertificate>,
    locked_object_versions: BTreeSet<OwnedObjectVersion>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CertificateMempoolError {
    #[error("certificate conflicts with an existing certificate for object/version {0:?}")]
    ObjectVersionConflict(OwnedObjectVersion),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MempoolLaneMetrics {
    pub pending_total: usize,
    pub pending_owned: usize,
    pub pending_mixed: usize,
    pub pending_consensus: usize,
    pub pending_consensus_lane: usize,
    pub pending_certificates: usize,
}

impl CertificateMempool {
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn insert(&mut self, cert: PooledCertificate) -> Result<(), CertificateMempoolError> {
        for object_version in &cert.certificate.object_versions {
            if self.locked_object_versions.contains(object_version) {
                return Err(CertificateMempoolError::ObjectVersionConflict(
                    object_version.clone(),
                ));
            }
        }
        self.locked_object_versions
            .extend(cert.certificate.object_versions.iter().cloned());
        self.pending.push(cert);
        Ok(())
    }

    pub fn drain_ready(&mut self, max_certs: usize) -> Vec<OwnedObjectCertificate> {
        self.pending
            .sort_by(|a, b| b.max_priority_fee_per_gas.cmp(&a.max_priority_fee_per_gas));
        let drain_count = self.pending.len().min(max_certs);
        let drained = self
            .pending
            .drain(..drain_count)
            .map(|p| p.certificate)
            .collect::<Vec<_>>();
        self.rebuild_locks();
        drained
    }

    fn rebuild_locks(&mut self) {
        self.locked_object_versions.clear();
        for pending in &self.pending {
            self.locked_object_versions
                .extend(pending.certificate.object_versions.iter().cloned());
        }
    }
}

impl Mempool {
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn lane_metrics_with_certificates(
        &self,
        cert_pool: &CertificateMempool,
    ) -> MempoolLaneMetrics {
        let mut metrics = self.lane_metrics();
        metrics.pending_certificates = cert_pool.len();
        metrics
    }

    pub fn insert(&mut self, tx: PooledTx) {
        self.pending.push(tx);
    }

    pub fn lane_metrics(&self) -> MempoolLaneMetrics {
        let mut metrics = MempoolLaneMetrics {
            pending_total: self.pending.len(),
            ..MempoolLaneMetrics::default()
        };
        for p in &self.pending {
            match p.tx.execution_scope() {
                TxExecutionScope::Owned { .. } => metrics.pending_owned += 1,
                TxExecutionScope::Mixed { .. } => metrics.pending_mixed += 1,
                TxExecutionScope::Consensus => metrics.pending_consensus += 1,
            }
        }
        metrics.pending_consensus_lane = metrics.pending_mixed + metrics.pending_consensus;
        metrics
    }

    pub fn drain_ready_gas_budget(&mut self, max_gas: u64, base_fee: u128) -> Vec<PooledTx> {
        self.drain_ready_gas_budget_filtered(max_gas, base_fee, |_| true)
    }

    pub fn drain_ready_gas_budget_filtered(
        &mut self,
        max_gas: u64,
        base_fee: u128,
        mut include: impl FnMut(&Transaction) -> bool,
    ) -> Vec<PooledTx> {
        self.pending.sort_by(|a, b| {
            b.tx.is_owned_object_tx()
                .cmp(&a.tx.is_owned_object_tx())
                .then_with(|| b.max_priority_fee_per_gas.cmp(&a.max_priority_fee_per_gas))
        });
        let mut taken = Vec::new();
        let mut rest = Vec::new();
        let mut owned_objects = BTreeSet::<OwnedObjectId>::new();
        let mut used: u64 = 0;
        for p in self.pending.drain(..) {
            if !include(&p.tx) {
                rest.push(p);
                continue;
            }
            if p.max_fee_per_gas < base_fee {
                rest.push(p);
                continue;
            }
            let Ok(g) = fractal_core::tx_gas_limit(&p.tx) else {
                rest.push(p);
                continue;
            };
            if used.saturating_add(g) <= max_gas {
                if let TxExecutionScope::Owned { objects, .. } = p.tx.execution_scope() {
                    if objects.iter().any(|o| owned_objects.contains(o)) {
                        rest.push(p);
                        continue;
                    }
                    owned_objects.extend(objects);
                }
                used = used.saturating_add(g);
                taken.push(p);
            } else {
                rest.push(p);
            }
        }
        self.pending = rest;
        taken
    }

    /// Drain up to `max_txs` transactions that satisfy `max_fee_per_gas >= base_fee`,
    /// highest effective priority first.
    pub fn drain_ready(&mut self, max_txs: usize, base_fee: u128) -> Vec<Transaction> {
        self.pending.sort_by(|a, b| {
            b.tx.is_owned_object_tx()
                .cmp(&a.tx.is_owned_object_tx())
                .then_with(|| b.max_priority_fee_per_gas.cmp(&a.max_priority_fee_per_gas))
        });
        let mut taken = Vec::new();
        let mut rest = Vec::new();
        let mut owned_objects = BTreeSet::<OwnedObjectId>::new();
        for p in self.pending.drain(..) {
            if taken.len() < max_txs && p.max_fee_per_gas >= base_fee {
                if let TxExecutionScope::Owned { objects, .. } = p.tx.execution_scope() {
                    if objects.iter().any(|o| owned_objects.contains(o)) {
                        rest.push(p);
                        continue;
                    }
                    owned_objects.extend(objects);
                }
                taken.push(p.tx);
            } else {
                rest.push(p);
            }
        }
        self.pending = rest;
        taken
    }
}

/// PRD testnet targets (`docs/prd.md` §9.4): `target_gas_per_block = 30_000_000`, denominator 8.
#[derive(Clone, Debug)]
pub struct BaseFeeParams {
    pub min_base_fee: u128,
    pub target_gas_per_block: u64,
    pub denominator: u64,
}

impl Default for BaseFeeParams {
    fn default() -> Self {
        Self {
            min_base_fee: 1,
            target_gas_per_block: 30_000_000,
            denominator: 8,
        }
    }
}

/// Ethereum-style base fee update (integer math, `u128` fee space).
pub fn next_base_fee(parent_base_fee: u128, parent_gas_used: u64, p: &BaseFeeParams) -> u128 {
    let mut bf = parent_base_fee.max(p.min_base_fee);
    let target = p.target_gas_per_block as u128;
    let used = parent_gas_used as u128;
    if target == 0 {
        return bf;
    }
    if used == target {
        return bf;
    }
    if used > target {
        let gas_delta = used - target;
        let base_fee_per_gas_delta =
            (bf.saturating_mul(gas_delta) / target / u128::from(p.denominator)).max(1);
        bf = bf.saturating_add(base_fee_per_gas_delta);
    } else {
        let gas_delta = target - used;
        let base_fee_per_gas_delta =
            bf.saturating_mul(gas_delta) / target / u128::from(p.denominator);
        bf = bf.saturating_sub(base_fee_per_gas_delta);
    }
    bf.max(p.min_base_fee)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_fee_increases_when_over_target() {
        let p = BaseFeeParams {
            min_base_fee: 100,
            target_gas_per_block: 1_000_000,
            denominator: 8,
        };
        let next = next_base_fee(1000, 2_000_000, &p);
        assert!(next > 1000);
    }

    fn owned_noop(signer: [u8; 20], nonce: u64, tip: u128) -> PooledTx {
        PooledTx {
            tx: Transaction {
                signer,
                nonce,
                vm: fractal_core::VmKind::Native,
                body: fractal_core::TxBody::Native(fractal_core::NativeCall::NoOp),
            },
            max_priority_fee_per_gas: tip,
            max_fee_per_gas: u128::MAX,
            eth_signed_raw: None,
        }
    }

    fn mixed_file_dispute(signer: [u8; 20], nonce: u64, tip: u128) -> PooledTx {
        PooledTx {
            tx: Transaction {
                signer,
                nonce,
                vm: fractal_core::VmKind::Native,
                body: fractal_core::TxBody::Native(fractal_core::NativeCall::FileDispute {
                    receipt_id: [3u8; 32],
                    reason_code: 1,
                    evidence_hash: [4u8; 32],
                }),
            },
            max_priority_fee_per_gas: tip,
            max_fee_per_gas: u128::MAX,
            eth_signed_raw: None,
        }
    }

    fn cert_for_object(
        object_id: OwnedObjectId,
        version: u64,
        tx_hash_byte: u8,
    ) -> PooledCertificate {
        PooledCertificate {
            certificate: OwnedObjectCertificate {
                tx_hash: [tx_hash_byte; 32],
                owner: [7u8; 20],
                signer_nonce: 0,
                object_versions: vec![OwnedObjectVersion { object_id, version }],
                signer_indices: Vec::new(),
                validator_signatures: Vec::new(),
            },
            max_priority_fee_per_gas: u128::from(tx_hash_byte),
        }
    }

    fn validator_keys(
        count: u8,
    ) -> (
        Vec<fractal_crypto::BlsSecretKey>,
        Vec<fractal_crypto::BlsPublicKey>,
    ) {
        let secrets = (1..=count)
            .map(|seed| fractal_crypto::BlsSecretKey::from_ikm(&[seed; 32]).unwrap())
            .collect::<Vec<_>>();
        let pubkeys = secrets
            .iter()
            .map(fractal_crypto::BlsSecretKey::public_key)
            .collect();
        (secrets, pubkeys)
    }

    fn update_agent_tx(signer: [u8; 20], nonce: u64, agent_id: u64, uri: &str) -> Transaction {
        Transaction {
            signer,
            nonce,
            vm: fractal_core::VmKind::Native,
            body: fractal_core::TxBody::Native(fractal_core::NativeCall::UpdateAgent {
                agent_id,
                new_metadata_uri: uri.to_owned(),
                new_pubkey: None,
            }),
        }
    }

    fn valid_certificate_for_tx(
        tx: &Transaction,
        agent_id: u64,
        agent_version: u64,
        validators: &[fractal_crypto::BlsSecretKey],
    ) -> OwnedObjectCertificate {
        let object_versions = vec![
            OwnedObjectVersion {
                object_id: OwnedObjectId::AccountNonce(tx.signer),
                version: tx.nonce,
            },
            OwnedObjectVersion {
                object_id: OwnedObjectId::Agent(agent_id),
                version: agent_version,
            },
        ];
        let unsigned =
            OwnedObjectCertificate::from_owned_transaction(tx, object_versions.clone(), Vec::new())
                .unwrap();
        let body = unsigned.sign_body();
        let signatures = (0..3)
            .map(|idx| {
                OwnedObjectCertificate::countersign(&body, idx as u32, &validators[idx]).unwrap()
            })
            .collect::<Vec<_>>();
        OwnedObjectCertificate::aggregate(tx, object_versions, signatures, 3).unwrap()
    }

    #[test]
    fn drain_keeps_conflicting_owned_object_queued() {
        let signer = [7u8; 20];
        let mut mp = Mempool::default();
        mp.insert(owned_noop(signer, 0, 1));
        mp.insert(owned_noop(signer, 1, 2));

        let drained = mp.drain_ready(10, 1);

        assert_eq!(drained.len(), 1);
        assert_eq!(mp.len(), 1);
    }

    #[test]
    fn owned_transactions_are_prioritized_over_shared_transactions() {
        let signer = [7u8; 20];
        let mut mp = Mempool::default();
        mp.insert(PooledTx {
            tx: Transaction {
                signer,
                nonce: 0,
                vm: fractal_core::VmKind::Evm,
                body: fractal_core::TxBody::Transfer {
                    to: [8u8; 20],
                    amount: 1,
                },
            },
            max_priority_fee_per_gas: 100,
            max_fee_per_gas: u128::MAX,
            eth_signed_raw: None,
        });
        mp.insert(owned_noop([9u8; 20], 0, 1));

        let drained = mp.drain_ready(2, 1);

        assert!(drained[0].is_owned_object_tx());
    }

    #[test]
    fn lane_metrics_count_owned_mixed_and_consensus_pending_transactions() {
        let signer = [7u8; 20];
        let mut mp = Mempool::default();
        mp.insert(owned_noop(signer, 0, 1));
        mp.insert(mixed_file_dispute(signer, 1, 1));
        mp.insert(PooledTx {
            tx: Transaction {
                signer,
                nonce: 2,
                vm: fractal_core::VmKind::Evm,
                body: fractal_core::TxBody::Transfer {
                    to: [8u8; 20],
                    amount: 1,
                },
            },
            max_priority_fee_per_gas: 1,
            max_fee_per_gas: u128::MAX,
            eth_signed_raw: None,
        });

        assert_eq!(
            mp.lane_metrics(),
            MempoolLaneMetrics {
                pending_total: 3,
                pending_owned: 1,
                pending_mixed: 1,
                pending_consensus: 1,
                pending_consensus_lane: 2,
                pending_certificates: 0,
            }
        );
    }

    #[test]
    fn certificate_mempool_rejects_duplicate_object_version_conflicts() {
        let mut cp = CertificateMempool::default();
        cp.insert(cert_for_object(OwnedObjectId::Agent(42), 3, 1))
            .unwrap();

        assert_eq!(
            cp.insert(cert_for_object(OwnedObjectId::Agent(42), 3, 2)),
            Err(CertificateMempoolError::ObjectVersionConflict(
                OwnedObjectVersion {
                    object_id: OwnedObjectId::Agent(42),
                    version: 3,
                }
            ))
        );
        assert_eq!(cp.len(), 1);
    }

    #[test]
    fn certificate_mempool_drains_by_tip_and_releases_object_versions() {
        let mut cp = CertificateMempool::default();
        cp.insert(cert_for_object(OwnedObjectId::Agent(1), 0, 1))
            .unwrap();
        cp.insert(cert_for_object(OwnedObjectId::Agent(2), 0, 9))
            .unwrap();

        let drained = cp.drain_ready(1);

        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].tx_hash, [9u8; 32]);
        assert_eq!(cp.len(), 1);
        cp.insert(cert_for_object(OwnedObjectId::Agent(2), 0, 3))
            .unwrap();
    }

    #[test]
    fn lane_metrics_can_include_certificate_lane() {
        let signer = [7u8; 20];
        let mut mp = Mempool::default();
        let mut cp = CertificateMempool::default();
        mp.insert(owned_noop(signer, 0, 1));
        cp.insert(cert_for_object(OwnedObjectId::Agent(42), 0, 1))
            .unwrap();

        assert_eq!(
            mp.lane_metrics_with_certificates(&cp).pending_certificates,
            1
        );
    }

    #[test]
    fn certificate_pool_accepts_valid_certificates_and_reports_finality() {
        let (secrets, pubkeys) = validator_keys(4);
        let tx = update_agent_tx([7u8; 20], 0, 42, "bench://a");
        let cert = valid_certificate_for_tx(&tx, 42, 3, &secrets);
        let target = cert.object_versions[1].clone();
        let mut pool = CertificatePool::default();

        let hash = pool.insert(cert.clone(), &pubkeys, 3).unwrap();

        let finality = pool.finality_for_object_version(&target).unwrap();
        assert_eq!(finality.certificate_hash, hash);
        assert_eq!(finality.certificate, cert);
        assert_eq!(pool.accepted_certificates(), vec![cert]);
    }

    #[test]
    fn certificate_pool_rejects_conflicts_as_slashable_evidence() {
        let (secrets, pubkeys) = validator_keys(4);
        let signer = [7u8; 20];
        let tx_a = update_agent_tx(signer, 0, 42, "bench://a");
        let tx_b = update_agent_tx(signer, 1, 42, "bench://b");
        let cert_a = valid_certificate_for_tx(&tx_a, 42, 3, &secrets);
        let cert_b = valid_certificate_for_tx(&tx_b, 42, 3, &secrets);
        let mut pool = CertificatePool::default();
        pool.insert(cert_a, &pubkeys, 3).unwrap();

        let err = pool.insert(cert_b, &pubkeys, 3).unwrap_err();

        let CertificatePoolError::ObjectVersionConflict {
            object_version,
            conflict,
        } = err
        else {
            panic!("expected conflict");
        };
        assert_eq!(
            object_version,
            OwnedObjectVersion {
                object_id: OwnedObjectId::Agent(42),
                version: 3,
            }
        );
        assert_eq!(conflict.finding.slashable_validator_indices, vec![0, 1, 2]);
        assert_eq!(pool.conflict_count(), 1);
    }
}
