//! Skill dependency graph utilities.
//!
//! Builds a deterministic dependency graph over skill ids, detects cycles, and
//! produces a topological load order with dependencies before dependents.

use std::collections::{BTreeMap, BTreeSet};

use crate::{Result, error::Error};

/// A skill and the skill ids it depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDep {
    /// Stable skill id.
    pub id: String,
    /// Stable ids that must load before this skill.
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Done,
}

/// Return true when the dependency graph contains a directed cycle.
pub fn has_cycle(deps: &[SkillDep]) -> bool {
    load_order(deps).is_err()
}

/// Produce a deterministic topological load order, or an error when cyclic.
///
/// Referenced dependencies that do not have their own [`SkillDep`] entry are
/// treated as leaf nodes so the returned order still names every prerequisite.
pub fn load_order(deps: &[SkillDep]) -> Result<Vec<String>> {
    let graph = build_graph(deps);
    let mut states = BTreeMap::new();
    let mut order = Vec::new();

    for id in graph.keys() {
        visit(id, &graph, &mut states, &mut order)?;
    }

    Ok(order)
}

fn build_graph(deps: &[SkillDep]) -> BTreeMap<String, BTreeSet<String>> {
    let mut graph = BTreeMap::new();
    for dep in deps {
        graph
            .entry(dep.id.clone())
            .or_insert_with(BTreeSet::new)
            .extend(dep.depends_on.iter().cloned());
        for dependency in &dep.depends_on {
            graph
                .entry(dependency.clone())
                .or_insert_with(BTreeSet::new);
        }
    }
    graph
}

fn visit(
    id: &str,
    graph: &BTreeMap<String, BTreeSet<String>>,
    states: &mut BTreeMap<String, VisitState>,
    order: &mut Vec<String>,
) -> Result<()> {
    match states.get(id).copied() {
        Some(VisitState::Done) => return Ok(()),
        Some(VisitState::Visiting) => {
            return Err(Error::InvalidArtifact(format!(
                "skill dependency cycle detected at {id}"
            )));
        }
        None => {}
    }

    states.insert(id.to_string(), VisitState::Visiting);
    if let Some(dependencies) = graph.get(id) {
        for dependency in dependencies {
            visit(dependency, graph, states, order)?;
        }
    }
    states.insert(id.to_string(), VisitState::Done);
    order.push(id.to_string());
    Ok(())
}
