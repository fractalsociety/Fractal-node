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

/// Permissionless validator enrollment (`RegisterValidator`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorRegistryEntry {
    pub operator: crate::Address,
    pub bls_pubkey: [u8; 48],
}
