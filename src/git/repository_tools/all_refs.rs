use crate::git;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Reference {
    pub name: String,
    pub oid: String,
    pub kind: String,
    pub symbolic: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Browser {
    pub refs: Vec<Reference>,
    pub history: String,
    pub root_fallback: bool,
}

pub(crate) fn read(repo: &Path, namespace: &str) -> Result<Browser, String> {
    let root = !namespace.is_empty()
        && namespace
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
    if !namespace.is_empty()
        && !root
        && (!namespace.starts_with("refs/")
            || namespace.contains(['\n', '\r', '\0', '*', '?', '[']))
    {
        return Err(
            "Choose a literal root ref (such as HEAD) or a ref/prefix beginning with refs/".into(),
        );
    }
    let format = "--format=%(refname)%00%(objectname)%00%(objecttype)%00%(symref)";
    let (code, out) = git::run_code(
        Some(repo),
        &["for-each-ref", "--include-root-refs", format],
        &[0, 129],
    )?;
    let root_fallback = code != 0;
    let out = if root_fallback {
        git::run(Some(repo), &["for-each-ref", format])?
    } else {
        String::from_utf8(out).map_err(|e| e.to_string())?
    };
    let mut refs: Vec<Reference> = out
        .lines()
        .filter_map(|line| {
            let f: Vec<_> = line.split('\0').collect();
            (f.len() == 4).then(|| Reference {
                name: f[0].into(),
                oid: f[1].into(),
                kind: f[2].into(),
                symbolic: f[3].into(),
            })
        })
        .collect();
    // %(symref) resolves a whole chain. Keep the immediate target so Follow can visit
    // intermediate symbolic/root refs instead of silently jumping over them.
    for reference in refs.iter_mut().filter(|r| !r.symbolic.is_empty()) {
        reference.symbolic = git::run(
            Some(repo),
            &["symbolic-ref", "--no-recurse", "-q", &reference.name],
        )?
        .trim()
        .to_owned();
    }
    // Older Git cannot enumerate arbitrary root refs. Explicitly inspect its established
    // pseudorefs without treating a failed resolve as a nonexistent normal ref.
    for name in [
        "HEAD",
        "ORIG_HEAD",
        "FETCH_HEAD",
        "MERGE_HEAD",
        "AUTO_MERGE",
        "CHERRY_PICK_HEAD",
        "REBASE_HEAD",
        "REVERT_HEAD",
        "BISECT_HEAD",
    ] {
        if refs.iter().any(|r| r.name == name) {
            continue;
        }
        let Ok(oid) = git::run(
            Some(repo),
            &["rev-parse", "--verify", "--end-of-options", name],
        ) else {
            continue;
        };
        let oid = oid.trim().to_owned();
        let kind = git::run(Some(repo), &["cat-file", "-t", &oid])?
            .trim()
            .to_owned();
        let symbolic = git::run_ok_codes(
            Some(repo),
            &["symbolic-ref", "--no-recurse", "-q", name],
            &[0, 1],
        )?
        .trim()
        .to_owned();
        refs.push(Reference {
            name: name.into(),
            oid,
            kind,
            symbolic,
        });
    }
    refs.sort_by(|a, b| a.name.cmp(&b.name));
    let selected: Vec<_> = refs
        .iter()
        .filter(|r| {
            namespace.is_empty()
                || r.name == namespace
                || r.name
                    .starts_with(&format!("{}/", namespace.trim_end_matches('/')))
        })
        .collect();
    let mut tips = Vec::new();
    for r in &selected {
        if r.kind == "commit" {
            tips.push(r.oid.clone());
        } else if r.kind == "tag"
            && let Ok(commit) = git::run(
                Some(repo),
                &["rev-parse", "--verify", &format!("{}^{{commit}}", r.oid)],
            )
        {
            tips.push(commit.trim().to_owned());
        }
    }
    tips.sort();
    tips.dedup();
    let history = if tips.is_empty() {
        String::new()
    } else {
        git::run_input(
            Some(repo),
            &[
                "--no-pager",
                "log",
                "--graph",
                "--no-color",
                "--no-show-signature",
                "--format=%h %s",
                "-n",
                "200",
                "--stdin",
            ],
            &(tips.join("\n") + "\n"),
        )?
    };
    Ok(Browser {
        refs,
        history,
        root_fallback,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::fixture::Repo;
    #[test]
    fn roots_symbolic_private_bisect_original_and_namespace_refs() -> Result<(), String> {
        let r = Repo::new("all-refs")?;
        r.commit("base", &[])?;
        for name in [
            "refs/bisect/good-test",
            "refs/original/refs/heads/main",
            "refs/gitcraft/checkpoint",
            "refs/namespaces/team/refs/heads/main",
        ] {
            r.git(&["update-ref", name, "HEAD"])?;
        }
        r.git(&["symbolic-ref", "refs/gitcraft/current", "refs/heads/main"])?;
        r.git(&["symbolic-ref", "refs/gitcraft/root", "HEAD"])?;
        let b = read(r.path(), "refs/namespaces/team")?;
        assert!(
            b.refs
                .iter()
                .any(|x| x.name == "HEAD" && x.symbolic == "refs/heads/main")
        );
        assert!(
            b.refs
                .iter()
                .any(|x| x.name == "refs/gitcraft/current" && x.symbolic == "refs/heads/main")
        );
        assert!(b.refs.iter().any(|x| x.name.starts_with("refs/original/")));
        assert!(b.history.contains("base"));
        let root = b
            .refs
            .iter()
            .find(|v| v.name == "refs/gitcraft/root")
            .ok_or("missing symbolic root ref")?;
        assert_eq!(root.symbolic, "HEAD");
        assert!(read(r.path(), &root.symbolic)?.history.contains("base"));
        assert!(read(r.path(), "--all").is_err());
        Ok(())
    }
}
