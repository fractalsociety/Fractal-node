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

/// Tool classes (`docs/wallet.md` §8.1 v2.0 catalog).
///
/// **Borsh discriminants 0–3** are the Phase 1 launch set (`docs/wallet.md` §25.1) and must remain
/// stable. Additional §8.1 classes use 4..=13; discriminant equals the bit index in
/// [`Self::bit`] / [`Scope::tool_class_mask`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize,
)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ToolClass {
    /// `BROWSER`
    Browser = 0,
    /// `LLM_INFERENCE`
    LlmInference = 1,
    /// `TEST_RUNNER`
    TestRunner = 2,
    /// `FILE_STORAGE`
    FileStorage = 3,
    /// `WEB_SCRAPE`
    WebScrape = 4,
    /// `GITHUB_READ`
    GithubRead = 5,
    /// `GITHUB_WRITE`
    GithubWrite = 6,
    /// `EMBEDDING`
    Embedding = 7,
    /// `GPU_JOB`
    GpuJob = 8,
    /// `DATABASE_QUERY`
    DatabaseQuery = 9,
    /// `EMAIL_SEND`
    EmailSend = 10,
    /// `VECTOR_SEARCH`
    VectorSearch = 11,
    /// `OCR`
    Ocr = 12,
    /// `CODE_EXECUTION`
    CodeExecution = 13,
}

impl ToolClass {
    pub const COUNT: usize = 14;

    /// Every variant in ascending discriminant order (for UIs / `policy tool-classes`).
    pub const VARIANTS: [Self; Self::COUNT] = [
        Self::Browser,
        Self::LlmInference,
        Self::TestRunner,
        Self::FileStorage,
        Self::WebScrape,
        Self::GithubRead,
        Self::GithubWrite,
        Self::Embedding,
        Self::GpuJob,
        Self::DatabaseQuery,
        Self::EmailSend,
        Self::VectorSearch,
        Self::Ocr,
        Self::CodeExecution,
    ];

    pub fn bit(self) -> u64 {
        1u64 << (self as u8 as u32)
    }

    pub fn from_bit(bit: u32) -> Option<Self> {
        match bit {
            0 => Some(Self::Browser),
            1 => Some(Self::LlmInference),
            2 => Some(Self::TestRunner),
            3 => Some(Self::FileStorage),
            4 => Some(Self::WebScrape),
            5 => Some(Self::GithubRead),
            6 => Some(Self::GithubWrite),
            7 => Some(Self::Embedding),
            8 => Some(Self::GpuJob),
            9 => Some(Self::DatabaseQuery),
            10 => Some(Self::EmailSend),
            11 => Some(Self::VectorSearch),
            12 => Some(Self::Ocr),
            13 => Some(Self::CodeExecution),
            _ => None,
        }
    }

    pub fn from_discriminant(d: u8) -> Option<Self> {
        Self::from_bit(d as u32)
    }

    /// Phase 1 (`docs/wallet.md` §25.1): the four shipped launch classes only.
    pub fn all_phase1_mask() -> u64 {
        Self::Browser.bit()
            | Self::LlmInference.bit()
            | Self::TestRunner.bit()
            | Self::FileStorage.bit()
    }

    /// Phase 2 hardening slice (`docs/wallet.md` §25.2): GitHub + DB + sandboxed code.
    pub fn phase2_tool_class_mask() -> u64 {
        Self::GithubRead.bit()
            | Self::GithubWrite.bit()
            | Self::DatabaseQuery.bit()
            | Self::CodeExecution.bit()
    }

    /// All §8.1 classes (bits `0..COUNT`).
    pub fn all_v2_catalog_mask() -> u64 {
        (1u64 << Self::COUNT as u32) - 1
    }

    /// Uppercase name from the §8.1 table (`BROWSER`, `LLM_INFERENCE`, …).
    pub fn spec_name(self) -> &'static str {
        match self {
            Self::Browser => "BROWSER",
            Self::LlmInference => "LLM_INFERENCE",
            Self::TestRunner => "TEST_RUNNER",
            Self::FileStorage => "FILE_STORAGE",
            Self::WebScrape => "WEB_SCRAPE",
            Self::GithubRead => "GITHUB_READ",
            Self::GithubWrite => "GITHUB_WRITE",
            Self::Embedding => "EMBEDDING",
            Self::GpuJob => "GPU_JOB",
            Self::DatabaseQuery => "DATABASE_QUERY",
            Self::EmailSend => "EMAIL_SEND",
            Self::VectorSearch => "VECTOR_SEARCH",
            Self::Ocr => "OCR",
            Self::CodeExecution => "CODE_EXECUTION",
        }
    }

    /// Verbatim **Pricing** column from `docs/wallet.md` §8.1 (operator reference).
    pub fn spec_pricing_notes(self) -> &'static str {
        match self {
            Self::Browser => "Per request",
            Self::WebScrape => "Per page",
            Self::GithubRead => "Per call",
            Self::GithubWrite => "Per call",
            Self::LlmInference => "Per token (committed)",
            Self::Embedding => "Per token",
            Self::GpuJob => "Per second / per job",
            Self::DatabaseQuery => "Per query",
            Self::EmailSend => "Per message",
            Self::TestRunner => "Per run",
            Self::FileStorage => "Per MB-day",
            Self::VectorSearch => "Per query",
            Self::Ocr => "Per page",
            Self::CodeExecution => "Per second",
        }
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

impl ToolClass {
    /// Default verification posture for quotes / policy hints (`docs/wallet.md` §8.1–§8.2).
    ///
    /// Operators can still tighten tiers via caveats (e.g. `TeeAttestationRequired`).
    pub fn default_verification_tier(self) -> VerificationTier {
        match self {
            Self::Browser | Self::WebScrape | Self::LlmInference | Self::Embedding
            | Self::FileStorage | Self::VectorSearch | Self::Ocr | Self::TestRunner => {
                VerificationTier::Optimistic
            }
            Self::GithubRead => VerificationTier::Trusted,
            Self::GithubWrite | Self::GpuJob | Self::DatabaseQuery | Self::EmailSend
            | Self::CodeExecution => VerificationTier::Attested,
        }
    }
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

#[cfg(test)]
mod tool_class_tests {
    use super::{ToolClass, VerificationTier};

    #[test]
    fn phase1_mask_unchanged() {
        assert_eq!(ToolClass::all_phase1_mask(), 0xf);
    }

    #[test]
    fn catalog_masks() {
        assert_eq!(ToolClass::all_v2_catalog_mask(), 0x3fff);
        assert_eq!(
            ToolClass::phase2_tool_class_mask(),
            ToolClass::GithubRead.bit()
                | ToolClass::GithubWrite.bit()
                | ToolClass::DatabaseQuery.bit()
                | ToolClass::CodeExecution.bit()
        );
    }

    #[test]
    fn from_bit_roundtrip_all_variants() {
        for c in ToolClass::VARIANTS {
            let b = c as u8 as u32;
            assert_eq!(ToolClass::from_bit(b), Some(c));
            assert_eq!(ToolClass::from_discriminant(c as u8), Some(c));
        }
        assert!(ToolClass::from_bit(14).is_none());
    }

    #[test]
    fn borsh_discriminants_stable_for_phase1() {
        assert_eq!(borsh::to_vec(&ToolClass::Browser).unwrap(), vec![0]);
        assert_eq!(borsh::to_vec(&ToolClass::LlmInference).unwrap(), vec![1]);
        assert_eq!(borsh::to_vec(&ToolClass::TestRunner).unwrap(), vec![2]);
        assert_eq!(borsh::to_vec(&ToolClass::FileStorage).unwrap(), vec![3]);
    }

    #[test]
    fn borsh_newest_variant() {
        let v = ToolClass::CodeExecution;
        assert_eq!(borsh::to_vec(&v).unwrap(), vec![13]);
        assert_eq!(
            borsh::from_slice::<ToolClass>(&[13]).unwrap(),
            ToolClass::CodeExecution
        );
    }

    #[test]
    fn default_tier_examples() {
        assert_eq!(
            ToolClass::GithubRead.default_verification_tier(),
            VerificationTier::Trusted
        );
        assert_eq!(
            ToolClass::GithubWrite.default_verification_tier(),
            VerificationTier::Attested
        );
        assert_eq!(
            ToolClass::Browser.default_verification_tier(),
            VerificationTier::Optimistic
        );
    }
}