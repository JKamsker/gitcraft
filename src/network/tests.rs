use super::detect::{from_windows, parse_linux};
use super::*;

const NOTHING: Detected = Detected::UNKNOWN;
const METERED: Detected = Detected {
    connected: Some(true),
    metered: Some(true),
};
const DOWN: Detected = Detected {
    connected: Some(false),
    metered: None,
};

#[test]
fn the_switch_wins_then_no_connection_then_metered() {
    assert_eq!(pause_for(false, true, NOTHING), None);
    assert_eq!(pause_for(true, true, NOTHING), Some(Pause::Switch));
    assert_eq!(pause_for(true, true, DOWN), Some(Pause::Switch));
    assert_eq!(pause_for(false, true, DOWN), Some(Pause::NoConnection));
    assert_eq!(pause_for(false, true, METERED), Some(Pause::Metered));
    // Told not to mind a metered network.
    assert_eq!(pause_for(false, false, METERED), None);
    let free = Detected {
        connected: Some(true),
        metered: Some(false),
    };
    assert_eq!(pause_for(false, true, free), None);
}

#[test]
fn reads_what_windows_says() {
    let d = |connected, metered| Detected { connected, metered };
    for ((level, cost, roaming, over, near), want) in [
        (
            (Some(3), Some(1), false, false, false),
            d(Some(true), Some(false)),
        ),
        (
            (Some(3), Some(2), false, false, false),
            d(Some(true), Some(true)),
        ),
        (
            (Some(3), Some(3), false, false, false),
            d(Some(true), Some(true)),
        ),
        (
            (Some(3), Some(1), true, false, false),
            d(Some(true), Some(true)),
        ),
        (
            (Some(3), Some(1), false, true, false),
            d(Some(true), Some(true)),
        ),
        (
            (Some(3), Some(0), false, false, true),
            d(Some(true), Some(true)),
        ),
        ((Some(0), None, false, false, false), d(Some(false), None)),
        // Local or limited access: a blocked probe as often as no connection.
        (
            (Some(1), Some(1), false, false, false),
            d(None, Some(false)),
        ),
        (
            (Some(2), Some(1), false, false, false),
            d(None, Some(false)),
        ),
        ((None, None, false, false, false), NOTHING),
    ] {
        assert_eq!(from_windows(level, cost, roaming, over, near), want);
    }
}

#[test]
fn reads_what_network_manager_says() {
    let d = |connected, metered| Detected { connected, metered };
    assert_eq!(
        parse_linux(Some("u 4"), Some("u 4")),
        d(Some(true), Some(false))
    );
    assert_eq!(
        parse_linux(Some("u 4"), Some("u 1")),
        d(Some(true), Some(true))
    );
    assert_eq!(
        parse_linux(Some("u 2"), Some("u 3")),
        d(Some(true), Some(true))
    );
    assert_eq!(parse_linux(Some("u 1"), Some("u 0")), d(Some(false), None));
    assert_eq!(parse_linux(Some("u 0"), None), NOTHING);
    assert_eq!(parse_linux(None, None), NOTHING);
    assert_eq!(parse_linux(Some("s \"x\""), Some("u")), NOTHING);
}
