use fractal_society::pkgs::graph_projection::{GraphEdge, GraphNode, ResearchGraph};

#[test]
fn counts_nodes_and_edges() {
    let mut graph = ResearchGraph::new();
    graph.add_node(GraphNode::Person("alice".to_string()));
    graph.add_edge(
        GraphNode::Person("alice".to_string()),
        GraphNode::Agent("agent-1".to_string()),
        GraphEdge::Created,
    );
    graph.add_edge(
        GraphNode::Agent("agent-1".to_string()),
        GraphNode::Run("run-1".to_string()),
        GraphEdge::Used,
    );

    assert_eq!(graph.node_count(), 3);
    assert_eq!(graph.edge_count(), 2);
}

#[test]
fn duplicate_node_and_edge_adds_do_not_double_count() {
    let mut graph = ResearchGraph::new();
    let from = GraphNode::Proof("proof-1".to_string());
    let to = GraphNode::Review("review-1".to_string());

    graph.add_node(from.clone());
    graph.add_node(from.clone());
    graph.add_edge(from.clone(), to.clone(), GraphEdge::ReviewedBy);
    graph.add_edge(from, to, GraphEdge::ReviewedBy);

    assert_eq!(graph.node_count(), 2);
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn has_edge_reports_true_and_false_correctly() {
    let mut graph = ResearchGraph::new();
    let proof = GraphNode::Proof("proof-1".to_string());
    let review = GraphNode::Review("review-1".to_string());
    let run = GraphNode::Run("run-1".to_string());

    graph.add_edge(proof.clone(), review.clone(), GraphEdge::VerifiedBy);

    assert!(graph.has_edge(&proof, &review, &GraphEdge::VerifiedBy));
    assert!(!graph.has_edge(&review, &proof, &GraphEdge::VerifiedBy));
    assert!(!graph.has_edge(&proof, &run, &GraphEdge::ReplicatedBy));
}

#[test]
fn default_creates_empty_graph() {
    let graph = ResearchGraph::default();

    assert_eq!(graph.node_count(), 0);
    assert_eq!(graph.edge_count(), 0);
}
