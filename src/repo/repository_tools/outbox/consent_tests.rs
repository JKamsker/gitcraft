use super::*;
#[test]
fn pure_outbox_stale_preparation_requires_same_profile_context_and_tab_signin_generation() {
    let a = crate::settings::profiles::UserProfile::default();
    let context = crate::connections::Context {
        choices: a.connections.clone(),
        ..Default::default()
    };
    assert!(selection_current(
        &context,
        a.id,
        &a.connections,
        4,
        &context,
        &a,
        4
    ));
    let mut b = a.clone();
    b.id += 1;
    assert!(!selection_current(
        &context,
        a.id,
        &a.connections,
        4,
        &context,
        &b,
        4
    ));
    b = a.clone();
    b.connections.github.user = "different-account".into();
    assert!(!selection_current(
        &context,
        a.id,
        &a.connections,
        4,
        &context,
        &b,
        4
    ));
    assert!(!selection_current(
        &context,
        a.id,
        &a.connections,
        4,
        &context,
        &a,
        5
    ));
    let mut changed = context.clone();
    changed.git_config = Some("/synthetic/new-immutable-config".into());
    assert!(!selection_current(
        &context,
        a.id,
        &a.connections,
        4,
        &changed,
        &a,
        4
    ));
}
