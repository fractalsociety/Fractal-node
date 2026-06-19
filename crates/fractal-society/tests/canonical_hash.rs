//! Gate P01-N02: canonical serialization is deterministic regardless of struct
//! field order or map insertion order.

use fractal_society::canonical;
use fractal_society::protocol::{ChainReference, Hash};
use serde_json::json;

#[test]
fn same_value_different_key_order_same_hash() {
    let a = json!({"z": 1, "a": 2, "m": {"y": 3, "x": 1}});
    let b = json!({"a": 2, "m": {"x": 1, "y": 3}, "z": 1});
    assert_eq!(
        canonical::content_hash(&a).unwrap(),
        canonical::content_hash(&b).unwrap()
    );
}

#[test]
fn different_values_different_hash() {
    let a = json!({"v": 1});
    let b = json!({"v": 2});
    assert_ne!(
        canonical::content_hash(&a).unwrap(),
        canonical::content_hash(&b).unwrap()
    );
}

#[test]
fn struct_hash_is_stable_across_construction() {
    let cr1 = ChainReference {
        network: "testnet".into(),
        transaction_hash: "0xabc".into(),
        block_number: 10,
        finalized: true,
    };
    let cr2 = ChainReference {
        network: "testnet".into(),
        transaction_hash: "0xabc".into(),
        block_number: 10,
        finalized: true,
    };
    assert_eq!(Hash::of(&cr1).unwrap(), Hash::of(&cr2).unwrap());
}
