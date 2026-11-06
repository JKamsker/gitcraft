//! API contract changes: an OpenAPI (or Swagger 2) document before and after the change,
//! compared for what breaks its clients: endpoints removed, parameters and request fields
//! that became required, request values narrowed, and responses that lost codes, fields or
//! guarantees. Each change has its place in the file.

use super::Sides;
use super::config::{self, same};
use super::tree::{Field, Node};

const METHODS: &[&str] = &[
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];
/// How deep schemas are compared (and `$ref`s followed): far enough for real APIs, bounded
/// for recursive ones.
const MAX_SCHEMA_DEPTH: usize = 8;
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

/// A document and its root, to follow `$ref`s in.
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

fn strings(node: Option<&Field>) -> Vec<String> {
    match node.map(|f| &f.node) {
        Some(Node::List(items)) => items
            .iter()
            .filter_map(|i| text(&i.node))
            .map(str::to_owned)
            .collect(),
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

impl<'a> Doc<'a> {
    /// `node` with its `$ref` followed (a local one, `#/…`), and the line it is at.
    fn resolve(self, node: &'a Node, line: u32) -> (&'a Node, u32) {
        let (mut node, mut line) = (node, line);
        for _ in 0..MAX_REFS {
            let Some(target) = get(node, "$ref").and_then(|f| text(&f.node)) else {
                break;
            };
            let Some(found) = self.pointer(target) else {
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
        "it isn't an OpenAPI or Swagger document (no `openapi` or `swagger` key at its top)",
    )?;
    let mut out = Vec::new();
    match (&old, &new) {
        (Some(old), Some(new)) => {
            let (a, b) = (Doc { root: old }, Doc { root: new });
            endpoints(a, b, &mut out);
        }
        (Some(_), None) => out.push(change(
            Severity::Breaking,
            "",
            "The whole API document was deleted",
            Some(1),
            None,
        )),
        _ => {}
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

/// Every operation of `doc`: (`GET /pets`, the path's field, the operation's field).
fn operations<'a>(doc: Doc<'a>) -> Vec<(String, &'a Field, &'a Field)> {
    let Some(paths) = get(doc.root, "paths") else {
        return Vec::new();
    };
    let Node::Map(paths) = &paths.node else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for p in paths {
        for m in METHODS {
            if let Some(op) = get(&p.node, m) {
                out.push((format!("{} {}", m.to_uppercase(), p.key), p, op));
            }
        }
    }
    out
}

fn endpoints(a: Doc, b: Doc, out: &mut Vec<ApiChange>) {
    let (old_ops, new_ops) = (operations(a), operations(b));
    for (name, _, op) in &old_ops {
        match new_ops.iter().find(|(n, ..)| n == name) {
            None => out.push(change(
                Severity::Breaking,
                name,
                "Endpoint removed",
                Some(op.line),
                None,
            )),
            Some((_, new_path, new_op)) => {
                let old_path = old_ops.iter().find(|(n, ..)| n == name).map(|(_, p, _)| *p);
                if let Some(old_path) = old_path {
                    operation(name, (a, old_path, op), (b, new_path, new_op), out);
                }
            }
        }
    }
    for (name, _, op) in &new_ops {
        if !old_ops.iter().any(|(n, ..)| n == name) {
            out.push(change(
                Severity::Added,
                name,
                "Endpoint added",
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
            let (name, place) = (field("name"), field("in"));
            if out.iter().any(|p| p.name == name && p.place == place) {
                continue;
            }
            let required = place == "path" || field("required") == "true";
            out.push(Param {
                name,
                place,
                required,
                node,
                line,
            });
        }
    }
    out
}

fn operation(
    name: &str,
    (a, old_path, old_op): (Doc, &Field, &Field),
    (b, new_path, new_op): (Doc, &Field, &Field),
    out: &mut Vec<ApiChange>,
) {
    let (old_params, new_params) = (params(a, old_path, old_op), params(b, new_path, new_op));
    for p in &new_params {
        // A body parameter (Swagger 2) is the request body, compared below.
        if p.place == "body" {
            continue;
        }
        match old_params
            .iter()
            .find(|o| o.name == p.name && o.place == p.place)
        {
            None if p.required => out.push(change(
                Severity::Breaking,
                name,
                format!("New required {} parameter `{}`", p.place, p.name),
                None,
                Some(p.line),
            )),
            Some(o) if p.required && !o.required => out.push(change(
                Severity::Breaking,
                name,
                format!("The {} parameter `{}` is now required", p.place, p.name),
                Some(o.line),
                Some(p.line),
            )),
            Some(o) => {
                let what = format!("{} parameter `{}`", p.place, p.name);
                let (os, ol) = schema_of(a, o.node, o.line);
                let (ns, nl) = schema_of(b, p.node, p.line);
                schema(
                    &what,
                    Dir::Request,
                    (a, os, ol),
                    (b, ns, nl),
                    name,
                    out,
                    0,
                    &mut Vec::new(),
                );
            }
            None => {}
        }
    }
    for o in &old_params {
        if o.place != "body"
            && !new_params
                .iter()
                .any(|p| p.name == o.name && p.place == o.place)
        {
            out.push(change(
                Severity::Breaking,
                name,
                format!(
                    "The {} parameter `{}` was removed (clients sending it may be refused)",
                    o.place, o.name
                ),
                Some(o.line),
                None,
            ));
        }
    }
    request_body(
        name,
        (a, old_op, &old_params),
        (b, new_op, &new_params),
        out,
    );
    responses(name, (a, old_op), (b, new_op), out);
}

/// A parameter's schema: its `schema` (OpenAPI 3, Swagger body), or the parameter itself.
fn schema_of<'a>(doc: Doc<'a>, param: &'a Node, line: u32) -> (&'a Node, u32) {
    match get(param, "schema") {
        Some(f) => doc.resolve(&f.node, f.line),
        None => (param, line),
    }
}

/// The schema of a body (`content` → a media type → `schema`, JSON preferred), or of a
/// Swagger 2 body parameter.
fn body_schema<'a>(doc: Doc<'a>, body: &'a Node, line: u32) -> Option<(&'a Node, u32)> {
    if let Some(f) = get(body, "schema") {
        return Some(doc.resolve(&f.node, f.line));
    }
    let Node::Map(media) = &get(body, "content")?.node else {
        return None;
    };
    let pick = media
        .iter()
        .find(|m| m.key.contains("json"))
        .or_else(|| media.first())?;
    let f = get(&pick.node, "schema")?;
    let _ = line;
    Some(doc.resolve(&f.node, f.line))
}

/// The `$ref` a body's schema is, to compare a schema that refers back to itself once.
fn body_ref(body: &Node) -> Vec<String> {
    let schema = get(body, "schema").or_else(|| {
        let Node::Map(media) = &get(body, "content")?.node else {
            return None;
        };
        let pick = media
            .iter()
            .find(|m| m.key.contains("json"))
            .or_else(|| media.first())?;
        get(&pick.node, "schema")
    });
    schema
        .and_then(|f| ref_of(&f.node))
        .map(str::to_owned)
        .into_iter()
        .collect()
}

/// A request body: whether it is required, its line, and its schema with the schema's line.
type Body = Option<(bool, u32, Option<(Node, u32)>)>;

fn request_body(
    name: &str,
    (a, old_op, old_params): (Doc, &Field, &[Param]),
    (b, new_op, new_params): (Doc, &Field, &[Param]),
    out: &mut Vec<ApiChange>,
) {
    let body = |doc: Doc<'_>, op: &Field, params: &[Param]| -> Body {
        if let Some(f) = get(&op.node, "requestBody") {
            let (node, line) = doc.resolve(&f.node, f.line);
            let required = get(node, "required").and_then(|r| text(&r.node)) == Some("true");
            let schema = body_schema(doc, node, line).map(|(n, l)| (n.clone(), l));
            return Some((required, line, schema));
        }
        let p = params.iter().find(|p| p.place == "body")?;
        let schema = body_schema(doc, p.node, p.line).map(|(n, l)| (n.clone(), l));
        Some((p.required, p.line, schema))
    };
    let (old, new) = (body(a, old_op, old_params), body(b, new_op, new_params));
    match (&old, &new) {
        (None, Some((true, line, _))) => out.push(change(
            Severity::Breaking,
            name,
            "A request body is now required",
            None,
            Some(*line),
        )),
        (Some((false, ol, _)), Some((true, nl, _))) => out.push(change(
            Severity::Breaking,
            name,
            "The request body is now required",
            Some(*ol),
            Some(*nl),
        )),
        _ => {}
    }
    if let (Some((_, _, Some((os, ol)))), Some((_, _, Some((ns, nl))))) = (old, new) {
        schema(
            "request body",
            Dir::Request,
            (a, &os, ol),
            (b, &ns, nl),
            name,
            out,
            0,
            &mut Vec::new(),
        );
    }
}

fn responses(
    name: &str,
    (a, old_op): (Doc, &Field),
    (b, new_op): (Doc, &Field),
    out: &mut Vec<ApiChange>,
) {
    let codes = |op: &Field| match get(&op.node, "responses").map(|f| &f.node) {
        Some(Node::Map(fields)) => fields.clone(),
        _ => Vec::new(),
    };
    let (old, new) = (codes(old_op), codes(new_op));
    for o in &old {
        let Some(n) = new.iter().find(|n| n.key == o.key) else {
            // Only a success code taken away breaks what clients rely on.
            if o.key.starts_with('2') || o.key == "default" {
                out.push(change(
                    Severity::Breaking,
                    name,
                    format!("Response `{}` removed", o.key),
                    Some(o.line),
                    None,
                ));
            }
            continue;
        };
        let (on, ol) = a.resolve(&o.node, o.line);
        let (nn, nl) = b.resolve(&n.node, n.line);
        if let (Some((os, osl)), Some((ns, nsl))) = (body_schema(a, on, ol), body_schema(b, nn, nl))
        {
            let what = format!("response `{}`", o.key);
            schema(
                &what,
                Dir::Response,
                (a, os, osl),
                (b, ns, nsl),
                name,
                out,
                0,
                &mut body_ref(on),
            );
        }
    }
}

/// Which way a schema's values travel: what the client sends, or what it gets back.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Dir {
    Request,
    Response,
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
        for part in parts {
            let (part, _) = doc.resolve(&part.node, part.line);
            let (p, r) = properties(doc, part);
            props.extend(p);
            required.extend(r);
        }
    }
    (props, required)
}

fn type_of(node: &Node) -> Option<String> {
    let f = get(node, "type")?;
    match &f.node {
        Node::Scalar { text, .. } => Some(text.clone()),
        Node::List(items) => Some(
            items
                .iter()
                .filter_map(|i| text(&i.node))
                .collect::<Vec<_>>()
                .join(" | "),
        ),
        Node::Map(_) => None,
    }
}

/// Compares the schema `what` (a parameter, the request body, a response) travelling `dir`.
#[allow(clippy::too_many_arguments)]
fn schema(
    what: &str,
    dir: Dir,
    (a, old, ol): (Doc, &Node, u32),
    (b, new, nl): (Doc, &Node, u32),
    name: &str,
    out: &mut Vec<ApiChange>,
    depth: usize,
    seen: &mut Vec<String>,
) {
    // Two schemas that are the same text and follow no `$ref` are the same; a `$ref` can
    // point at something else on each side.
    if depth > MAX_SCHEMA_DEPTH || (same(old, new) && !refers(old)) {
        return;
    }
    let breaking = |out: &mut Vec<ApiChange>, text: String, o: Option<u32>, n: Option<u32>| {
        out.push(change(Severity::Breaking, name, text, o, n));
    };
    if let (Some(ot), Some(nt)) = (type_of(old), type_of(new))
        && ot != nt
    {
        breaking(
            out,
            format!("{} changed type: {ot} → {nt}", cap(what)),
            Some(ol),
            Some(nl),
        );
        return;
    }
    // Enums: a request may no longer send a value it could; a response may now return one
    // clients don't know.
    let (oe, ne) = (strings(get(old, "enum")), strings(get(new, "enum")));
    if !oe.is_empty() || !ne.is_empty() {
        let line = |n: &Node, l| get(n, "enum").map_or(l, |f| f.line);
        let missing: Vec<&String> = oe.iter().filter(|v| !ne.contains(v)).collect();
        let extra: Vec<&String> = ne.iter().filter(|v| !oe.contains(v)).collect();
        if dir == Dir::Request && !ne.is_empty() && (!missing.is_empty() || oe.is_empty()) {
            let values = if oe.is_empty() {
                "an enum".to_owned()
            } else {
                list(&missing)
            };
            breaking(
                out,
                format!("{} no longer accepts {values}", cap(what)),
                Some(line(old, ol)),
                Some(line(new, nl)),
            );
        }
        if dir == Dir::Response && !extra.is_empty() && !oe.is_empty() {
            breaking(
                out,
                format!("{} can now return {}", cap(what), list(&extra)),
                Some(line(old, ol)),
                Some(line(new, nl)),
            );
        }
    }
    let (op, or) = properties(a, old);
    let (np, nr) = properties(b, new);
    for o in &op {
        let n = np.iter().find(|n| n.key == o.key);
        let field = format!("{what} field `{}`", o.key);
        match n {
            None if dir == Dir::Response => {
                breaking(out, format!("{} removed", cap(&field)), Some(o.line), None)
            }
            None => {}
            Some(n) => {
                if dir == Dir::Response && or.contains(&o.key) && !nr.contains(&o.key) {
                    breaking(
                        out,
                        format!("{} is no longer always returned", cap(&field)),
                        Some(o.line),
                        Some(n.line),
                    );
                }
                if dir == Dir::Request && !or.contains(&o.key) && nr.contains(&o.key) {
                    breaking(
                        out,
                        format!("{} is now required", cap(&field)),
                        Some(o.line),
                        Some(n.line),
                    );
                }
                // A schema that refers back to one being compared (a tree of itself) was
                // compared already.
                if !recurs(&o.node, seen) {
                    let (os, osl) = a.resolve(&o.node, o.line);
                    let (ns, nsl) = b.resolve(&n.node, n.line);
                    let depth_seen = seen.len();
                    seen.extend(ref_of(&o.node).map(str::to_owned));
                    let next = (a, os, osl);
                    schema(&field, dir, next, (b, ns, nsl), name, out, depth + 1, seen);
                    seen.truncate(depth_seen);
                }
            }
        }
    }
    if dir == Dir::Request {
        for n in np.iter().filter(|n| !op.iter().any(|o| o.key == n.key)) {
            if nr.contains(&n.key) {
                breaking(
                    out,
                    format!("New required {what} field `{}`", n.key),
                    None,
                    Some(n.line),
                );
            }
        }
    }
    // Items of an array.
    if let (Some(oi), Some(ni)) = (get(old, "items"), get(new, "items"))
        && !recurs(&oi.node, seen)
    {
        let (os, osl) = a.resolve(&oi.node, oi.line);
        let (ns, nsl) = b.resolve(&ni.node, ni.line);
        let depth_seen = seen.len();
        seen.extend(ref_of(&oi.node).map(str::to_owned));
        let what = format!("{what} items");
        schema(
            &what,
            dir,
            (a, os, osl),
            (b, ns, nsl),
            name,
            out,
            depth + 1,
            seen,
        );
        seen.truncate(depth_seen);
    }
}

/// The `$ref` a schema is, if it is one.
fn ref_of(node: &Node) -> Option<&str> {
    get(node, "$ref").and_then(|f| text(&f.node))
}

/// Whether `node` is a `$ref` to a schema already being compared further up.
fn recurs(node: &Node, seen: &[String]) -> bool {
    ref_of(node).is_some_and(|r| seen.iter().any(|s| s == r))
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

fn list(values: &[&String]) -> String {
    values
        .iter()
        .map(|v| format!("`{v}`"))
        .collect::<Vec<_>>()
        .join(", ")
}
