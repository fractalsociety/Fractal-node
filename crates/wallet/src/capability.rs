//! Capability token: serialize, sign, verify (`docs/wallet.md` §4.2).

use borsh::{BorshDeserialize, BorshSerialize};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use thiserror::Error;

use crate::caveat::{caveats_attenuate_parent, Caveat};
use crate::types::{PublicKey, Scope, TimestampMs};

pub type CapabilityId = [u8; 32];

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CapabilitySignBody {
    pub version: u16,
    pub cap_id: CapabilityId,
    pub chain_id: u32,
    pub issuer: PublicKey,
    pub subject: PublicKey,
    pub parent_cap_id: Option<CapabilityId>,
    pub scope: Scope,
    pub caveats: Vec<Caveat>,
    pub budget_account: u64,
    pub not_before: TimestampMs,
    pub not_after: TimestampMs,
    pub nonce: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CapabilityToken {
    pub body: CapabilitySignBody,
    pub signature: [u8; 64],
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CapabilityVerifyError {
    #[error("ed25519 signature invalid")]
    BadSignature,
    #[error("capability expired or not yet valid")]
    OutsideValidity,
    #[error("autonomous capability requires non-zero tool_class_mask")]
    EmptyToolMask,
    #[error("issuer public key invalid")]
    BadIssuerKey,
}

impl CapabilityToken {
    pub fn signing_bytes(body: &CapabilitySignBody) -> Result<Vec<u8>, std::io::Error> {
        borsh::to_vec(body)
    }

    pub fn sign(body: CapabilitySignBody, issuer: &SigningKey) -> Result<Self, std::io::Error> {
        let msg = Self::signing_bytes(&body)?;
        let sig = issuer.sign(&msg);
        Ok(Self {
            body,
            signature: sig.to_bytes(),
        })
    }

    pub fn verify(&self) -> Result<(), CapabilityVerifyError> {
        let vk = VerifyingKey::from_bytes(&self.body.issuer).map_err(|_| CapabilityVerifyError::BadIssuerKey)?;
        let sig = Signature::from_bytes(&self.signature);
        let msg = Self::signing_bytes(&self.body).map_err(|_| CapabilityVerifyError::BadSignature)?;
        vk.verify(&msg, &sig)
            .map_err(|_| CapabilityVerifyError::BadSignature)
    }

    /// `now_ms` is the proposer/verifier clock bound from the chain (not wall clock in validators).
    pub fn verify_time(&self, now_ms: TimestampMs) -> Result<(), CapabilityVerifyError> {
        self.verify()?;
        if now_ms < self.body.not_before || now_ms > self.body.not_after {
            return Err(CapabilityVerifyError::OutsideValidity);
        }
        Ok(())
    }

    /// Phase 1 autonomous rule: tool mask must be non-zero (§4.3).
    pub fn verify_autonomous_tool_mask(&self) -> Result<(), CapabilityVerifyError> {
        if self.body.scope.tool_class_mask == 0 {
            return Err(CapabilityVerifyError::EmptyToolMask);
        }
        Ok(())
    }

    /// Minted child must be narrower than parent in scope, time, and caveats.
    pub fn verify_attenuation_from_parent(
        child: &CapabilitySignBody,
        parent: &CapabilitySignBody,
    ) -> bool {
        if child.parent_cap_id != Some(parent.cap_id) {
            return false;
        }
        if child.chain_id != parent.chain_id {
            return false;
        }
        if child.not_before < parent.not_before {
            return false;
        }
        if child.not_after > parent.not_after {
            return false;
        }
        if !child.scope.is_subset_of(&parent.scope) {
            return false;
        }
        caveats_attenuate_parent(&parent.caveats, &child.caveats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caveat::Caveat;
    use crate::types::{Scope, ToolClass};
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn sign_verify_and_attenuate() {
        let mut rng = OsRng;
        let issuer = SigningKey::generate(&mut rng);
        let subject = SigningKey::generate(&mut rng);
        let parent_body = CapabilitySignBody {
            version: 1,
            cap_id: [1u8; 32],
            chain_id: 41,
            issuer: issuer.verifying_key().to_bytes(),
            subject: subject.verifying_key().to_bytes(),
            parent_cap_id: None,
            scope: Scope {
                workspace_id: None,
                project_id: None,
                task_id: None,
                tool_class_mask: ToolClass::all_phase1_mask(),
                providers: None,
            },
            caveats: vec![Caveat::MaxTotalSpend(100)],
            budget_account: 1,
            not_before: 0,
            not_after: 1_000_000,
            nonce: 1,
        };
        let parent = CapabilityToken::sign(parent_body, &issuer).unwrap();
        parent.verify().unwrap();

        let child_body = CapabilitySignBody {
            version: 1,
            cap_id: [2u8; 32],
            chain_id: 41,
            issuer: issuer.verifying_key().to_bytes(),
            subject: subject.verifying_key().to_bytes(),
            parent_cap_id: Some(parent.body.cap_id),
            scope: Scope {
                workspace_id: Some(7),
                project_id: None,
                task_id: None,
                tool_class_mask: ToolClass::Browser.bit(),
                providers: None,
            },
            caveats: vec![Caveat::MaxTotalSpend(50)],
            budget_account: 1,
            not_before: 10,
            not_after: 500_000,
            nonce: 2,
        };
        assert!(CapabilityToken::verify_attenuation_from_parent(&child_body, &parent.body));
        let child = CapabilityToken::sign(child_body, &issuer).unwrap();
        child.verify().unwrap();
    }
}

/// Derive `cap_id = BLAKE3(root_secret || serial)` per §4.2 (caller chooses serial monotonicity).
pub fn derive_cap_id(root_secret: &[u8], serial: u64) -> CapabilityId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(root_secret);
    hasher.update(&serial.to_le_bytes());
    *hasher.finalize().as_bytes()
}
