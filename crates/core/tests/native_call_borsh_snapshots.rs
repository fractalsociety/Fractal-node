//! Stable borsh wire bytes for Solidity / tooling (Fractal native precompile calldata).

use fractal_core::NativeCall;

#[test]
fn noop_native_call_borsh_is_single_byte_discriminant() {
    let v = borsh::to_vec(&NativeCall::NoOp).unwrap();
    assert_eq!(v.as_slice(), &[13u8], "NoOp must stay variant index 13 for on-chain docs");
}
