//! AR-08: per-entry provenance tagging on decision traces.

use fractal_society::protocol::{DecisionTrace, ProvenanceTag, RiskDecision};

fn ts() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(0, 0).unwrap()
}

#[test]
fn provenance_round_trips_through_serialization() {
    let trace = DecisionTrace {
        step: 1,
        observation_hash: fractal_society::protocol::Hash::new(b"obs"),
        action: serde_json::json!({ "arm": 0 }),
        risk_decision: RiskDecision::Approved,
        outcome: serde_json::json!({ "reward": 1.0 }),
        provenance: Some(ProvenanceTag::AiExecuted),
        timestamp: ts(),
    };

    let json = serde_json::to_string(&trace).unwrap();
    assert!(
        json.contains("\"provenance\":\"ai_executed\""),
        "got: {json}"
    );
    let back: DecisionTrace = serde_json::from_str(&json).unwrap();
    assert_eq!(back.provenance, Some(ProvenanceTag::AiExecuted));
}

#[test]
fn provenance_can_be_absent() {
    let trace = DecisionTrace {
        step: 0,
        observation_hash: fractal_society::protocol::Hash::new(b"obs"),
        action: serde_json::json!({}),
        risk_decision: RiskDecision::Approved,
        outcome: serde_json::json!({}),
        provenance: None,
        timestamp: ts(),
    };
    let json = serde_json::to_string(&trace).unwrap();
    assert!(json.contains("\"provenance\":null"), "got: {json}");
    let back: DecisionTrace = serde_json::from_str(&json).unwrap();
    assert_eq!(back.provenance, None);
}

#[test]
fn default_provenance_is_human() {
    assert_eq!(ProvenanceTag::default(), ProvenanceTag::Human);
}
