//! Keeps [`crate::reputation::ReputationLedgerSummary`] in sync with [`crate::market::ToolMarket`]
//! settlement transitions (`docs/wallet.md` §10.4 + §7 tool market).

use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap};

use crate::budget::BudgetAccount;
use crate::challenge::{AdjudicationDecision, Challenge};
use crate::market::{
    MatchError, PostIntentError, PostReceiptError, ProviderStake, Quote, ResolveError, SettleError,
    ToolIntent, ToolMarket, ChallengeError,
};
use crate::reputation::{
    compute_reputation_score_milli, ReputationLedgerSummary, ReputationParams, SettlementEvent,
};
use crate::types::{Amount, IntentId, ProviderId, PublicKey, ToolClass};

fn client_tag(agent_session: &PublicKey) -> u32 {
    u32::from_le_bytes(agent_session[0..4].try_into().expect("4 bytes"))
}

#[derive(Clone, Debug, Default)]
struct TrackerCore {
    ledgers: HashMap<(ProviderId, ToolClass), ReputationLedgerSummary>,
    clients: HashMap<(ProviderId, ToolClass), BTreeSet<u32>>,
    stakes: HashMap<ProviderId, Amount>,
}

impl TrackerCore {
    fn ledger_mut(&mut self, provider: ProviderId, class: ToolClass, now_ms: u64) -> &mut ReputationLedgerSummary {
        let k = (provider, class);
        let stake = self.stakes.get(&provider).copied().unwrap_or(0);
        let le = match self.ledgers.entry(k) {
            Entry::Vacant(e) => e.insert(ReputationLedgerSummary {
                tool_class: class,
                successful: Vec::new(),
                failed_settlements: 0,
                slashing_events: 0,
                first_seen_ms: now_ms,
                now_ms,
                available_stake: stake,
                distinct_client_count: 0,
            }),
            Entry::Occupied(e) => e.into_mut(),
        };
        le.now_ms = now_ms;
        le.available_stake = stake;
        le
    }

    fn refresh_stake_fields(&mut self, provider: ProviderId) {
        let s = self.stakes.get(&provider).copied().unwrap_or(0);
        for ((pid, _), le) in self.ledgers.iter_mut() {
            if *pid == provider {
                le.available_stake = s;
            }
        }
    }

    fn recompute_diversity(&mut self, provider: ProviderId, class: ToolClass) {
        let k = (provider, class);
        let n = self.clients.get(&k).map(|c| c.len()).unwrap_or(0) as u32;
        if let Some(le) = self.ledgers.get_mut(&k) {
            le.distinct_client_count = n;
        }
    }

    fn touch_client(&mut self, provider: ProviderId, class: ToolClass, agent: &PublicKey) {
        let tag = client_tag(agent);
        let k = (provider, class);
        self.clients.entry(k).or_default().insert(tag);
        self.recompute_diversity(provider, class);
    }

    fn on_match(&mut self, provider: ProviderId, class: ToolClass, agent: &PublicKey, now_ms: u64) {
        {
            let le = self.ledger_mut(provider, class, now_ms);
            le.first_seen_ms = le.first_seen_ms.min(now_ms);
        }
        self.touch_client(provider, class, agent);
    }

    fn on_settled_paid(
        &mut self,
        provider: ProviderId,
        class: ToolClass,
        agent: &PublicKey,
        reserved: Amount,
        settled_at_ms: u64,
        now_ms: u64,
    ) {
        self.touch_client(provider, class, agent);
        let le = self.ledger_mut(provider, class, now_ms);
        let w = reserved.max(1);
        le.successful.push(SettlementEvent {
            settled_at_ms,
            weight: w,
        });
    }

    fn on_slash(&mut self, provider: ProviderId, class: ToolClass, now_ms: u64) {
        let le = self.ledger_mut(provider, class, now_ms);
        le.slashing_events = le.slashing_events.saturating_add(1);
    }
}

/// [`ToolMarket`] + automatic §10.4 ledger rows driven by settle / dispute outcomes.
pub struct ToolMarketWithReputation {
    pub market: ToolMarket,
    params: ReputationParams,
    rep: TrackerCore,
}

impl std::fmt::Debug for ToolMarketWithReputation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolMarketWithReputation")
            .field("intent_count", &self.market.intents.len())
            .field("slash_multiplier", &self.market.slash_multiplier)
            .field("params", &self.params)
            .field("ledger_entries", &self.rep.ledgers.len())
            .finish()
    }
}

impl Default for ToolMarketWithReputation {
    fn default() -> Self {
        Self {
            market: ToolMarket::default(),
            params: ReputationParams::default(),
            rep: TrackerCore::default(),
        }
    }
}

impl ToolMarketWithReputation {
    pub fn new(params: ReputationParams) -> Self {
        Self {
            market: ToolMarket::default(),
            params,
            rep: TrackerCore::default(),
        }
    }

    pub fn set_provider_stake_amount(&mut self, provider: ProviderId, available: Amount) {
        self.rep.stakes.insert(provider, available);
        self.rep.refresh_stake_fields(provider);
    }

    pub fn reputation_params(&self) -> &ReputationParams {
        &self.params
    }

    pub fn ledger_summary(
        &self,
        provider: ProviderId,
        class: ToolClass,
    ) -> Option<ReputationLedgerSummary> {
        self.rep.ledgers.get(&(provider, class)).cloned()
    }

    pub fn score_milli(&self, provider: ProviderId, class: ToolClass) -> Option<crate::reputation::ReputationMilli> {
        self.rep
            .ledgers
            .get(&(provider, class))
            .map(|s| compute_reputation_score_milli(s, &self.params))
    }

    /// Off-chain slash signal (on-chain slashing is separate); increments `slashing_events` for §10.4.
    pub fn record_provider_slash(&mut self, provider: ProviderId, class: ToolClass, now_ms: u64) {
        self.rep.on_slash(provider, class, now_ms);
    }

    pub fn post_intent(&mut self, intent: ToolIntent) -> Result<(), PostIntentError> {
        self.market.post_intent(intent)
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
        let (class, agent, provider) = {
            let rec = self.market.intents.get(&intent_id).ok_or(MatchError::NotFound)?;
            (
                rec.intent.body.tool_class,
                rec.intent.body.agent_session,
                quote.body.provider_id,
            )
        };
        self.market
            .match_intent(intent_id, quote, budget, stake, provider_pk, now_ms)?;
        self.rep.on_match(provider, class, &agent, now_ms);
        Ok(())
    }

    pub fn post_receipt(
        &mut self,
        intent_id: IntentId,
        output_commitment: [u8; 32],
        now_ms: u64,
    ) -> Result<(), PostReceiptError> {
        self.market.post_receipt(intent_id, output_commitment, now_ms)
    }

    pub fn settle_after_window(
        &mut self,
        intent_id: IntentId,
        now_ms: u64,
        budget: &mut BudgetAccount,
        stake: &mut ProviderStake,
    ) -> Result<(), SettleError> {
        let (class, agent, info) = {
            let snap = self.market.intents.get(&intent_id).ok_or(SettleError::NotFound)?;
            match &snap.state {
                crate::market::IntentState::Delivered(info) => (
                    snap.intent.body.tool_class,
                    snap.intent.body.agent_session,
                    info.clone(),
                ),
                _ => return Err(SettleError::BadState),
            }
        };
        self.market
            .settle_after_window(intent_id, now_ms, budget, stake)?;
        self.rep.on_settled_paid(
            info.provider,
            class,
            &agent,
            info.reserved,
            info.delivered_at_ms,
            now_ms,
        );
        Ok(())
    }

    pub fn submit_challenge(
        &mut self,
        intent_id: IntentId,
        now_ms: u64,
        challenge: Challenge,
    ) -> Result<(), ChallengeError> {
        self.market.submit_challenge(intent_id, now_ms, challenge)
    }

    pub fn resolve_challenge(
        &mut self,
        intent_id: IntentId,
        decision: AdjudicationDecision,
        budget: &mut BudgetAccount,
        stake: &mut ProviderStake,
        now_ms: u64,
    ) -> Result<(), ResolveError> {
        let (class, agent, info) = {
            let snap = self.market.intents.get(&intent_id).ok_or(ResolveError::NotFound)?;
            match &snap.state {
                crate::market::IntentState::Disputed { info, .. } => (
                    snap.intent.body.tool_class,
                    snap.intent.body.agent_session,
                    info.clone(),
                ),
                _ => return Err(ResolveError::NotDisputed),
            }
        };
        self.market
            .resolve_challenge(intent_id, decision, budget, stake)?;
        match decision {
            AdjudicationDecision::ProviderWins => {
                self.rep.on_settled_paid(
                    info.provider,
                    class,
                    &agent,
                    info.reserved,
                    info.delivered_at_ms,
                    now_ms,
                );
            }
            AdjudicationDecision::ChallengerWins => {
                // Adjudicated loss: stake bond is forfeited in [`ToolMarket`] (`burn_locked`); ledger
                // records a slash (not merely an unpaid settlement) for §10.4.
                self.rep.on_slash(info.provider, class, now_ms);
            }
        }
        Ok(())
    }

    pub fn settle_trusted(
        &mut self,
        intent_id: IntentId,
        budget: &mut BudgetAccount,
        stake: &mut ProviderStake,
    ) -> Result<(), SettleError> {
        let now_ms = match &self.market.intents.get(&intent_id).ok_or(SettleError::NotFound)?.state {
            crate::market::IntentState::Delivered(i) => i.challenge_deadline_ms,
            _ => return Err(SettleError::BadState),
        };
        self.settle_after_window(intent_id, now_ms, budget, stake)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::QuoteBody;
    use crate::types::VerificationTier;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn trusted_settle_updates_ledger_and_score() {
        let mut rng = OsRng;
        let agent = SigningKey::generate(&mut rng);
        let prov = SigningKey::generate(&mut rng);
        let pk = prov.verifying_key().to_bytes();
        let pid = crate::market::provider_id_from_public_key(&pk);

        let mut m = ToolMarketWithReputation::default();
        m.set_provider_stake_amount(pid, 1_000_000);

        let body = crate::market::ToolIntentBody {
            intent_id: [5u8; 32],
            agent_session: agent.verifying_key().to_bytes(),
            task_id: 1,
            tool_class: ToolClass::Browser,
            payload_commitment: [0u8; 32],
            max_price: 500,
            verification_tier: VerificationTier::Trusted,
            deadline_ms: 9_999_999,
            nonce: 0,
        };
        let intent = ToolIntent::sign(body, &agent).unwrap();
        m.post_intent(intent).unwrap();

        let quote = Quote::sign(
            QuoteBody {
                quote_id: [6u8; 32],
                intent_id: [5u8; 32],
                provider_id: pid,
                price: 100,
                expiry_ms: 9_999_999,
            },
            &prov,
        )
        .unwrap();

        let mut budget = crate::budget::BudgetAccount::new(1, None, 1000);
        let mut pstake = ProviderStake {
            amount: 2_000_000,
            locked: 0,
        };
        m.match_intent(
            [5u8; 32],
            &quote,
            &mut budget,
            &mut pstake,
            &pk,
            10_000,
        )
        .unwrap();
        m.post_receipt([5u8; 32], [7u8; 32], 10_000).unwrap();
        m.settle_after_window([5u8; 32], 10_000, &mut budget, &mut pstake)
            .unwrap();

        let led = m.ledger_summary(pid, ToolClass::Browser).expect("ledger");
        assert_eq!(led.successful.len(), 1);
        let sc = m.score_milli(pid, ToolClass::Browser).expect("score");
        assert!(sc > 0);
    }

    #[test]
    fn challenger_wins_burns_stake_and_records_slash_in_ledger() {
        let mut rng = OsRng;
        let agent = SigningKey::generate(&mut rng);
        let prov = SigningKey::generate(&mut rng);
        let challenger = SigningKey::generate(&mut rng);
        let pk = prov.verifying_key().to_bytes();
        let pid = crate::market::provider_id_from_public_key(&pk);

        let mut m = ToolMarketWithReputation::default();
        m.set_provider_stake_amount(pid, 1_000_000);

        let body = crate::market::ToolIntentBody {
            intent_id: [8u8; 32],
            agent_session: agent.verifying_key().to_bytes(),
            task_id: 1,
            tool_class: ToolClass::Browser,
            payload_commitment: [0u8; 32],
            max_price: 500,
            verification_tier: VerificationTier::Optimistic,
            deadline_ms: 9_999_999,
            nonce: 0,
        };
        let intent = ToolIntent::sign(body, &agent).unwrap();
        m.post_intent(intent).unwrap();

        let quote = Quote::sign(
            QuoteBody {
                quote_id: [9u8; 32],
                intent_id: [8u8; 32],
                provider_id: pid,
                price: 100,
                expiry_ms: 9_999_999,
            },
            &prov,
        )
        .unwrap();

        let mut budget = crate::budget::BudgetAccount::new(1, None, 1000);
        let mut pstake = ProviderStake {
            amount: 2_000_000,
            locked: 0,
        };
        m.match_intent(
            [8u8; 32],
            &quote,
            &mut budget,
            &mut pstake,
            &pk,
            10_000,
        )
        .unwrap();
        m.post_receipt([8u8; 32], [7u8; 32], 10_000).unwrap();
        let ch = crate::challenge::Challenge {
            challenge_id: 1,
            intent_id: [8u8; 32],
            challenger: challenger.verifying_key().to_bytes(),
            kind: crate::challenge::ChallengeKind::WrongOutput,
            evidence_hash: [1u8; 32],
            bond: 1,
        };
        m.submit_challenge([8u8; 32], 10_100, ch).unwrap();
        m.resolve_challenge(
            [8u8; 32],
            crate::challenge::AdjudicationDecision::ChallengerWins,
            &mut budget,
            &mut pstake,
            10_200,
        )
        .unwrap();

        let lock = 100u128 * m.market.slash_multiplier as u128;
        assert_eq!(pstake.amount, 2_000_000 - lock);
        assert_eq!(pstake.locked, 0);

        let led = m.ledger_summary(pid, ToolClass::Browser).expect("ledger");
        assert_eq!(led.slashing_events, 1);
        assert_eq!(led.failed_settlements, 0);
    }
}
