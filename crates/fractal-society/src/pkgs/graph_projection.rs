//! Minimal research-graph projection package.
//!
//! Minimal relational research graph (Person/Agent/Run/Proof/Review nodes;
//! created/used/verified-by/reviewed-by/replicated-by edges) built from records.

use std::collections::HashSet;

/// Node in the projected research graph.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum GraphNode {
    /// Person node.
    Person(String),
    /// Agent node.
    Agent(String),
    /// Run node.
    Run(String),
    /// Proof node.
    Proof(String),
    /// Review node.
    Review(String),
}

/// Edge kind in the projected research graph.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum GraphEdge {
    /// Source created target.
    Created,
    /// Source used target.
    Used,
    /// Source was verified by target.
    VerifiedBy,
    /// Source was reviewed by target.
    ReviewedBy,
    /// Source was replicated by target.
    ReplicatedBy,
}

/// Minimal relational research graph.
#[derive(Debug, Clone)]
pub struct ResearchGraph {
    nodes: HashSet<GraphNode>,
    edges: HashSet<(GraphNode, GraphNode, GraphEdge)>,
}

impl ResearchGraph {
    /// Create an empty research graph.
    pub fn new() -> Self {
        Self {
            nodes: HashSet::new(),
            edges: HashSet::new(),
        }
    }

    /// Add a node to the graph.
    pub fn add_node(&mut self, node: GraphNode) {
        self.nodes.insert(node);
    }

    /// Add an edge to the graph and ensure both endpoint nodes are present.
    pub fn add_edge(&mut self, from: GraphNode, to: GraphNode, edge: GraphEdge) {
        self.nodes.insert(from.clone());
        self.nodes.insert(to.clone());
        self.edges.insert((from, to, edge));
    }

    /// Number of unique nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of unique edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Return true if the exact directed edge exists.
    pub fn has_edge(&self, from: &GraphNode, to: &GraphNode, edge: &GraphEdge) -> bool {
        self.edges
            .contains(&(from.clone(), to.clone(), edge.clone()))
    }
}

impl Default for ResearchGraph {
    fn default() -> Self {
        Self::new()
    }
}
