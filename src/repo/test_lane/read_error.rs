use super::*;

#[derive(Clone)]
pub(super) struct ReadError {
    raw: String,
    detail: String,
}
impl TestLane {
    pub(super) fn remember_read_error(&mut self, detail: String) {
        let e = &detail;
        let raw = format!("Couldn't read the test results: {e}");
        self.error = Some(raw.clone());
        self.read_error = Some(ReadError { raw, detail });
    }
    pub(super) fn display_read_error(&self, raw: &str) -> String {
        match &self.read_error {
            Some(receipt) if receipt.raw == raw => crate::i18n::message(
                "repo-workflows-test_lane-read-test-lane-46a9cf6f",
                &[("e", receipt.detail.as_str().into())],
            ),
            _ => raw.to_owned(),
        }
    }
}
