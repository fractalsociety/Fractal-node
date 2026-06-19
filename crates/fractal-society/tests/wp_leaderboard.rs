use fractal_society::pkgs::leaderboard::{rank, LeaderboardEntry};

fn entry(
    agent_id: &str,
    net_return: f64,
    max_drawdown: f64,
    policy_violations: u64,
) -> LeaderboardEntry {
    LeaderboardEntry {
        agent_id: agent_id.to_string(),
        net_return,
        max_drawdown,
        policy_violations,
    }
}

#[test]
fn higher_score_ranks_first() {
    let ranked = rank(&[
        entry("agent-low", 0.10, 0.02, 0),
        entry("agent-high", 0.20, 0.02, 0),
    ]);

    assert_eq!(ranked[0].entry.agent_id, "agent-high");
    assert_eq!(ranked[0].rank, 1);
    assert!(ranked[0].score > ranked[1].score);
}

#[test]
fn drawdown_and_violations_penalize_rank_vs_raw_return() {
    let ranked = rank(&[
        entry("raw-return", 0.40, 0.50, 2),
        entry("robust", 0.20, 0.02, 0),
    ]);

    assert_eq!(ranked[0].entry.agent_id, "robust");
    assert!(ranked[0].score > ranked[1].score);
}

#[test]
fn equal_scores_tie_break_by_agent_id() {
    let ranked = rank(&[entry("bravo", 0.20, 0.20, 0), entry("alpha", 0.10, 0.00, 0)]);

    assert_eq!(ranked[0].score, ranked[1].score);
    assert_eq!(ranked[0].entry.agent_id, "alpha");
    assert_eq!(ranked[1].entry.agent_id, "bravo");
}

#[test]
fn ranks_are_contiguous_from_one() {
    let ranked = rank(&[
        entry("c", 0.03, 0.0, 0),
        entry("a", 0.01, 0.0, 0),
        entry("b", 0.02, 0.0, 0),
    ]);

    let ranks: Vec<u32> = ranked.iter().map(|entry| entry.rank).collect();
    assert_eq!(ranks, vec![1, 2, 3]);
}
