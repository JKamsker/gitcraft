use super::*;

struct Targets {
    repo: PathBuf,
    window: u8,
    busy: bool,
    outcomes: VecDeque<Result<(), String>>,
    applied: Vec<(PathBuf, PathBuf, u8)>,
    consumed: usize,
}

impl PatchTargets for Targets {
    fn busy(&mut self, repo: &Path) -> Option<bool> {
        (repo == self.repo).then_some(self.busy)
    }
    fn outcome(&mut self, _: &Path) -> Option<Result<(), String>> {
        let result = self.outcomes.pop_front();
        if result.is_some() {
            self.consumed += 1;
            self.busy = false;
        }
        result
    }
    fn apply(&mut self, repo: &Path, file: &Path) -> Result<(), String> {
        self.applied
            .push((repo.to_path_buf(), file.to_path_buf(), self.window));
        self.busy = true;
        Ok(())
    }
}

#[test]
fn hidden_continuations_wait_advance_once_and_follow_the_live_session() {
    let repo = PathBuf::from("/recorded/session");
    let mut targets = Targets {
        repo: repo.clone(),
        window: 1,
        busy: true,
        outcomes: VecDeque::new(),
        applied: vec![],
        consumed: 0,
    };
    let mut queue = PatchQueue::default();
    queue.push("first.patch".into(), repo.clone());
    queue.push("second.patch".into(), repo.clone());
    assert!(queue.advance_with(&mut targets).is_empty());
    assert!(targets.applied.is_empty());
    targets.busy = false;
    assert!(queue.advance_with(&mut targets).is_empty());
    assert_eq!(targets.applied, [(repo.clone(), "first.patch".into(), 1)]);
    targets.window = 2;
    targets.outcomes.push_back(Ok(()));
    assert!(queue.advance_with(&mut targets).is_empty());
    assert_eq!(
        targets.applied,
        [
            (repo.clone(), "first.patch".into(), 1),
            (repo, "second.patch".into(), 2)
        ]
    );
    assert_eq!(targets.consumed, 1);
    assert!(queue.advance_with(&mut targets).is_empty());
    assert_eq!(targets.applied.len(), 2);
    targets.outcomes.push_back(Ok(()));
    assert!(queue.advance_with(&mut targets).is_empty());
    assert!(queue.is_empty());
    assert_eq!(targets.consumed, 2);
}
