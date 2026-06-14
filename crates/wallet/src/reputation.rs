//! Provider reputation as **derived** state (`docs/wallet.md` §10.4).
//!
//! Scores are computed from summarized settlement history (typically filled by an indexer
//! or native `core::reputation` later). They are **not** directly writable — only earned via
//! settled work and lost via failures / slashing. Parameters are integer‑scaled for
//! deterministic, governance‑tunable behavior.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::market::Quote;
use crate::types::{Amount, ProviderId, ToolClass};

/// Sub‑point score: **effective value = `milli / 1000`** (e.g. `1500` → 1.5).
pub type ReputationMilli = u128;

/// One successful **paid** settlement contributing to reputation (§10.4: weighted by recency).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SettlementEvent {
    pub settled_at_ms: u64,
    /// Relative weight (job size, FRAC notoriety, etc.). `0` is treated as `1`.
    pub weight: u128,
}

/// Aggregated inputs an indexer (or chain module) supplies for `score(provider, class)`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ReputationLedgerSummary {
    pub tool_class: ToolClass,
    pub successful: Vec<SettlementEvent>,
    pub failed_settlements: u64,
    pub slashing_events: u64,
    /// First time this provider was seen (registration or first quote), for age bonus.
    pub first_seen_ms: u64,
    pub now_ms: u64,
    /// Skin‑in‑the‑game weight (§10.4): typically `ProviderStake.available` or total stake.
    pub available_stake: Amount,
    /// Distinct agent sessions / client keys that received successful paid work (anti‑collusion).
    pub distinct_client_count: u32,
}

/// Integer weights for the §10.4 formula; all governance‑tunable.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ReputationParams {
    /// Milli‑points per unit weighted success **before** recency scaling.
    pub success_weight_milli: u64,
    /// Half‑life for recency decay: factor `half_life / (half_life + age_ms)` (0 = no decay).
    pub recency_half_life_ms: u64,
    pub fail_penalty_milli: u64,
    pub slash_penalty_milli: u64,
    /// Milli‑points added per **day** of provider age since `first_seen_ms`.
    pub age_bonus_per_day_milli: u64,
    /// Milli‑points per `log2(available_stake + 1)` unit (bounded stake signal).
    pub stake_log_weight_milli: u64,
    /// Milli‑points per distinct paying client (capped by `max_diversity_clients`).
    pub diversity_bonus_milli: u64,
    pub max_diversity_clients: u32,
}

impl Default for ReputationParams {
    fn default() -> Self {
        Self {
            success_weight_milli: 1_000,
            recency_half_life_ms: 86_400_000 * 30,
            fail_penalty_milli: 5_000,
            slash_penalty_milli: 500_000,
            age_bonus_per_day_milli: 50,
            stake_log_weight_milli: 200,
            diversity_bonus_milli: 2_000,
            max_diversity_clients: 64,
        }
    }
}

/// Bootstrap stake multiplier when `score ≈ 0` (§10.4: larger multipliers until history exists).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapStakeParams {
    /// Multiplier at exactly **zero** score, in **basis points** (10_000 = 1×).
    pub zero_score_stake_bps: u64,
    /// As `score_milli` approaches this, multiplier approaches `1×` (10_000 bps).
    pub score_milli_full_trust: u128,
}

impl Default for BootstrapStakeParams {
    fn default() -> Self {
        Self {
            zero_score_stake_bps: 30_000,
            score_milli_full_trust: 500_000,
        }
    }
}

/// §7.4 auto‑selection preference (off‑chain wallet policy).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum QuoteSelectionPreference {
    #[default]
    /// Lowest `price` among candidates passing gates.
    Cheapest,
    /// Lowest `estimated_latency_ms`.
    Fastest,
    /// Highest `reputation_milli`, tie‑break cheaper.
    MostReputable,
}

/// One quote plus off‑chain estimates used for selection (does not change `Quote` wire encoding).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuoteCandidate {
    pub quote: Quote,
    pub provider_id: ProviderId,
    pub tool_class: ToolClass,
    pub reputation_milli: ReputationMilli,
    pub available_stake: Amount,
    pub estimated_latency_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuoteSelectionGates {
    /// Minimum `reputation_milli × stake / 1000` (§7.4 `(reputation × stake)` style gate).
    pub min_reputation_stake_product: u128,
    pub max_latency_ms: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum QuoteSelectionError {
    NoCandidates,
}

/// Recency factor in **micro‑units** (1.0 = 1_000_000): `half_life / (half_life + dt)`.
#[must_use]
pub fn recency_factor_micro(settled_at_ms: u64, now_ms: u64, half_life_ms: u64) -> u128 {
    if half_life_ms == 0 {
        return 1_000_000;
    }
    let dt = now_ms.saturating_sub(settled_at_ms);
    let hl = half_life_ms as u128;
    let d = hl.saturating_add(dt as u128);
    if d == 0 {
        return 1_000_000;
    }
    1_000_000u128.saturating_mul(hl) / d.max(1)
}

#[must_use]
fn ilog2_u128(x: u128) -> u32 {
    if x <= 1 {
        return 0;
    }
    128 - x.leading_zeros() - 1
}

/// §10.4 style score in **milli‑points** (≥ 0, saturating).
#[must_use]
pub fn compute_reputation_score_milli(
    summary: &ReputationLedgerSummary,
    p: &ReputationParams,
) -> ReputationMilli {
    let _ = summary.tool_class;
    let mut pos: u128 = 0;

    let sw = p.success_weight_milli as u128;
    for ev in &summary.successful {
        let w = if ev.weight == 0 { 1u128 } else { ev.weight };
        let r = recency_factor_micro(ev.settled_at_ms, summary.now_ms, p.recency_half_life_ms);
        // (sw * w * r) / 1e6 / 1e3  — sw is milli per unit weight at full recency; r is micro
        let term = sw.saturating_mul(w).saturating_mul(r) / 1_000_000 / 1000;
        pos = pos.saturating_add(term);
    }

    let days = summary.now_ms.saturating_sub(summary.first_seen_ms) / 86_400_000;
    pos =
        pos.saturating_add((p.age_bonus_per_day_milli as u128).saturating_mul(days as u128) / 1000);

    let lg = ilog2_u128(summary.available_stake.saturating_add(1));
    pos = pos.saturating_add((p.stake_log_weight_milli as u128).saturating_mul(lg as u128) / 1000);

    let dc = summary.distinct_client_count.min(p.max_diversity_clients) as u128;
    pos = pos.saturating_add((p.diversity_bonus_milli as u128).saturating_mul(dc) / 1000);

    let fail_pen =
        (p.fail_penalty_milli as u128).saturating_mul(summary.failed_settlements as u128) / 1000;
    let slash_pen =
        (p.slash_penalty_milli as u128).saturating_mul(summary.slashing_events as u128) / 1000;

    pos.saturating_sub(fail_pen).saturating_sub(slash_pen)
}

/// Extra stake multiplier (bps) for providers with little history (§10.4 bootstrap).
#[must_use]
pub fn bootstrap_stake_multiplier_bps(
    score_milli: ReputationMilli,
    p: &BootstrapStakeParams,
) -> u64 {
    if p.score_milli_full_trust == 0 {
        return 10_000;
    }
    let cap = p.zero_score_stake_bps.max(10_000);
    if score_milli >= p.score_milli_full_trust {
        return 10_000;
    }
    // Linear interpolate from zero_score at 0 → 10_000 at full_trust
    let num = (cap - 10_000) as u128 * (p.score_milli_full_trust.saturating_sub(score_milli));
    let den = p.score_milli_full_trust.max(1);
    let extra = (num / den) as u64;
    10_000u64.saturating_add(extra)
}

/// §7.4 style `reputation × stake` with milli reputation (result in arbitrary scale; use only for comparison).
#[must_use]
pub fn reputation_stake_product(reputation_milli: ReputationMilli, stake: Amount) -> u128 {
    reputation_milli.saturating_mul(stake) / 1000
}

/// Filter by gates, then pick by `preference`. Tie‑break: lower `quote.body.price`, then lower latency.
#[must_use]
pub fn select_quote<'a>(
    candidates: &'a [QuoteCandidate],
    gates: &QuoteSelectionGates,
    preference: QuoteSelectionPreference,
) -> Result<&'a QuoteCandidate, QuoteSelectionError> {
    let mut best: Option<(usize, u128, u128, u128)> = None;
    for (i, c) in candidates.iter().enumerate() {
        if c.estimated_latency_ms > gates.max_latency_ms {
            continue;
        }
        let prod = reputation_stake_product(c.reputation_milli, c.available_stake);
        if prod < gates.min_reputation_stake_product {
            continue;
        }
        let price = c.quote.body.price;
        let rank = match preference {
            QuoteSelectionPreference::Cheapest => u128::MAX.saturating_sub(price),
            QuoteSelectionPreference::Fastest => {
                u128::MAX.saturating_sub(c.estimated_latency_ms as u128)
            }
            QuoteSelectionPreference::MostReputable => c.reputation_milli,
        };
        let tie1 = u128::MAX.saturating_sub(price);
        let tie2 = u128::MAX.saturating_sub(c.estimated_latency_ms as u128);
        let better = match best {
            None => true,
            Some((_, r0, t10, t20)) => (rank, tie1, tie2) > (r0, t10, t20),
        };
        if better {
            best = Some((i, rank, tie1, tie2));
        }
    }
    let idx = best.map(|b| b.0).ok_or(QuoteSelectionError::NoCandidates)?;
    Ok(&candidates[idx])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::QuoteBody;
    use ed25519_dalek::SigningKey;

    fn dummy_quote(price: u128, latency: u64, rep: u128, stake: u128) -> QuoteCandidate {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let pk = sk.verifying_key().to_bytes();
        let pid = crate::market::provider_id_from_public_key(&pk);
        let body = QuoteBody {
            quote_id: [1u8; 32],
            intent_id: [2u8; 32],
            provider_id: pid,
            price,
            expiry_ms: u64::MAX,
        };
        let quote = Quote::sign(body, &sk).expect("sign");
        QuoteCandidate {
            quote,
            provider_id: pid,
            tool_class: ToolClass::Browser,
            reputation_milli: rep,
            available_stake: stake,
            estimated_latency_ms: latency,
        }
    }

    #[test]
    fn new_provider_score_zero_without_events() {
        let p = ReputationParams::default();
        let s = ReputationLedgerSummary {
            tool_class: ToolClass::Browser,
            successful: vec![],
            failed_settlements: 0,
            slashing_events: 0,
            first_seen_ms: 1_000,
            now_ms: 1_000,
            available_stake: 0,
            distinct_client_count: 0,
        };
        assert_eq!(compute_reputation_score_milli(&s, &p), 0);
        let b = BootstrapStakeParams::default();
        assert_eq!(bootstrap_stake_multiplier_bps(0, &b), 30_000);
    }

    #[test]
    fn successes_increase_score_recency_decays() {
        let p = ReputationParams {
            success_weight_milli: 10_000,
            recency_half_life_ms: 86_400_000,
            ..Default::default()
        };
        let now = 10_000_000u64;
        let old = now.saturating_sub(10 * 86_400_000);
        let s = ReputationLedgerSummary {
            tool_class: ToolClass::LlmInference,
            successful: vec![
                SettlementEvent {
                    settled_at_ms: now,
                    weight: 1,
                },
                SettlementEvent {
                    settled_at_ms: old,
                    weight: 1,
                },
            ],
            failed_settlements: 0,
            slashing_events: 0,
            first_seen_ms: 0,
            now_ms: now,
            available_stake: 0,
            distinct_client_count: 0,
        };
        let score = compute_reputation_score_milli(&s, &p);
        assert!(score > 0);
        // Recent event should weigh more than old at same base weight
        let r_new = recency_factor_micro(now, now, p.recency_half_life_ms);
        let r_old = recency_factor_micro(old, now, p.recency_half_life_ms);
        assert!(r_new > r_old);
    }

    #[test]
    fn failures_and_slashes_reduce_score() {
        let p = ReputationParams::default();
        let base = ReputationLedgerSummary {
            tool_class: ToolClass::Browser,
            successful: vec![SettlementEvent {
                settled_at_ms: 100,
                weight: 10,
            }],
            failed_settlements: 0,
            slashing_events: 0,
            first_seen_ms: 0,
            now_ms: 200,
            available_stake: 1_000_000,
            distinct_client_count: 2,
        };
        let with_fail = ReputationLedgerSummary {
            failed_settlements: 3,
            ..base.clone()
        };
        assert!(
            compute_reputation_score_milli(&with_fail, &p)
                < compute_reputation_score_milli(&base, &p)
        );
        let with_slash = ReputationLedgerSummary {
            slashing_events: 1,
            ..base
        };
        assert_eq!(compute_reputation_score_milli(&with_slash, &p), 0);
    }

    #[test]
    fn reputation_ledger_summary_borsh_round_trip() {
        let s = ReputationLedgerSummary {
            tool_class: ToolClass::GithubWrite,
            successful: vec![SettlementEvent {
                settled_at_ms: 123,
                weight: 99,
            }],
            failed_settlements: 1,
            slashing_events: 0,
            first_seen_ms: 10,
            now_ms: 500,
            available_stake: 1_000,
            distinct_client_count: 3,
        };
        let v = borsh::to_vec(&s).unwrap();
        let back: ReputationLedgerSummary = borsh::from_slice(&v).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn select_quote_prefers_cheapest_when_gates_pass() {
        let a = dummy_quote(100, 50, 500_000, 1_000);
        let b = dummy_quote(80, 100, 400_000, 1_000);
        let candidates = [a, b];
        let gates = QuoteSelectionGates {
            min_reputation_stake_product: 0,
            max_latency_ms: 200,
        };
        let got = select_quote(&candidates, &gates, QuoteSelectionPreference::Cheapest).unwrap();
        assert_eq!(got.quote.body.price, 80);
    }

    #[test]
    fn select_quote_respects_reputation_stake_gate() {
        let low = dummy_quote(10, 10, 1, 100);
        let high = dummy_quote(50, 10, 10_000, 1_000);
        let candidates = [low, high];
        let gates = QuoteSelectionGates {
            min_reputation_stake_product: 5_000,
            max_latency_ms: 100,
        };
        let got = select_quote(&candidates, &gates, QuoteSelectionPreference::Cheapest).unwrap();
        assert_eq!(got.quote.body.price, 50);
    }

    #[test]
    fn select_quote_errors_when_none_pass() {
        let c = dummy_quote(1, 10_000, 1, 1);
        let candidates = [c];
        let gates = QuoteSelectionGates {
            min_reputation_stake_product: u128::MAX,
            max_latency_ms: 1,
        };
        assert_eq!(
            select_quote(&candidates, &gates, QuoteSelectionPreference::Cheapest),
            Err(QuoteSelectionError::NoCandidates)
        );
    }
}
