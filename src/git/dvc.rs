//! Fixed DVC transfer command routed through Git's portable shell and operation runner.
pub(crate) const PULL_ALIAS: &str = "gitcraft-dvc-pull";
pub(crate) fn pull_steps() -> Vec<Vec<String>> {
    vec![vec!["-c".into(), "alias.gitcraft-dvc-pull=!f() { command -v dvc >/dev/null 2>&1 || { echo 'DVC is not installed on this repository machine' >&2; exit 127; }; exec dvc pull; }; f".into(), PULL_ALIAS.into()]]
}
#[cfg(test)]
mod tests {
    #[test]
    fn pull_is_fixed_argv_and_uses_network_operation_guards() {
        let steps = super::pull_steps();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].last().map(String::as_str), Some(super::PULL_ALIAS));
        assert!(crate::git::goes_over_network(
            &steps[0].iter().map(String::as_str).collect::<Vec<_>>()
        ));
        assert!(crate::repo::operations::reaches_a_remote(&steps[0]));
    }
}
