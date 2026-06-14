use fractal_network::{
    shard_sync_protocol, shard_timeouts_topic, shard_votes_topic, sync_protocols_for_shard,
    votes_topic_for_shard, VOTES_TOPIC_STR,
};

#[test]
fn monolith_uses_global_topics() {
    assert_eq!(votes_topic_for_shard(0, 1), VOTES_TOPIC_STR);
    assert_eq!(sync_protocols_for_shard(0, 1).len(), 1);
}

#[test]
fn shard_scoped_topics() {
    assert_eq!(shard_votes_topic(3), "/fractalchain/shard/3/votes/1.0.0");
    assert_eq!(
        shard_timeouts_topic(3),
        "/fractalchain/shard/3/timeouts/1.0.0"
    );
    assert!(shard_sync_protocol(3).as_ref().contains("/shard/3/"));
}
