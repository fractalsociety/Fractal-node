//! W6-c — `fractal_sdk::provider` re-exports + quote round-trip smoke.

use ed25519_dalek::SigningKey;
use fractal_sdk::provider::{
    provider_id_from_public_key, IntentPollFilter, Quote, QuoteBody, ToolClass, ToolIntent,
    ToolIntentBody, VerificationTier,
};
use rand::rngs::OsRng;

#[test]
fn provider_id_matches_blake3_of_pubkey() {
    let mut rng = OsRng;
    let sk = SigningKey::generate(&mut rng);
    let pk = sk.verifying_key().to_bytes();
    let id = provider_id_from_public_key(&pk);
    assert_eq!(id.as_slice(), blake3::hash(&pk).as_bytes().as_slice());
}

#[test]
fn intent_poll_filter_respects_tool_class_bit() {
    let mut rng = OsRng;
    let agent = SigningKey::generate(&mut rng);
    let body = ToolIntentBody {
        intent_id: [1u8; 32],
        agent_session: agent.verifying_key().to_bytes(),
        task_id: 1,
        tool_class: ToolClass::Browser,
        payload_commitment: [0u8; 32],
        max_price: 10,
        verification_tier: VerificationTier::Trusted,
        deadline_ms: 1_000_000,
        nonce: 0,
    };
    let intent = ToolIntent::sign(body, &agent).unwrap();

    let f = IntentPollFilter {
        tool_class_mask: ToolClass::LlmInference.bit(),
    };
    assert!(!f.matches_intent(&intent));

    let f2 = IntentPollFilter {
        tool_class_mask: ToolClass::Browser.bit(),
    };
    assert!(f2.matches_intent(&intent));
}

#[test]
fn quote_sign_verify_round_trip() {
    let mut rng = OsRng;
    let prov = SigningKey::generate(&mut rng);
    let pk = prov.verifying_key().to_bytes();
    let pid = provider_id_from_public_key(&pk);
    let body = QuoteBody {
        quote_id: [7u8; 32],
        intent_id: [8u8; 32],
        provider_id: pid,
        price: 99,
        expiry_ms: 2_000_000,
    };
    let q = Quote::sign(body, &prov).unwrap();
    q.verify(&pk).unwrap();
}
