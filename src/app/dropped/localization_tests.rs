use super::*;

struct Recorded {
    repo: PathBuf,
    raw_error: String,
    applied: Vec<PathBuf>,
}

impl PatchTargets for Recorded {
    fn busy(&mut self, repo: &Path) -> Option<bool> {
        (repo == self.repo).then_some(false)
    }
    fn outcome(&mut self, _: &Path) -> Option<Result<(), String>> {
        Some(Err(self.raw_error.clone()))
    }
    fn apply(&mut self, _: &Path, file: &Path) -> Result<(), String> {
        self.applied.push(file.to_owned());
        Err(self.raw_error.clone())
    }
}

#[test]
fn actual_recorded_queue_keeps_raw_outcomes_and_paths_while_translating_only_captions() {
    crate::i18n::set_language("de");
    let repo = PathBuf::from("/opaque/repo");
    let file = PathBuf::from("/opaque/日本語 failed: literal.patch");
    let raw = "external diagnostic 日本語\n: untouched".to_owned();
    let mut targets = Recorded {
        repo: repo.clone(),
        raw_error: raw.clone(),
        applied: vec![],
    };
    let mut queue = PatchQueue {
        current: Some((file.clone(), repo.clone())),
        waiting: VecDeque::new(),
    };
    queue.push("one.patch".into(), repo.clone());
    queue.push("two.patch".into(), repo.clone());
    let gone = PathBuf::from("/opaque/closed");
    queue.push("closed.patch".into(), gone);
    let said = queue.advance_with(&mut targets);
    assert_eq!(said.len(), 2);
    assert_eq!(
        said[0],
        format!(
            "日本語 failed: literal.patch ist fehlgeschlagen: {raw} (deshalb wurden die 2 danach abgelegten Patches nicht angewendet)"
        )
    );
    assert_eq!(
        said[1],
        "closed.patch wurde nicht angewendet: Der Tab des zugehörigen Repositorys wurde geschlossen"
    );
    assert!(queue.is_empty());
    assert!(targets.applied.is_empty());
    assert_eq!(targets.raw_error, raw);
    // A direct apply rejection keeps the original file request and external diagnostic.
    queue.push(file.clone(), repo.clone());
    assert_eq!(
        queue.advance_with(&mut targets),
        [format!(
            "日本語 failed: literal.patch wurde nicht angewendet: {raw}"
        )]
    );
    assert_eq!(targets.applied.as_slice(), std::slice::from_ref(&file));
    assert!(queue.is_empty());
    // Singular and zero variants use the same original retain decision.
    queue.push("later.patch".into(), repo.clone());
    assert!(
        queue
            .failed(&file, &repo, &raw)
            .ends_with("(deshalb wurde der danach abgelegte Patch nicht angewendet)")
    );
    assert_eq!(
        queue.failed(&file, &repo, &raw),
        format!("日本語 failed: literal.patch ist fehlgeschlagen: {raw}")
    );
    crate::i18n::set_language("en-US");
    queue.push("later.patch".into(), repo.clone());
    assert_eq!(
        queue.failed(&file, &repo, &raw),
        format!(
            "日本語 failed: literal.patch failed: {raw} (so the 1 patch dropped after it was not applied)"
        )
    );
    assert_eq!(
        closed(&file),
        "日本語 failed: literal.patch wasn't applied: its repository's tab was closed"
    );
}

#[test]
fn actual_folder_classifier_and_target_guidance_preserve_opaque_names_and_unknown_errors() {
    crate::i18n::set_language("de");
    let path = Path::new("/opaque/日本語 isn't in a git repository");
    let raw = "FATAL: NOT A GIT REPOSITORY";
    assert_eq!(
        folder_error(path, raw),
        "日本語 isn't in a git repository liegt nicht in einem Git-Repository: Ziehe einen Repository-Ordner hierher, um ihn zu öffnen"
    );
    assert_eq!(raw, "FATAL: NOT A GIT REPOSITORY");
    let unknown = "external 日本語\n: diagnostic";
    assert_eq!(
        folder_error(path, unknown),
        format!("日本語 isn't in a git repository konnte nicht geöffnet werden: {unknown}")
    );
    assert_eq!(
        patch_target_message(true, "literal.patch"),
        "Zeige den Tab des zugehörigen Repositorys und lege literal.patch dort ab, um den Patch anzuwenden"
    );
    assert!(
        patch_target_message(false, "日本語.patch")
            .starts_with("Um 日本語.patch anzuwenden, muss zuerst ein Repository geöffnet sein")
    );
    crate::i18n::set_language("en-US");
    assert_eq!(
        patch_target_message(true, "literal.patch"),
        "To apply literal.patch, show its repository's tab, then drop the patch there"
    );
    assert_eq!(
        folder_error(Path::new("/x/y"), unknown),
        format!("Could not open y: {unknown}")
    );
}
