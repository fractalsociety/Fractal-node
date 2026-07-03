//! RLVR-052 dispute placeholder.
//!
//! This is intentionally hash-only and payout-disabled. It gives future Fractal
//! Chain dispute logic a stable schema without activating economics.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{hash_bytes, scan_privacy_tags, RlvrError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RlvrDisputeTarget {
    BadRouteClaim,
    InflatedReward,
    FakeEval,
    WrongAdapterHash,
    PolicyMismatch,
}

impl RlvrDisputeTarget {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BadRouteClaim => "bad_route_claim",
            Self::InflatedReward => "inflated_reward",
            Self::FakeEval => "fake_eval",
            Self::WrongAdapterHash => "wrong_adapter_hash",
            Self::PolicyMismatch => "policy_mismatch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlvrDisputeRecord {
    pub target: RlvrDisputeTarget,
    pub disputed_proof_hash: String,
    pub challenger_node_id: String,
    pub evidence_hash: String,
    pub route_policy_hash: Option<String>,
    pub adapter_hash: Option<String>,
    pub eval_result_hash: Option<String>,
    pub reward_policy_hash: Option<String>,
    pub timestamp_ms: u64,
    pub payouts_enabled: bool,
}

#[derive(Serialize)]
struct CanonicalDisputeRecordPayload<'a> {
    target: RlvrDisputeTarget,
    disputed_proof_hash: &'a str,
    challenger_node_id: &'a str,
    evidence_hash: &'a str,
    route_policy_hash: Option<&'a str>,
    adapter_hash: Option<&'a str>,
    eval_result_hash: Option<&'a str>,
    reward_policy_hash: Option<&'a str>,
    timestamp_ms: u64,
    payouts_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RlvrDisputeStoreMetrics {
    pub dispute_total: usize,
    pub bad_route_claim_total: usize,
    pub inflated_reward_total: usize,
    pub fake_eval_total: usize,
    pub wrong_adapter_hash_total: usize,
    pub policy_mismatch_total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RlvrDisputeStore {
    disputes: BTreeMap<String, RlvrDisputeRecord>,
    metrics: RlvrDisputeStoreMetrics,
}

impl RlvrDisputeRecord {
    pub fn new(
        target: RlvrDisputeTarget,
        disputed_proof_hash: impl Into<String>,
        challenger_node_id: impl Into<String>,
        evidence_hash: impl Into<String>,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            target,
            disputed_proof_hash: disputed_proof_hash.into(),
            challenger_node_id: challenger_node_id.into(),
            evidence_hash: evidence_hash.into(),
            route_policy_hash: None,
            adapter_hash: None,
            eval_result_hash: None,
            reward_policy_hash: None,
            timestamp_ms,
            payouts_enabled: false,
        }
    }

    pub fn with_route_policy_hash(mut self, route_policy_hash: impl Into<String>) -> Self {
        self.route_policy_hash = Some(route_policy_hash.into());
        self
    }

    pub fn with_adapter_hash(mut self, adapter_hash: impl Into<String>) -> Self {
        self.adapter_hash = Some(adapter_hash.into());
        self
    }

    pub fn with_eval_result_hash(mut self, eval_result_hash: impl Into<String>) -> Self {
        self.eval_result_hash = Some(eval_result_hash.into());
        self
    }

    pub fn with_reward_policy_hash(mut self, reward_policy_hash: impl Into<String>) -> Self {
        self.reward_policy_hash = Some(reward_policy_hash.into());
        self
    }

    pub fn validate_placeholder(&self) -> Result<(), RlvrError> {
        validate_hex_hash("disputed_proof_hash", &self.disputed_proof_hash)?;
        validate_hex_hash("evidence_hash", &self.evidence_hash)?;
        validate_optional_hash("route_policy_hash", self.route_policy_hash.as_deref())?;
        validate_optional_hash("adapter_hash", self.adapter_hash.as_deref())?;
        validate_optional_hash("eval_result_hash", self.eval_result_hash.as_deref())?;
        validate_optional_hash("reward_policy_hash", self.reward_policy_hash.as_deref())?;
        validate_compact_reference("challenger_node_id", &self.challenger_node_id)?;
        if self.timestamp_ms == 0 {
            return Err(RlvrError::Config(
                "rlvr dispute timestamp_ms must be greater than zero".into(),
            ));
        }
        if self.payouts_enabled {
            return Err(RlvrError::Config(
                "rlvr dispute placeholder cannot enable payouts".into(),
            ));
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, RlvrError> {
        self.validate_placeholder()?;
        serde_json::to_vec(&CanonicalDisputeRecordPayload {
            target: self.target,
            disputed_proof_hash: &self.disputed_proof_hash,
            challenger_node_id: &self.challenger_node_id,
            evidence_hash: &self.evidence_hash,
            route_policy_hash: self.route_policy_hash.as_deref(),
            adapter_hash: self.adapter_hash.as_deref(),
            eval_result_hash: self.eval_result_hash.as_deref(),
            reward_policy_hash: self.reward_policy_hash.as_deref(),
            timestamp_ms: self.timestamp_ms,
            payouts_enabled: self.payouts_enabled,
        })
        .map_err(RlvrError::from)
    }

    pub fn challenge_hash(&self) -> Result<String, RlvrError> {
        Ok(hash_bytes(&self.canonical_bytes()?))
    }
}

impl RlvrDisputeStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.disputes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.disputes.is_empty()
    }

    pub fn metrics(&self) -> &RlvrDisputeStoreMetrics {
        &self.metrics
    }

    pub fn get(&self, challenge_hash: &str) -> Option<&RlvrDisputeRecord> {
        self.disputes.get(challenge_hash)
    }

    pub fn insert(&mut self, record: RlvrDisputeRecord) -> Result<String, RlvrError> {
        record.validate_placeholder()?;
        let challenge_hash = record.challenge_hash()?;
        if self.disputes.contains_key(&challenge_hash) {
            return Err(RlvrError::Config(format!(
                "rlvr dispute store already contains challenge_hash {challenge_hash}"
            )));
        }
        self.disputes.insert(challenge_hash.clone(), record);
        self.refresh_metrics();
        Ok(challenge_hash)
    }

    pub fn list(&self) -> Vec<(String, RlvrDisputeRecord)> {
        self.disputes
            .iter()
            .map(|(hash, record)| (hash.clone(), record.clone()))
            .collect()
    }

    fn refresh_metrics(&mut self) {
        self.metrics.dispute_total = self.disputes.len();
        self.metrics.bad_route_claim_total = self.count_target(RlvrDisputeTarget::BadRouteClaim);
        self.metrics.inflated_reward_total = self.count_target(RlvrDisputeTarget::InflatedReward);
        self.metrics.fake_eval_total = self.count_target(RlvrDisputeTarget::FakeEval);
        self.metrics.wrong_adapter_hash_total =
            self.count_target(RlvrDisputeTarget::WrongAdapterHash);
        self.metrics.policy_mismatch_total = self.count_target(RlvrDisputeTarget::PolicyMismatch);
    }

    fn count_target(&self, target: RlvrDisputeTarget) -> usize {
        self.disputes
            .values()
            .filter(|record| record.target == target)
            .count()
    }
}

fn validate_optional_hash(name: &str, value: Option<&str>) -> Result<(), RlvrError> {
    if let Some(value) = value {
        validate_hex_hash(name, value)?;
    }
    Ok(())
}

fn validate_hex_hash(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RlvrError::Config(format!(
            "{name} must be a 64-character hex hash"
        )));
    }
    Ok(())
}

fn validate_compact_reference(name: &str, value: &str) -> Result<(), RlvrError> {
    let lower = value.to_ascii_lowercase();
    if value.trim().is_empty()
        || value.chars().any(char::is_whitespace)
        || scan_privacy_tags(value).is_private
        || lower.contains("raw_prompt")
        || lower.contains("raw_answer")
        || lower.contains("api key")
        || lower.contains("private file")
        || lower.contains("file contents")
    {
        return Err(RlvrError::Config(format!(
            "{name} must be a compact reference and cannot contain raw user data"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dispute(target: RlvrDisputeTarget) -> RlvrDisputeRecord {
        RlvrDisputeRecord::new(
            target,
            hash_bytes(b"proof"),
            "node-a",
            hash_bytes(b"evidence"),
            1_700_000_000_000,
        )
    }

    #[test]
    fn dispute_targets_cover_prd_placeholder_cases() {
        let targets = [
            RlvrDisputeTarget::BadRouteClaim,
            RlvrDisputeTarget::InflatedReward,
            RlvrDisputeTarget::FakeEval,
            RlvrDisputeTarget::WrongAdapterHash,
            RlvrDisputeTarget::PolicyMismatch,
        ];

        assert_eq!(targets.len(), 5);
        assert_eq!(RlvrDisputeTarget::BadRouteClaim.as_str(), "bad_route_claim");
        assert_eq!(
            RlvrDisputeTarget::InflatedReward.as_str(),
            "inflated_reward"
        );
        assert_eq!(RlvrDisputeTarget::FakeEval.as_str(), "fake_eval");
        assert_eq!(
            RlvrDisputeTarget::WrongAdapterHash.as_str(),
            "wrong_adapter_hash"
        );
        assert_eq!(
            RlvrDisputeTarget::PolicyMismatch.as_str(),
            "policy_mismatch"
        );
    }

    #[test]
    fn dispute_record_is_hash_only_and_has_deterministic_challenge_hash() {
        let record = dispute(RlvrDisputeTarget::BadRouteClaim)
            .with_route_policy_hash(hash_bytes(b"route-policy"))
            .with_reward_policy_hash(hash_bytes(b"reward-policy"));

        record.validate_placeholder().unwrap();
        assert_eq!(
            record.challenge_hash().unwrap(),
            record.challenge_hash().unwrap()
        );
        let json = serde_json::to_string(&record).unwrap();
        assert!(!json.contains("raw prompt"));
        assert!(!json.contains("actual user answer"));
        assert!(json.contains("bad_route_claim"));
    }

    #[test]
    fn dispute_record_rejects_raw_data_and_payouts() {
        let mut raw_node = dispute(RlvrDisputeTarget::FakeEval);
        raw_node.challenger_node_id = "private file contents from /Users/alice/tax.pdf".into();
        assert!(raw_node.validate_placeholder().is_err());

        let mut payouts = dispute(RlvrDisputeTarget::InflatedReward);
        payouts.payouts_enabled = true;
        let err = payouts.validate_placeholder().unwrap_err();
        assert!(err.to_string().contains("cannot enable payouts"));
    }

    #[test]
    fn dispute_store_keys_by_challenge_hash_rejects_duplicates_and_tracks_metrics() {
        let mut store = RlvrDisputeStore::new();
        let bad_route = dispute(RlvrDisputeTarget::BadRouteClaim)
            .with_route_policy_hash(hash_bytes(b"route-policy"));
        let bad_route_hash = store.insert(bad_route.clone()).unwrap();
        let wrong_adapter_hash = store
            .insert(
                dispute(RlvrDisputeTarget::WrongAdapterHash)
                    .with_adapter_hash(hash_bytes(b"adapter")),
            )
            .unwrap();

        assert_eq!(store.len(), 2);
        assert!(store.get(&bad_route_hash).is_some());
        assert!(store.get(&wrong_adapter_hash).is_some());
        assert_eq!(store.metrics().dispute_total, 2);
        assert_eq!(store.metrics().bad_route_claim_total, 1);
        assert_eq!(store.metrics().wrong_adapter_hash_total, 1);

        let err = store.insert(bad_route).unwrap_err();
        assert!(err.to_string().contains("already contains challenge_hash"));
    }

    #[test]
    fn dispute_store_accepts_all_placeholder_targets_as_hash_commitments() {
        let mut store = RlvrDisputeStore::new();
        store
            .insert(dispute(RlvrDisputeTarget::BadRouteClaim))
            .unwrap();
        store
            .insert(
                dispute(RlvrDisputeTarget::InflatedReward)
                    .with_reward_policy_hash(hash_bytes(b"reward-policy")),
            )
            .unwrap();
        store
            .insert(
                dispute(RlvrDisputeTarget::FakeEval)
                    .with_eval_result_hash(hash_bytes(b"eval-result")),
            )
            .unwrap();
        store
            .insert(
                dispute(RlvrDisputeTarget::WrongAdapterHash)
                    .with_adapter_hash(hash_bytes(b"adapter")),
            )
            .unwrap();
        store
            .insert(
                dispute(RlvrDisputeTarget::PolicyMismatch)
                    .with_route_policy_hash(hash_bytes(b"route-policy")),
            )
            .unwrap();

        assert_eq!(store.metrics().dispute_total, 5);
        assert_eq!(store.metrics().bad_route_claim_total, 1);
        assert_eq!(store.metrics().inflated_reward_total, 1);
        assert_eq!(store.metrics().fake_eval_total, 1);
        assert_eq!(store.metrics().wrong_adapter_hash_total, 1);
        assert_eq!(store.metrics().policy_mismatch_total, 1);
    }
}
