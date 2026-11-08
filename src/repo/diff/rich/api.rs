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
    let version = side_version(&new).or_else(|| side_version(&old)).ok_or(
        crate::i18n::text("repo-core-1985"),
    )?;
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

/// Comparing one operation on both sides.
struct Cx<'a, 'o> {
    a: Doc<'a>,
    b: Doc<'a>,
    /// The endpoint, as it is named now.
    name: String,
    out: &'o mut Vec<ApiChange>,
    /// The pairs of `$ref`s compared already for this operation (each compared once).
    visited: HashSet<(String, String, Dir)>,
}

/// A schema being compared: where it is in the body or parameter (`a.b[].c`), what holds it
/// (`Request body`, `Response 200`, `Query parameter limit`).
#[derive(Clone)]
struct At {
    owner: String,
    path: String,
}

impl At {
    fn child(&self, key: &str) -> At {
        At {
            owner: self.owner.clone(),
            path: if self.path.is_empty() {
                key.to_owned()
            } else {
                format!("{}.{key}", self.path)
            },
        }
    }

    /// `Request body field `a.b``, or the owner itself at its top.
    fn name(&self) -> String {
        if self.path.is_empty() {
            self.owner.clone()
        } else {
            crate::i18n::message("repo-core-1996", &[("owner", format!("{}", self.owner).into()), ("path", format!("{}", self.path).into())])
        }
    }
}

impl<'a> Cx<'a, '_> {
    fn breaking(&mut self, what: String, old: Option<u32>, new: Option<u32>) {
        let c = change(Severity::Breaking, &self.name, what, old, new);
        self.out.push(c);
    }

    fn operation(
        &mut self,
        (old_path, old_op): (&'a Field, &'a Field),
        (new_path, new_op): (&'a Field, &'a Field),
    ) {
        let (a, b) = (self.a, self.b);
        let (old_params, new_params) = (params(a, old_path, old_op), params(b, new_path, new_op));
        for p in &new_params {
            // A body parameter (Swagger 2) is the request body, compared below.
            if p.place == "body" {
                continue;
            }
            let label = crate::i18n::message("repo-core-1997", &[("place", format!("{}", cap(&p.place)).into()), ("name", format!("{}", p.name).into())]);
            match old_params.iter().find(|o| o.is(p)) {
                None if p.required => self.breaking(
                    crate::i18n::message("repo-core-1998", &[("place", format!("{}", p.place).into()), ("name", format!("{}", p.name).into())]),
                    None,
                    Some(p.line),
                ),
                Some(o) if p.required && !o.required => self.breaking(
                    crate::i18n::message("repo-core-1999", &[("label", format!("{}", label).into())]),
                    Some(o.line),
                    Some(p.line),
                ),
                Some(o) => {
                    let old = schema_of(a, o.node, o.line);
                    let new = schema_of(b, p.node, p.line);
                    let at = At {
                        owner: label,
                        path: String::new(),
                    };
                    self.schema(&at, Dir::Request, old, new, 0);
                }
                None => {}
            }
        }
        for o in &old_params {
            if o.place != "body" && !new_params.iter().any(|p| p.is(o)) {
                self.breaking(
                    crate::i18n::message("repo-core-2000", &[("place", format!("{}", cap(&o.place)).into()), ("name", format!("{}", o.name).into())]),
                    Some(o.line),
                    None,
                );
            }
        }
        self.request_body((old_op, &old_params), (new_op, &new_params));
        self.responses(old_op, new_op);
    }

    fn request_body(
        &mut self,
        (old_op, old_params): (&'a Field, &[Param<'a>]),
        (new_op, new_params): (&'a Field, &[Param<'a>]),
    ) {
        let (a, b) = (self.a, self.b);
        let old = body(a, old_op, old_params);
        let new = body(b, new_op, new_params);
        match (&old, &new) {
            (None, Some(n)) if n.required => self.breaking(
                crate::i18n::text("repo-core-2001").to_owned(),
                None,
                Some(n.line),
            ),
            (Some(o), Some(n)) if n.required && !o.required => self.breaking(
                crate::i18n::text("repo-core-2002").to_owned(),
                Some(o.line),
                Some(n.line),
            ),
            _ => {}
        }
        let (Some(o), Some(n)) = (old, new) else {
            return;
        };
        // A media type the client sent may no longer be accepted.
        for m in &o.media {
            if !n.media.is_empty() && !n.media.contains(m) {
                self.breaking(
                    crate::i18n::message("repo-core-2003", &[("m", format!("{}", m).into())]),
                    Some(o.line),
                    Some(n.line),
                );
            }
        }
        if let (Some(os), Some(ns)) = (o.schema, n.schema) {
            let at = At {
                owner: crate::i18n::text("repo-core-2004").to_owned(),
                path: String::new(),
            };
            self.schema(&at, Dir::Request, os, ns, 0);
        }
    }

    fn responses(&mut self, old_op: &'a Field, new_op: &'a Field) {
        let (a, b) = (self.a, self.b);
        let codes = |op: &'a Field| -> &'a [Field] {
            match get(&op.node, "responses").map(|f| &f.node) {
                Some(Node::Map(fields)) => fields,
                _ => &[],
            }
        };
        let (old, new) = (codes(old_op), codes(new_op));
        for o in old {
            let Some(n) = new.iter().find(|n| n.key == o.key) else {
                // Only a success code taken away breaks what clients rely on.
                if o.key.starts_with('2') || o.key == "default" {
                    self.breaking(
                        crate::i18n::message("repo-core-2007", &[("key", format!("{}", o.key).into())]),
                        Some(o.line),
                        None,
                    );
                }
                continue;
            };
            let (on, ol) = a.resolve(&o.node, o.line);
            let (nn, nl) = b.resolve(&n.node, n.line);
            if let (Some(os), Some(ns)) = (body_schema(a, on, ol).0, body_schema(b, nn, nl).0) {
                let at = At {
                    owner: crate::i18n::message("repo-core-2008", &[("key", format!("{}", o.key).into())]),
                    path: String::new(),
                };
                self.schema(&at, Dir::Response, os, ns, 0);
            }
        }
    }

    /// Compares the schema at `at`, travelling `dir`; each `$ref` pair once per operation.
    fn schema(
        &mut self,
        at: &At,
        dir: Dir,
        (old, ol): (&'a Node, u32),
        (new, nl): (&'a Node, u32),
        depth: usize,
    ) {
        if depth > MAX_SCHEMA_DEPTH {
            return;
        }
        // Through `$ref`s: a pair of them compared already is not compared again (that
        // makes schemas that refer to each other end, and reports each change once).
        if let (Some(r), Some(s)) = (ref_of(old), ref_of(new))
            && !self.visited.insert((r.to_owned(), s.to_owned(), dir))
        {
            return;
        }
        let (a, b) = (self.a, self.b);
        let (old, ol) = a.resolve(old, ol);
        let (new, nl) = b.resolve(new, nl);
        if same(old, new) && !refers(old) {
            return;
        }
        self.types(at, dir, (old, ol), (new, nl));
        self.enums(at, dir, (old, ol), (new, nl));
        // Fields beyond those listed: allowed unless `additionalProperties: false`.
        let closed =
            |n: &Node| get(n, "additionalProperties").and_then(|f| text(&f.node)) == Some("false");
        if dir == Dir::Request && !closed(old) && closed(new) {
            self.breaking(
                crate::i18n::message("repo-core-2010", &[("name", format!("{}", at.name()).into())]),
                Some(ol),
                Some(nl),
            );
        }
        let (op, or) = properties(a, old);
        let (np, nr) = properties(b, new);
        for o in &op {
            let child = at.child(&o.key);
            let Some(n) = np.iter().find(|n| n.key == o.key) else {
                // A field always returned that's gone; one that might not be there was
                // never to be relied on.
                if dir == Dir::Response && or.contains(&o.key) {
                    self.breaking(crate::i18n::message("repo-core-2011", &[("name", format!("{}", child.name()).into())]), Some(o.line), None);
                }
                continue;
            };
            // Clients don't send what is read-only.
            let read_only = flag(b.resolve(&n.node, n.line).0, "readOnly");
            if dir == Dir::Response && or.contains(&o.key) && !nr.contains(&o.key) {
                self.breaking(
                    crate::i18n::message("repo-core-2013", &[("name", format!("{}", child.name()).into())]),
                    Some(o.line),
                    Some(n.line),
                );
            }
            if dir == Dir::Request && !or.contains(&o.key) && nr.contains(&o.key) && !read_only {
                self.breaking(
                    crate::i18n::message("repo-core-2014", &[("name", format!("{}", child.name()).into())]),
                    Some(o.line),
                    Some(n.line),
                );
            }
            self.schema(&child, dir, (&o.node, o.line), (&n.node, n.line), depth + 1);
        }
        if dir == Dir::Request {
            for n in np.iter().filter(|n| !op.iter().any(|o| o.key == n.key)) {
                let read_only = flag(b.resolve(&n.node, n.line).0, "readOnly");
                if nr.contains(&n.key) && !read_only {
                    let path = at.child(&n.key).path;
                    self.breaking(
                        crate::i18n::message("repo-core-2015", &[("owner", format!("{}", at.owner).into()), ("path", format!("{}", path).into())]),
                        None,
                        Some(n.line),
                    );
                }
            }
        }
        if let (Some(oi), Some(ni)) = (get(old, "items"), get(new, "items")) {
            let items = At {
                owner: at.owner.clone(),
                path: format!("{}[]", at.path),
            };
            self.schema(
                &items,
                dir,
                (&oi.node, oi.line),
                (&ni.node, ni.line),
                depth + 1,
            );
        }
    }

    /// A type that no longer takes what it took (a request) or gives what clients expect (a
    /// response). `integer` is a kind of `number`; a list of types is a choice of them.
    fn types(&mut self, at: &At, dir: Dir, (old, ol): (&Node, u32), (new, nl): (&Node, u32)) {
        let (Some(ot), Some(nt)) = (types_of(old), types_of(new)) else {
            return;
        };
        let covers = |wide: &HashSet<String>, narrow: &HashSet<String>| {
            narrow
                .iter()
                .all(|t| wide.contains(t) || (t == "integer" && wide.contains("number")))
        };
        let breaks = match dir {
            Dir::Request => !covers(&nt, &ot),
            Dir::Response => !covers(&ot, &nt),
        };
        if breaks {
            let show = |s: &HashSet<String>| {
                let mut v: Vec<&str> = s.iter().map(String::as_str).collect();
                v.sort_unstable();
                v.join(" | ")
            };
            self.breaking(
                crate::i18n::message("repo-core-2018", &[("name", format!("{}", at.name()).into()), ("ot", format!("{}", show(&ot)).into()), ("nt", format!("{}", show(&nt)).into())]),
                Some(ol),
                Some(nl),
            );
        }
    }

    /// A request that may no longer send a value it could; a response that may now return one
    /// clients don't know.
    fn enums(&mut self, at: &At, dir: Dir, (old, ol): (&Node, u32), (new, nl): (&Node, u32)) {
        let (oe, ne) = (strings(get(old, "enum")), strings(get(new, "enum")));
        if oe.is_empty() && ne.is_empty() {
            return;
        }
        let line = |n: &Node, l| get(n, "enum").map_or(l, |f| f.line);
        let quote = |v: &[&String]| {
            v.iter()
                .map(|v| format!("`{v}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        match dir {
            Dir::Request if oe.is_empty() => {
                let only: Vec<&String> = ne.iter().collect();
                self.breaking(
                    crate::i18n::message("repo-core-2021", &[("name", format!("{}", at.name()).into()), ("only", format!("{}", quote(&only)).into())]),
                    Some(ol),
                    Some(line(new, nl)),
                );
            }
            Dir::Request if !ne.is_empty() => {
                let missing: Vec<&String> = oe.iter().filter(|v| !ne.contains(v)).collect();
                if !missing.is_empty() {
                    self.breaking(
                        crate::i18n::message("repo-core-2022", &[("name", format!("{}", at.name()).into()), ("missing", format!("{}", quote(&missing)).into())]),
                        Some(line(old, ol)),
                        Some(line(new, nl)),
                    );
                }
            }
            Dir::Response if !oe.is_empty() => {
                let extra: Vec<&String> = ne.iter().filter(|v| !oe.contains(v)).collect();
                if !extra.is_empty() {
                    self.breaking(
                        crate::i18n::message("repo-core-2023", &[("name", format!("{}", at.name()).into()), ("extra", format!("{}", quote(&extra)).into())]),
                        Some(line(old, ol)),
                        Some(line(new, nl)),
                    );
                }
            }
            _ => {}
        }
    }
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
