//! Shared identifiers and enums (`docs/wallet.md` §4, §8).

use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeSet;

pub type Amount = u128;
pub type TimestampMs = u64;
pub type PublicKey = [u8; 32];
pub type WorkspaceId = u64;
pub type TaskId = u64;
pub type ProviderId = [u8; 32];
pub type IntentId = [u8; 32];
pub type QuoteId = [u8; 32];
pub type ReceiptId = [u8; 32];

/// Phase 1 tool classes (§25.1).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize,
)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ToolClass {
    Browser = 0,
    LlmInference = 1,
    TestRunner = 2,
    FileStorage = 3,
}

impl ToolClass {
    pub const COUNT: usize = 4;

    pub fn bit(self) -> u64 {
        1u64 << (self as u8 as u32)
    }

    pub fn from_bit(bit: u32) -> Option<Self> {
        match bit {
            0 => Some(Self::Browser),
            1 => Some(Self::LlmInference),
            2 => Some(Self::TestRunner),
            3 => Some(Self::FileStorage),
            _ => None,
        }
    }

    pub fn all_phase1_mask() -> u64 {
        Self::Browser.bit()
            | Self::LlmInference.bit()
            | Self::TestRunner.bit()
            | Self::FileStorage.bit()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum VerificationTier {
    Trusted = 0,
    Optimistic = 1,
    Attested = 2,
    Replicated = 3,
    Proven = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum TeeType {
    IntelTdx = 0,
    AmdSevSnp = 1,
    AwsNitro = 2,
}

/// `Scope` from §4.3 — `None` means `ANY` for workspace/project/task; tool mask must be non-zero
/// for autonomous agents per spec (enforced in `capability::verify`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Scope {
    pub workspace_id: Option<WorkspaceId>,
    pub project_id: Option<u64>,
    pub task_id: Option<TaskId>,
    pub tool_class_mask: u64,
    pub providers: Option<BTreeSet<ProviderId>>,
}

impl Scope {
    pub fn is_subset_of(&self, parent: &Scope) -> bool {
        if !mask_subset(self.tool_class_mask, parent.tool_class_mask) {
            return false;
        }
        if !opt_id_subset(self.workspace_id, parent.workspace_id) {
            return false;
        }
        if !opt_id_subset(self.project_id, parent.project_id) {
            return false;
        }
        if !opt_id_subset(self.task_id, parent.task_id) {
            return false;
        }
        providers_subset(&self.providers, &parent.providers)
    }
}

fn mask_subset(child: u64, parent: u64) -> bool {
    (child & parent) == child
}

fn opt_id_subset(child: Option<u64>, parent: Option<u64>) -> bool {
    match (parent, child) {
        (None, _) => true, // parent ANY
        (Some(_), None) => false, // parent specific, child ANY — too broad
        (Some(p), Some(c)) => c == p,
    }
}

fn providers_subset(
    child: &Option<BTreeSet<ProviderId>>,
    parent: &Option<BTreeSet<ProviderId>>,
) -> bool {
    match (parent, child) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(ps), Some(cs)) => cs.is_subset(ps),
    }
}