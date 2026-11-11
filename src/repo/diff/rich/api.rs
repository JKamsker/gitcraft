//! API contract changes: an OpenAPI (or Swagger 2) document before and after the change,
//! compared for what breaks its clients: endpoints removed, parameters and request fields
//! that became required, request values narrowed, and responses that lost codes, fields or
//! guarantees. Each change has its place in the file.

use std::collections::{HashMap, HashSet};

use super::Sides;
use super::config::{self, same};
use super::tree::{Field, Node};

const METHODS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];
/// How deep schemas are compared: far enough for real APIs, bounded for odd ones.
const MAX_SCHEMA_DEPTH: usize = 12;
const MAX_REFS: usize = 32;

/// How bad a change is for the API's clients.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Clients that worked before can fail now.
    Breaking,
    /// Added: nothing breaks.
    Added,
}

impl Severity {
    pub(super) fn label(self) -> &'static str {
        crate::i18n::text(match self {
            Self::Breaking => "repo-core-change-breaking",
            Self::Added => "repo-core-change-added",
        })
    }
}

/// One change, where it is: the endpoint it is about, what happened, and its line on each side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiChange {
    pub severity: Severity,
    /// `GET /pets/{id}`, or empty for the document as a whole.
    pub endpoint: String,
    pub what: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApiDiff {
    /// `OpenAPI 3.1.0`, `Swagger 2.0`.
    pub version: String,
    pub changes: Vec<ApiChange>,
}

/// A document's root, to follow `$ref`s in.
#[derive(Clone, Copy)]
struct Doc<'a> {
    root: &'a Node,
}

fn get<'a>(node: &'a Node, key: &str) -> Option<&'a Field> {
    match node {
        Node::Map(fields) => fields.iter().find(|f| f.key == key),
        _ => None,
    }
}

fn text(node: &Node) -> Option<&str> {
    match node {
        Node::Scalar { text, .. } => Some(text),
        _ => None,
    }
}

fn flag(node: &Node, key: &str) -> bool {
    get(node, key).and_then(|f| text(&f.node)) == Some("true")
}

fn strings(node: Option<&Field>) -> Vec<String> {
    match node.map(|f| &f.node) {
        Some(Node::List(items)) => items
            .iter()
            .filter_map(|i| text(&i.node))
            .map(str::to_owned)
            .collect(),
        Some(Node::Scalar { text, .. }) => vec![text.clone()],
        _ => Vec::new(),
    }
}

/// The version the document says it is, if it is an OpenAPI or Swagger document.
pub fn version_of(root: &Node) -> Option<String> {
    let v = |key: &str, name: &str| {
        get(root, key)
            .and_then(|f| text(&f.node))
            .map(|t| format!("{name} {t}"))
    };
    v("openapi", "OpenAPI").or_else(|| v("swagger", "Swagger"))
}

/// The `$ref` a node is, if it is one.
fn ref_of(node: &Node) -> Option<&str> {
    get(node, "$ref").and_then(|f| text(&f.node))
}

impl<'a> Doc<'a> {
    /// `node` with its `$ref`s followed (local ones, `#/…`), and the line it is at.
    fn resolve(self, node: &'a Node, line: u32) -> (&'a Node, u32) {
        let (mut node, mut line) = (node, line);
        for _ in 0..MAX_REFS {
            let Some(found) = ref_of(node).and_then(|r| self.pointer(r)) else {
                break;
            };
            (node, line) = (&found.node, found.line);
        }
        (node, line)
    }

    /// The field a JSON pointer (`#/components/schemas/Pet`) names.
    fn pointer(self, target: &str) -> Option<&'a Field> {
        let path = target.strip_prefix("#/")?;
        let mut node = self.root;
        let mut found = None;
        for step in path.split('/') {
            let step = step.replace("~1", "/").replace("~0", "~");
            let field = get(node, &step)?;
            node = &field.node;
            found = Some(field);
        }
        found
    }
}

/// Compares both sides of an API document; `Err` when they can't be read or aren't one.
pub fn diff(path: &str, sides: &Sides) -> Result<ApiDiff, String> {
    let (_, old, new) = config::read(path, sides)?;
    let side_version = |n: &Option<Node>| n.as_ref().and_then(version_of);
    let version = side_version(&new)
        .or_else(|| side_version(&old))
        .ok_or(crate::i18n::text("repo-core-1985"))?;
    let empty = Node::Map(Vec::new());
    let mut out = Vec::new();
    match (&old, &new) {
        (Some(_), None) => out.push(change(
            Severity::Breaking,
            "",
            crate::i18n::text("repo-core-1986"),
            Some(1),
            None,
        )),
        _ => {
            // A new document: every endpoint is added.
            let old = old.as_ref().unwrap_or(&empty);
            let new = new.as_ref().unwrap_or(&empty);
            endpoints(Doc { root: old }, Doc { root: new }, &mut out);
        }
    }
    out.sort_by_key(|c| c.severity);
    Ok(ApiDiff {
        version,
        changes: out,
    })
}

fn change(
    severity: Severity,
    endpoint: &str,
    what: impl Into<String>,
    old_line: Option<u32>,
    new_line: Option<u32>,
) -> ApiChange {
    ApiChange {
        severity,
        endpoint: endpoint.to_owned(),
        what: what.into(),
        old_line,
        new_line,
    }
}

/// An operation: its name (`GET /pets/{id}`), the path's field and the operation's.
type Op<'a> = (String, &'a Field, &'a Field);

/// Every operation of `doc`, by the shape of its path (`/pets/{}`): a path parameter renamed
/// is the same endpoint.
fn operations(doc: Doc<'_>) -> Vec<(String, Op<'_>)> {
    let Some(Field {
        node: Node::Map(paths),
        ..
    }) = get(doc.root, "paths")
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for p in paths {
        let shape = shape(&p.key);
        for m in METHODS {
            if let Some(op) = get(&p.node, m) {
                let name = format!("{} {}", m.to_uppercase(), p.key);
                out.push((format!("{m} {shape}"), (name, p, op)));
            }
        }
    }
    out
}

/// A path with the names of its parameters left out.
fn shape(path: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in path.chars() {
        match c {
            '{' => {
                inside = true;
                out.push('{');
            }
            '}' => {
                inside = false;
                out.push('}');
            }
            _ if inside => {}
            _ => out.push(c),
        }
    }
    out
}

fn endpoints(a: Doc, b: Doc, out: &mut Vec<ApiChange>) {
    let (old_ops, new_ops) = (operations(a), operations(b));
    let new_by: HashMap<&str, &Op> = new_ops.iter().map(|(k, op)| (k.as_str(), op)).collect();
    let old_keys: HashSet<&str> = old_ops.iter().map(|(k, _)| k.as_str()).collect();
    for (key, (name, path, op)) in &old_ops {
        match new_by.get(key.as_str()) {
            None => out.push(change(
                Severity::Breaking,
                name,
                crate::i18n::text("repo-core-1989"),
                Some(op.line),
                None,
            )),
            Some((new_name, new_path, new_op)) => {
                let mut cx = Cx {
                    a,
                    b,
                    name: new_name.clone(),
                    out: &mut *out,
                    visited: HashSet::new(),
                };
                cx.operation((path, op), (new_path, new_op));
            }
        }
    }
    for (key, (name, _, op)) in &new_ops {
        if !old_keys.contains(key.as_str()) {
            out.push(change(
                Severity::Added,
                name,
                crate::i18n::text("repo-core-1990"),
                None,
                Some(op.line),
            ));
        }
    }
}

/// A parameter: its name, where it goes, whether it is required, its schema and its line.
struct Param<'a> {
    name: String,
    place: String,
    required: bool,
    node: &'a Node,
    line: u32,
}

impl Param<'_> {
    /// Whether it is the same parameter as `other`: a header's name in any case.
    fn is(&self, other: &Param) -> bool {
        self.place == other.place
            && if self.place == "header" {
                self.name.eq_ignore_ascii_case(&other.name)
            } else {
                self.name == other.name
            }
    }
}

/// The parameters of an operation and of its path, `$ref`s followed; the operation's win.
fn params<'a>(doc: Doc<'a>, path: &'a Field, op: &'a Field) -> Vec<Param<'a>> {
    let mut out: Vec<Param> = Vec::new();
    for owner in [op, path] {
        let Some(Field {
            node: Node::List(items),
            ..
        }) = get(&owner.node, "parameters")
        else {
            continue;
        };
        for item in items {
            let (node, line) = doc.resolve(&item.node, item.line);
            let field = |k: &str| {
                get(node, k)
                    .and_then(|f| text(&f.node))
                    .unwrap_or_default()
                    .to_owned()
            };
            let p = Param {
                name: field("name"),
                place: field("in"),
                required: field("in") == "path" || flag(node, "required"),
                node,
                line,
            };
            if !out.iter().any(|o| o.is(&p)) {
                out.push(p);
            }
        }
    }
    out
}

/// Which way a schema's values travel: what the client sends, or what it gets back.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Dir {
    Request,
    Response,
}

/// A parameter's schema: its `schema` (OpenAPI 3, Swagger body), or the parameter itself.
fn schema_of<'a>(doc: Doc<'a>, param: &'a Node, line: u32) -> (&'a Node, u32) {
    match get(param, "schema") {
        Some(f) => (&f.node, f.line),
        None => doc.resolve(param, line),
    }
}

/// A request body: whether it is required, its line, the media types it takes and its schema.
struct Body<'a> {
    required: bool,
    line: u32,
    media: Vec<String>,
    schema: Option<(&'a Node, u32)>,
}

fn body<'a>(doc: Doc<'a>, op: &'a Field, params: &[Param<'a>]) -> Option<Body<'a>> {
    if let Some(f) = get(&op.node, "requestBody") {
        let (node, line) = doc.resolve(&f.node, f.line);
        let (schema, media) = body_schema(doc, node, line);
        return Some(Body {
            required: flag(node, "required"),
            line,
            media,
            schema,
        });
    }
    let p = params.iter().find(|p| p.place == "body")?;
    let (schema, media) = body_schema(doc, p.node, p.line);
    Some(Body {
        required: p.required,
        line: p.line,
        media,
        schema,
    })
}

/// The schema of a body (`content` → a media type → `schema`, JSON preferred), or of a
/// Swagger 2 body parameter; and the media types it has.
fn body_schema<'a>(
    doc: Doc<'a>,
    body: &'a Node,
    line: u32,
) -> (Option<(&'a Node, u32)>, Vec<String>) {
    let _ = (doc, line);
    if let Some(f) = get(body, "schema") {
        return (Some((&f.node, f.line)), Vec::new());
    }
    let Some(Field {
        node: Node::Map(media),
        ..
    }) = get(body, "content")
    else {
        return (None, Vec::new());
    };
    let names = media.iter().map(|m| m.key.clone()).collect();
    let pick = media
        .iter()
        .find(|m| m.key.contains("json"))
        .or_else(|| media.first());
    let schema = pick
        .and_then(|m| get(&m.node, "schema"))
        .map(|f| (&f.node, f.line));
    (schema, names)
}

/// A schema's properties (`allOf` parts merged) and its required ones.
fn properties<'a>(doc: Doc<'a>, node: &'a Node) -> (Vec<&'a Field>, Vec<String>) {
    let mut props = Vec::new();
    let mut required = strings(get(node, "required"));
    if let Some(Field {
        node: Node::Map(fields),
        ..
    }) = get(node, "properties")
    {
        props.extend(fields.iter());
    }
    if let Some(Field {
        node: Node::List(parts),
        ..
    }) = get(node, "allOf")
    {
        for part in parts.iter().take(MAX_REFS) {
            let (part, _) = doc.resolve(&part.node, part.line);
            if let Node::Map(_) = part
                && get(part, "allOf").is_none()
            {
                let (p, r) = properties(doc, part);
                props.extend(p);
                required.extend(r);
            }
        }
    }
    (props, required)
}

/// The types a schema allows (`type`, or a list of them; `nullable: true` adds `null`).
fn types_of(node: &Node) -> Option<HashSet<String>> {
    let mut types: HashSet<String> = strings(get(node, "type")).into_iter().collect();
    if types.is_empty() {
        return None;
    }
    if flag(node, "nullable") {
        types.insert("null".to_owned());
    }
    Some(types)
}

/// Whether a schema follows a `$ref` anywhere in it.
fn refers(node: &Node) -> bool {
    match node {
        Node::Scalar { .. } => false,
        Node::Map(fields) => fields.iter().any(|f| f.key == "$ref" || refers(&f.node)),
        Node::List(items) => items.iter().any(|i| refers(&i.node)),
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

mod schema;

mod operations;

use operations::{At, Cx};
