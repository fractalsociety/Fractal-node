//! On-chain policy templates with inheritance (`docs/wallet.md` §15).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;

use crate::caveat::Caveat;
use crate::types::{Amount, PublicKey, TeeType, ToolClass};

pub type TemplateId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SemVer {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BudgetSpec {
    pub total_cap: Amount,
    pub per_tool: BTreeMap<ToolClass, Amount>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RateLimitSpec {
    pub count: u32,
    pub window_seconds: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PolicyTemplate {
    pub template_id: TemplateId,
    pub version: SemVer,
    pub name: String,
    pub description: String,
    pub inherits: Option<TemplateId>,
    pub base_caveats: Vec<Caveat>,
    pub required_attestations: BTreeSet<(ToolClass, TeeType)>,
    pub default_budget: BudgetSpec,
    pub rate_limits: BTreeMap<ToolClass, RateLimitSpec>,
    pub publisher: PublicKey,
    pub audit_record_uri: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct PolicyRegistry {
    templates: HashMap<TemplateId, PolicyTemplate>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("template id already registered")]
    Duplicate,
    #[error("inherited template missing")]
    MissingParent,
    #[error("inheritance cycle detected")]
    Cycle,
}

impl PolicyRegistry {
    pub fn register(&mut self, tpl: PolicyTemplate) -> Result<(), PolicyError> {
        if self.templates.contains_key(&tpl.template_id) {
            return Err(PolicyError::Duplicate);
        }
        if let Some(pid) = tpl.inherits {
            if !self.templates.contains_key(&pid) {
                return Err(PolicyError::MissingParent);
            }
            if would_cycle(&self.templates, tpl.template_id, pid) {
                return Err(PolicyError::Cycle);
            }
        }
        self.templates.insert(tpl.template_id, tpl);
        Ok(())
    }

    pub fn get(&self, id: TemplateId) -> Option<&PolicyTemplate> {
        self.templates.get(&id)
    }

    /// Flatten `inherits` chain (root → leaf): concatenated caveats, merged maps (child overrides on collision).
    pub fn resolve(&self, id: TemplateId) -> Result<ResolvedPolicy, PolicyError> {
        if !self.templates.contains_key(&id) {
            return Err(PolicyError::MissingParent);
        }
        let mut chain: Vec<TemplateId> = Vec::new();
        let mut cur = Some(id);
        let mut seen = HashSet::new();
        while let Some(tid) = cur {
            if !seen.insert(tid) {
                return Err(PolicyError::Cycle);
            }
            let t = self.templates.get(&tid).ok_or(PolicyError::MissingParent)?;
            chain.push(tid);
            cur = t.inherits;
        }
        chain.reverse();

        let mut caveats: Vec<Caveat> = Vec::new();
        let mut rate_limits = BTreeMap::new();
        let mut attestations = BTreeSet::new();
        let mut budget = None;

        for tid in chain {
            let t = self.templates.get(&tid).expect("chain valid");
            caveats.extend(t.base_caveats.iter().cloned());
            for (k, v) in &t.rate_limits {
                rate_limits.insert(*k, v.clone());
            }
            attestations.extend(t.required_attestations.iter().cloned());
            budget = Some(t.default_budget.clone());
        }

        Ok(ResolvedPolicy {
            template_id: id,
            caveats,
            rate_limits,
            required_attestations: attestations,
            default_budget: budget.unwrap_or(BudgetSpec {
                total_cap: 0,
                per_tool: BTreeMap::new(),
            }),
        })
    }
}

fn would_cycle(
    existing: &HashMap<TemplateId, PolicyTemplate>,
    new_id: TemplateId,
    mut parent: TemplateId,
) -> bool {
    let mut guard = 0usize;
    loop {
        if parent == new_id {
            return true;
        }
        let Some(p) = existing.get(&parent) else {
            return false;
        };
        match p.inherits {
            None => return false,
            Some(next) => parent = next,
        }
        guard += 1;
        if guard > existing.len() + 2 {
            return true;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedPolicy {
    pub template_id: TemplateId,
    pub caveats: Vec<Caveat>,
    pub rate_limits: BTreeMap<ToolClass, RateLimitSpec>,
    pub required_attestations: BTreeSet<(ToolClass, TeeType)>,
    pub default_budget: BudgetSpec,
}

/// `docs/wallet.md` §15.2 — protocol-shipped policy templates (Phase 1 only).
///
/// Phase 1 tool classes are `Browser`, `LlmInference`, `TestRunner`, `FileStorage`
/// (`docs/wallet.md` §25.1). `GITHUB_*` / `EMAIL_SEND` etc. are out of scope and
/// are therefore omitted from these templates' allowed-class masks rather than
/// re-encoded as forbidden caveats — the always-on forbidden set is enforced
/// in `caveat::Caveat::FORBIDDEN_ACTIONS` (§4.5), not here.
pub mod builtins {
    use super::*;
    use crate::types::ToolClass;

    /// Built-in template IDs (stable; first three reserved for §15.2 names).
    pub const RESEARCH_AGENT_V1_ID: TemplateId = 1;
    pub const CODING_AGENT_V1_ID: TemplateId = 2;
    pub const VERIFIER_AGENT_V1_ID: TemplateId = 3;
    /// Phase 2 production slice: GitHub + DB + sandboxed code with TEE caveats.
    pub const CODING_AGENT_V2_PRODUCTION_ID: TemplateId = 4;

    /// One-FRAC-in-base-units. `Amount` (= `u128`) is in the smallest indivisible
    /// FRAC unit; the protocol's accounting precision matches Ethereum's wei.
    pub const FRAC: Amount = 1_000_000_000_000_000_000u128; // 1e18

    fn semver_1_0_0() -> SemVer {
        SemVer {
            major: 1,
            minor: 0,
            patch: 0,
        }
    }

    /// `tpl:research-agent-v1` (§15.2) — browser + LLM read/synthesis.
    pub fn research_agent_v1() -> PolicyTemplate {
        PolicyTemplate {
            template_id: RESEARCH_AGENT_V1_ID,
            version: semver_1_0_0(),
            name: "tpl:research-agent-v1".into(),
            description: "Browsing + LLM synthesis (Phase 1)".into(),
            inherits: None,
            base_caveats: vec![
                Caveat::MaxTotalSpend(3 * FRAC),
                Caveat::MaxPerCallSpend {
                    class: ToolClass::Browser,
                    max: FRAC,
                },
                Caveat::MaxPerCallSpend {
                    class: ToolClass::LlmInference,
                    max: 2 * FRAC,
                },
            ],
            required_attestations: BTreeSet::new(),
            default_budget: BudgetSpec {
                total_cap: 3 * FRAC,
                per_tool: BTreeMap::from([
                    (ToolClass::Browser, FRAC),
                    (ToolClass::LlmInference, 2 * FRAC),
                ]),
            },
            rate_limits: BTreeMap::from([
                (
                    ToolClass::Browser,
                    RateLimitSpec {
                        count: 50,
                        window_seconds: 3600,
                    },
                ),
                (
                    ToolClass::LlmInference,
                    RateLimitSpec {
                        count: 100,
                        window_seconds: 3600,
                    },
                ),
            ]),
            publisher: [0u8; 32],
            audit_record_uri: None,
        }
    }

    /// `tpl:coding-agent-v1` (§15.2) — code, tests, LLM. Phase 1 omits GITHUB_WRITE / TEE caveat.
    pub fn coding_agent_v1() -> PolicyTemplate {
        PolicyTemplate {
            template_id: CODING_AGENT_V1_ID,
            version: semver_1_0_0(),
            name: "tpl:coding-agent-v1".into(),
            description: "LLM + test runner + file storage (Phase 1)".into(),
            inherits: None,
            base_caveats: vec![
                Caveat::MaxTotalSpend(10 * FRAC),
                Caveat::MaxPerCallSpend {
                    class: ToolClass::TestRunner,
                    max: 2 * FRAC,
                },
                Caveat::MaxPerCallSpend {
                    class: ToolClass::LlmInference,
                    max: 5 * FRAC,
                },
            ],
            required_attestations: BTreeSet::new(),
            default_budget: BudgetSpec {
                total_cap: 10 * FRAC,
                per_tool: BTreeMap::from([
                    (ToolClass::TestRunner, 2 * FRAC),
                    (ToolClass::LlmInference, 5 * FRAC),
                ]),
            },
            rate_limits: BTreeMap::from([
                (
                    ToolClass::LlmInference,
                    RateLimitSpec {
                        count: 200,
                        window_seconds: 3600,
                    },
                ),
                (
                    ToolClass::TestRunner,
                    RateLimitSpec {
                        count: 20,
                        window_seconds: 3600,
                    },
                ),
            ]),
            publisher: [0u8; 32],
            audit_record_uri: None,
        }
    }

    /// `tpl:coding-agent-v2-production` — Phase 2 tool classes + TEE requirements (§25.2).
    pub fn coding_agent_v2_production() -> PolicyTemplate {
        PolicyTemplate {
            template_id: CODING_AGENT_V2_PRODUCTION_ID,
            version: semver_1_0_0(),
            name: "tpl:coding-agent-v2-production".into(),
            description: "LLM + tests + Phase 2 GitHub/DB/code with TEE attestation".into(),
            inherits: Some(CODING_AGENT_V1_ID),
            base_caveats: vec![
                Caveat::TeeAttestationRequired {
                    class: ToolClass::GithubWrite,
                    tee: crate::types::TeeType::IntelTdx,
                },
                Caveat::OutputCommitmentRequired(ToolClass::GithubWrite),
            ],
            required_attestations: BTreeSet::from([
                (ToolClass::GithubWrite, crate::types::TeeType::IntelTdx),
                (ToolClass::CodeExecution, crate::types::TeeType::AwsNitro),
            ]),
            default_budget: BudgetSpec {
                total_cap: 20 * FRAC,
                per_tool: BTreeMap::new(),
            },
            rate_limits: BTreeMap::new(),
            publisher: [0u8; 32],
            audit_record_uri: None,
        }
    }

    /// `tpl:verifier-agent-v1` (§15.2) — independent verification (read + test).
    pub fn verifier_agent_v1() -> PolicyTemplate {
        PolicyTemplate {
            template_id: VERIFIER_AGENT_V1_ID,
            version: semver_1_0_0(),
            name: "tpl:verifier-agent-v1".into(),
            description: "Independent verification: file read + test + LLM (Phase 1)".into(),
            inherits: None,
            base_caveats: vec![Caveat::MaxTotalSpend(2 * FRAC)],
            required_attestations: BTreeSet::new(),
            default_budget: BudgetSpec {
                total_cap: 2 * FRAC,
                per_tool: BTreeMap::new(),
            },
            rate_limits: BTreeMap::new(),
            publisher: [0u8; 32],
            audit_record_uri: None,
        }
    }

    /// Suggested `Scope::tool_class_mask` for each built-in template.
    ///
    /// `PolicyTemplate` has no `allowed_classes` field; the spec's "allowed: X, Y"
    /// list is realized at capability-mint time as a `Scope` mask. This helper
    /// returns the Phase 1 intersection of §15.2's allowed list and the available
    /// `ToolClass` enum.
    pub fn suggested_tool_class_mask(template_id: TemplateId) -> Option<u64> {
        match template_id {
            RESEARCH_AGENT_V1_ID => Some(
                ToolClass::Browser.bit() | ToolClass::LlmInference.bit() | ToolClass::FileStorage.bit(),
            ),
            CODING_AGENT_V1_ID => Some(
                ToolClass::LlmInference.bit() | ToolClass::TestRunner.bit() | ToolClass::FileStorage.bit(),
            ),
            VERIFIER_AGENT_V1_ID => Some(
                ToolClass::FileStorage.bit() | ToolClass::TestRunner.bit() | ToolClass::LlmInference.bit(),
            ),
            CODING_AGENT_V2_PRODUCTION_ID => Some(
                ToolClass::phase2_tool_class_mask()
                    | ToolClass::LlmInference.bit()
                    | ToolClass::TestRunner.bit()
                    | ToolClass::FileStorage.bit(),
            ),
            _ => None,
        }
    }

    /// Register Phase 1 templates plus the Phase 2 production coding template.
    pub fn register_builtins(reg: &mut PolicyRegistry) -> Result<(), PolicyError> {
        reg.register(research_agent_v1())?;
        reg.register(coding_agent_v1())?;
        reg.register(verifier_agent_v1())?;
        reg.register(coding_agent_v2_production())?;
        Ok(())
    }

    /// Names of all built-in templates in stable order (matches `register_builtins`).
    pub fn all_ids() -> [(TemplateId, &'static str); 4] {
        [
            (RESEARCH_AGENT_V1_ID, "tpl:research-agent-v1"),
            (CODING_AGENT_V1_ID, "tpl:coding-agent-v1"),
            (VERIFIER_AGENT_V1_ID, "tpl:verifier-agent-v1"),
            (
                CODING_AGENT_V2_PRODUCTION_ID,
                "tpl:coding-agent-v2-production",
            ),
        ]
    }

    /// Stable `(name, description)` for UI / `policy dump-builtins` (matches template constructors).
    pub fn meta(id: TemplateId) -> Option<(&'static str, &'static str)> {
        match id {
            RESEARCH_AGENT_V1_ID => Some((
                "tpl:research-agent-v1",
                "Browsing + LLM synthesis (Phase 1)",
            )),
            CODING_AGENT_V1_ID => Some((
                "tpl:coding-agent-v1",
                "LLM + test runner + file storage (Phase 1)",
            )),
            VERIFIER_AGENT_V1_ID => Some((
                "tpl:verifier-agent-v1",
                "Independent verification: file read + test + LLM (Phase 1)",
            )),
            CODING_AGENT_V2_PRODUCTION_ID => Some((
                "tpl:coding-agent-v2-production",
                "LLM + tests + Phase 2 GitHub/DB/code with TEE attestation",
            )),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TeeType, ToolClass};

    #[test]
    fn inheritance_merges_and_cycle_rejected() {
        let mut reg = PolicyRegistry::default();
        let parent = PolicyTemplate {
            template_id: 1,
            version: SemVer {
                major: 1,
                minor: 0,
                patch: 0,
            },
            name: "parent".into(),
            description: "".into(),
            inherits: None,
            base_caveats: vec![Caveat::MaxTotalSpend(100)],
            required_attestations: BTreeSet::from([(ToolClass::Browser, TeeType::AwsNitro)]),
            default_budget: BudgetSpec {
                total_cap: 100,
                per_tool: BTreeMap::new(),
            },
            rate_limits: BTreeMap::new(),
            publisher: [0u8; 32],
            audit_record_uri: None,
        };
        reg.register(parent).unwrap();

        let child = PolicyTemplate {
            template_id: 2,
            version: SemVer {
                major: 1,
                minor: 0,
                patch: 0,
            },
            name: "child".into(),
            description: "".into(),
            inherits: Some(1),
            base_caveats: vec![Caveat::MaxPerCallSpend {
                class: ToolClass::Browser,
                max: 5,
            }],
            required_attestations: BTreeSet::new(),
            default_budget: BudgetSpec {
                total_cap: 50,
                per_tool: BTreeMap::new(),
            },
            rate_limits: BTreeMap::new(),
            publisher: [0u8; 32],
            audit_record_uri: None,
        };
        reg.register(child).unwrap();

        let r = reg.resolve(2).unwrap();
        assert_eq!(r.caveats.len(), 2);
        assert_eq!(r.default_budget.total_cap, 50);

        let bad = PolicyTemplate {
            template_id: 3,
            version: SemVer {
                major: 1,
                minor: 0,
                patch: 0,
            },
            name: "bad".into(),
            description: "".into(),
            inherits: Some(3),
            base_caveats: vec![],
            required_attestations: BTreeSet::new(),
            default_budget: BudgetSpec {
                total_cap: 0,
                per_tool: BTreeMap::new(),
            },
            rate_limits: BTreeMap::new(),
            publisher: [0u8; 32],
            audit_record_uri: None,
        };
        assert_eq!(reg.register(bad), Err(PolicyError::MissingParent));
    }

    #[test]
    fn builtins_register_and_resolve() {
        use super::builtins::{
            all_ids, register_builtins, suggested_tool_class_mask, CODING_AGENT_V1_ID, FRAC,
            RESEARCH_AGENT_V1_ID, VERIFIER_AGENT_V1_ID,
        };
        let mut reg = PolicyRegistry::default();
        register_builtins(&mut reg).unwrap();
        assert_eq!(all_ids().len(), 4);

        let r = reg.resolve(RESEARCH_AGENT_V1_ID).unwrap();
        assert_eq!(r.default_budget.total_cap, 3 * FRAC);
        assert_eq!(r.rate_limits.get(&ToolClass::Browser).unwrap().count, 50);
        assert_eq!(
            r.rate_limits.get(&ToolClass::LlmInference).unwrap().count,
            100
        );
        assert!(r
            .caveats
            .iter()
            .any(|c| matches!(c, Caveat::MaxTotalSpend(v) if *v == 3 * FRAC)));
        assert_eq!(
            suggested_tool_class_mask(RESEARCH_AGENT_V1_ID).unwrap(),
            ToolClass::Browser.bit()
                | ToolClass::LlmInference.bit()
                | ToolClass::FileStorage.bit()
        );

        let c = reg.resolve(CODING_AGENT_V1_ID).unwrap();
        assert_eq!(c.default_budget.total_cap, 10 * FRAC);
        assert_eq!(
            c.default_budget.per_tool.get(&ToolClass::TestRunner),
            Some(&(2 * FRAC))
        );
        assert_eq!(
            c.default_budget.per_tool.get(&ToolClass::LlmInference),
            Some(&(5 * FRAC))
        );

        let v = reg.resolve(VERIFIER_AGENT_V1_ID).unwrap();
        assert_eq!(v.default_budget.total_cap, 2 * FRAC);
        assert!(v.rate_limits.is_empty());
    }
}
