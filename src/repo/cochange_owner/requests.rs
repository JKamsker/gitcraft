//! Monotonic latest-read identity. Retiring a read does not settle a mutation;
//! the operation completion channel owns already executing mutation outcomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Request(u64);
#[derive(Default)]
pub(super) struct Requests {
    generation: u64,
    pending: Option<Request>,
}
impl Requests {
    pub fn start(&mut self) -> Option<Request> {
        self.pending = None;
        self.generation = self.generation.checked_add(1)?;
        let request = Request(self.generation);
        self.pending = Some(request);
        Some(request)
    }
    pub fn retire(&mut self) {
        self.pending = None;
    }
    pub fn pending(&self) -> Option<Request> {
        self.pending
    }
    pub fn finish(&mut self, request: Request) -> bool {
        if self.pending != Some(request) {
            return false;
        }
        self.pending = None;
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_b_a_and_out_of_order_completion_keep_only_the_latest_request() -> Result<(), String> {
        let mut requests = Requests::default();
        let a = requests.start().ok_or("request identity exhausted")?;
        let b = requests.start().ok_or("request identity exhausted")?;
        let second_a = requests.start().ok_or("request identity exhausted")?;
        assert!(!requests.finish(a));
        assert!(!requests.finish(b));
        assert!(requests.finish(second_a));
        assert!(!requests.finish(second_a));
        Ok(())
    }
    #[test]
    fn retirement_and_reopen_never_reuse_a_request_identity() -> Result<(), String> {
        let mut requests = Requests::default();
        let old = requests.start().ok_or("request identity exhausted")?;
        requests.retire();
        assert!(!requests.finish(old));
        let reopened = requests.start().ok_or("request identity exhausted")?;
        assert_ne!(old, reopened);
        assert!(!requests.finish(old));
        assert_eq!(requests.pending(), Some(reopened));
        assert!(requests.finish(reopened));
        Ok(())
    }
}
