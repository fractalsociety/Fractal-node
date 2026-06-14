use borsh::{BorshDeserialize, BorshSerialize};

use crate::address::Address;
use crate::native_types::{OnChainTaskReceipt, SettleBatchPayload};
use fractal_crypto::hash::keccak256;
use fractal_crypto::{BlsSignature, Hash256};
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
    /// Anchor `keccak256(borsh(TaskReceipt))` (see `wallet_anchor::task_receipt_commitment`).
    /// Empty `receipt_witness`: stores commitment under signer (dev trust). Non-empty witness:
    /// requires `fractal-core` `--features wallet` and must deserialize to a matching `TaskReceipt`.
    WalletTaskReceiptAnchorV1 {
        commitment: Hash256,
        receipt_witness: Vec<u8>,
    },
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
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum TxExecutionScope {
    /// Must enter the ordered consensus lane because it touches shared state.
    Consensus,
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
    #[error("owned-object certificate borsh encoding failed")]
    Encode,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OwnedObjectVersion {
    pub object_id: OwnedObjectId,
    pub version: u64,
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

impl OwnedObjectCertificate {
    pub fn sign_body(&self) -> OwnedObjectCertificateSignBody {
        OwnedObjectCertificateSignBody {
            tx_hash: self.tx_hash,
            owner: self.owner,
            signer_nonce: self.signer_nonce,
            object_versions: self.object_versions.clone(),
        }
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

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub signer: Address,
    pub nonce: u64,
    pub vm: VmKind,
    pub body: TxBody,
}

impl Transaction {
    pub fn execution_scope(&self) -> TxExecutionScope {
        let owned = |mut objects: Vec<OwnedObjectId>| {
            objects.insert(0, OwnedObjectId::AccountNonce(self.signer));
            objects.sort();
            objects.dedup();
            TxExecutionScope::Owned {
                owner: self.signer,
                objects,
            }
        };

        match (&self.vm, &self.body) {
            (VmKind::Native, TxBody::Native(NativeCall::UpdateAgent { agent_id, .. })) => {
                owned(vec![OwnedObjectId::Agent(*agent_id)])
            }
            (VmKind::Native, TxBody::Native(NativeCall::SettleReceipt(receipt))) => {
                owned(vec![OwnedObjectId::Receipt(receipt.receipt_id)])
            }
            (
                VmKind::Native,
                TxBody::Native(NativeCall::WalletTaskReceiptAnchorV1 { commitment, .. }),
            ) => owned(vec![OwnedObjectId::WalletTaskReceipt(*commitment)]),
            (VmKind::Native, TxBody::Native(NativeCall::NoOp)) => owned(Vec::new()),
            _ => TxExecutionScope::Consensus,
        }
    }

    pub fn is_owned_object_tx(&self) -> bool {
        matches!(self.execution_scope(), TxExecutionScope::Owned { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer() -> Address {
        [7u8; 20]
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

        let bytes = borsh::to_vec(&cert).unwrap();
        let round_trip = OwnedObjectCertificate::try_from_slice(&bytes).unwrap();
        assert_eq!(round_trip, cert);
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
}
