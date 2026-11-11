//! Budget for Windows anonymous pipes and bounded remote-cleanup callbacks.
//!
//! Urgent children use additional scheduler slots and need the same pipe capacity.
const URGENT_EXTRA: usize = super::super::URGENT_EXTRA;

pub(super) fn threads(active: usize, windows: bool) -> usize {
    if windows {
        3 * (active + URGENT_EXTRA) + 2
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_schedulable_child_has_pipe_capacity_including_urgent_slots() {
        for active in [1, 2, 3, 12] {
            for urgent in 0..=URGENT_EXTRA {
                let children = active + urgent;
                assert!(threads(active, true) >= children * 3 + 2);
            }
        }
    }

    #[test]
    fn unix_async_pipes_only_need_the_cleanup_workers() {
        for active in [1, 2, 3, 12] {
            assert_eq!(threads(active, false), 2);
        }
    }
}
