use super::*;
#[test]
fn r2_pure_operation_cancellation_scope_restores_worker_and_bounds_independent_cleanup_scope() {
    let worker = cancellation();
    let operation = worker.child();
    with_cancellation(operation.clone(), || {
        operation.cancel();
        assert!(cancellation().is_cancelled());
        with_cancellation(Cancellation::default(), || {
            assert!(!cancellation().is_cancelled())
        });
        assert!(cancellation().is_cancelled());
    });
    assert!(!worker.is_cancelled());
    assert!(!cancellation().is_cancelled());
}
