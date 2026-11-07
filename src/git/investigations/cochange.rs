//! Local evidence for staging-time coupling. Historical frequency is a hint, never a rule.
use super::{Reader, oid};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
const COMMITS: usize = 600;
const BULK: usize = 20;
#[derive(Clone, Debug)]
pub(crate) struct Record {
    pub id: String,
    pub subject: String,
    pub files: BTreeSet<String>,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct History {
    pub records: Vec<Record>,
    pub skipped: usize,
    pub limited: bool,
    pub notes: Vec<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Hint {
    pub source: String,
    pub companion: String,
    pub source_changes: usize,
    pub evidence: Vec<(String, String)>,
}
fn generated(path: &str) -> bool {
    path.split('/').any(|p| {
        matches!(
            p,
            "vendor" | "node_modules" | "target" | "dist" | "generated" | ".next" | "build"
        )
    }) || path.ends_with(".lock")
        || matches!(path, "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml")
        || path.ends_with(".min.js")
        || path.ends_with(".min.css")
        || path.ends_with(".map")
        || path.ends_with(".g.cs")
}
fn header(fields: &[&[u8]], i: usize) -> bool {
    fields.get(i) == Some(&&b""[..])
        && fields.get(i + 1) == Some(&&b""[..])
        && fields
            .get(i + 2)
            .is_some_and(|b| std::str::from_utf8(b).is_ok_and(oid))
        && fields.get(i + 4) == Some(&&b""[..])
        && fields.get(i + 5) == Some(&&b""[..])
}
fn parse(bytes: &[u8]) -> Result<History, String> {
    if bytes.last() != Some(&0) {
        return Err("Truncated coupling history".into());
    }
    let fields = bytes.split(|b| *b == 0).collect::<Vec<_>>();
    let mut out = History::default();
    let mut i = 0;
    while i + 1 < fields.len() {
        if !header(&fields, i) {
            if fields[i].is_empty() {
                i += 1;
                continue;
            }
            return Err("Malformed coupling history header".into());
        }
        let id = std::str::from_utf8(fields[i + 2])
            .map_err(|_| "Invalid history ID")?
            .to_owned();
        let subject = String::from_utf8_lossy(fields[i + 3])
            .chars()
            .take(300)
            .collect::<String>();
        i += 6;
        let mut files = BTreeSet::new();
        let mut first = true;
        let mut unsupported = false;
        while i + 1 < fields.len() && !header(&fields, i) {
            let raw = if first {
                first = false;
                fields[i].strip_prefix(b"\n").unwrap_or(fields[i])
            } else {
                fields[i]
            };
            i += 1;
            if raw.is_empty() {
                continue;
            }
            let Ok(path) = std::str::from_utf8(raw) else {
                unsupported = true;
                continue;
            };
            if path.chars().any(char::is_control)
                || path.starts_with('/')
                || path
                    .split('/')
                    .any(|p| matches!(p, "" | "." | ".." | ".git"))
            {
                unsupported = true;
                continue;
            }
            files.insert(path.to_owned());
        }
        let title = subject.to_ascii_lowercase();
        if unsupported
            || files.len() > BULK
            || files.is_empty()
            || title.starts_with("generated ")
            || title.starts_with("regenerate ")
            || title.starts_with("auto-generated")
            || files.iter().filter(|p| generated(p)).count() * 2 >= files.len()
        {
            out.skipped += 1;
            continue;
        }
        files.retain(|p| !generated(p));
        out.records.push(Record { id, subject, files });
        if out.records.len() + out.skipped > COMMITS + 1 {
            return Err("Coupling history exceeded commit budget".into());
        }
    }
    Ok(out)
}
pub(crate) fn load(repo: &Path, head: &str, active: &dyn Fn() -> bool) -> Result<History, String> {
    if !oid(head) {
        return Err("Coupling needs a pinned committed HEAD".into());
    }
    let reader = Reader::new(repo, active);
    let bytes = reader.run(
        &[
            "log",
            "-n601",
            "--no-merges",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--no-show-signature",
            "--format=%x00%x00%H%x00%s%x00%x00",
            "--name-only",
            "-z",
            head,
            "--",
        ],
        vec![],
        &[0],
    )?;
    let mut history = parse(&bytes)?;
    if history.records.len() + history.skipped > COMMITS {
        history.limited = true;
        history.records.truncate(COMMITS);
    }
    let all = history
        .records
        .iter()
        .flat_map(|r| r.files.iter().cloned())
        .collect::<BTreeSet<_>>();
    if all.len() > 10000 {
        return Err("Coupling history exceeds 10000 distinct paths".into());
    }
    let mut input = Vec::new();
    for p in &all {
        input.extend_from_slice(p.as_bytes());
        input.push(0);
    }
    let attrs = reader.run(
        &[
            "check-attr",
            &format!("--source={head}"),
            "-z",
            "linguist-generated",
            "generated",
            "--stdin",
        ],
        input,
        &[0],
    );
    match attrs {
        Ok(bytes)=>{
            let fields=bytes.split(|b|*b==0).filter(|f|!f.is_empty()).collect::<Vec<_>>();
            if fields.len()%3!=0{return Err("Malformed generated-file attributes".into());}
            let mut excluded=BTreeSet::new();for triple in fields.chunks_exact(3){if matches!(triple[2],b"set"|b"true"){let path=std::str::from_utf8(triple[0]).map_err(|_|"Invalid attribute path")?;excluded.insert(path.to_owned());}}
            history.records.retain_mut(|record|{let generated=record.files.intersection(&excluded).count();if generated*2>=record.files.len(){history.skipped+=1;return false;}record.files.retain(|p|!excluded.contains(p));true});
        },
        Err(_)=>history.notes.push("Generated attributes could not be read at this commit; known generated paths and commit subjects were excluded. Git 2.43+ supports pinned attribute queries.".into())
    }
    if reader.run(&["rev-parse", "--is-shallow-repository"], vec![], &[0])? == b"true\n" {
        history.limited = true;
        history
            .notes
            .push("Shallow repository: older cochanges are unavailable locally.".into());
    }
    history.notes.push("Sample: latest 600 nonmerge commits. Bulk changes (>20 files), generated commit subjects, generated paths and generated attributes are excluded. At least three shared changes and 60% source confidence are required; this is historical association, not proof of a missing change.".into());
    Ok(history)
}
pub(crate) fn rank(
    history: &History,
    staged: &BTreeSet<String>,
    changed: &BTreeSet<String>,
) -> Vec<Hint> {
    let mut hints = Vec::new();
    for source in staged.iter().take(200) {
        let records = history
            .records
            .iter()
            .filter(|r| r.files.contains(source))
            .collect::<Vec<_>>();
        if records.len() < 3 {
            continue;
        }
        let mut together = BTreeMap::<String, Vec<(String, String)>>::new();
        for record in &records {
            for path in &record.files {
                if path != source && !changed.contains(path) {
                    together
                        .entry(path.clone())
                        .or_default()
                        .push((record.id.clone(), record.subject.clone()));
                }
            }
        }
        for (companion, evidence) in together {
            if evidence.len() >= 3 && evidence.len() * 100 >= records.len() * 60 {
                hints.push(Hint {
                    source: source.clone(),
                    companion,
                    source_changes: records.len(),
                    evidence,
                });
            }
        }
    }
    hints.sort_by(|a, b| {
        (b.evidence.len() * a.source_changes)
            .cmp(&(a.evidence.len() * b.source_changes))
            .then_with(|| b.evidence.len().cmp(&a.evidence.len()))
            .then_with(|| a.companion.cmp(&b.companion))
    });
    hints.truncate(20);
    hints
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::investigations::tests::Repo;
    #[test]
    fn staging_hints_show_real_evidence_and_omit_bulk_generated_and_already_changed_files()
    -> Result<(), String> {
        let r = Repo::new()?;
        for n in 0..5 {
            for p in ["serializer.rs", "compat.fixture"] {
                std::fs::write(r.0.join(p), format!("version {n}")).map_err(|e| e.to_string())?;
            }
            r.git(&["add", "-A"])?;
            r.commit("test@invalid", 1_700_000_000 + n)?;
        }
        for n in 0..3 {
            for p in 0..22 {
                std::fs::write(r.0.join(format!("bulk-{p}")), format!("{n}"))
                    .map_err(|e| e.to_string())?;
            }
            std::fs::write(r.0.join("serializer.rs"), format!("bulk{n}"))
                .map_err(|e| e.to_string())?;
            r.git(&["add", "-A"])?;
            r.commit("test@invalid", 1_700_000_010 + n)?;
        }
        std::fs::write(
            r.0.join(".gitattributes"),
            "generated.rs linguist-generated\n",
        )
        .map_err(|e| e.to_string())?;
        for n in 0..3 {
            for p in ["serializer.rs", "generated.rs"] {
                std::fs::write(r.0.join(p), format!("generated {n}")).map_err(|e| e.to_string())?;
            }
            r.git(&["add", "-A"])?;
            r.commit("test@invalid", 1_700_000_020 + n)?;
        }
        let head = r.git(&["rev-parse", "HEAD"])?;
        let refs = r.git(&["show-ref"])?;
        let index = std::fs::read(r.0.join(".git/index")).map_err(|e| e.to_string())?;
        let history = load(&r.0, &head, &|| true)?;
        assert!(history.skipped >= 3);
        let staged = BTreeSet::from(["serializer.rs".into()]);
        let hints = rank(&history, &staged, &staged);
        let hint = hints
            .iter()
            .find(|h| h.companion == "compat.fixture")
            .ok_or("Missing fixture hint")?;
        assert_eq!(hint.evidence.len(), 5);
        assert!(
            hints
                .iter()
                .all(|h| h.companion != "generated.rs" && !h.companion.starts_with("bulk-"))
        );
        let changed = BTreeSet::from(["serializer.rs".into(), "compat.fixture".into()]);
        assert!(rank(&history, &staged, &changed).is_empty());
        assert_eq!(r.git(&["show-ref"])?, refs);
        assert_eq!(
            std::fs::read(r.0.join(".git/index")).map_err(|e| e.to_string())?,
            index
        );
        assert!(load(&r.0, &head, &|| false).is_err());
        Ok(())
    }
    #[test]
    fn names_that_look_like_record_markers_cannot_forge_evidence() {
        let id = "a".repeat(40);
        let bytes = format!("\0\0{id}\0subject\0\0\0\n{id}\0path with spaces\0");
        let h = parse(bytes.as_bytes()).unwrap();
        assert_eq!(h.records.len(), 1);
        assert!(h.records[0].files.contains(&id));
        assert!(parse(b"truncated").is_err());
    }
}
