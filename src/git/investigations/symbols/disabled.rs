use super::Symbol;
pub(super) fn supported(_: &str) -> bool {
    false
}
pub(super) fn parse(
    _: &str,
    _: &str,
    _: &str,
    _: &dyn Fn() -> bool,
) -> Result<Vec<Symbol>, String> {
    Err(crate::i18n::text(
        "backend-this-build-has-symbol-history-disabled-enable-the-symbol-history-c9b07c",
    )
    .into())
}
