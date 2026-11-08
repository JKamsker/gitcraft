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
    Err("This build has symbol history disabled; enable the symbol-history Cargo feature".into())
}
