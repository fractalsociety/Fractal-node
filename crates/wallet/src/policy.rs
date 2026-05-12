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
}
