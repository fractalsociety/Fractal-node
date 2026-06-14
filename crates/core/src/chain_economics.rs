//! PRD §12 / Phase 3: testnet vs mainnet economics (`docs/prd.md` §12.1–12.4, §7).

use borsh::{BorshDeserialize, BorshSerialize};

/// Smallest FRAC unit (18 decimals), matching wallet `policy::FRAC`.
pub const WEI_PER_FRAC: u128 = 1_000_000_000_000_000_000;

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
}
