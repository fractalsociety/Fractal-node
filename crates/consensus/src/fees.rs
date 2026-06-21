//! Fee policy categories for proof-ingestion accounting.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeeCostCategory {
    DaBytes,
    ProofVerify,
    SharedStateExecution,
}

impl FeeCostCategory {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DaBytes => "da_bytes",
            Self::ProofVerify => "proof_verify",
            Self::SharedStateExecution => "shared_state_execution",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeePolicyV1 {
    pub da_fee_per_byte: u128,
    pub proof_verify_base_fee: u128,
    pub proof_verify_fee_per_byte: u128,
    pub shared_state_gas_price: u128,
}

impl FeePolicyV1 {
    #[must_use]
    pub const fn cost_categories() -> [FeeCostCategory; 3] {
        [
            FeeCostCategory::DaBytes,
            FeeCostCategory::ProofVerify,
            FeeCostCategory::SharedStateExecution,
        ]
    }

    #[must_use]
    pub fn da_bytes_fee(&self, encoded_bytes: u64) -> u128 {
        u128::from(encoded_bytes).saturating_mul(self.da_fee_per_byte)
    }

    #[must_use]
    pub fn da_gas_fee(&self, da_gas_used: u64) -> u128 {
        self.da_bytes_fee(da_gas_used)
    }

    #[must_use]
    pub fn proof_verify_fee(&self, proof_bytes: usize) -> u128 {
        self.proof_verify_base_fee
            .saturating_add((proof_bytes as u128).saturating_mul(self.proof_verify_fee_per_byte))
    }

    #[must_use]
    pub fn shared_state_execution_fee(&self, execution_gas: u64) -> u128 {
        u128::from(execution_gas).saturating_mul(self.shared_state_gas_price)
    }
}

impl Default for FeePolicyV1 {
    fn default() -> Self {
        Self {
            da_fee_per_byte: crate::DEFAULT_DA_FEE_PER_GAS,
            proof_verify_base_fee: 10_000,
            proof_verify_fee_per_byte: 1,
            shared_state_gas_price: 1,
        }
    }
}

#[must_use]
pub fn default_fee_policy() -> FeePolicyV1 {
    FeePolicyV1::default()
}
