use super::*;
#[test]
fn execution_bridge_close_replays_only_matching_error_and_clears_other_results() {
    let mut receipt = None;
    let result = close_finished(
        Err("owner; lease release: provider".into()),
        Err("lease); framing".into()),
        &mut receipt,
    );
    let saved = receipt.clone();
    for _ in 0..2 {
        assert_eq!(
            replay_close(&result.as_ref().err().cloned(), &saved, &mut receipt),
            result
        );
        assert!(receipt.is_some());
    }
    assert!(replay_close(&None, &saved, &mut receipt).is_ok());
    assert!(receipt.is_none());
    assert!(replay_close(&Some("unrelated".into()), &saved, &mut receipt).is_err());
    assert!(receipt.is_none());
    let mut receipt = saved;
    assert!(
        transfer_finished(
            Err(TransferFailure::Git("git denied".into())),
            Ok(()),
            &mut receipt
        )
        .is_err()
    );
    assert!(receipt.is_none());
    assert!(matches!(
        transfer_finished(
            Err(TransferFailure::Git("git denied".into())),
            Ok(()),
            &mut receipt
        ),
        Err(TransferFailure::Git(_))
    ));
    assert!(transfer_finished(Ok(()), Ok(()), &mut receipt).is_ok());
    assert!(receipt.is_none());
}
