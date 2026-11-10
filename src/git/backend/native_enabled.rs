use super::*;
use gix::bstr::ByteSlice;
use std::collections::HashSet;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn text(bytes: &gix::bstr::BStr) -> Result<String, String> {
    bytes.to_str().map(str::to_owned).map_err(err)
}
/// Open a fresh handle for each read. Cloned gix repositories can retain their index
/// and packed-refs snapshots when a file is rewritten within the same timestamp tick.
/// A fresh handle keeps staged changes and refs immediately visible to native readers.
pub(super) fn open(path: &Path) -> Result<gix::Repository, String> {
    let mut permissions = gix::open::Permissions::default();
    permissions.config.git_binary = true;
    gix::discover_opts(
        path,
        Default::default(),
        gix::open::Options::default()
            .permissions(permissions)
            .bail_if_untrusted(true)
            .strict_config(true),
    )
    .map_err(err)
}
pub(super) fn head(path: &Path) -> Result<String, String> {
    let repo = open(path)?;
    let head = repo.head().map_err(err)?;
    if head.is_unborn() {
        return Ok(String::new());
    }
    Ok(head.id().map(|id| id.to_string()).unwrap_or_default())
}
pub(super) fn git_dir(path: &Path) -> Result<PathBuf, String> {
    let repo = open(path)?;
    std::fs::canonicalize(repo.git_dir()).map_err(err)
}
pub(super) fn remotes(path: &Path) -> Result<Vec<super::super::Remote>, String> {
    let repo = open(path)?;
    let config = repo.config_snapshot();
    // Git's URL rewriting and legacy remotes files remain the CLI's responsibility.
    if config.plumbing().sections_by_name("url").is_some()
        || repo.common_dir().join("remotes").exists()
        || repo.common_dir().join("branches").exists()
    {
        return Err("legacy or rewritten remote".into());
    }
    repo.remote_names()
        .into_iter()
        .map(|name| {
            let name = text(name.as_bstr())?;
            if name.contains(['\t', '\r', '\n']) {
                return Err("remote display requires Git".into());
            }
            let values = config.plumbing().strings(&format!("remote.{name}.url"));
            let url = values
                .and_then(|v| v.into_iter().next())
                .ok_or("remote has no fetch URL")?;
            let url = text(url.as_bstr())?;
            if url.contains(['\r', '\n']) {
                return Err("remote display requires Git".into());
            }
            Ok(super::super::Remote { name, url })
        })
        .collect()
}
fn reachable(
    repo: &gix::Repository,
    id: gix::ObjectId,
    remaining_visits: &mut usize,
) -> Result<HashSet<gix::ObjectId>, String> {
    let mut ids = HashSet::new();
    for info in repo
        .find_object(id)
        .map_err(err)?
        .id()
        .ancestors()
        .all()
        .map_err(err)?
    {
        if *remaining_visits == 0 {
            return Err("tracking graph exceeds native bound".into());
        }
        *remaining_visits -= 1;
        ids.insert(info.map_err(err)?.id);
    }
    Ok(ids)
}
pub(super) fn refs(path: &Path) -> Result<super::super::Refs, String> {
    refs_bounded(path, 100_000)
}
pub(super) fn refs_bounded(path: &Path, visit_budget: usize) -> Result<super::super::Refs, String> {
    let repo = open(path)?;
    if repo.common_dir().join("info/grafts").exists() || repo.common_dir().join("shallow").exists()
    {
        return Err("history-overlays".into());
    }
    let head = repo.head().map_err(err)?;
    let head_name = head
        .referent_name()
        .map(|name| text(name.as_bstr()))
        .transpose()?;
    let head_id = if head.is_unborn() {
        String::new()
    } else {
        head.id().map(|id| id.to_string()).unwrap_or_default()
    };
    let names: Vec<String> = repo
        .remote_names()
        .into_iter()
        .map(|n| text(n.as_bstr()))
        .collect::<Result<_, _>>()?;
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut out = super::super::Refs {
        head: head_id,
        ..Default::default()
    };
    let platform = repo.references().map_err(err)?;
    let mut list = platform
        .all()
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    if list
        .iter()
        .any(|r| r.name().as_bstr().starts_with(b"refs/replace/"))
    {
        return Err("history-overlays".into());
    }
    list.sort_by(|a, b| a.name().as_bstr().cmp(b.name().as_bstr()));
    // Bound total traversal work for the whole refresh, including repeated visits
    // to shared history across many diverged branches.
    let mut remaining_visits = visit_budget;
    for mut reference in list {
        let full = text(reference.name().as_bstr())?;
        let prefix = ["refs/heads/", "refs/remotes/", "refs/tags/"]
            .into_iter()
            .find(|p| full.starts_with(p));
        let Some(prefix) = prefix else {
            continue;
        };
        let short = &full[prefix.len()..];
        if prefix == "refs/remotes/" && short.ends_with("/HEAD") {
            continue;
        }
        if prefix == "refs/heads/"
            && let Some(remote) = reference.remote_name(gix::remote::Direction::Push)
        {
            let remote = text(remote.as_bstr())?;
            if !remote.is_empty() {
                out.push_remotes.insert(short.to_owned(), remote);
            }
        }
        let raw = reference
            .try_id()
            .ok_or("symbolic reference requires Git")?;
        let object = raw.object().map_err(err)?;
        let target = if object.kind == gix::object::Kind::Tag {
            if prefix != "refs/tags/" {
                return Err("non-commit branch requires Git".into());
            }
            reference.peel_to_id().map_err(err)?.to_string()
        } else {
            raw.to_string()
        };
        let upstream = if prefix == "refs/heads/" {
            // Gitoxide's remote-tracking resolver returns None for the special
            // local repository remote. Git still displays and tracks that upstream.
            if repo
                .config_snapshot()
                .string(&format!("branch.{short}.remote"))
                .is_some_and(|remote| remote.as_bstr() == b".")
            {
                return Err("local upstream requires Git".into());
            }
            reference
                .remote_tracking_ref_name(gix::remote::Direction::Fetch)
                .transpose()
                .map_err(err)?
        } else {
            None
        };
        let mut upstream_name = String::new();
        let mut track = String::new();
        if let Some(upstream) = upstream {
            let upstream_full = text(upstream.as_bstr())?;
            // Short-name ambiguity uses Git's exact disambiguation rules.
            let shortened = upstream_full
                .strip_prefix("refs/remotes/")
                .or_else(|| upstream_full.strip_prefix("refs/heads/"))
                .ok_or("unsupported upstream namespace")?;
            // Git's DWIM expansion also considers refs/<name>, local branches,
            // remote refs and a remote's HEAD. Any competing spelling can change
            // %(upstream:short), so keep the CLI authoritative for that operation.
            let candidates = [
                format!("refs/{shortened}"),
                format!("refs/tags/{shortened}"),
                format!("refs/heads/{shortened}"),
                format!("refs/remotes/{shortened}"),
                format!("refs/remotes/{shortened}/HEAD"),
            ];
            for candidate in candidates {
                if candidate != upstream_full
                    && repo
                        .try_find_reference(candidate.as_str())
                        .map_err(err)?
                        .is_some()
                {
                    return Err("ambiguous upstream short name".into());
                }
            }
            // Literal pseudo-ref candidates live outside refs/. These filesystem
            // checks are conservative; an invalid candidate also safely uses Git.
            if repo.git_dir().join(shortened).is_file()
                || repo.common_dir().join(shortened).is_file()
            {
                return Err("ambiguous upstream short name".into());
            }
            upstream_name = shortened.to_owned();
            match repo
                .try_find_reference(upstream_full.as_str())
                .map_err(err)?
            {
                None => track = "[gone]".into(),
                Some(mut remote) => {
                    let local_id = reference.peel_to_id().map_err(err)?.detach();
                    let remote_id = remote.peel_to_id().map_err(err)?.detach();
                    if local_id != remote_id {
                        let ours = reachable(&repo, local_id, &mut remaining_visits)?;
                        let theirs = reachable(&repo, remote_id, &mut remaining_visits)?;
                        let ahead = ours.difference(&theirs).count();
                        let behind = theirs.difference(&ours).count();
                        track = match (ahead, behind) {
                            (0, 0) => String::new(),
                            (a, 0) => format!("[ahead {a}]"),
                            (0, b) => format!("[behind {b}]"),
                            (a, b) => format!("[ahead {a}, behind {b}]"),
                        };
                    }
                }
            }
        }
        let branch = super::super::Branch {
            name: short.to_owned(),
            target,
            is_head: head_name.as_deref() == Some(full.as_str()),
            upstream: upstream_name,
            track,
        };
        match prefix {
            "refs/heads/" => {
                if branch.is_head {
                    out.current = Some(branch.name.clone());
                }
                out.local.push(branch);
            }
            "refs/tags/" => out.tags.push(branch),
            _ => {
                let remote = super::super::refs::remote_of(short, &names).to_owned();
                match out.remotes.iter_mut().find(|(r, _)| r == &remote) {
                    Some((_, branches)) => branches.push(branch),
                    None => out.remotes.push((remote, vec![branch])),
                }
            }
        }
    }
    Ok(out)
}
