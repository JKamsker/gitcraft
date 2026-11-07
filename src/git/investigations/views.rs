//! Relation-based presets, deliberately separate from search operators or graph ordering.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
const MAX: usize = 30_000;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Kind {
    MyUnpushed,
    RecentAncestors,
    Descendants { anchor: String },
    MergesInto { branch: String },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Preset {
    pub name: String,
    pub kind: Kind,
    pub days: u32,
}
impl Preset {
    pub fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.chars().count() <= 80
            && !self.name.chars().any(char::is_control)
            && (1..=3650).contains(&self.days)
            && match &self.kind {
                Kind::Descendants { anchor } => oid(anchor),
                Kind::MergesInto { branch } => {
                    !branch.is_empty()
                        && branch.len() <= 4096
                        && !branch.chars().any(char::is_control)
                }
                _ => true,
            }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Scope {
    /// Immutable tips of the graph's currently visible commit history.
    pub tips: Vec<String>,
    pub remotes: Vec<String>,
    pub email: String,
    pub default_branch: Option<(String, String)>,
    pub now: i64,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Selection {
    pub commits: BTreeSet<String>,
    pub limited: bool,
    pub explanation: String,
}
fn revisions(include: &[String], exclude: &[String]) -> Result<Vec<u8>, String> {
    if include.len() + exclude.len() > MAX {
        return Err("View scope exceeds 30000 pinned revision tips".into());
    }
    let mut input = Vec::new();
    for (ids, prefix) in [(include, ""), (exclude, "^")] {
        for id in ids {
            if !oid(id) {
                return Err("View has an invalid immutable commit ID".into());
            }
            input.extend_from_slice(format!("{prefix}{id}\n").as_bytes());
        }
    }
    Ok(input)
}
fn walk(
    reader: &Reader<'_>,
    include: &[String],
    exclude: &[String],
    extra: &[&str],
) -> Result<(BTreeSet<String>, bool), String> {
    if include.is_empty() {
        return Ok((BTreeSet::new(), false));
    }
    let mut args = vec!["rev-list", "--max-count=30001"];
    args.extend_from_slice(extra);
    args.push("--stdin");
    let out = reader.run(&args, revisions(include, exclude)?, &[0])?;
    let text = std::str::from_utf8(&out).map_err(|_| "View returned invalid commit IDs")?;
    let mut ids = BTreeSet::new();
    let mut limited = false;
    for id in text.lines() {
        if !oid(id) {
            return Err("View returned an invalid commit ID".into());
        }
        if ids.len() == MAX {
            limited = true;
            break;
        }
        ids.insert(id.into());
    }
    Ok((ids, limited))
}
fn mine(
    reader: &Reader<'_>,
    ids: &BTreeSet<String>,
    email: &str,
    since: Option<i64>,
) -> Result<BTreeSet<String>, String> {
    if email.is_empty() || email.contains(['\0', '\n', '\r']) {
        return Err(
            "The current Git author email is unavailable; configure an author first".into(),
        );
    }
    if ids.is_empty() {
        return Ok(BTreeSet::new());
    }
    let include: Vec<_> = ids.iter().cloned().collect();
    let out = reader.run(
        &[
            "log",
            "--no-walk",
            "--no-show-signature",
            "--encoding=UTF-8",
            "--format=%H%x00%ae%x00%at%x00",
            "--stdin",
        ],
        revisions(&include, &[])?,
        &[0],
    )?;
    let fields: Vec<_> = out.split(|b| *b == 0).collect();
    let mut selected = BTreeSet::new();
    for record in fields.chunks(3) {
        if record.len() == 1 && record[0].iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let [id, author, time] = record else {
            return Err("Truncated author view metadata".into());
        };
        let id = std::str::from_utf8(id)
            .map_err(|_| "Invalid author-view object ID")?
            .trim_start_matches('\n');
        if !oid(id) || !ids.contains(id) {
            return Err("Author view metadata does not match its pinned inputs".into());
        }
        let time = std::str::from_utf8(time)
            .map_err(|_| "Invalid author time")?
            .parse::<i64>()
            .map_err(|_| "Invalid author time")?;
        if *author == email.as_bytes() && since.is_none_or(|since| time >= since) {
            selected.insert(id.into());
        }
    }
    Ok(selected)
}
pub(crate) fn query(
    repo: &Path,
    preset: &Preset,
    scope: &Scope,
    active: &dyn Fn() -> bool,
) -> Result<Selection, String> {
    if !preset.valid() {
        return Err("Saved graph view is invalid".into());
    }
    let profile = crate::connections::current();
    profile.run(||{
        let reader=Reader::new(repo,active);
        let shallow=reader.run(&["rev-parse","--is-shallow-repository"],Vec::new(),&[0])?;
        let shallow=match shallow.trim_ascii(){b"true"=>true,b"false"=>false,_=>return Err("Invalid shallow-history metadata".into())};
        let (commits,limited,mut explanation)=match &preset.kind{
            Kind::MyUnpushed=>{
                let (ids,limited)=walk(&reader,&scope.tips,&scope.remotes,&[])?;
                (mine(&reader,&ids,&scope.email,None)?,limited,format!("Commits authored as {} that are absent from every pinned remote-tracking branch",scope.email))
            }
            Kind::RecentAncestors=>{
                let (ids,mut limited)=walk(&reader,&scope.tips,&[],&[])?;
                let cutoff=scope.now.saturating_sub(i64::from(preset.days)*86400);
                let mine=mine(&reader,&ids,&scope.email,Some(cutoff))?;
                let tips:Vec<_>=mine.into_iter().collect();let (ancestors,more)=walk(&reader,&tips,&[],&[])?;limited|=more;
                (ancestors,limited,format!("My last {} days (author email {}) and their ancestors, within the visible graph scope",preset.days,scope.email))
            }
            Kind::Descendants{anchor}=>{
                let (mut ids,limited)=walk(&reader,&scope.tips,std::slice::from_ref(anchor),&["--ancestry-path"])?;ids.insert(anchor.clone());
                (ids,limited,"The pinned anchor and descendants reachable from the visible graph tips".into())
            }
            Kind::MergesInto{branch}=>{
                let (actual,target)=scope.default_branch.as_ref().ok_or("No default branch is available; choose a branch for this view")?;
                if actual!=branch{return Err("The selected default branch changed; update this view".into());}
                let (ids,limited)=walk(&reader,std::slice::from_ref(target),&[],&["--first-parent","--merges"])?;
                (ids,limited,format!("Merge commits on the first-parent history of {branch}"))
            }
        };
        if shallow {explanation.push_str(" · shallow history: ancestry stops at the local boundary");}
        if !reader.active(){return Err("Saved graph view cancelled".into());}
        Ok(Selection{commits,limited,explanation})
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persisted_presets_and_revision_inputs_reject_options_and_control_bytes() {
        assert!(revisions(&["--all".into()], &[]).is_err());
        let preset = Preset {
            name: "View".into(),
            kind: Kind::Descendants {
                anchor: "-malicious".into(),
            },
            days: 30,
        };
        assert!(!preset.valid());
        let preset = Preset {
            name: "View".into(),
            kind: Kind::MyUnpushed,
            days: 30,
        };
        assert!(preset.valid());
        let json = serde_json::to_string(&preset).unwrap_or_default();
        assert_eq!(serde_json::from_str::<Preset>(&json).ok(), Some(preset));
    }
}
