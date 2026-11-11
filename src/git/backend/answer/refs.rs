//! `for-each-ref --format=<format> [--sort=[-]committerdate] [<pattern>…]` with the
//! atoms `refname`, `objectname`, `objecttype`, `symref`, `parent`, `creatordate:unix`,
//! `committerdate:unix` and `contents` (of commits), in the main worktree.

use super::*;

enum Piece {
    Text(Vec<u8>),
    RefName,
    ObjectName,
    ObjectType,
    SymRef,
    Parent,
    CreatorDate,
    CommitterDate,
    Contents,
}

/// Parse the supported `for-each-ref` format atoms and escaped percent signs.
fn format(text: &str) -> Option<Vec<Piece>> {
    let bytes = text.as_bytes();
    let mut pieces = Vec::new();
    let mut literal = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            literal.push(bytes[i]);
            i += 1;
            continue;
        }
        match bytes.get(i + 1)? {
            b'%' => {
                literal.push(b'%');
                i += 2;
            }
            b'(' => {
                let end = i + text[i..].find(')')?;
                let piece = match &text[i + 2..end] {
                    "refname" => Piece::RefName,
                    "objectname" => Piece::ObjectName,
                    "objecttype" => Piece::ObjectType,
                    "symref" => Piece::SymRef,
                    "parent" => Piece::Parent,
                    "creatordate:unix" => Piece::CreatorDate,
                    "committerdate:unix" => Piece::CommitterDate,
                    "contents" => Piece::Contents,
                    _ => return None,
                };
                if !literal.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut literal)));
                }
                pieces.push(piece);
                i = end + 1;
            }
            _ => {
                let hex = text.get(i + 1..i + 3)?;
                literal.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
        }
    }
    if !literal.is_empty() {
        pieces.push(Piece::Text(literal));
    }
    Some(pieces)
}

/// A pattern as `for-each-ref` matches it: in full or up to a `/`, or as a glob of `*`
/// (not across `/`) and `**` (whole folders).
struct Pattern {
    text: String,
    glob: Option<regex::Regex>,
}

impl Pattern {
    /// Accept only reference patterns whose matching semantics are implemented here.
    fn new(text: &str) -> Option<Self> {
        if text.is_empty() || text.contains(['?', '[', '\\']) {
            return None;
        }
        let glob = if text.contains('*') {
            let mut re = String::from("^");
            let parts: Vec<&str> = text.split('/').collect();
            for (i, part) in parts.iter().enumerate() {
                let last = i + 1 == parts.len();
                if *part == "**" {
                    re.push_str(if last { ".*" } else { "(?:.*/)?" });
                    continue;
                }
                if part.contains("**") {
                    return None;
                }
                let pieces: Vec<String> = part.split('*').map(regex::escape).collect();
                re.push_str(&pieces.join("[^/]*"));
                if !last {
                    re.push('/');
                }
            }
            re.push('$');
            Some(regex::Regex::new(&re).ok()?)
        } else {
            None
        };
        Some(Self {
            text: text.to_owned(),
            glob,
        })
    }

    /// Check whether a reference name matches this validated prefix or glob.
    fn matches(&self, name: &str) -> bool {
        if let Some(rest) = name.strip_prefix(self.text.as_str())
            && (rest.is_empty() || rest.starts_with('/') || self.text.ends_with('/'))
        {
            return true;
        }
        self.glob.as_ref().is_some_and(|g| g.is_match(name))
    }
}

struct Listed {
    name: String,
    id: gix::ObjectId,
    symref: Option<String>,
}

/// Which refs `--merged=<commit>` and `--contains=<commit>` keep.
enum Filter<'a> {
    /// Whose tip `commit` reaches.
    Merged(&'a str),
    /// Whose tip reaches `commit`.
    Contains(&'a str),
}

struct Query<'a> {
    pieces: Vec<Piece>,
    sort: Option<bool>,
    patterns: Vec<Pattern>,
    filters: Vec<Filter<'a>>,
}

/// Separate the supported CLI grammar from repository traversal and formatting.
fn query<'a>(args: &[&'a str]) -> Option<Query<'a>> {
    let mut pieces = None;
    let mut sort = None;
    let mut patterns = Vec::new();
    let mut filters = Vec::new();
    for arg in args {
        if let Some(rev) = arg.strip_prefix("--merged=") {
            filters.push(Filter::Merged(rev));
        } else if let Some(rev) = arg.strip_prefix("--contains=") {
            filters.push(Filter::Contains(rev));
        } else if let Some(f) = arg.strip_prefix("--format=") {
            if pieces.is_some() {
                return None;
            }
            pieces = Some(format(f)?);
        } else if let Some(key) = arg.strip_prefix("--sort=") {
            if sort.is_some() {
                return None;
            }
            sort = Some(match key {
                "committerdate" => false,
                "-committerdate" => true,
                _ => return None,
            });
        } else if arg.starts_with('-') {
            return None;
        } else {
            patterns.push(Pattern::new(arg)?);
        }
    }
    Some(Query {
        pieces: pieces?,
        sort,
        patterns,
        filters,
    })
}

/// Format supported `git for-each-ref` queries using the current ref snapshot.
pub(super) fn answer(path: &Path, args: &[&str]) -> Option<Answer> {
    let Query {
        pieces,
        sort,
        patterns,
        filters,
    } = query(args)?;
    let repo = open(path)?;
    plain(&repo)?;
    // A linked worktree has refs of its own besides the shared ones.
    if repo.git_dir() != repo.common_dir() {
        return None;
    }
    let platform = repo.references().ok()?;
    let mut listed = Vec::new();
    for reference in platform.all().ok()? {
        let reference = reference.ok()?;
        let name = reference.name().as_bstr().to_str().ok()?.to_owned();
        if !patterns.is_empty() && !patterns.iter().any(|p| p.matches(&name)) {
            continue;
        }
        let symref = match reference.target() {
            gix::refs::TargetRef::Symbolic(target) => {
                Some(target.as_bstr().to_str().ok()?.to_owned())
            }
            gix::refs::TargetRef::Object(_) => None,
        };
        // A dangling symbolic ref is skipped by git with a warning.
        let id = target_id(&repo, &reference).ok()?;
        listed.push(Listed { name, id, symref });
    }
    listed.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
    if filters.len() > 1 {
        return None;
    }
    if let Some(filter) = filters.first() {
        let (Filter::Merged(rev) | Filter::Contains(rev)) = filter;
        let commit = resolve(&repo, &format!("{rev}^{{commit}}")).ok()??;
        let cache = repo.commit_graph_if_enabled().ok()?;
        let mut graph = repo.revision_graph(cache.as_ref());
        let mut kept = Vec::new();
        for item in listed {
            // Tips that aren't commits (a tag of a tree…) are never listed.
            let Some(tip) = peel_to(&repo, item.id, gix::object::Kind::Commit).ok()? else {
                continue;
            };
            let (ancestor, descendant) = match filter {
                Filter::Merged(_) => (tip, commit),
                Filter::Contains(_) => (commit, tip),
            };
            let reaches = ancestor == descendant
                || repo
                    .merge_base_with_graph(ancestor, descendant, &mut graph)
                    .ok()?
                    .is_some_and(|base| base == ancestor);
            if reaches {
                kept.push(item);
            }
        }
        listed = kept;
    }
    if let Some(descending) = sort {
        let mut dated = Vec::new();
        for item in listed {
            let object = repo.find_object(item.id).ok()?;
            if object.kind != gix::object::Kind::Commit {
                return None;
            }
            let commit = gix::objs::CommitRef::from_bytes(&object.data, repo.object_hash()).ok()?;
            let seconds = commit.committer().ok()?.time().ok()?.seconds;
            dated.push((seconds, item));
        }
        dated.sort_by_key(|(seconds, _)| *seconds);
        // Which of two refs of the same second git lists first is left to it.
        if dated.windows(2).any(|w| w[0].0 == w[1].0) {
            return None;
        }
        if descending {
            dated.reverse();
        }
        listed = dated.into_iter().map(|(_, item)| item).collect();
    }
    let needs_object = pieces.iter().any(|p| {
        matches!(
            p,
            Piece::ObjectType
                | Piece::Parent
                | Piece::CreatorDate
                | Piece::CommitterDate
                | Piece::Contents
        )
    });
    let mut out = Vec::new();
    for item in &listed {
        let object = if needs_object {
            Some(repo.find_object(item.id).ok()?)
        } else {
            None
        };
        for piece in &pieces {
            match piece {
                Piece::Text(text) => out.extend_from_slice(text),
                Piece::RefName => out.extend_from_slice(item.name.as_bytes()),
                Piece::ObjectName => out.extend_from_slice(item.id.to_string().as_bytes()),
                Piece::SymRef => {
                    out.extend_from_slice(item.symref.as_deref().unwrap_or("").as_bytes());
                }
                piece => {
                    let object = object.as_ref()?;
                    field(&repo, object, piece, &mut out)?;
                }
            }
        }
        out.push(b'\n');
    }
    Some(Answer::out(out))
}

/// What an atom about the object itself prints.
fn field(
    repo: &gix::Repository,
    object: &gix::Object<'_>,
    piece: &Piece,
    out: &mut Vec<u8>,
) -> Option<()> {
    use gix::object::Kind;
    if let Piece::ObjectType = piece {
        out.extend_from_slice(object.kind.as_bytes());
        return Some(());
    }
    match object.kind {
        Kind::Commit => {
            let commit = gix::objs::CommitRef::from_bytes(&object.data, repo.object_hash()).ok()?;
            match piece {
                Piece::Parent => {
                    let parents: Vec<String> = commit.parents().map(|p| p.to_string()).collect();
                    out.extend_from_slice(parents.join(" ").as_bytes());
                }
                Piece::CreatorDate | Piece::CommitterDate => {
                    let seconds = commit.committer().ok()?.time().ok()?.seconds;
                    out.extend_from_slice(seconds.to_string().as_bytes());
                }
                Piece::Contents => out.extend_from_slice(commit.message),
                _ => return None,
            }
        }
        Kind::Tag => {
            let tag = gix::objs::TagRef::from_bytes(&object.data, repo.object_hash()).ok()?;
            match piece {
                Piece::Parent | Piece::CommitterDate => {}
                Piece::CreatorDate => {
                    if let Some(tagger) = tag.tagger().ok()? {
                        let seconds = tagger.time().ok()?.seconds;
                        out.extend_from_slice(seconds.to_string().as_bytes());
                    }
                }
                // A tag's signature is part of its contents or not depending on the atom.
                _ => return None,
            }
        }
        Kind::Blob | Kind::Tree => match piece {
            Piece::Parent | Piece::CreatorDate | Piece::CommitterDate => {}
            _ => return None,
        },
    }
    Some(())
}
