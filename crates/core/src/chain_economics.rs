//! PRD §12 / Phase 3: testnet vs mainnet economics (`docs/prd.md` §12.1–12.4, §7).

use borsh::{BorshDeserialize, BorshSerialize};
use serde::Deserialize;
use std::{fs, path::Path};

/// Smallest FRAC unit (18 decimals), matching wallet `policy::FRAC`.
pub const WEI_PER_FRAC: u128 = 1_000_000_000_000_000_000;
pub const MAX_SUPPLY_WEI: u128 = 23_000_000 * WEI_PER_FRAC;

/// PRD §12.1 Phase 2 — permissioned testnet minimum bond.
pub const TESTNET_MIN_VALIDATOR_STAKE_WEI: u128 = 1_000_000 * WEI_PER_FRAC;

/// PRD §12.1 Phase 3 / mainnet permissionless threshold.
pub const MAINNET_MIN_VALIDATOR_STAKE_WEI: u128 = 5_000_000 * WEI_PER_FRAC;

/// PRD §12.4 testnet unbonding (7 days).
pub const TESTNET_UNBONDING_PERIOD_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// PRD §12.4 mainnet unbonding (21 days).
pub const MAINNET_UNBONDING_PERIOD_MS: u64 = 21 * 24 * 60 * 60 * 1000;

/// On-chain economics profile (`State.chain_economics`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ChainEconomicsParams {
    pub version: u8,
    pub min_validator_stake_wei: u128,
    pub unbonding_period_ms: u64,
    /// When true, [`crate::NativeCall::RegisterValidator`] may enroll fingerprints that meet the stake minimum.
    pub permissionless_validator_entry: bool,
    /// When true, [`crate::finalize_block_hooks`] destroys `base_fee * evm_gas_used` (PRD §12.2 mainnet burn).
    pub evm_base_fee_burn: bool,
    pub phase_config: ProtocolPhaseConfig,
    pub prover_rewards: ProverRewardParams,
    pub sequencer_rewards: SequencerRewardParams,
    pub emission: EmissionParams,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EmissionParams {
    pub enabled: bool,
    pub total_pool_wei: u128,
    pub quarter_count: u64,
    pub decay_bps: u32,
    pub blocks_per_quarter: u64,
    pub provider_pool_bps: u16,
    pub consensus_pool_bps: u16,
    pub intelligence_pool_bps: u16,
    pub provider_storage_bps: u16,
    pub provider_compute_bps: u16,
    pub provider_tx_count_bps: u16,
    pub intelligence_frontier_bps: u16,
    pub intelligence_accessible_bps: u16,
    pub intelligence_efficient_bps: u16,
    pub lens_frontier_bps: u16,
    pub lens_accessible_bps: u16,
    pub lens_efficient_bps: u16,
    pub royalty_hops_bps: Vec<u16>,
    pub min_effect_bps: u16,
    pub vesting_epochs: u64,
    pub provenance_bond_wei: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmissionBlock {
    pub quarter: u64,
    pub total_wei: u128,
    pub provider_pool_wei: u128,
    pub consensus_pool_wei: u128,
    pub intelligence_pool_wei: u128,
}

impl EmissionParams {
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            total_pool_wei: 0,
            quarter_count: 0,
            decay_bps: 0,
            blocks_per_quarter: 0,
            provider_pool_bps: 0,
            consensus_pool_bps: 0,
            intelligence_pool_bps: 0,
            provider_storage_bps: 0,
            provider_compute_bps: 0,
            provider_tx_count_bps: 0,
            intelligence_frontier_bps: 0,
            intelligence_accessible_bps: 0,
            intelligence_efficient_bps: 0,
            lens_frontier_bps: 0,
            lens_accessible_bps: 0,
            lens_efficient_bps: 0,
            royalty_hops_bps: Vec::new(),
            min_effect_bps: 0,
            vesting_epochs: 0,
            provenance_bond_wei: 0,
        }
    }

    #[must_use]
    pub fn fractal_emission_v3() -> Self {
        Self {
            enabled: true,
            total_pool_wei: MAX_SUPPLY_WEI,
            quarter_count: 40,
            decay_bps: 9_400,
            // Devnet/simulator default: one minute blocks, 90-day quarters.
            blocks_per_quarter: 90 * 24 * 60,
            provider_pool_bps: 5_500,
            consensus_pool_bps: 2_000,
            intelligence_pool_bps: 2_500,
            provider_storage_bps: 4_000,
            provider_compute_bps: 4_000,
            provider_tx_count_bps: 2_000,
            intelligence_frontier_bps: 4_000,
            intelligence_accessible_bps: 4_000,
            intelligence_efficient_bps: 2_000,
            lens_frontier_bps: 4_000,
            lens_accessible_bps: 3_000,
            lens_efficient_bps: 3_000,
            royalty_hops_bps: vec![1_000, 100, 10],
            min_effect_bps: 50,
            vesting_epochs: 168,
            provenance_bond_wei: 50 * WEI_PER_FRAC,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.enabled {
            return Ok(());
        }
        if self.total_pool_wei == 0 {
            return Err("emission total pool must be non-zero");
        }
        if self.quarter_count == 0 || self.blocks_per_quarter == 0 {
            return Err("emission quarter and block counts must be non-zero");
        }
        if self.decay_bps > 10_000 {
            return Err("emission decay bps must be <= 10000");
        }
        require_bps_sum(
            self.provider_pool_bps,
            self.consensus_pool_bps,
            self.intelligence_pool_bps,
            "emission pool split",
        )?;
        require_bps_sum(
            self.provider_storage_bps,
            self.provider_compute_bps,
            self.provider_tx_count_bps,
            "provider work split",
        )?;
        require_bps_sum(
            self.intelligence_frontier_bps,
            self.intelligence_accessible_bps,
            self.intelligence_efficient_bps,
            "intelligence split",
        )?;
        require_bps_sum(
            self.lens_frontier_bps,
            self.lens_accessible_bps,
            self.lens_efficient_bps,
            "lens split",
        )?;
        Ok(())
    }
}

fn require_bps_sum(a: u16, b: u16, c: u16, name: &'static str) -> Result<(), &'static str> {
    if u32::from(a) + u32::from(b) + u32::from(c) != 10_000 {
        return Err(name);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProtocolPhaseConfig {
    pub owned_object_certificates: bool,
    pub da_sampling: bool,
    pub proof_final_settlement: bool,
    pub execution_zones: bool,
    pub forced_inclusion: bool,
    pub prover_rewards: bool,
    pub sequencer_rewards: bool,
}

impl ProtocolPhaseConfig {
    #[must_use]
    pub fn testnet() -> Self {
        Self {
            owned_object_certificates: true,
            da_sampling: true,
            proof_final_settlement: false,
            execution_zones: false,
            forced_inclusion: false,
            prover_rewards: false,
            sequencer_rewards: false,
        }
    }

    #[must_use]
    pub fn mainnet() -> Self {
        Self {
            owned_object_certificates: true,
            da_sampling: true,
            proof_final_settlement: true,
            execution_zones: true,
            forced_inclusion: true,
            prover_rewards: true,
            sequencer_rewards: true,
        }
    }
}

impl Default for ProtocolPhaseConfig {
    fn default() -> Self {
        Self::testnet()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProverRewardParams {
    pub enabled: bool,
    pub treasury: crate::Address,
    pub base_reward_per_block_wei: u128,
    pub lag_half_life_seconds: u32,
}

impl ProverRewardParams {
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            treasury: [0u8; 20],
            base_reward_per_block_wei: 0,
            lag_half_life_seconds: 1,
        }
    }

    #[must_use]
    pub fn mainnet(treasury: crate::Address) -> Self {
        Self {
            enabled: true,
            treasury,
            base_reward_per_block_wei: WEI_PER_FRAC / 100,
            lag_half_life_seconds: 60,
        }
    }
}

impl Default for ProverRewardParams {
    fn default() -> Self {
        Self::disabled()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SequencerRewardParams {
    pub enabled: bool,
    pub treasury: crate::Address,
    pub base_reward_per_zone_block_wei: u128,
    pub da_byte_reward_wei: u128,
    pub forced_inclusion_penalty_wei: u128,
    pub late_forced_inclusion_penalty_per_block_wei: u128,
}

impl SequencerRewardParams {
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            treasury: [0u8; 20],
            base_reward_per_zone_block_wei: 0,
            da_byte_reward_wei: 0,
            forced_inclusion_penalty_wei: 0,
            late_forced_inclusion_penalty_per_block_wei: 0,
        }
    }

    #[must_use]
    pub fn mainnet(treasury: crate::Address) -> Self {
        Self {
            enabled: true,
            treasury,
            base_reward_per_zone_block_wei: WEI_PER_FRAC / 1_000,
            da_byte_reward_wei: 1_000_000_000,
            forced_inclusion_penalty_wei: WEI_PER_FRAC / 10,
            late_forced_inclusion_penalty_per_block_wei: WEI_PER_FRAC / 100,
        }
    }
}

impl Default for SequencerRewardParams {
    fn default() -> Self {
        Self::disabled()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProverWorkReceipt {
    pub covered_blocks: u64,
    pub lag_seconds: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SequencerWorkReceipt {
    pub zone_blocks: u64,
    pub da_bytes: u64,
}

impl Default for ChainEconomicsParams {
    fn default() -> Self {
        Self::testnet()
    }
}

impl ChainEconomicsParams {
    pub const VERSION: u8 = 1;

    #[must_use]
    pub fn testnet() -> Self {
        Self {
            version: Self::VERSION,
            min_validator_stake_wei: TESTNET_MIN_VALIDATOR_STAKE_WEI,
            unbonding_period_ms: TESTNET_UNBONDING_PERIOD_MS,
            permissionless_validator_entry: false,
            evm_base_fee_burn: false,
            phase_config: ProtocolPhaseConfig::testnet(),
            prover_rewards: ProverRewardParams::disabled(),
            sequencer_rewards: SequencerRewardParams::disabled(),
            emission: EmissionParams::disabled(),
        }
    }

    #[must_use]
    pub fn mainnet() -> Self {
        Self {
            version: Self::VERSION,
            min_validator_stake_wei: MAINNET_MIN_VALIDATOR_STAKE_WEI,
            unbonding_period_ms: MAINNET_UNBONDING_PERIOD_MS,
            permissionless_validator_entry: true,
            evm_base_fee_burn: true,
            phase_config: ProtocolPhaseConfig::mainnet(),
            prover_rewards: ProverRewardParams::mainnet([0u8; 20]),
            sequencer_rewards: SequencerRewardParams::mainnet([0u8; 20]),
            emission: EmissionParams::fractal_emission_v3(),
        }
    }

    #[must_use]
    pub fn from_profile_name(profile: &str) -> Self {
        match profile.trim().to_ascii_lowercase().as_str() {
            "mainnet" => Self::mainnet(),
            _ => Self::testnet(),
        }
    }
}

#[must_use]
pub fn quarter_of(height: u64, params: &EmissionParams) -> Option<u64> {
    if !params.enabled || params.blocks_per_quarter == 0 {
        return None;
    }
    let q = height / params.blocks_per_quarter;
    (q < params.quarter_count).then_some(q)
}

#[must_use]
pub fn emission_for_block(height: u64, params: &EmissionParams) -> EmissionBlock {
    let Some(quarter) = quarter_of(height, params) else {
        return EmissionBlock {
            quarter: params.quarter_count,
            total_wei: 0,
            provider_pool_wei: 0,
            consensus_pool_wei: 0,
            intelligence_pool_wei: 0,
        };
    };
    let budget = emission_budget_for_quarter(params, quarter);
    let offset = height % params.blocks_per_quarter;
    let base = budget / u128::from(params.blocks_per_quarter);
    let remainder = budget % u128::from(params.blocks_per_quarter);
    let total_wei = base + u128::from(offset).lt(&remainder) as u128;
    split_emission_block(quarter, total_wei, params)
}

fn split_emission_block(quarter: u64, total_wei: u128, params: &EmissionParams) -> EmissionBlock {
    let provider_pool_wei = total_wei.saturating_mul(u128::from(params.provider_pool_bps)) / 10_000;
    let consensus_pool_wei =
        total_wei.saturating_mul(u128::from(params.consensus_pool_bps)) / 10_000;
    let intelligence_pool_wei = total_wei
        .saturating_sub(provider_pool_wei)
        .saturating_sub(consensus_pool_wei);
    EmissionBlock {
        quarter,
        total_wei,
        provider_pool_wei,
        consensus_pool_wei,
        intelligence_pool_wei,
    }
}

#[must_use]
pub fn emission_budget_for_quarter(params: &EmissionParams, quarter: u64) -> u128 {
    if !params.enabled || quarter >= params.quarter_count {
        return 0;
    }
    let weights = decay_weights(params);
    let denominator: u128 = weights.iter().sum();
    if denominator == 0 {
        return 0;
    }
    let allocated_before: u128 = weights
        .iter()
        .take(quarter as usize)
        .map(|w| params.total_pool_wei.saturating_mul(*w) / denominator)
        .sum();
    if quarter + 1 == params.quarter_count {
        return params.total_pool_wei.saturating_sub(allocated_before);
    }
    params
        .total_pool_wei
        .saturating_mul(weights[quarter as usize])
        / denominator
}

#[must_use]
pub fn projected_emission_total(params: &EmissionParams) -> u128 {
    (0..params.quarter_count)
        .map(|q| emission_budget_for_quarter(params, q))
        .sum()
}

fn decay_weights(params: &EmissionParams) -> Vec<u128> {
    let mut weights = Vec::with_capacity(params.quarter_count as usize);
    let mut weight = 1_000_000_000_000_000_000_u128;
    for _ in 0..params.quarter_count {
        weights.push(weight);
        weight = weight.saturating_mul(u128::from(params.decay_bps)) / 10_000;
    }
    weights
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LifeGenesisParamsJson {
    emission: Option<EmissionParamsJson>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmissionParamsJson {
    total_pool_frac: Option<u128>,
    total_pool_wei: Option<u128>,
    quarter_count: u64,
    decay_bps: u32,
    blocks_per_quarter: u64,
    splits_bps: PoolSplitJson,
    provider_work_weights_bps: PoolSplitJson,
    intelligence_weights_bps: PoolSplitJson,
    lens_split_bps: PoolSplitJson,
    royalty_hops_bps: Vec<u16>,
    min_effect_bps: u16,
    vesting_epochs: u64,
    provenance_bond_frac: Option<u128>,
    provenance_bond_wei: Option<u128>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PoolSplitJson {
    provider: Option<u16>,
    consensus: Option<u16>,
    intelligence: Option<u16>,
    storage: Option<u16>,
    compute: Option<u16>,
    tx_count: Option<u16>,
    frontier: Option<u16>,
    accessible: Option<u16>,
    efficient: Option<u16>,
}

pub fn load_emission_params_from_life_genesis(
    path: impl AsRef<Path>,
) -> Result<EmissionParams, Box<dyn std::error::Error + Send + Sync>> {
    let raw = fs::read_to_string(path)?;
    let parsed: LifeGenesisParamsJson = serde_json::from_str(&raw)?;
    let Some(emission) = parsed.emission else {
        return Ok(EmissionParams::disabled());
    };
    let params = EmissionParams {
        enabled: true,
        total_pool_wei: emission.total_pool_wei.unwrap_or_else(|| {
            emission
                .total_pool_frac
                .unwrap_or(0)
                .saturating_mul(WEI_PER_FRAC)
        }),
        quarter_count: emission.quarter_count,
        decay_bps: emission.decay_bps,
        blocks_per_quarter: emission.blocks_per_quarter,
        provider_pool_bps: emission.splits_bps.provider.unwrap_or(0),
        consensus_pool_bps: emission.splits_bps.consensus.unwrap_or(0),
        intelligence_pool_bps: emission.splits_bps.intelligence.unwrap_or(0),
        provider_storage_bps: emission.provider_work_weights_bps.storage.unwrap_or(0),
        provider_compute_bps: emission.provider_work_weights_bps.compute.unwrap_or(0),
        provider_tx_count_bps: emission.provider_work_weights_bps.tx_count.unwrap_or(0),
        intelligence_frontier_bps: emission.intelligence_weights_bps.frontier.unwrap_or(0),
        intelligence_accessible_bps: emission.intelligence_weights_bps.accessible.unwrap_or(0),
        intelligence_efficient_bps: emission.intelligence_weights_bps.efficient.unwrap_or(0),
        lens_frontier_bps: emission.lens_split_bps.frontier.unwrap_or(0),
        lens_accessible_bps: emission.lens_split_bps.accessible.unwrap_or(0),
        lens_efficient_bps: emission.lens_split_bps.efficient.unwrap_or(0),
        royalty_hops_bps: emission.royalty_hops_bps,
        min_effect_bps: emission.min_effect_bps,
        vesting_epochs: emission.vesting_epochs,
        provenance_bond_wei: emission.provenance_bond_wei.unwrap_or_else(|| {
            emission
                .provenance_bond_frac
                .unwrap_or(0)
                .saturating_mul(WEI_PER_FRAC)
        }),
    };
    params.validate()?;
    Ok(params)
}

#[must_use]
pub fn prover_reward_wei(params: &ProverRewardParams, work: ProverWorkReceipt) -> u128 {
    if !params.enabled || params.base_reward_per_block_wei == 0 || work.covered_blocks == 0 {
        return 0;
    }
    let range_reward = params
        .base_reward_per_block_wei
        .saturating_mul(u128::from(work.covered_blocks));
    let half_life = u128::from(params.lag_half_life_seconds.max(1));
    let denominator = u128::from(work.lag_seconds).saturating_add(half_life);
    range_reward.saturating_mul(half_life) / denominator
}

#[must_use]
pub fn sequencer_reward_wei(params: &SequencerRewardParams, work: SequencerWorkReceipt) -> u128 {
    if !params.enabled {
        return 0;
    }
    params
        .base_reward_per_zone_block_wei
        .saturating_mul(u128::from(work.zone_blocks))
        .saturating_add(
            params
                .da_byte_reward_wei
                .saturating_mul(u128::from(work.da_bytes)),
        )
}

#[must_use]
pub fn forced_inclusion_penalty_wei(params: &SequencerRewardParams, late_by_blocks: u64) -> u128 {
    if !params.enabled {
        return 0;
    }
    params.forced_inclusion_penalty_wei.saturating_add(
        params
            .late_forced_inclusion_penalty_per_block_wei
            .saturating_mul(u128::from(late_by_blocks)),
    )
}

/// Permissionless validator enrollment (`RegisterValidator`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorRegistryEntry {
    pub operator: crate::Address,
    pub bls_pubkey: [u8; 48],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prover_reward_scales_with_range_and_decays_with_lag() {
        let params = ProverRewardParams {
            enabled: true,
            treasury: [1u8; 20],
            base_reward_per_block_wei: 100,
            lag_half_life_seconds: 10,
        };

        assert_eq!(
            prover_reward_wei(
                &params,
                ProverWorkReceipt {
                    covered_blocks: 5,
                    lag_seconds: 0,
                },
            ),
            500
        );
        assert_eq!(
            prover_reward_wei(
                &params,
                ProverWorkReceipt {
                    covered_blocks: 5,
                    lag_seconds: 10,
                },
            ),
            250
        );
        assert_eq!(
            prover_reward_wei(
                &ProverRewardParams::disabled(),
                ProverWorkReceipt {
                    covered_blocks: 5,
                    lag_seconds: 0,
                }
            ),
            0
        );
    }

    #[test]
    fn sequencer_reward_and_forced_inclusion_penalty_are_configured() {
        let params = SequencerRewardParams {
            enabled: true,
            treasury: [2u8; 20],
            base_reward_per_zone_block_wei: 100,
            da_byte_reward_wei: 2,
            forced_inclusion_penalty_wei: 1_000,
            late_forced_inclusion_penalty_per_block_wei: 50,
        };

        assert_eq!(
            sequencer_reward_wei(
                &params,
                SequencerWorkReceipt {
                    zone_blocks: 3,
                    da_bytes: 20,
                },
            ),
            340
        );
        assert_eq!(forced_inclusion_penalty_wei(&params, 4), 1_200);
        assert_eq!(
            forced_inclusion_penalty_wei(&SequencerRewardParams::disabled(), 4),
            0
        );
    }

    #[test]
    fn phase_profiles_gate_rollout_features() {
        let testnet = ChainEconomicsParams::testnet();
        assert!(testnet.phase_config.owned_object_certificates);
        assert!(testnet.phase_config.da_sampling);
        assert!(!testnet.phase_config.proof_final_settlement);
        assert!(!testnet.prover_rewards.enabled);

        let mainnet = ChainEconomicsParams::mainnet();
        assert!(mainnet.phase_config.proof_final_settlement);
        assert!(mainnet.phase_config.execution_zones);
        assert!(mainnet.phase_config.forced_inclusion);
        assert!(mainnet.prover_rewards.enabled);
        assert!(mainnet.sequencer_rewards.enabled);
    }

    #[test]
    fn fractal_emission_v3_projects_exact_pool_and_stops_after_decade() {
        let params = EmissionParams {
            blocks_per_quarter: 10,
            ..EmissionParams::fractal_emission_v3()
        };
        params.validate().unwrap();

        assert_eq!(projected_emission_total(&params), MAX_SUPPLY_WEI);
        assert_eq!(
            (0..params.quarter_count * params.blocks_per_quarter)
                .map(|h| emission_for_block(h, &params).total_wei)
                .sum::<u128>(),
            MAX_SUPPLY_WEI
        );
        assert_eq!(
            emission_for_block(params.quarter_count * params.blocks_per_quarter, &params).total_wei,
            0
        );
    }

    #[test]
    fn emission_block_splits_and_quarter_boundaries_are_deterministic() {
        let params = EmissionParams {
            total_pool_wei: 1_000_000,
            quarter_count: 2,
            decay_bps: 9_000,
            blocks_per_quarter: 5,
            ..EmissionParams::fractal_emission_v3()
        };

        assert_eq!(quarter_of(0, &params), Some(0));
        assert_eq!(quarter_of(4, &params), Some(0));
        assert_eq!(quarter_of(5, &params), Some(1));
        assert_eq!(quarter_of(10, &params), None);
        assert_eq!(
            emission_budget_for_quarter(&params, 0) + emission_budget_for_quarter(&params, 1),
            1_000_000
        );
        let block = emission_for_block(0, &params);
        assert_eq!(block.provider_pool_wei, block.total_wei * 55 / 100);
        assert_eq!(block.consensus_pool_wei, block.total_wei * 20 / 100);
        assert_eq!(
            block.provider_pool_wei + block.consensus_pool_wei + block.intelligence_pool_wei,
            block.total_wei
        );
    }

    #[test]
    fn loader_accepts_master_life_genesis_emission_section_when_present() {
        let path = Path::new("/Users/jamesstar/fractalmaster/life-genesis-params.json");
        if path.exists() {
            let params = load_emission_params_from_life_genesis(path).unwrap();
            if params.enabled {
                params.validate().unwrap();
                assert_eq!(params.total_pool_wei, MAX_SUPPLY_WEI);
            }
        }
    }
}
