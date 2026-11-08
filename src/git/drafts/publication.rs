//! Durable exact-operation receipts. The native state transaction creates its unique
//! receipt ref atomically with the state/ref updates. Failed acknowledgement plus an
//! unavailable/missing/different receipt is Unknown, never a rollback permission.
use super::native_files::CommitOutcome;
use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct Intent {
    pub operation: String,
    pub state_ref: String,
    pub previous: String,
    pub proposed: String,
    pub branch: Option<String>,
    pub expected_head: Option<String>,
    pub evidence: String,
    pub receipt_ref: String,
}
#[derive(Clone)]
pub struct Report {
    pub intent: Intent,
    pub committed: bool,
    pub detail: String,
}
pub(super) struct Pending {
    intent: Intent,
}
fn receipt(ctx: &Context, reference: &str) -> Result<Option<String>, String> {
    let raw = ctx.source(
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(symref)",
            reference,
        ],
        vec![],
        &[0],
        &|| true,
    )?;
    if raw.len() > 4096 {
        return Err(crate::i18n::text(
            "backend-publication-receipt-enumeration-exceeds-limit-549096",
        )
        .into());
    }
    let text = std::str::from_utf8(&raw)
        .map_err(|_| crate::i18n::text("backend-invalid-receipt-metadata-64b00a"))?;
    for row in text.lines() {
        let fields = row.split('\0').collect::<Vec<_>>();
        if fields.len() == 3 && fields[0] == reference {
            if !fields[2].is_empty() || !oid_valid(fields[1]) {
                return Err(crate::i18n::text(
                    "backend-publication-receipt-is-not-a-direct-immutable-object-ref-5ed70d",
                )
                .into());
            }
            return Ok(Some(fields[1].into()));
        }
    }
    Ok(None)
}
fn classify(
    acknowledged: bool,
    expected: &str,
    observed: Result<Option<String>, String>,
) -> CommitOutcome {
    if acknowledged
        || observed
            .as_ref()
            .is_ok_and(|oid| oid.as_deref() == Some(expected))
    {
        CommitOutcome::Committed
    } else {
        CommitOutcome::Unknown(match observed {
            Ok(None) => crate::i18n::text("backend-receipt-is-absent-e2fe09").into(),
            Ok(Some(_)) => crate::i18n::text("backend-receipt-differs-04859a").into(),
            Err(e) => crate::i18n::message(
                "backend-receipt-cannot-be-read-e-24e7af",
                &[("detail", (e).to_string().into())],
            ),
        })
    }
}
pub(super) fn begin(
    ctx: &Context,
    scope: &str,
    state_ref: &str,
    (previous, proposed): (&str, &str),
    source: &Source,
    evidence: &str,
    active: &dyn Fn() -> bool,
) -> Result<Pending, String> {
    if scope.is_empty()
        || !scope
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(crate::i18n::text("backend-invalid-publication-scope-c7a4fc").into());
    }
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let operation = format!(
        "{scope}-{}-{}-{}",
        git::now(),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let intent = Intent {
        operation: operation.clone(),
        state_ref: state_ref.into(),
        previous: previous.into(),
        proposed: proposed.into(),
        branch: source.branch.clone(),
        expected_head: source.head.clone(),
        evidence: evidence.into(),
        receipt_ref: format!(
            "refs/gitcraft/state-publication-receipts/{}/{operation}",
            ctx.id
        ),
    };
    let bytes = serde_json::to_vec(&intent).map_err(|e| e.to_string())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(crate::i18n::text(
            "backend-publication-recovery-context-exceeds-2-mib-no-state-transaction-w-e58ef3",
        )
        .into());
    }
    let blob = ctx.blob(bytes, active)?;
    let input = format!("100644 blob {blob}\tintent.json\n040000 tree {proposed}\tproposed\n");
    let root = ctx.text(&["mktree"], input.into_bytes(), active)?;
    let pending = format!(
        "refs/gitcraft/state-publication-pending/{}/{operation}",
        ctx.id
    );
    ctx.source(
        &[
            "update-ref",
            "--no-deref",
            &pending,
            &root,
            &"0".repeat(root.len()),
        ],
        vec![],
        &[0],
        active,
    )
    .map_err(|e| {
        crate::i18n::message(
            "backend-no-state-transaction-was-dispatched-pending-journal-operation-may-fddb32",
            &[
                ("operation", (operation).to_string().into()),
                ("detail", (e).to_string().into()),
            ],
        )
    })?;
    Ok(Pending { intent })
}
impl Pending {
    pub(super) fn append_receipt(&self, transaction: &mut String) {
        let ending = "prepare\ncommit\n";
        assert!(transaction.ends_with(ending));
        transaction.truncate(transaction.len() - ending.len());
        transaction.push_str(&format!(
            "option no-deref\ncreate {} {}\n{ending}",
            self.intent.receipt_ref, self.intent.proposed
        ));
    }
    pub(super) fn finish(&self, ctx: &Context, result: Result<Vec<u8>, String>) -> CommitOutcome {
        let acknowledged = result.is_ok();
        let error = result.err().unwrap_or_default();
        let outcome = classify(
            acknowledged,
            &self.intent.proposed,
            if acknowledged {
                Ok(None)
            } else {
                receipt(ctx, &self.intent.receipt_ref)
            },
        );
        match outcome {
            CommitOutcome::Committed => CommitOutcome::Committed,
            CommitOutcome::Unknown(reason) => CommitOutcome::Unknown(crate::i18n::message(
                "backend-state-publication-is-pending-exact-reconciliation-reason-error-no-f75f60",
                &[
                    ("intent", (self.intent.operation).to_string().into()),
                    ("reason", (reason).to_string().into()),
                    ("detail", (error).to_string().into()),
                    ("intent4", (self.intent.receipt_ref).to_string().into()),
                    ("intent5", (self.intent.evidence).to_string().into()),
                ],
            )),
        }
    }
}
fn reports(ctx: &Context) -> Result<Vec<Report>, String> {
    let prefix = format!("refs/gitcraft/state-publication-pending/{}/", ctx.id);
    let rows = ctx.source(
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(symref)",
            &prefix,
        ],
        vec![],
        &[0],
        &|| true,
    )?;
    if rows.len() > 256 * 1024 {
        return Err(crate::i18n::text(
            "backend-publication-journal-enumeration-exceeds-256-kib-a299e2",
        )
        .into());
    }
    let mut reports = Vec::new();
    for row in std::str::from_utf8(&rows)
        .map_err(|_| crate::i18n::text("backend-invalid-pending-publication-refs-66eca9"))?
        .lines()
    {
        if reports.len() >= 256 {
            return Err(crate::i18n::text(
                "backend-publication-journal-has-256-entries-inspect-archive-resolved-jour-51f4a9",
            )
            .into());
        }
        let fields = row.split('\0').collect::<Vec<_>>();
        if fields.len() != 3
            || !fields[0].starts_with(&prefix)
            || !fields[2].is_empty()
            || !oid_valid(fields[1])
        {
            return Err(crate::i18n::text(
                "backend-pending-publication-ref-is-not-a-direct-application-object-13c69a",
            )
            .into());
        }
        let spec = format!("{}:intent.json", fields[1]);
        let size = ctx
            .text(&["cat-file", "-s", &spec], vec![], &|| true)?
            .parse::<usize>()
            .map_err(|_| crate::i18n::text("backend-invalid-publication-journal-size-83996c"))?;
        if size > 2 * 1024 * 1024 {
            return Err(crate::i18n::text(
                "backend-publication-journal-entry-exceeds-2-mib-4b0c7d",
            )
            .into());
        }
        let intent: Intent = serde_json::from_slice(&ctx.run(
            &["cat-file", "blob", &spec],
            vec![],
            &[0],
            &|| true,
        )?)
        .map_err(|_| crate::i18n::text("backend-invalid-publication-journal-3280e5"))?;
        let expected_receipt = format!(
            "refs/gitcraft/state-publication-receipts/{}/{}",
            ctx.id, intent.operation
        );
        if !oid_valid(&intent.proposed)
            || intent.receipt_ref != expected_receipt
            || fields[0] != format!("{prefix}{}", intent.operation)
        {
            return Err(crate::i18n::text(
                "backend-publication-journal-context-mismatches-its-operation-77a2c5",
            )
            .into());
        }
        let outcome = classify(false, &intent.proposed, receipt(ctx, &intent.receipt_ref));
        let (committed,detail)=match outcome {CommitOutcome::Committed=>(true,crate::i18n::text("backend-exact-native-transaction-receipt-confirms-committed-metadata-if-a-77d0fc").into()),CommitOutcome::Unknown(e)=>(false,crate::i18n::message("backend-pending-e-current-materialization-and-backups-must-be-inspected-n-43737f", &[("detail", (e).to_string().into())]))};
        reports.push(Report {
            intent,
            committed,
            detail,
        });
    }
    Ok(reports)
}
pub(super) fn guard(ctx: &Context) -> Result<(), String> {
    if let Some(report) = reports(ctx)?.into_iter().find(|r| !r.committed) {
        return Err(crate::i18n::message(
            "backend-state-publication-remains-pending-use-inspect-state-publication-r-247eda",
            &[
                ("report", (report.intent.operation).to_string().into()),
                ("detail", (report.detail).to_string().into()),
                ("report3", (report.intent.evidence).to_string().into()),
            ],
        ));
    }
    Ok(())
}
pub fn read(repo: &Path, active: &dyn Fn() -> bool) -> Result<Vec<Report>, String> {
    reports(&Context::new(repo, active)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pure_publication_lost_ack_failed_or_advanced_receipt_never_authorizes_rollback() {
        let proposed = "a".repeat(40);
        assert_eq!(
            classify(false, &proposed, Ok(Some(proposed.clone()))),
            CommitOutcome::Committed
        );
        for observed in [
            Ok(None),
            Err("recorded read failure".into()),
            Ok(Some("b".repeat(40))),
        ] {
            assert!(matches!(
                classify(false, &proposed, observed),
                CommitOutcome::Unknown(_)
            ));
        }
        assert_eq!(
            classify(true, &proposed, Err("not needed".into())),
            CommitOutcome::Committed
        );
        // A later state root is irrelevant: the unique receipt pins the committed
        // proposed root, rather than comparing the mutable current state ref.
        let pending = Pending {
            intent: Intent {
                operation: "lists-1".into(),
                state_ref: "refs/gitcraft/lists/state".into(),
                previous: "b".repeat(40),
                proposed: proposed.clone(),
                branch: Some("refs/heads/main".into()),
                expected_head: Some("c".repeat(40)),
                evidence: "owned recovery path".into(),
                receipt_ref: "refs/gitcraft/receipts/lists-1".into(),
            },
        };
        let mut tx = "start\nupdate HEAD a a\nprepare\ncommit\n".to_owned();
        pending.append_receipt(&mut tx);
        assert!(tx.contains(&format!(
            "option no-deref\ncreate {} {proposed}\nprepare\ncommit\n",
            pending.intent.receipt_ref
        )));
    }
}
