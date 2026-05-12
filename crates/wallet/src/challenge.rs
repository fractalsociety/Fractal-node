//! Challenge / adjudication types (`docs/wallet.md` §9.3).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::types::{Amount, IntentId, PublicKey};

pub type ChallengeId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ChallengeKind {
    NotExecuted = 0,
    WrongOutput = 1,
    Overcharged = 2,
    Unattested = 3,
}

/// Evidence is opaque bytes at this layer (DA / structured packets in full node).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Challenge {
    pub challenge_id: ChallengeId,
    pub intent_id: IntentId,
    pub challenger: PublicKey,
    pub kind: ChallengeKind,
    pub evidence_hash: [u8; 32],
    pub bond: Amount,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum AdjudicationDecision {
    ProviderWins = 0,
    ChallengerWins = 1,
}
