//! §14.5 wallet task lifecycle mirror for reputation derivation (`docs/wallet.md` §10.4).

use std::collections::BTreeMap;

use fractal_core::{Address, NativeCall};
use serde::{Deserialize, Serialize};

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

    pub fn apply_wallet_native(
        &mut self,
        _signer: Address,
        _call: &NativeCall,
        _block_timestamp_ms: u64,
    ) {
        self.pending_finalize = None;
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
    fn current_chain_wallet_anchor_does_not_emit_finalize() {
        let mut m = WalletChainMirror::default();
        let signer = [0x01u8; 20];
        m.apply_wallet_native(
            signer,
            &NativeCall::WalletTaskReceiptAnchorV1 {
                commitment: [0xabu8; 32],
                receipt_witness: vec![1, 2, 3],
            },
            3000,
        );
        assert!(m.take_pending_finalize().is_none());
    }
}
