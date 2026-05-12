//! Tool market: intent → quote → match → receipt → settle (`docs/wallet.md` §7, §9.3 W5).

use std::collections::HashMap;

use borsh::{BorshDeserialize, BorshSerialize};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use thiserror::Error;

use crate::budget::{BudgetAccount, BudgetError};
use crate::challenge::{AdjudicationDecision, Challenge};
use crate::types::{
    Amount, IntentId, PublicKey, QuoteId, TaskId, ToolClass, VerificationTier,
};

pub type ProviderId = crate::types::ProviderId;

/// Default ~256 × 500 ms when blocks are 500 ms (`docs/wallet.md` §9.3).
pub const DEFAULT_OPTIMISTIC_CHALLENGE_MS: u64 = 256 * 500;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ToolIntentBody {
    pub intent_id: IntentId,
    pub agent_session: PublicKey,
    pub task_id: TaskId,
    pub tool_class: ToolClass,
    pub payload_commitment: [u8; 32],
    pub max_price: Amount,
    pub verification_tier: VerificationTier,
    pub deadline_ms: u64,
    pub nonce: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ToolIntent {
    pub body: ToolIntentBody,
    pub signature: [u8; 64],
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SigVerifyError {
    #[error("invalid public key")]
    BadKey,
    #[error("invalid signature")]
    BadSig,
    #[error("encode error")]
    Encode,
}

impl ToolIntent {
    pub fn sign(body: ToolIntentBody, sk: &SigningKey) -> Result<Self, std::io::Error> {
        let msg = borsh::to_vec(&body)?;
        let sig = sk.sign(&msg);
        Ok(Self {
            body,
            signature: sig.to_bytes(),
        })
    }

    pub fn verify(&self) -> Result<(), SigVerifyError> {
        let vk = VerifyingKey::from_bytes(&self.body.agent_session).map_err(|_| SigVerifyError::BadKey)?;
        let sig = Signature::from_bytes(&self.signature);
        let msg = borsh::to_vec(&self.body).map_err(|_| SigVerifyError::Encode)?;
        vk.verify(&msg, &sig).map_err(|_| SigVerifyError::BadSig)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct QuoteBody {
    pub quote_id: QuoteId,
    pub intent_id: IntentId,
    pub provider_id: ProviderId,
    pub price: Amount,
    pub expiry_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Quote {
    pub body: QuoteBody,
    pub signature: [u8; 64],
}

impl Quote {
    pub fn sign(body: QuoteBody, provider_sk: &SigningKey) -> Result<Self, std::io::Error> {
        let msg = borsh::to_vec(&body)?;
        let sig = provider_sk.sign(&msg);
        Ok(Self {
            body,
            signature: sig.to_bytes(),
        })
    }

    pub fn verify(&self, provider_pk: &PublicKey) -> Result<(), SigVerifyError> {
        let vk = VerifyingKey::from_bytes(provider_pk).map_err(|_| SigVerifyError::BadKey)?;
        let sig = Signature::from_bytes(&self.signature);
        let msg = borsh::to_vec(&self.body).map_err(|_| SigVerifyError::Encode)?;
        vk.verify(&msg, &sig).map_err(|_| SigVerifyError::BadSig)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveredInfo {
    pub quote_id: QuoteId,
    pub provider: ProviderId,
    pub reserved: Amount,
    pub stake_locked: Amount,
    pub output_commitment: [u8; 32],
    pub delivered_at_ms: u64,
    pub challenge_deadline_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntentState {
    Proposed,
    Matched {
        quote_id: QuoteId,
        provider: ProviderId,
        reserved: Amount,
        stake_locked: Amount,
    },
    Delivered(DeliveredInfo),
    Disputed {
        info: DeliveredInfo,
        challenge: Challenge,
    },
    /// Provider paid from reservation after challenge window or winning adjudication.
    SettledPaid,
    /// Reservation returned to budget after challenger wins.
    SettledRefunded,
}

pub struct IntentRecord {
    pub intent: ToolIntent,
    pub state: IntentState,
}

#[derive(Clone, Debug, Default)]
pub struct ProviderStake {
    pub amount: Amount,
    pub locked: Amount,
}

impl ProviderStake {
    pub fn lock(&mut self, x: Amount) -> Result<(), MatchError> {
        let avail = self.amount.saturating_sub(self.locked);
        if x > avail {
            return Err(MatchError::InsufficientStake);
        }
        self.locked = self.locked.saturating_add(x);
        Ok(())
    }

    pub fn unlock(&mut self, x: Amount) {
        self.locked = self.locked.saturating_sub(x);
    }
}

pub struct ToolMarket {
    pub intents: HashMap<IntentId, IntentRecord>,
    pub slash_multiplier: u64,
}

impl Default for ToolMarket {
    fn default() -> Self {
        Self {
            intents: HashMap::new(),
            slash_multiplier: 2,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PostIntentError {
    #[error("intent id collision")]
    Duplicate,
    #[error("signature invalid")]
    BadSig,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MatchError {
    #[error("intent not found")]
    NotFound,
    #[error("bad state for match")]
    BadState,
    #[error("quote intent mismatch")]
    IntentMismatch,
    #[error("price above max")]
    PriceTooHigh,
    #[error("insufficient provider stake")]
    InsufficientStake,
    #[error("budget reserve failed")]
    Budget(#[from] BudgetError),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PostReceiptError {
    #[error("intent not found")]
    NotFound,
    #[error("bad state for receipt")]
    BadState,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SettleError {
    #[error("intent not found")]
    NotFound,
    #[error("bad state for settle")]
    BadState,
    #[error("challenge window still open")]
    ChallengeWindowOpen,
    #[error("budget settle failed")]
    Budget,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChallengeError {
    #[error("intent not found")]
    NotFound,
    #[error("bad state for challenge")]
    BadState,
    #[error("challenge window closed")]
    WindowClosed,
    #[error("challenge intent mismatch")]
    IntentMismatch,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResolveError {
    #[error("intent not found")]
    NotFound,
    #[error("not disputed")]
    NotDisputed,
    #[error("budget operation failed")]
    Budget,
}

impl ToolMarket {
    pub fn post_intent(&mut self, intent: ToolIntent) -> Result<(), PostIntentError> {
        intent.verify().map_err(|_| PostIntentError::BadSig)?;
        let id = intent.body.intent_id;
        if self.intents.contains_key(&id) {
            return Err(PostIntentError::Duplicate);
        }
        self.intents.insert(
            id,
            IntentRecord {
                intent,
                state: IntentState::Proposed,
            },
        );
        Ok(())
    }

    pub fn match_intent(
        &mut self,
        intent_id: IntentId,
        quote: &Quote,
        budget: &mut BudgetAccount,
        stake: &mut ProviderStake,
        provider_pk: &PublicKey,
        now_ms: u64,
    ) -> Result<(), MatchError> {
        let rec = self.intents.get_mut(&intent_id).ok_or(MatchError::NotFound)?;
        if !matches!(rec.state, IntentState::Proposed) {
            return Err(MatchError::BadState);
        }
        if quote.body.intent_id != intent_id {
            return Err(MatchError::IntentMismatch);
        }
        quote.verify(provider_pk).map_err(|_| MatchError::BadState)?;
        if quote.body.price > rec.intent.body.max_price {
            return Err(MatchError::PriceTooHigh);
        }
        if now_ms > quote.body.expiry_ms {
            return Err(MatchError::BadState);
        }
        let lock = quote
            .body
            .price
            .saturating_mul(self.slash_multiplier as u128);
        stake.lock(lock)?;
        budget.reserve(rec.intent.body.tool_class, quote.body.price)?;
        rec.state = IntentState::Matched {
            quote_id: quote.body.quote_id,
            provider: quote.body.provider_id,
            reserved: quote.body.price,
            stake_locked: lock,
        };
        Ok(())
    }

    pub fn post_receipt(
        &mut self,
        intent_id: IntentId,
        output_commitment: [u8; 32],
        now_ms: u64,
    ) -> Result<(), PostReceiptError> {
        let rec = self.intents.get_mut(&intent_id).ok_or(PostReceiptError::NotFound)?;
        let (quote_id, provider, reserved, stake_locked) = match &rec.state {
            IntentState::Matched {
                quote_id,
                provider,
                reserved,
                stake_locked,
            } => (*quote_id, *provider, *reserved, *stake_locked),
            _ => return Err(PostReceiptError::BadState),
        };
        let challenge_deadline_ms = match rec.intent.body.verification_tier {
            VerificationTier::Trusted => now_ms,
            VerificationTier::Optimistic => now_ms.saturating_add(DEFAULT_OPTIMISTIC_CHALLENGE_MS),
            _ => now_ms.saturating_add(DEFAULT_OPTIMISTIC_CHALLENGE_MS),
        };
        rec.state = IntentState::Delivered(DeliveredInfo {
            quote_id,
            provider,
            reserved,
            stake_locked,
            output_commitment,
            delivered_at_ms: now_ms,
            challenge_deadline_ms,
        });
        Ok(())
    }

    /// Settle after `challenge_deadline_ms` (Trusted: deadline == delivery time).
    pub fn settle_after_window(
        &mut self,
        intent_id: IntentId,
        now_ms: u64,
        budget: &mut BudgetAccount,
        stake: &mut ProviderStake,
    ) -> Result<(), SettleError> {
        let rec = self.intents.get_mut(&intent_id).ok_or(SettleError::NotFound)?;
        let info = match &rec.state {
            IntentState::Delivered(i) => i.clone(),
            _ => return Err(SettleError::BadState),
        };
        if now_ms < info.challenge_deadline_ms {
            return Err(SettleError::ChallengeWindowOpen);
        }
        budget.settle(info.reserved).map_err(|_| SettleError::Budget)?;
        stake.unlock(info.stake_locked);
        rec.state = IntentState::SettledPaid;
        Ok(())
    }

    /// During the challenge window, a bonded party may dispute (`docs/wallet.md` §9.3).
    pub fn submit_challenge(
        &mut self,
        intent_id: IntentId,
        now_ms: u64,
        challenge: Challenge,
    ) -> Result<(), ChallengeError> {
        if challenge.intent_id != intent_id {
            return Err(ChallengeError::IntentMismatch);
        }
        let rec = self.intents.get_mut(&intent_id).ok_or(ChallengeError::NotFound)?;
        let info = match &rec.state {
            IntentState::Delivered(i) => i.clone(),
            _ => return Err(ChallengeError::BadState),
        };
        if now_ms >= info.challenge_deadline_ms {
            return Err(ChallengeError::WindowClosed);
        }
        rec.state = IntentState::Disputed {
            info,
            challenge,
        };
        Ok(())
    }

    pub fn resolve_challenge(
        &mut self,
        intent_id: IntentId,
        decision: AdjudicationDecision,
        budget: &mut BudgetAccount,
        stake: &mut ProviderStake,
    ) -> Result<(), ResolveError> {
        let rec = self.intents.get_mut(&intent_id).ok_or(ResolveError::NotFound)?;
        let (info, _challenge) = match &rec.state {
            IntentState::Disputed { info, challenge } => (info.clone(), challenge.clone()),
            _ => return Err(ResolveError::NotDisputed),
        };
        match decision {
            AdjudicationDecision::ProviderWins => {
                budget.settle(info.reserved).map_err(|_| ResolveError::Budget)?;
                stake.unlock(info.stake_locked);
                rec.state = IntentState::SettledPaid;
            }
            AdjudicationDecision::ChallengerWins => {
                budget.refund(info.reserved).map_err(|_| ResolveError::Budget)?;
                stake.unlock(info.stake_locked);
                rec.state = IntentState::SettledRefunded;
            }
        }
        Ok(())
    }

    /// Back-compat: settle trusted path (deadline already satisfied).
    pub fn settle_trusted(
        &mut self,
        intent_id: IntentId,
        budget: &mut BudgetAccount,
        stake: &mut ProviderStake,
    ) -> Result<(), SettleError> {
        let now_ms = match &self.intents.get(&intent_id).ok_or(SettleError::NotFound)?.state {
            IntentState::Delivered(i) => i.challenge_deadline_ms,
            _ => return Err(SettleError::BadState),
        };
        self.settle_after_window(intent_id, now_ms, budget, stake)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::challenge::{Challenge, ChallengeKind};
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn sample_intent(tier: VerificationTier) -> (SigningKey, ToolIntent) {
        let mut rng = OsRng;
        let agent = SigningKey::generate(&mut rng);
        let body = ToolIntentBody {
            intent_id: [1u8; 32],
            agent_session: agent.verifying_key().to_bytes(),
            task_id: 42,
            tool_class: ToolClass::Browser,
            payload_commitment: [2u8; 32],
            max_price: 100,
            verification_tier: tier,
            deadline_ms: 9_999_999,
            nonce: 0,
        };
        let intent = ToolIntent::sign(body, &agent).unwrap();
        (agent, intent)
    }

    #[test]
    fn trusted_flow_settles_immediately_after_receipt() {
        let mut rng = OsRng;
        let prov = SigningKey::generate(&mut rng);
        let provider_pk = prov.verifying_key().to_bytes();
        let provider_id = *blake3::hash(&provider_pk).as_bytes();

        let (_agent, intent) = sample_intent(VerificationTier::Trusted);
        let mut market = ToolMarket::default();
        market.post_intent(intent).unwrap();

        let quote = Quote::sign(
            QuoteBody {
                quote_id: [3u8; 32],
                intent_id: [1u8; 32],
                provider_id,
                price: 80,
                expiry_ms: 9_999_999,
            },
            &prov,
        )
        .unwrap();

        let mut budget = BudgetAccount::new(1, None, 1000);
        let mut stake = ProviderStake {
            amount: 1000,
            locked: 0,
        };

        market
            .match_intent(
                [1u8; 32],
                &quote,
                &mut budget,
                &mut stake,
                &provider_pk,
                0u64,
            )
            .unwrap();
        market.post_receipt([1u8; 32], [4u8; 32], 1000).unwrap();
        market
            .settle_after_window([1u8; 32], 1000, &mut budget, &mut stake)
            .unwrap();
        assert_eq!(budget.spent, 80);
        assert_eq!(stake.locked, 0);
    }

    #[test]
    fn optimistic_cannot_settle_before_deadline() {
        let mut rng = OsRng;
        let prov = SigningKey::generate(&mut rng);
        let provider_pk = prov.verifying_key().to_bytes();
        let provider_id = *blake3::hash(&provider_pk).as_bytes();

        let (_agent, intent) = sample_intent(VerificationTier::Optimistic);
        let mut market = ToolMarket::default();
        market.post_intent(intent).unwrap();
        let quote = Quote::sign(
            QuoteBody {
                quote_id: [3u8; 32],
                intent_id: [1u8; 32],
                provider_id,
                price: 80,
                expiry_ms: 9_999_999,
            },
            &prov,
        )
        .unwrap();
        let mut budget = BudgetAccount::new(1, None, 1000);
        let mut stake = ProviderStake {
            amount: 1000,
            locked: 0,
        };
        market
            .match_intent(
                [1u8; 32],
                &quote,
                &mut budget,
                &mut stake,
                &provider_pk,
                0u64,
            )
            .unwrap();
        market.post_receipt([1u8; 32], [4u8; 32], 0).unwrap();
        assert_eq!(
            market.settle_after_window([1u8; 32], 1000, &mut budget, &mut stake),
            Err(SettleError::ChallengeWindowOpen)
        );
        let deadline = DEFAULT_OPTIMISTIC_CHALLENGE_MS;
        market
            .settle_after_window([1u8; 32], deadline, &mut budget, &mut stake)
            .unwrap();
        assert_eq!(budget.spent, 80);
    }

    #[test]
    fn challenge_then_challenger_wins_refunds() {
        let mut rng = OsRng;
        let prov = SigningKey::generate(&mut rng);
        let challenger = SigningKey::generate(&mut rng);
        let provider_pk = prov.verifying_key().to_bytes();
        let provider_id = *blake3::hash(&provider_pk).as_bytes();

        let (_agent, intent) = sample_intent(VerificationTier::Optimistic);
        let mut market = ToolMarket::default();
        market.post_intent(intent).unwrap();
        let quote = Quote::sign(
            QuoteBody {
                quote_id: [3u8; 32],
                intent_id: [1u8; 32],
                provider_id,
                price: 80,
                expiry_ms: 9_999_999,
            },
            &prov,
        )
        .unwrap();
        let mut budget = BudgetAccount::new(1, None, 1000);
        let mut stake = ProviderStake {
            amount: 1000,
            locked: 0,
        };
        market
            .match_intent(
                [1u8; 32],
                &quote,
                &mut budget,
                &mut stake,
                &provider_pk,
                0u64,
            )
            .unwrap();
        market.post_receipt([1u8; 32], [4u8; 32], 0).unwrap();
        let ch = Challenge {
            challenge_id: 1,
            intent_id: [1u8; 32],
            challenger: challenger.verifying_key().to_bytes(),
            kind: ChallengeKind::WrongOutput,
            evidence_hash: [9u8; 32],
            bond: 10,
        };
        market.submit_challenge([1u8; 32], 100, ch).unwrap();
        market
            .resolve_challenge(
                [1u8; 32],
                AdjudicationDecision::ChallengerWins,
                &mut budget,
                &mut stake,
            )
            .unwrap();
        assert_eq!(budget.spent, 0);
        assert_eq!(budget.reserved, 0);
        assert_eq!(stake.locked, 0);
    }
}
