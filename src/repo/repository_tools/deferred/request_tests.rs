use super::*;
#[test]
fn invalidation_resets_pending_reads_and_markers_without_reusing_generation() {
    let mut cache = Cache {
        reading: true,
        pending: Some(10),
        automatic: Some(11),
        outbox_poll: Some(12),
        ..Default::default()
    };
    cache.markers.push(Marker {
        hash: "old".into(),
        branch: "old".into(),
        conflicts: Vec::new(),
    });
    cache.reset_requests();
    assert!(!cache.reading);
    assert!(cache.pending.is_none() && cache.automatic.is_none() && cache.outbox_poll.is_none());
    assert!(cache.markers.is_empty() && cache.last.is_none());
    let first = cache.epoch();
    cache.reset_requests();
    assert!(cache.epoch() > first);
}
