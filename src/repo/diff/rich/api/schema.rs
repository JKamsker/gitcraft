use super::*;
pub(super) fn compare<'a>(
    cx: &mut Cx<'a, '_>,
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
        && !cx.visited.insert((r.to_owned(), s.to_owned(), dir))
    {
        return;
    }
    let (a, b) = (cx.a, cx.b);
    let (old, ol) = a.resolve(old, ol);
    let (new, nl) = b.resolve(new, nl);
    if same(old, new) && !refers(old) {
        return;
    }
    types(cx, at, dir, (old, ol), (new, nl));
    enums(cx, at, dir, (old, ol), (new, nl));
    // Fields beyond those listed: allowed unless `additionalProperties: false`.
    let closed =
        |n: &Node| get(n, "additionalProperties").and_then(|f| text(&f.node)) == Some("false");
    if dir == Dir::Request && !closed(old) && closed(new) {
        cx.breaking(
            crate::i18n::message("repo-core-2010", &[("name", at.name().to_string().into())]),
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
                cx.breaking(
                    crate::i18n::message(
                        "repo-core-2011",
                        &[("name", child.name().to_string().into())],
                    ),
                    Some(o.line),
                    None,
                );
            }
            continue;
        };
        // Clients don't send what is read-only.
        let read_only = flag(b.resolve(&n.node, n.line).0, "readOnly");
        if dir == Dir::Response && or.contains(&o.key) && !nr.contains(&o.key) {
            cx.breaking(
                crate::i18n::message(
                    "repo-core-2013",
                    &[("name", child.name().to_string().into())],
                ),
                Some(o.line),
                Some(n.line),
            );
        }
        if dir == Dir::Request && !or.contains(&o.key) && nr.contains(&o.key) && !read_only {
            cx.breaking(
                crate::i18n::message(
                    "repo-core-2014",
                    &[("name", child.name().to_string().into())],
                ),
                Some(o.line),
                Some(n.line),
            );
        }
        cx.schema(&child, dir, (&o.node, o.line), (&n.node, n.line), depth + 1);
    }
    if dir == Dir::Request {
        for n in np.iter().filter(|n| !op.iter().any(|o| o.key == n.key)) {
            let read_only = flag(b.resolve(&n.node, n.line).0, "readOnly");
            if nr.contains(&n.key) && !read_only {
                let path = at.child(&n.key).path;
                cx.breaking(
                    crate::i18n::message(
                        "repo-core-2015",
                        &[
                            ("owner", at.owner.to_string().into()),
                            ("path", path.to_string().into()),
                        ],
                    ),
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
        cx.schema(
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
fn types<'a>(
    cx: &mut Cx<'a, '_>,
    at: &At,
    dir: Dir,
    (old, ol): (&Node, u32),
    (new, nl): (&Node, u32),
) {
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
        cx.breaking(
            crate::i18n::message(
                "repo-core-2018",
                &[
                    ("name", at.name().to_string().into()),
                    ("ot", show(&ot).to_string().into()),
                    ("nt", show(&nt).to_string().into()),
                ],
            ),
            Some(ol),
            Some(nl),
        );
    }
}

/// A request that may no longer send a value it could; a response that may now return one
/// clients don't know.
fn enums<'a>(
    cx: &mut Cx<'a, '_>,
    at: &At,
    dir: Dir,
    (old, ol): (&Node, u32),
    (new, nl): (&Node, u32),
) {
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
            cx.breaking(
                crate::i18n::message(
                    "repo-core-2021",
                    &[
                        ("name", at.name().to_string().into()),
                        ("only", quote(&only).to_string().into()),
                    ],
                ),
                Some(ol),
                Some(line(new, nl)),
            );
        }
        Dir::Request if !ne.is_empty() => {
            let missing: Vec<&String> = oe.iter().filter(|v| !ne.contains(v)).collect();
            if !missing.is_empty() {
                cx.breaking(
                    crate::i18n::message(
                        "repo-core-2022",
                        &[
                            ("name", at.name().to_string().into()),
                            ("missing", quote(&missing).to_string().into()),
                        ],
                    ),
                    Some(line(old, ol)),
                    Some(line(new, nl)),
                );
            }
        }
        Dir::Response if !oe.is_empty() => {
            let extra: Vec<&String> = ne.iter().filter(|v| !oe.contains(v)).collect();
            if !extra.is_empty() {
                cx.breaking(
                    crate::i18n::message(
                        "repo-core-2023",
                        &[
                            ("name", at.name().to_string().into()),
                            ("extra", quote(&extra).to_string().into()),
                        ],
                    ),
                    Some(line(old, ol)),
                    Some(line(new, nl)),
                );
            }
        }
        _ => {}
    }
}
