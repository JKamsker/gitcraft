use super::detect::{parse_linux, parse_windows};
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
    for (out, want) in [
        ("3 1 False False False", d(Some(true), Some(false))),
        ("3 2 False False False", d(Some(true), Some(true))),
        ("3 3 False False False", d(Some(true), Some(true))),
        ("3 1 True False False", d(Some(true), Some(true))),
        ("3 1 False True False", d(Some(true), Some(true))),
        ("3 0 False False True", d(Some(true), Some(true))),
        ("1 1 False False False", d(Some(false), Some(false))),
        ("none", d(Some(false), None)),
        ("", NOTHING),
        ("garbage", NOTHING),
        ("3 x False False False", d(Some(true), None)),
    ] {
        assert_eq!(parse_windows(out), want, "{out:?}");
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
