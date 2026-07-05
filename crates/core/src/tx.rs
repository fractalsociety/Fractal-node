use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeSet;

use crate::address::Address;
use crate::chain_economics::ChainEconomicsParams;
use crate::native_types::{OnChainTaskReceipt, SettleBatchPayload};
use fractal_crypto::hash::keccak256;
use fractal_crypto::{BlsPublicKey, BlsSecretKey, BlsSignature, Hash256};
use thiserror::Error;

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum VmKind {
    Native,
    Evm,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum NativeCall {
    RegisterAgent {
        operator: Address,
        pubkey: [u8; 32],
        kind: u8,
        metadata_uri: String,
    },
    UpdateAgent {
        agent_id: u64,
        new_metadata_uri: String,
        new_pubkey: Option<[u8; 32]>,
    },
    SuspendAgent {
        agent_id: u64,
        reason: String,
    },
    SettleReceipt(OnChainTaskReceipt),
    SettleBatch(SettleBatchPayload),
    ClaimPayout {
        batch_id: fractal_crypto::Hash256,
        account: Address,
        amount: u128,
        leaf_index: u32,
        proof: Vec<fractal_crypto::Hash256>,
    },
    FileDispute {
        receipt_id: fractal_crypto::Hash256,
        reason_code: u32,
        evidence_hash: fractal_crypto::Hash256,
    },
    ResolveDispute {
        dispute_id: u64,
        resolution: u8,
        payouts_diff: i128,
    },
    Stake {
        amount: u128,
    },
    Unstake {
        amount: u128,
    },
    Slash {
        validator_id: Address,
        evidence_hash: fractal_crypto::Hash256,
    },
    Delegate {
        validator: Address,
        amount: u128,
    },
    WithdrawRewards {
        validator: Address,
    },
    NoOp,
    SetChainEconomics {
        params: ChainEconomicsParams,
    },
    /// Anchor `keccak256(borsh(TaskReceipt))` (see `wallet_anchor::task_receipt_commitment`).
    /// Empty `receipt_witness`: stores commitment under signer (dev trust). Non-empty witness:
    /// requires `fractal-core` `--features wallet` and must deserialize to a matching `TaskReceipt`.
    WalletTaskReceiptAnchorV1 {
        commitment: Hash256,
        receipt_witness: Vec<u8>,
    },
    /// Anchor a Fractal Society research proof/package hash as a first-class
    /// native transaction so explorers and indexers can audit it from blocks.
    ProofCommitmentV1 {
        proof_hash: Hash256,
    },
    /// RealLifeAI / Agent Life command. The chain stores a deterministic,
    /// hash-bound command envelope; indexers and Fractalwork retain the rich
    /// payload body off-chain.
    LifeCommandV1(LifeCommandV1),
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum LifeCommandKind {
    BirthGrant,
    BirthSpawn,
    BirthPlayerFunded,
    RentCharge,
    LoanOpen,
    LoanAccept,
    LoanRepay,
    ExtensionPurchase,
    WillRegister,
    WillUpdate,
    OwnerTopUp,
    WithdrawalRequest,
    WithdrawalSettlement,
    SiiCommit,
    LadderCommit,
    BenchmarkFreeze,
    IntelligencePayout,
    ProvenanceBond,
    FeedbackArtifact,
    SealedSale,
    ReaperEpoch,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct LifeCommandV1 {
    pub command_id: Hash256,
    pub kind: LifeCommandKind,
    pub soul_id_hash: Hash256,
    pub counterparty_hash: Option<Hash256>,
    pub epoch: u64,
    pub amount_micro_credits: u128,
    pub payload_hash: Hash256,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum TxBody {
    Transfer {
        to: Address,
        amount: u128,
    },
    Native(NativeCall),
    /// Minimal EVM call (M4): execute EVM bytecode and/or precompiles.
    /// `gas_limit` is an execution cap; actual gas accounting is handled separately.
    EvmCall {
        to: Address,
        value: u128,
        calldata: Vec<u8>,
        gas_limit: u64,
    },
    /// Minimal EVM CREATE (M4): store deployed code deterministically.
    /// `init_code` is treated as "runtime code" for devnet until full EVM init execution lands.
    EvmCreate {
        value: u128,
        init_code: Vec<u8>,
        gas_limit: u64,
    },
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OwnedObjectId {
    AccountNonce(Address),
    Agent(u64),
    Receipt(Hash256),
    WalletTaskReceipt(Hash256),
    ProofCommitment(Hash256),
    LifeCommand(Hash256),
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum TxExecutionScope {
    /// Must enter the ordered consensus lane because it touches shared state.
    Consensus,
    /// Must enter the ordered consensus lane because it touches both owned and shared state.
    Mixed {
        owner: Address,
        owned_objects: Vec<OwnedObjectId>,
    },
    /// Can use the certified owned-object lane once validators countersign it.
    Owned {
        owner: Address,
        objects: Vec<OwnedObjectId>,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OwnedObjectCertificateError {
    #[error("transaction is not eligible for owned-object certificate path")]
    NotOwnedObject,
    #[error("object version set does not match transaction owned-object set")]
    ObjectVersionSet,
    #[error("validator signer set does not match validator signatures")]
    SignerSet,
    #[error("validator index {0} is out of range")]
    ValidatorIndex(u32),
    #[error("owned-object certificate has too few signatures: got {got}, need {need}")]
    InsufficientSignatures { got: usize, need: usize },
    #[error("owned-object certificate validator signature failed")]
    BadSignature,
    #[error("owned-object certificate borsh encoding failed")]
    Encode,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OwnedObjectCertificateEvidenceError {
    #[error("first owned-object certificate is invalid: {0}")]
    CertificateA(OwnedObjectCertificateError),
    #[error("second owned-object certificate is invalid: {0}")]
    CertificateB(OwnedObjectCertificateError),
    #[error("owned-object certificate evidence repeats the same certificate")]
    SameCertificate,
    #[error("owned-object certificate evidence does not contain a shared object/version conflict")]
    NoObjectVersionConflict,
    #[error("owned-object certificate evidence does not identify a validator that signed both conflicts")]
    NoSlashableSigner,
    #[error("owned-object certificate evidence borsh encoding failed")]
    Encode,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OwnedObjectPrecheckError {
    #[error("transaction is not eligible for owned-object certificate path")]
    NotOwnedObject,
    #[error("owned transaction owner does not match signer")]
    Owner,
    #[error("unknown signer account")]
    UnknownSigner,
    #[error("bad nonce: expected {expected}, got {actual}")]
    BadNonce { expected: u64, actual: u64 },
    #[error("object version set does not match transaction owned-object set")]
    ObjectVersionSet,
    #[error("object version mismatch for {object_id:?}: expected {expected}, got {actual}")]
    ObjectVersion {
        object_id: OwnedObjectId,
        expected: u64,
        actual: u64,
    },
    #[error("transaction gas {tx_gas} exceeds limit {gas_limit}")]
    GasLimit { tx_gas: u64, gas_limit: u64 },
    #[error("max fee per gas {max_fee_per_gas} is below base fee {base_fee_per_gas}")]
    FeeBelowBase {
        max_fee_per_gas: u128,
        base_fee_per_gas: u128,
    },
    #[error("signer balance {balance} cannot cover max fee {required}")]
    InsufficientFeeBalance { balance: u128, required: u128 },
    #[error("gas fee arithmetic overflow")]
    FeeOverflow,
    #[error("owned-object precheck borsh encoding failed")]
    Encode,
    #[error("invalid transaction shape")]
    InvalidShape,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OwnedObjectVersion {
    pub object_id: OwnedObjectId,
    pub version: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct OwnedObjectPrecheck {
    pub tx_hash: Hash256,
    pub owner: Address,
    pub signer_nonce: u64,
    pub object_versions: Vec<OwnedObjectVersion>,
    pub tx_gas: u64,
    pub max_fee_per_gas: u128,
    pub base_fee_per_gas: u128,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct OwnedObjectValidatorSignature {
    pub validator_index: u32,
    pub signature: BlsSignature,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct OwnedObjectCertificateSignBody {
    pub tx_hash: Hash256,
    pub owner: Address,
    pub signer_nonce: u64,
    pub object_versions: Vec<OwnedObjectVersion>,
}

impl OwnedObjectCertificateSignBody {
    pub fn sign_bytes(&self) -> Result<Vec<u8>, OwnedObjectCertificateError> {
        borsh::to_vec(self).map_err(|_| OwnedObjectCertificateError::Encode)
    }
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct OwnedObjectCertificate {
    pub tx_hash: Hash256,
    pub owner: Address,
    pub signer_nonce: u64,
    pub object_versions: Vec<OwnedObjectVersion>,
    /// Validator indexes are explicit so the certificate remains compact without
    /// committing this wire type to one validator-set bitmap width.
    pub signer_indices: Vec<u32>,
    pub validator_signatures: Vec<OwnedObjectValidatorSignature>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct OwnedObjectConflictingCertificateEvidence {
    pub certificate_a: OwnedObjectCertificate,
    pub certificate_b: OwnedObjectCertificate,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct OwnedObjectConflictingCertificateFinding {
    pub evidence_hash: Hash256,
    pub conflicting_object_versions: Vec<OwnedObjectVersion>,
    pub slashable_validator_indices: Vec<u32>,
}

impl OwnedObjectCertificate {
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, OwnedObjectCertificateError> {
        borsh::to_vec(self).map_err(|_| OwnedObjectCertificateError::Encode)
    }

    pub fn certificate_hash(&self) -> Result<Hash256, OwnedObjectCertificateError> {
        Ok(keccak256(&self.canonical_bytes()?))
    }

    pub fn sign_body(&self) -> OwnedObjectCertificateSignBody {
        OwnedObjectCertificateSignBody {
            tx_hash: self.tx_hash,
            owner: self.owner,
            signer_nonce: self.signer_nonce,
            object_versions: self.object_versions.clone(),
        }
    }

    pub fn countersign(
        sign_body: &OwnedObjectCertificateSignBody,
        validator_index: u32,
        validator_secret: &BlsSecretKey,
    ) -> Result<OwnedObjectValidatorSignature, OwnedObjectCertificateError> {
        let bytes = sign_body.sign_bytes()?;
        Ok(OwnedObjectValidatorSignature {
            validator_index,
            signature: validator_secret.sign(&bytes),
        })
    }

    pub fn aggregate(
        tx: &Transaction,
        object_versions: Vec<OwnedObjectVersion>,
        validator_signatures: Vec<OwnedObjectValidatorSignature>,
        quorum_threshold: usize,
    ) -> Result<Self, OwnedObjectCertificateError> {
        let cert = Self::from_owned_transaction(tx, object_versions, validator_signatures)?;
        if cert.validator_signatures.len() < quorum_threshold {
            return Err(OwnedObjectCertificateError::InsufficientSignatures {
                got: cert.validator_signatures.len(),
                need: quorum_threshold,
            });
        }
        Ok(cert)
    }

    pub fn verify(
        &self,
        validator_pubkeys: &[BlsPublicKey],
        quorum_threshold: usize,
    ) -> Result<(), OwnedObjectCertificateError> {
        if self.validator_signatures.len() < quorum_threshold {
            return Err(OwnedObjectCertificateError::InsufficientSignatures {
                got: self.validator_signatures.len(),
                need: quorum_threshold,
            });
        }
        let signer_indices = self
            .validator_signatures
            .iter()
            .map(|s| s.validator_index)
            .collect::<Vec<_>>();
        if signer_indices != self.signer_indices {
            return Err(OwnedObjectCertificateError::SignerSet);
        }
        if signer_indices.windows(2).any(|w| w[0] >= w[1]) {
            return Err(OwnedObjectCertificateError::SignerSet);
        }

        let bytes = self.sign_body().sign_bytes()?;
        for sig in &self.validator_signatures {
            let pk = validator_pubkeys.get(sig.validator_index as usize).ok_or(
                OwnedObjectCertificateError::ValidatorIndex(sig.validator_index),
            )?;
            sig.signature
                .verify(&bytes, pk)
                .map_err(|_| OwnedObjectCertificateError::BadSignature)?;
        }
        Ok(())
    }

    pub fn from_owned_transaction(
        tx: &Transaction,
        mut object_versions: Vec<OwnedObjectVersion>,
        mut validator_signatures: Vec<OwnedObjectValidatorSignature>,
    ) -> Result<Self, OwnedObjectCertificateError> {
        let TxExecutionScope::Owned { owner, objects } = tx.execution_scope() else {
            return Err(OwnedObjectCertificateError::NotOwnedObject);
        };

        object_versions.sort();
        object_versions.dedup();
        let versioned_objects = object_versions
            .iter()
            .map(|v| v.object_id.clone())
            .collect::<Vec<_>>();
        if versioned_objects != objects {
            return Err(OwnedObjectCertificateError::ObjectVersionSet);
        }

        validator_signatures.sort_by_key(|s| s.validator_index);
        validator_signatures.dedup_by_key(|s| s.validator_index);
        let signer_indices = validator_signatures
            .iter()
            .map(|s| s.validator_index)
            .collect();
        let tx_hash =
            keccak256(&borsh::to_vec(tx).map_err(|_| OwnedObjectCertificateError::Encode)?);
        Ok(Self {
            tx_hash,
            owner,
            signer_nonce: tx.nonce,
            object_versions,
            signer_indices,
            validator_signatures,
        })
    }
}

impl OwnedObjectConflictingCertificateEvidence {
    pub fn evidence_hash(&self) -> Result<Hash256, OwnedObjectCertificateEvidenceError> {
        borsh::to_vec(self)
            .map(|bytes| keccak256(&bytes))
            .map_err(|_| OwnedObjectCertificateEvidenceError::Encode)
    }

    pub fn verify(
        &self,
        validator_pubkeys: &[BlsPublicKey],
        quorum_threshold: usize,
    ) -> Result<OwnedObjectConflictingCertificateFinding, OwnedObjectCertificateEvidenceError> {
        self.certificate_a
            .verify(validator_pubkeys, quorum_threshold)
            .map_err(OwnedObjectCertificateEvidenceError::CertificateA)?;
        self.certificate_b
            .verify(validator_pubkeys, quorum_threshold)
            .map_err(OwnedObjectCertificateEvidenceError::CertificateB)?;

        if self
            .certificate_a
            .certificate_hash()
            .map_err(OwnedObjectCertificateEvidenceError::CertificateA)?
            == self
                .certificate_b
                .certificate_hash()
                .map_err(OwnedObjectCertificateEvidenceError::CertificateB)?
        {
            return Err(OwnedObjectCertificateEvidenceError::SameCertificate);
        }
        if self.certificate_a.sign_body() == self.certificate_b.sign_body() {
            return Err(OwnedObjectCertificateEvidenceError::SameCertificate);
        }

        let a_versions = self
            .certificate_a
            .object_versions
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let b_versions = self
            .certificate_b
            .object_versions
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let conflicting_object_versions = a_versions
            .intersection(&b_versions)
            .cloned()
            .collect::<Vec<_>>();
        if conflicting_object_versions.is_empty() {
            return Err(OwnedObjectCertificateEvidenceError::NoObjectVersionConflict);
        }

        let a_signers = self
            .certificate_a
            .signer_indices
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let b_signers = self
            .certificate_b
            .signer_indices
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let slashable_validator_indices = a_signers
            .intersection(&b_signers)
            .copied()
            .collect::<Vec<_>>();
        if slashable_validator_indices.is_empty() {
            return Err(OwnedObjectCertificateEvidenceError::NoSlashableSigner);
        }

        Ok(OwnedObjectConflictingCertificateFinding {
            evidence_hash: self.evidence_hash()?,
            conflicting_object_versions,
            slashable_validator_indices,
        })
    }
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub signer: Address,
    pub nonce: u64,
    pub vm: VmKind,
    pub body: TxBody,
}

impl Transaction {
    pub fn execution_scope(&self) -> TxExecutionScope {
        let normalize = |mut objects: Vec<OwnedObjectId>| {
            objects.insert(0, OwnedObjectId::AccountNonce(self.signer));
            objects.sort();
            objects.dedup();
            objects
        };
        let owned = |objects: Vec<OwnedObjectId>| TxExecutionScope::Owned {
            owner: self.signer,
            objects: normalize(objects),
        };
        let mixed = |objects: Vec<OwnedObjectId>| TxExecutionScope::Mixed {
            owner: self.signer,
            owned_objects: normalize(objects),
        };

        match (&self.vm, &self.body) {
            (VmKind::Native, TxBody::Native(NativeCall::SuspendAgent { agent_id, .. })) => {
                mixed(vec![OwnedObjectId::Agent(*agent_id)])
            }
            (VmKind::Native, TxBody::Native(NativeCall::UpdateAgent { agent_id, .. })) => {
                owned(vec![OwnedObjectId::Agent(*agent_id)])
            }
            (VmKind::Native, TxBody::Native(NativeCall::SettleReceipt(receipt))) => {
                owned(vec![OwnedObjectId::Receipt(receipt.receipt_id)])
            }
            (VmKind::Native, TxBody::Native(NativeCall::SettleBatch(payload))) => mixed(
                payload
                    .receipts
                    .iter()
                    .map(|receipt| OwnedObjectId::Receipt(receipt.receipt_id))
                    .collect(),
            ),
            (VmKind::Native, TxBody::Native(NativeCall::FileDispute { receipt_id, .. })) => {
                mixed(vec![OwnedObjectId::Receipt(*receipt_id)])
            }
            (
                VmKind::Native,
                TxBody::Native(NativeCall::WalletTaskReceiptAnchorV1 { commitment, .. }),
            ) => owned(vec![OwnedObjectId::WalletTaskReceipt(*commitment)]),
            (VmKind::Native, TxBody::Native(NativeCall::ProofCommitmentV1 { proof_hash })) => {
                owned(vec![OwnedObjectId::ProofCommitment(*proof_hash)])
            }
            (VmKind::Native, TxBody::Native(NativeCall::LifeCommandV1(command))) => {
                mixed(vec![OwnedObjectId::LifeCommand(command.command_id)])
            }
            (VmKind::Native, TxBody::Native(NativeCall::NoOp)) => owned(Vec::new()),
            _ => TxExecutionScope::Consensus,
        }
    }

    pub fn is_owned_object_tx(&self) -> bool {
        matches!(self.execution_scope(), TxExecutionScope::Owned { .. })
    }

    pub fn is_mixed_object_tx(&self) -> bool {
        matches!(self.execution_scope(), TxExecutionScope::Mixed { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer() -> Address {
        [7u8; 20]
    }

    fn receipt(receipt_id: Hash256) -> OnChainTaskReceipt {
        OnChainTaskReceipt {
            receipt_id,
            job_id: [1u8; 32],
            requester: signer(),
            worker: 1,
            verifier: 2,
            artifact_root: [3u8; 32],
            output_hash: [4u8; 32],
            score: 100,
            payout_amount: 10,
            verifier_fee: 1,
            protocol_fee: 1,
            final_status: 1,
            finalized_at: 123,
            schema_version: 1,
        }
    }

    fn update_agent_tx(nonce: u64, agent_id: u64, metadata_uri: &str) -> Transaction {
        Transaction {
            signer: signer(),
            nonce,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::UpdateAgent {
                agent_id,
                new_metadata_uri: metadata_uri.into(),
                new_pubkey: None,
            }),
        }
    }

    fn agent_versions(nonce: u64, agent_id: u64, agent_version: u64) -> Vec<OwnedObjectVersion> {
        vec![
            OwnedObjectVersion {
                object_id: OwnedObjectId::AccountNonce(signer()),
                version: nonce,
            },
            OwnedObjectVersion {
                object_id: OwnedObjectId::Agent(agent_id),
                version: agent_version,
            },
        ]
    }

    fn validator_keys(count: u8) -> (Vec<BlsSecretKey>, Vec<BlsPublicKey>) {
        let secrets = (1..=count)
            .map(|seed| BlsSecretKey::from_ikm(&[seed; 32]).unwrap())
            .collect::<Vec<_>>();
        let pubkeys = secrets.iter().map(BlsSecretKey::public_key).collect();
        (secrets, pubkeys)
    }

    fn aggregate_with_signers(
        tx: &Transaction,
        object_versions: Vec<OwnedObjectVersion>,
        validators: &[BlsSecretKey],
        signer_indices: &[usize],
        quorum_threshold: usize,
    ) -> OwnedObjectCertificate {
        let unsigned =
            OwnedObjectCertificate::from_owned_transaction(tx, object_versions.clone(), Vec::new())
                .unwrap();
        let body = unsigned.sign_body();
        let signatures = signer_indices
            .iter()
            .map(|idx| {
                OwnedObjectCertificate::countersign(&body, *idx as u32, &validators[*idx]).unwrap()
            })
            .collect::<Vec<_>>();
        OwnedObjectCertificate::aggregate(tx, object_versions, signatures, quorum_threshold)
            .unwrap()
    }

    #[test]
    fn update_agent_is_owned_by_signer_and_agent_object() {
        let tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::UpdateAgent {
                agent_id: 42,
                new_metadata_uri: "ipfs://new".into(),
                new_pubkey: None,
            }),
        };

        assert_eq!(
            tx.execution_scope(),
            TxExecutionScope::Owned {
                owner: signer(),
                objects: vec![
                    OwnedObjectId::AccountNonce(signer()),
                    OwnedObjectId::Agent(42)
                ],
            }
        );
    }

    #[test]
    fn wallet_anchor_is_owned_by_signer_and_commitment() {
        let commitment = [3u8; 32];
        let tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::WalletTaskReceiptAnchorV1 {
                commitment,
                receipt_witness: Vec::new(),
            }),
        };

        assert_eq!(
            tx.execution_scope(),
            TxExecutionScope::Owned {
                owner: signer(),
                objects: vec![
                    OwnedObjectId::AccountNonce(signer()),
                    OwnedObjectId::WalletTaskReceipt(commitment)
                ],
            }
        );
    }

    #[test]
    fn transfers_stay_on_consensus_path() {
        let tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::Transfer {
                to: [8u8; 20],
                amount: 1,
            },
        };

        assert_eq!(tx.execution_scope(), TxExecutionScope::Consensus);
    }

    #[test]
    fn settle_batch_is_explicitly_mixed() {
        let receipt_id = [9u8; 32];
        let tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::SettleBatch(SettleBatchPayload {
                batch_id: [8u8; 32],
                operator: signer(),
                receipts: vec![receipt(receipt_id)],
                payout_entries: Vec::new(),
                submitted_at: 123,
                operator_sig: [0u8; 64],
            })),
        };

        assert_eq!(
            tx.execution_scope(),
            TxExecutionScope::Mixed {
                owner: signer(),
                owned_objects: vec![
                    OwnedObjectId::AccountNonce(signer()),
                    OwnedObjectId::Receipt(receipt_id)
                ],
            }
        );
        assert!(tx.is_mixed_object_tx());
        assert!(!tx.is_owned_object_tx());
    }

    #[test]
    fn file_dispute_is_explicitly_mixed() {
        let receipt_id = [4u8; 32];
        let tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::FileDispute {
                receipt_id,
                reason_code: 7,
                evidence_hash: [6u8; 32],
            }),
        };

        assert_eq!(
            tx.execution_scope(),
            TxExecutionScope::Mixed {
                owner: signer(),
                owned_objects: vec![
                    OwnedObjectId::AccountNonce(signer()),
                    OwnedObjectId::Receipt(receipt_id)
                ],
            }
        );
    }

    #[test]
    fn owned_object_certificate_wire_type_binds_owned_scope() {
        let tx = Transaction {
            signer: signer(),
            nonce: 7,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::UpdateAgent {
                agent_id: 42,
                new_metadata_uri: "ipfs://new".into(),
                new_pubkey: None,
            }),
        };
        let object_versions = vec![
            OwnedObjectVersion {
                object_id: OwnedObjectId::Agent(42),
                version: 3,
            },
            OwnedObjectVersion {
                object_id: OwnedObjectId::AccountNonce(signer()),
                version: 7,
            },
        ];
        let sig = BlsSignature([5u8; 96]);
        let cert = OwnedObjectCertificate::from_owned_transaction(
            &tx,
            object_versions,
            vec![
                OwnedObjectValidatorSignature {
                    validator_index: 2,
                    signature: sig,
                },
                OwnedObjectValidatorSignature {
                    validator_index: 2,
                    signature: sig,
                },
            ],
        )
        .expect("certificate");

        assert_eq!(cert.owner, signer());
        assert_eq!(cert.signer_nonce, 7);
        assert_eq!(cert.tx_hash, keccak256(&borsh::to_vec(&tx).unwrap()));
        assert_eq!(
            cert.object_versions,
            vec![
                OwnedObjectVersion {
                    object_id: OwnedObjectId::AccountNonce(signer()),
                    version: 7,
                },
                OwnedObjectVersion {
                    object_id: OwnedObjectId::Agent(42),
                    version: 3,
                },
            ]
        );
        assert_eq!(cert.signer_indices, vec![2]);
        assert_eq!(cert.validator_signatures.len(), 1);
        assert!(!cert.sign_body().sign_bytes().unwrap().is_empty());
        assert_eq!(
            cert.certificate_hash().unwrap(),
            keccak256(&cert.canonical_bytes().unwrap())
        );

        let bytes = borsh::to_vec(&cert).unwrap();
        let round_trip = OwnedObjectCertificate::try_from_slice(&bytes).unwrap();
        assert_eq!(round_trip, cert);
        assert_eq!(round_trip.canonical_bytes().unwrap(), bytes);
    }

    #[test]
    fn owned_object_certificate_countersign_aggregate_and_verify() {
        let tx = update_agent_tx(7, 42, "ipfs://new");
        let object_versions = agent_versions(7, 42, 3);
        let (validators, pubkeys) = validator_keys(5);
        let cert = aggregate_with_signers(&tx, object_versions, &validators, &[0, 1, 2, 3, 4], 3);

        assert_eq!(cert.signer_indices, vec![0, 1, 2, 3, 4]);
        cert.verify(&pubkeys, 3).expect("certificate verifies");
    }

    #[test]
    fn valid_owned_object_certificate_creation_and_verification() {
        let tx = update_agent_tx(9, 77, "ipfs://valid");
        let object_versions = agent_versions(9, 77, 4);
        let (validators, pubkeys) = validator_keys(4);

        let cert = aggregate_with_signers(&tx, object_versions.clone(), &validators, &[0, 1, 2], 3);

        assert_eq!(cert.owner, signer());
        assert_eq!(cert.signer_nonce, 9);
        assert_eq!(cert.object_versions, object_versions);
        assert_eq!(cert.signer_indices, vec![0, 1, 2]);
        cert.verify(&pubkeys, 3).unwrap();
    }

    #[test]
    fn owned_object_certificate_aggregate_requires_quorum() {
        let tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let body = OwnedObjectCertificateSignBody {
            tx_hash: keccak256(&borsh::to_vec(&tx).unwrap()),
            owner: signer(),
            signer_nonce: 0,
            object_versions: vec![OwnedObjectVersion {
                object_id: OwnedObjectId::AccountNonce(signer()),
                version: 0,
            }],
        };
        let sk = BlsSecretKey::from_ikm(&[9u8; 32]).unwrap();
        let sig = OwnedObjectCertificate::countersign(&body, 0, &sk).unwrap();

        assert_eq!(
            OwnedObjectCertificate::aggregate(&tx, body.object_versions, vec![sig], 2),
            Err(OwnedObjectCertificateError::InsufficientSignatures { got: 1, need: 2 })
        );
    }

    #[test]
    fn owned_object_certificate_verify_rejects_tampered_signer_set() {
        let tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let object_versions = vec![OwnedObjectVersion {
            object_id: OwnedObjectId::AccountNonce(signer()),
            version: 0,
        }];
        let unsigned = OwnedObjectCertificate::from_owned_transaction(
            &tx,
            object_versions.clone(),
            Vec::new(),
        )
        .unwrap();
        let sk = BlsSecretKey::from_ikm(&[8u8; 32]).unwrap();
        let sig = OwnedObjectCertificate::countersign(&unsigned.sign_body(), 0, &sk).unwrap();
        let mut cert =
            OwnedObjectCertificate::aggregate(&tx, object_versions, vec![sig], 1).unwrap();
        cert.signer_indices = vec![1];

        assert_eq!(
            cert.verify(&[sk.public_key()], 1),
            Err(OwnedObjectCertificateError::SignerSet)
        );
    }

    #[test]
    fn owned_object_certificate_rejects_consensus_transaction() {
        let tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::Transfer {
                to: [8u8; 20],
                amount: 1,
            },
        };

        assert_eq!(
            OwnedObjectCertificate::from_owned_transaction(&tx, Vec::new(), Vec::new()),
            Err(OwnedObjectCertificateError::NotOwnedObject)
        );
    }

    #[test]
    fn owned_object_certificate_rejects_mixed_transaction() {
        let tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::FileDispute {
                receipt_id: [5u8; 32],
                reason_code: 1,
                evidence_hash: [6u8; 32],
            }),
        };

        assert_eq!(
            OwnedObjectCertificate::from_owned_transaction(&tx, Vec::new(), Vec::new()),
            Err(OwnedObjectCertificateError::NotOwnedObject)
        );
    }

    #[test]
    fn certificate_path_rejects_mixed_and_shared_transactions() {
        let shared_tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::Transfer {
                to: [8u8; 20],
                amount: 1,
            },
        };
        let mixed_tx = Transaction {
            signer: signer(),
            nonce: 1,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::SettleBatch(SettleBatchPayload {
                batch_id: [8u8; 32],
                operator: signer(),
                receipts: vec![receipt([9u8; 32])],
                payout_entries: Vec::new(),
                submitted_at: 123,
                operator_sig: [0u8; 64],
            })),
        };

        for tx in [&shared_tx, &mixed_tx] {
            assert_eq!(
                OwnedObjectCertificate::aggregate(tx, Vec::new(), Vec::new(), 0),
                Err(OwnedObjectCertificateError::NotOwnedObject)
            );
        }
    }

    #[test]
    fn owned_object_certificate_rejects_wrong_object_version_set() {
        let tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };

        assert_eq!(
            OwnedObjectCertificate::from_owned_transaction(
                &tx,
                vec![OwnedObjectVersion {
                    object_id: OwnedObjectId::Agent(42),
                    version: 0,
                }],
                Vec::new(),
            ),
            Err(OwnedObjectCertificateError::ObjectVersionSet)
        );
    }

    #[test]
    fn conflicting_certificate_evidence_verifies_and_identifies_offender() {
        let tx_a = update_agent_tx(7, 42, "ipfs://a");
        let tx_b = update_agent_tx(7, 42, "ipfs://b");
        let object_versions = agent_versions(7, 42, 3);
        let (validators, pubkeys) = validator_keys(5);
        let cert_a =
            aggregate_with_signers(&tx_a, object_versions.clone(), &validators, &[0, 1, 2], 3);
        let cert_b = aggregate_with_signers(&tx_b, object_versions, &validators, &[2, 3, 4], 3);
        let evidence = OwnedObjectConflictingCertificateEvidence {
            certificate_a: cert_a,
            certificate_b: cert_b,
        };

        let finding = evidence.verify(&pubkeys, 3).unwrap();

        assert_eq!(finding.slashable_validator_indices, vec![2]);
        assert!(finding
            .conflicting_object_versions
            .contains(&OwnedObjectVersion {
                object_id: OwnedObjectId::AccountNonce(signer()),
                version: 7,
            }));
        assert!(finding
            .conflicting_object_versions
            .contains(&OwnedObjectVersion {
                object_id: OwnedObjectId::Agent(42),
                version: 3,
            }));
        assert_eq!(finding.evidence_hash, evidence.evidence_hash().unwrap());
    }

    #[test]
    fn conflicting_certificate_evidence_rejects_non_conflicting_versions() {
        let tx_a = update_agent_tx(7, 42, "ipfs://a");
        let tx_b = update_agent_tx(8, 43, "ipfs://b");
        let (validators, pubkeys) = validator_keys(5);
        let cert_a =
            aggregate_with_signers(&tx_a, agent_versions(7, 42, 3), &validators, &[0, 1, 2], 3);
        let cert_b =
            aggregate_with_signers(&tx_b, agent_versions(8, 43, 3), &validators, &[2, 3, 4], 3);
        let evidence = OwnedObjectConflictingCertificateEvidence {
            certificate_a: cert_a,
            certificate_b: cert_b,
        };

        assert_eq!(
            evidence.verify(&pubkeys, 3),
            Err(OwnedObjectCertificateEvidenceError::NoObjectVersionConflict)
        );
    }
}
