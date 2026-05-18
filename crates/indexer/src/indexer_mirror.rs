//! Replay-lite chain view for indexer: agent ids, native stakes, disputes → receipt ids, settled receipt meta.

use std::collections::BTreeMap;

use fractal_core::{Address, NativeCall, OnChainTaskReceipt, DISPUTE_RESOLUTION_PROVIDER_FAULT};
use serde::{Deserialize, Serialize};

pub(crate) fn addr_hex(a: &Address) -> String {
    format!("0x{}", hex::encode(a.as_slice()))
}

fn receipt_id_hex(id: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(id))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReceiptMetaWire {
    pub worker: u64,
    #[serde(default)]
    pub tool_class: u8,
}

/// Mirrors [`fractal_core::State`] fields needed for §10.4 indexing (`RegisterAgent`, `Stake`, disputes, receipts).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexerChainMirror {
    /// Matches [`fractal_core::State::next_agent_id`] default `1` after genesis.
    #[serde(default = "default_next_seq")]
    pub next_agent_id: u64,
    #[serde(default = "default_next_seq")]
    pub next_dispute_id: u64,
    /// `agent_id` decimal string → controlling address `0x…20`.
    #[serde(default)]
    pub agent_id_to_address_hex: BTreeMap<String, String>,
    /// Native PRD stake map: address `0x…20` → wei (decimal string).
    #[serde(default)]
    pub stakes_wei_dec: BTreeMap<String, String>,
    /// `dispute_id` decimal → disputed `receipt_id` `0x…32`.
    #[serde(default)]
    pub dispute_id_to_receipt_id_hex: BTreeMap<String, String>,
    /// Settled `receipt_id` → worker + tool class (for dispute slash routing).
    #[serde(default)]
    pub receipt_id_hex_to_meta: BTreeMap<String, ReceiptMetaWire>,
}

fn default_next_seq() -> u64 {
    1
}

impl Default for IndexerChainMirror {
    fn default() -> Self {
        Self {
            next_agent_id: 1,
            next_dispute_id: 1,
            agent_id_to_address_hex: BTreeMap::new(),
            stakes_wei_dec: BTreeMap::new(),
            dispute_id_to_receipt_id_hex: BTreeMap::new(),
            receipt_id_hex_to_meta: BTreeMap::new(),
        }
    }
}

impl IndexerChainMirror {
    pub fn record_receipt_meta(&mut self, r: &OnChainTaskReceipt) {
        let rid = receipt_id_hex(&r.receipt_id);
        self.receipt_id_hex_to_meta.insert(
            rid,
            ReceiptMetaWire {
                worker: r.worker,
                tool_class: r.tool_class,
            },
        );
    }

    pub fn available_stake_wei_for_worker(&self, worker: u64) -> u128 {
        let addr = match self.agent_id_to_address_hex.get(&worker.to_string()) {
            Some(a) => a,
            None => return 0,
        };
        self.stakes_wei_dec
            .get(addr)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }

    /// Apply one native call (mined tx order). Returns `(worker, tool_class_u8)` when a dispute resolution should slash provider reputation.
    pub fn apply_native(&mut self, signer: Address, call: &NativeCall) -> Option<(u64, u8)> {
        match call {
            NativeCall::RegisterAgent { .. } => {
                let id = self.next_agent_id;
                self.next_agent_id = self.next_agent_id.saturating_add(1);
                self.agent_id_to_address_hex
                    .insert(id.to_string(), addr_hex(&signer));
                None
            }
            NativeCall::Stake { amount } => {
                let k = addr_hex(&signer);
                let cur = self
                    .stakes_wei_dec
                    .get(&k)
                    .and_then(|s| s.parse::<u128>().ok())
                    .unwrap_or(0);
                self.stakes_wei_dec
                    .insert(k, cur.saturating_add(*amount).to_string());
                None
            }
            NativeCall::Unstake { amount } => {
                let k = addr_hex(&signer);
                let cur = self
                    .stakes_wei_dec
                    .get(&k)
                    .and_then(|s| s.parse::<u128>().ok())
                    .unwrap_or(0);
                let n = cur.saturating_sub(*amount);
                if n == 0 {
                    self.stakes_wei_dec.remove(&k);
                } else {
                    self.stakes_wei_dec.insert(k, n.to_string());
                }
                None
            }
            NativeCall::Slash {
                validator_id,
                evidence_hash: _,
            } => {
                self.stakes_wei_dec.remove(&addr_hex(validator_id));
                None
            }
            NativeCall::FileDispute { receipt_id, .. } => {
                let id = self.next_dispute_id;
                self.next_dispute_id = self.next_dispute_id.saturating_add(1);
                self.dispute_id_to_receipt_id_hex
                    .insert(id.to_string(), receipt_id_hex(receipt_id));
                None
            }
            NativeCall::ResolveDispute {
                dispute_id,
                resolution,
                payouts_diff: _,
            } => {
                if *resolution != DISPUTE_RESOLUTION_PROVIDER_FAULT {
                    return None;
                }
                let rid = self
                    .dispute_id_to_receipt_id_hex
                    .get(&dispute_id.to_string())?;
                let m = self.receipt_id_hex_to_meta.get(rid)?;
                Some((m.worker, m.tool_class))
            }
            NativeCall::SettleBatch(p) => {
                for r in &p.receipts {
                    self.record_receipt_meta(r);
                }
                None
            }
            NativeCall::SettleReceipt(r) => {
                self.record_receipt_meta(r);
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stake_flow_updates_lookup_for_worker() {
        let mut m = IndexerChainMirror::default();
        let alice = [0xabu8; 20];
        m.apply_native(
            alice,
            &NativeCall::RegisterAgent {
                operator: alice,
                pubkey: [1u8; 32],
                kind: 0,
                metadata_uri: String::new(),
            },
        );
        assert_eq!(m.next_agent_id, 2);
        m.apply_native(alice, &NativeCall::Stake { amount: 500 });
        assert_eq!(m.available_stake_wei_for_worker(1), 500);
        m.apply_native(alice, &NativeCall::Unstake { amount: 200 });
        assert_eq!(m.available_stake_wei_for_worker(1), 300);
    }
}
