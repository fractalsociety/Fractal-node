//! Exploration graph (dead-end preservation) — AR-05.
//!
//! ARA's headline feature is preserving what was tried and *failed*, so no
//! agent rediscovers the same dead end. This module models that: an
//! [`ExplorationGraph`] is a DAG of [`ExplorationNode`]s covering hypotheses,
//! strategies, and approaches — including the ones that were disproven or
//! abandoned, with the reason they failed.
//!
//! The graph is serialized deterministically (nodes sorted by id, children
//! sorted), so [`ExplorationGraph::content_hash`] is byte-stable regardless of
//! insertion order. Every node carries a [`ProvenanceTag`] distinguishing
//! human-confirmed entries from AI-suggested/executed ones (shared with AR-08).

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::protocol::Hash;

// ProvenanceTag is defined in the canonical schema module (`protocol`) and
// re-exported here so exploration nodes and decision traces share one type.
pub use crate::protocol::ProvenanceTag;

/// What a node in the exploration graph represents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A falsifiable hypothesis under test.
    Hypothesis,
    /// A strategy or policy variant tried.
    Strategy,
    /// A broader approach or design direction.
    Approach,
    /// A configuration / hyperparameter choice.
    Config,
    /// An approach that was tried and led nowhere (a dead end).
    DeadEnd,
    /// An approach that was started then abandoned (superseded, not necessarily failed).
    Abandoned,
}

/// Lifecycle status of an exploration node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    /// Currently under investigation.
    Active,
    /// Confirmed / supported by evidence.
    Proven,
    /// Refuted by evidence.
    Disproven,
    /// Deliberately set aside.
    Abandoned,
    /// Replaced by a newer node.
    Superseded,
}

/// A single node in the research exploration graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorationNode {
    /// Stable node identifier (unique within the graph).
    pub id: String,
    /// What this node represents.
    pub kind: NodeKind,
    /// Lifecycle status.
    pub status: NodeStatus,
    /// Human-readable description of the idea.
    pub description: String,
    /// Optional one-line summary of the outcome.
    pub outcome_summary: Option<String>,
    /// Parent node id, if this descends from another (forms a DAG).
    pub parent: Option<String>,
    /// Child node ids (sorted for deterministic serialization).
    pub children: Vec<String>,
    /// Optional link to supporting evidence / proof (content hash).
    pub evidence_ref: Option<Hash>,
    /// Who originated this entry.
    pub provenance: ProvenanceTag,
    /// For dead-end / abandoned nodes: why it failed.
    pub dead_end_reason: Option<String>,
}

/// A research exploration graph: the branching set of approaches tried,
/// including the ones that did not work.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExplorationGraph {
    /// All nodes in the graph.
    pub nodes: Vec<ExplorationNode>,
}

impl ExplorationGraph {
    /// Create an empty exploration graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of nodes in the graph.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the graph has no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Add a node. Rejects empty ids and duplicate ids.
    pub fn add_node(&mut self, node: ExplorationNode) -> Result<()> {
        if node.id.trim().is_empty() {
            return Err(Error::InvalidArtifact(
                "exploration node id must be non-empty".to_string(),
            ));
        }
        if self.nodes.iter().any(|n| n.id == node.id) {
            return Err(Error::InvalidArtifact(format!(
                "duplicate exploration node id: {}",
                node.id
            )));
        }
        self.nodes.push(node);
        Ok(())
    }

    /// Borrow the dead-end / abandoned nodes (what was tried and failed).
    pub fn dead_ends(&self) -> Vec<&ExplorationNode> {
        self.nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::DeadEnd | NodeKind::Abandoned))
            .collect()
    }

    /// Sort nodes by id and each node's children by id, in place, so that two
    /// graphs with the same nodes serialize identically regardless of insertion
    /// order.
    pub fn canonicalize_order(&mut self) {
        self.nodes.sort_by(|a, b| a.id.cmp(&b.id));
        for node in &mut self.nodes {
            node.children.sort();
        }
    }

    /// Deterministic content hash of the graph (SHA-256 of its canonical JSON).
    ///
    /// Nodes are sorted by id and children sorted before hashing, so insertion
    /// order does not affect the hash.
    pub fn content_hash(&self) -> Result<Hash> {
        let mut canonical = self.clone();
        canonical.canonicalize_order();
        Hash::of(&canonical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, kind: NodeKind) -> ExplorationNode {
        ExplorationNode {
            id: id.to_string(),
            kind,
            status: NodeStatus::Active,
            description: format!("node {id}"),
            outcome_summary: None,
            parent: None,
            children: Vec::new(),
            evidence_ref: None,
            provenance: ProvenanceTag::Human,
            dead_end_reason: None,
        }
    }

    #[test]
    fn dead_ends_returns_only_failed_nodes() {
        let mut graph = ExplorationGraph::new();
        graph.add_node(node("h1", NodeKind::Hypothesis)).unwrap();
        graph.add_node(node("s1", NodeKind::Strategy)).unwrap();
        graph
            .add_node(ExplorationNode {
                id: "d1".to_string(),
                kind: NodeKind::DeadEnd,
                status: NodeStatus::Disproven,
                dead_end_reason: Some("overfit the training window".to_string()),
                ..node("d1", NodeKind::DeadEnd)
            })
            .unwrap();
        graph
            .add_node(ExplorationNode {
                id: "d2".to_string(),
                kind: NodeKind::Abandoned,
                status: NodeStatus::Abandoned,
                dead_end_reason: Some("superseded by s1".to_string()),
                ..node("d2", NodeKind::Abandoned)
            })
            .unwrap();

        let dead = graph.dead_ends();
        assert_eq!(dead.len(), 2);
        assert!(dead.iter().any(|n| n.id == "d1"));
        assert!(dead.iter().any(|n| n.id == "d2"));
    }

    #[test]
    fn rejects_empty_and_duplicate_ids() {
        let mut graph = ExplorationGraph::new();
        let empty = ExplorationNode {
            id: "  ".to_string(),
            ..node("x", NodeKind::Hypothesis)
        };
        assert!(graph.add_node(empty).is_err());
        graph.add_node(node("dup", NodeKind::Hypothesis)).unwrap();
        assert!(graph.add_node(node("dup", NodeKind::Hypothesis)).is_err());
    }

    #[test]
    fn serialize_round_trips() {
        let mut graph = ExplorationGraph::new();
        graph.add_node(node("a", NodeKind::Hypothesis)).unwrap();
        graph.add_node(node("b", NodeKind::Strategy)).unwrap();

        let json = serde_json::to_string(&graph).unwrap();
        let back: ExplorationGraph = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back.content_hash().unwrap(), graph.content_hash().unwrap());
    }

    #[test]
    fn content_hash_is_insertion_order_independent() {
        let mut g1 = ExplorationGraph::new();
        g1.add_node(node("c", NodeKind::Config)).unwrap();
        g1.add_node(node("a", NodeKind::Hypothesis)).unwrap();
        g1.add_node(node("b", NodeKind::Strategy)).unwrap();

        let mut g2 = ExplorationGraph::new();
        g2.add_node(node("a", NodeKind::Hypothesis)).unwrap();
        g2.add_node(node("b", NodeKind::Strategy)).unwrap();
        g2.add_node(node("c", NodeKind::Config)).unwrap();

        assert_eq!(g1.content_hash().unwrap(), g2.content_hash().unwrap());
    }

    #[test]
    fn content_hash_is_byte_stable_across_clones() {
        let mut graph = ExplorationGraph::new();
        graph.add_node(node("a", NodeKind::Hypothesis)).unwrap();
        graph.add_node(node("b", NodeKind::DeadEnd)).unwrap();

        let h1 = graph.content_hash().unwrap();
        let h2 = graph.content_hash().unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn provenance_default_is_human() {
        assert_eq!(ProvenanceTag::default(), ProvenanceTag::Human);
    }
}
