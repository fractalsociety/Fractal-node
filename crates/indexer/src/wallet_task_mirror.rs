//! §14.5 wallet task lifecycle mirror for reputation derivation (`docs/wallet.md` §10.4).

use std::collections::BTreeMap;

use fractal_core::{
    Address, NativeCall, ProviderRegistration, ProviderSlashRecord, WALLET_TASK_CHECKED_OUT,
    WALLET_TASK_FINALIZED, WALLET_TASK_POSTED, WALLET_TASK_SUBMITTED, WALLET_TASK_VERIFIED,
};
use serde::{Deserialize, Serialize};

use crate::indexer_mirror::addr_hex;

fn hash32_hex(h: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(h))
}

fn provider_class_key(provider_id_hex: &str, tool_class: u8) -> String {
    format!("{provider_id_hex}:{tool_class}")
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalletToolRootMetaWire {
    pub provider_id_hex: String,
    pub tool_class: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalletTaskMirrorWire {
    pub owner_hex: String,
    pub escrow_wei: String,
    pub tool_receipt_root_hex: String,
    pub verifier_score: u8,
    pub status: u8,
    pub checkout_signer_hex: Option<String>,
    pub posted_at_ms: u64,
}

/// Inputs for merging a finalized §14.5 task into `reputation_rows`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletTaskFinalizeEvent {
    pub task_id: u64,
    pub provider_id: [u8; 32],
    pub tool_class: u8,
    pub requester: Address,
    pub escrow_wei: u128,
    pub verifier_score: u8,
    pub finalized_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalletChainMirror {
    #[serde(default = "default_next_task_id")]
    pub next_wallet_task_id: u64,
    #[serde(default)]
    pub wallet_tasks: BTreeMap<String, WalletTaskMirrorWire>,
    /// Poster / provider operator `0x…` → `provider_id` hex (no `0x` prefix on value).
    #[serde(default)]
    pub provider_owner_hex_to_id: BTreeMap<String, String>,
    /// `provider_id_hex:tool_class` → available stake wei (decimal).
    #[serde(default)]
    pub provider_class_available_stake: BTreeMap<String, String>,
    /// `tool_receipt_root` `0x…` → provider + class from [`NativeCall::WalletBatchSettleV1`].
    #[serde(default)]
    pub tool_receipt_root_hex_to_meta: BTreeMap<String, WalletToolRootMetaWire>,
    /// Populated when the latest tx was [`NativeCall::WalletFinalizeTaskV1`]; consumed by reputation sync.
    #[serde(default)]
    pub pending_finalize: Option<WalletTaskFinalizeEventWire>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalletTaskFinalizeEventWire {
    pub task_id: u64,
    pub provider_id_hex: String,
    pub tool_class: u8,
    pub requester_hex: String,
    pub escrow_wei: String,
    pub verifier_score: u8,
    pub finalized_at_ms: u64,
}

fn default_next_task_id() -> u64 {
    1
}

impl Default for WalletChainMirror {
    fn default() -> Self {
        Self {
            next_wallet_task_id: default_next_task_id(),
            wallet_tasks: BTreeMap::new(),
            provider_owner_hex_to_id: BTreeMap::new(),
            provider_class_available_stake: BTreeMap::new(),
            tool_receipt_root_hex_to_meta: BTreeMap::new(),
            pending_finalize: None,
        }
    }
}

impl WalletTaskFinalizeEventWire {
    pub fn decode(&self) -> Option<WalletTaskFinalizeEvent> {
        let mut pid = [0u8; 32];
        let raw = self.provider_id_hex.trim_start_matches("0x");
        let bytes = hex::decode(raw).ok()?;
        if bytes.len() != 32 {
            return None;
        }
        pid.copy_from_slice(&bytes);
        let mut requester = [0u8; 20];
        let rh = self.requester_hex.trim_start_matches("0x");
        let rb = hex::decode(rh).ok()?;
        if rb.len() != 20 {
            return None;
        }
        requester.copy_from_slice(&rb);
        Some(WalletTaskFinalizeEvent {
            task_id: self.task_id,
            provider_id: pid,
            tool_class: self.tool_class,
            requester,
            escrow_wei: self.escrow_wei.parse().ok()?,
            verifier_score: self.verifier_score,
            finalized_at_ms: self.finalized_at_ms,
        })
    }
}

impl WalletChainMirror {
    pub fn available_stake_wei(&self, provider_id: &[u8; 32], tool_class: u8) -> u128 {
        let key = provider_class_key(&hex::encode(provider_id), tool_class);
        self.provider_class_available_stake
            .get(&key)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }

    pub fn take_pending_finalize(&mut self) -> Option<WalletTaskFinalizeEvent> {
        let w = self.pending_finalize.take()?;
        w.decode()
    }

    fn resolve_provider_for_task(&self, task: &WalletTaskMirrorWire) -> Option<( [u8; 32], u8)> {
        if let Some(meta) = self.tool_receipt_root_hex_to_meta.get(&task.tool_receipt_root_hex) {
            let mut pid = [0u8; 32];
            let raw = meta.provider_id_hex.trim_start_matches("0x");
            let bytes = hex::decode(raw).ok()?;
            if bytes.len() != 32 {
                return None;
            }
            pid.copy_from_slice(&bytes);
            return Some((pid, meta.tool_class));
        }
        let checkout = task.checkout_signer_hex.as_ref()?;
        let pid_hex = self.provider_owner_hex_to_id.get(checkout)?;
        let mut pid = [0u8; 32];
        let bytes = hex::decode(pid_hex).ok()?;
        if bytes.len() != 32 {
            return None;
        }
        pid.copy_from_slice(&bytes);
        Some((pid, 0))
    }

    pub fn apply_wallet_native(
        &mut self,
        signer: Address,
        call: &NativeCall,
        block_timestamp_ms: u64,
    ) {
        self.pending_finalize = None;
        match call {
            NativeCall::WalletRegisterProviderV1 { registration } => {
                self.record_provider_registration(registration);
            }
            NativeCall::WalletStakeForClassV1 {
                provider_id,
                tool_class,
                amount,
            } => {
                let key = provider_class_key(&hex::encode(provider_id), *tool_class);
                let cur = self
                    .provider_class_available_stake
                    .get(&key)
                    .and_then(|s| s.parse::<u128>().ok())
                    .unwrap_or(0);
                self.provider_class_available_stake
                    .insert(key, cur.saturating_add(*amount).to_string());
            }
            NativeCall::WalletSlashProviderV1 { provider_id, slash } => {
                self.apply_provider_slash(provider_id, slash);
            }
            NativeCall::WalletBatchSettleV1(p) => {
                self.tool_receipt_root_hex_to_meta.insert(
                    hash32_hex(&p.receipt_root),
                    WalletToolRootMetaWire {
                        provider_id_hex: hex::encode(p.provider_id),
                        tool_class: p.tool_class,
                    },
                );
            }
            NativeCall::WalletPostTaskV1 {
                bounty_budget,
                tool_budget,
                verifier_budget,
                ..
            } => {
                let id = self.next_wallet_task_id;
                self.next_wallet_task_id = self.next_wallet_task_id.saturating_add(1);
                let escrow = bounty_budget
                    .saturating_add(*tool_budget)
                    .saturating_add(*verifier_budget);
                self.wallet_tasks.insert(
                    id.to_string(),
                    WalletTaskMirrorWire {
                        owner_hex: addr_hex(&signer),
                        escrow_wei: escrow.to_string(),
                        tool_receipt_root_hex: hash32_hex(&[0u8; 32]),
                        verifier_score: 0,
                        status: WALLET_TASK_POSTED,
                        checkout_signer_hex: None,
                        posted_at_ms: block_timestamp_ms,
                    },
                );
            }
            NativeCall::WalletCheckoutTaskV1 { task_id, .. } => {
                if let Some(row) = self.wallet_tasks.get_mut(&task_id.to_string()) {
                    row.status = WALLET_TASK_CHECKED_OUT;
                    row.checkout_signer_hex = Some(addr_hex(&signer));
                }
            }
            NativeCall::WalletSubmitTaskV1 {
                task_id,
                tool_receipt_root,
                ..
            } => {
                if let Some(row) = self.wallet_tasks.get_mut(&task_id.to_string()) {
                    row.status = WALLET_TASK_SUBMITTED;
                    row.tool_receipt_root_hex = hash32_hex(tool_receipt_root);
                }
            }
            NativeCall::WalletVerifyTaskV1 {
                task_id,
                score,
                ..
            } => {
                if let Some(row) = self.wallet_tasks.get_mut(&task_id.to_string()) {
                    row.status = WALLET_TASK_VERIFIED;
                    row.verifier_score = *score;
                }
            }
            NativeCall::WalletFinalizeTaskV1 { task_id } => {
                let Some(task_snap) = self.wallet_tasks.get(&task_id.to_string()).cloned() else {
                    return;
                };
                if task_snap.status != WALLET_TASK_VERIFIED {
                    return;
                }
                let Some((provider_id, tool_class)) = self.resolve_provider_for_task(&task_snap)
                else {
                    return;
                };
                if let Some(row) = self.wallet_tasks.get_mut(&task_id.to_string()) {
                    let escrow_wei: u128 = task_snap.escrow_wei.parse().unwrap_or(0);
                    self.pending_finalize = Some(WalletTaskFinalizeEventWire {
                        task_id: *task_id,
                        provider_id_hex: hex::encode(provider_id),
                        tool_class,
                        requester_hex: task_snap.owner_hex.clone(),
                        escrow_wei: escrow_wei.to_string(),
                        verifier_score: task_snap.verifier_score,
                        finalized_at_ms: block_timestamp_ms,
                    });
                    row.status = WALLET_TASK_FINALIZED;
                    row.escrow_wei = "0".into();
                }
            }
            _ => {}
        }
    }

    pub(crate) fn record_provider_registration(&mut self, reg: &ProviderRegistration) {
        self.provider_owner_hex_to_id.insert(
            addr_hex(&reg.owner),
            hex::encode(reg.provider_id),
        );
    }

    fn apply_provider_slash(&mut self, provider_id: &[u8; 32], slash: &ProviderSlashRecord) {
        let key = provider_class_key(&hex::encode(provider_id), slash.tool_class);
        let cur = self
            .provider_class_available_stake
            .get(&key)
            .and_then(|s| s.parse::<u128>().ok())
            .unwrap_or(0);
        let next = cur.saturating_sub(slash.amount);
        if next == 0 {
            self.provider_class_available_stake.remove(&key);
        } else {
            self.provider_class_available_stake.insert(key, next.to_string());
        }
    }
}

/// Combined mirror persisted under [`crate::reputation::META_REPUTATION_CHAIN_MIRROR`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexerChainMirrorV2 {
    #[serde(flatten)]
    pub legacy: crate::indexer_mirror::IndexerChainMirror,
    #[serde(default)]
    pub wallet: WalletChainMirror,
}

impl Default for IndexerChainMirrorV2 {
    fn default() -> Self {
        Self {
            legacy: crate::indexer_mirror::IndexerChainMirror::default(),
            wallet: WalletChainMirror::default(),
        }
    }
}

impl IndexerChainMirrorV2 {
    pub fn apply_native(
        &mut self,
        signer: Address,
        call: &NativeCall,
        block_timestamp_ms: u64,
    ) -> Option<(u64, u8)> {
        self.wallet
            .apply_wallet_native(signer, call, block_timestamp_ms);
        self.legacy.apply_native(signer, call)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalize_emits_provider_from_batch_root_linkage() {
        let mut m = WalletChainMirror::default();
        let owner = [0x01u8; 20];
        let provider = [0x02u8; 20];
        let mut pid = [0u8; 32];
        pid[0] = 0xaa;
        let root = [0x33u8; 32];
        m.record_provider_registration(&ProviderRegistration {
            provider_id: pid,
            owner: provider,
            public_key: [3u8; 32],
            encryption_pubkey: [4u8; 32],
            metadata_uri: String::new(),
            endpoint_uri: String::new(),
            tool_classes: vec![1],
            tee_attestation_hash: None,
            registration_bond: 0,
        });
        m.apply_wallet_native(
            provider,
            &NativeCall::WalletBatchSettleV1(fractal_core::WalletToolBatchSettlePayload {
                batch_id: [9u8; 32],
                provider_id: pid,
                provider_public_key: [3u8; 32],
                tool_class: 1,
                receipt_root: root,
                total_cost: 10,
                payout_to: provider,
                receipts_borsh: vec![vec![1]],
                submitted_at: 0,
                provider_batch_sig: [0u8; 64],
            }),
            1000,
        );
        m.apply_wallet_native(
            owner,
            &NativeCall::WalletPostTaskV1 {
                metadata_uri: "m".into(),
                bounty_budget: 50,
                tool_budget: 25,
                verifier_budget: 5,
            },
            1000,
        );
        m.apply_wallet_native(
            provider,
            &NativeCall::WalletCheckoutTaskV1 {
                task_id: 1,
                agent_session: [0u8; 32],
                expiry_ms: 9_999,
            },
            1000,
        );
        m.apply_wallet_native(
            provider,
            &NativeCall::WalletSubmitTaskV1 {
                task_id: 1,
                artifact_pointer: "a".into(),
                tool_receipt_root: root,
            },
            1000,
        );
        m.apply_wallet_native(
            owner,
            &NativeCall::WalletVerifyTaskV1 {
                task_id: 1,
                verifier_sig: [0u8; 64],
                score: 90,
            },
            2000,
        );
        m.apply_wallet_native(
            provider,
            &NativeCall::WalletFinalizeTaskV1 { task_id: 1 },
            3000,
        );
        let ev = m.take_pending_finalize().expect("finalize event");
        assert_eq!(ev.tool_class, 1);
        assert_eq!(ev.provider_id, pid);
        assert_eq!(ev.escrow_wei, 80);
        assert_eq!(ev.verifier_score, 90);
    }
}
