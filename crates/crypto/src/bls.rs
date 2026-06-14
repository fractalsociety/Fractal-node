//! BLS12-381 keys, signatures, and aggregate verify for HotStuff-2 QCs (`docs/prd.md` §7.3, §18 M7).
//!
//! Wire encoding is min-pubkey-size (BLS12-381 G1 pubkeys, 48 bytes; G2 signatures, 96 bytes)
//! matching Ethereum / Lighthouse / Teku. Signatures use the IETF
//! `BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_` DST (proof-of-possession variant) for
//! protection against rogue-key attacks once we add per-validator key registration.
//!
//! The public byte layouts (`BlsPublicKey([u8; 48])`, `BlsSignature([u8; 96])`,
//! `AggregateSignature { bytes: [u8; 96] }`) are intentionally unchanged from the M1
//! placeholder so existing borsh wire formats (e.g. `consensus::qc::QuorumCertificate`)
//! stay compatible.

use blst::min_pk;
use blst::BLST_ERROR;
use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;

/// IETF BLS proof-of-possession DST (`docs/prd.md` §7.3). Stable across the protocol.
pub const BLS_DST: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

/// 32-byte BLS12-381 scalar (validator signing key).
///
/// Borsh-serializable so operators can ship key material via env vars / config files,
/// but `Debug` deliberately omits the bytes.
#[derive(Clone, BorshSerialize, BorshDeserialize, PartialEq, Eq)]
pub struct BlsSecretKey(pub [u8; 32]);

impl std::fmt::Debug for BlsSecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlsSecretKey")
            .field("bytes", &"<redacted>")
            .finish()
    }
}

/// 48-byte compressed BLS12-381 G1 public key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct BlsPublicKey(pub [u8; 48]);

/// 96-byte compressed BLS12-381 G2 signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct BlsSignature(pub [u8; 96]);

/// Aggregate signature over multiple validators signing **the same** message.
/// Wire-identical to a single `BlsSignature` (96-byte compressed G2 point).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct AggregateSignature {
    pub bytes: [u8; 96],
}

#[derive(Debug, Error)]
pub enum BlsError {
    #[error("blst: {0:?}")]
    Blst(BLST_ERROR),
    #[error("BLS key/signature byte layout invalid")]
    BadEncoding,
    #[error("aggregate verify requires at least one public key")]
    EmptyPubkeySet,
}

impl From<BLST_ERROR> for BlsError {
    fn from(e: BLST_ERROR) -> Self {
        BlsError::Blst(e)
    }
}

fn ok(e: BLST_ERROR) -> Result<(), BlsError> {
    match e {
        BLST_ERROR::BLST_SUCCESS => Ok(()),
        other => Err(BlsError::Blst(other)),
    }
}

impl BlsSecretKey {
    /// Derive a secret key from `ikm` (≥ 32 bytes recommended; `blst::min_pk::SecretKey::key_gen`).
    pub fn from_ikm(ikm: &[u8]) -> Result<Self, BlsError> {
        let sk = min_pk::SecretKey::key_gen(ikm, &[]).map_err(BlsError::from)?;
        Ok(Self(sk.to_bytes()))
    }

    /// Reconstruct from canonical 32-byte serialization.
    pub fn from_bytes(b: &[u8; 32]) -> Result<Self, BlsError> {
        let _ = min_pk::SecretKey::from_bytes(b).map_err(BlsError::from)?;
        Ok(Self(*b))
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    pub fn public_key(&self) -> BlsPublicKey {
        let sk = min_pk::SecretKey::from_bytes(&self.0).expect("validated on construct");
        BlsPublicKey(sk.sk_to_pk().to_bytes())
    }

    pub fn sign(&self, msg: &[u8]) -> BlsSignature {
        let sk = min_pk::SecretKey::from_bytes(&self.0).expect("validated on construct");
        let sig = sk.sign(msg, BLS_DST, &[]);
        BlsSignature(sig.to_bytes())
    }
}

impl BlsPublicKey {
    fn to_blst(&self) -> Result<min_pk::PublicKey, BlsError> {
        let pk = min_pk::PublicKey::from_bytes(&self.0).map_err(BlsError::from)?;
        pk.validate().map_err(BlsError::from)?;
        Ok(pk)
    }
}

impl BlsSignature {
    fn to_blst(&self) -> Result<min_pk::Signature, BlsError> {
        let sig = min_pk::Signature::from_bytes(&self.0).map_err(BlsError::from)?;
        sig.validate(true).map_err(BlsError::from)?;
        Ok(sig)
    }

    /// Verify this single signature against `msg` and `pk`.
    pub fn verify(&self, msg: &[u8], pk: &BlsPublicKey) -> Result<(), BlsError> {
        let sig = self.to_blst()?;
        let pk = pk.to_blst()?;
        ok(sig.verify(true, msg, BLS_DST, &[], &pk, true))
    }
}

impl AggregateSignature {
    /// Aggregate one-or-more component signatures into a single 96-byte point.
    pub fn from_signatures(sigs: &[BlsSignature]) -> Result<Self, BlsError> {
        if sigs.is_empty() {
            return Err(BlsError::EmptyPubkeySet);
        }
        let mut refs: Vec<min_pk::Signature> = Vec::with_capacity(sigs.len());
        for s in sigs {
            refs.push(s.to_blst()?);
        }
        let sig_refs: Vec<&min_pk::Signature> = refs.iter().collect();
        let agg = min_pk::AggregateSignature::aggregate(&sig_refs, true).map_err(BlsError::from)?;
        Ok(Self {
            bytes: agg.to_signature().to_bytes(),
        })
    }

    /// Fast aggregate verify: every supplied pubkey signed **the same** `msg`.
    /// Used by QC verification where all 2f+1 validators voted for the same header.
    pub fn verify(&self, msg: &[u8], pubkeys: &[BlsPublicKey]) -> Result<(), BlsError> {
        if pubkeys.is_empty() {
            return Err(BlsError::EmptyPubkeySet);
        }
        let agg_sig = min_pk::Signature::from_bytes(&self.bytes).map_err(BlsError::from)?;
        agg_sig.validate(true).map_err(BlsError::from)?;
        let mut pks: Vec<min_pk::PublicKey> = Vec::with_capacity(pubkeys.len());
        for p in pubkeys {
            pks.push(p.to_blst()?);
        }
        let pk_refs: Vec<&min_pk::PublicKey> = pks.iter().collect();
        ok(agg_sig.fast_aggregate_verify(true, msg, BLS_DST, &pk_refs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sk_from_seed_byte(b: u8) -> BlsSecretKey {
        let ikm = [b; 32];
        BlsSecretKey::from_ikm(&ikm).expect("key_gen")
    }

    #[test]
    fn sign_verify_round_trip() {
        let sk = sk_from_seed_byte(0xa1);
        let pk = sk.public_key();
        let msg = b"hello fractal";
        let sig = sk.sign(msg);
        sig.verify(msg, &pk).expect("verify ok");
    }

    #[test]
    fn verify_fails_for_tampered_message() {
        let sk = sk_from_seed_byte(0xa2);
        let pk = sk.public_key();
        let sig = sk.sign(b"original");
        assert!(sig.verify(b"tampered", &pk).is_err());
    }

    #[test]
    fn verify_fails_for_wrong_pubkey() {
        let sk1 = sk_from_seed_byte(0x01);
        let sk2 = sk_from_seed_byte(0x02);
        let sig = sk1.sign(b"x");
        assert!(sig.verify(b"x", &sk2.public_key()).is_err());
    }

    #[test]
    fn aggregate_seven_signatures_verify_together() {
        let sks: Vec<_> = (1u8..=7).map(sk_from_seed_byte).collect();
        let pks: Vec<_> = sks.iter().map(BlsSecretKey::public_key).collect();
        let msg = b"qc payload bytes";
        let sigs: Vec<_> = sks.iter().map(|sk| sk.sign(msg)).collect();
        let agg = AggregateSignature::from_signatures(&sigs).expect("aggregate");
        agg.verify(msg, &pks).expect("aggregate verify ok");
    }

    #[test]
    fn aggregate_fails_if_pubkey_set_too_small() {
        let sks: Vec<_> = (10u8..=14).map(sk_from_seed_byte).collect();
        let pks: Vec<_> = sks.iter().map(BlsSecretKey::public_key).collect();
        let msg = b"five-of-five message";
        let sigs: Vec<_> = sks.iter().map(|sk| sk.sign(msg)).collect();
        let agg = AggregateSignature::from_signatures(&sigs).unwrap();
        // Only present 4 of 5 pubkeys → fast_aggregate_verify must fail.
        assert!(agg.verify(msg, &pks[..4]).is_err());
    }

    #[test]
    fn aggregate_fails_for_wrong_message() {
        let sks: Vec<_> = (20u8..=22).map(sk_from_seed_byte).collect();
        let pks: Vec<_> = sks.iter().map(BlsSecretKey::public_key).collect();
        let sigs: Vec<_> = sks.iter().map(|sk| sk.sign(b"A")).collect();
        let agg = AggregateSignature::from_signatures(&sigs).unwrap();
        assert!(agg.verify(b"B", &pks).is_err());
    }

    #[test]
    fn empty_aggregate_inputs_rejected() {
        let pk = sk_from_seed_byte(0x33).public_key();
        let empty: Vec<BlsSignature> = Vec::new();
        assert!(AggregateSignature::from_signatures(&empty).is_err());
        let sig = AggregateSignature {
            bytes: [0u8; 96], // not a valid point; verify() will hit decode/validate before pubkey loop
        };
        let empty_pks: Vec<BlsPublicKey> = Vec::new();
        assert!(sig.verify(b"x", &empty_pks).is_err());
        // sanity: empty `pubkeys` rejected even for a real signature.
        let real = sk_from_seed_byte(0x34).sign(b"x");
        let real_agg = AggregateSignature::from_signatures(&[real]).unwrap();
        let none: Vec<BlsPublicKey> = Vec::new();
        assert!(real_agg.verify(b"x", &none).is_err());
        // unused
        let _ = pk;
    }

    #[test]
    fn secret_key_round_trips_through_bytes() {
        let sk = sk_from_seed_byte(0x55);
        let bytes = sk.to_bytes();
        let sk2 = BlsSecretKey::from_bytes(&bytes).expect("from_bytes");
        assert_eq!(sk, sk2);
        // Public keys match.
        assert_eq!(sk.public_key(), sk2.public_key());
        // Same message yields the same signature (BLS is deterministic).
        let m = b"determinism";
        assert_eq!(sk.sign(m), sk2.sign(m));
    }
}
