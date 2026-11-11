use super::*;

/// Comparing one operation on both sides.
pub(super) struct Cx<'a, 'o> {
    pub(super) a: Doc<'a>,
    pub(super) b: Doc<'a>,
    /// The endpoint, as it is named now.
    pub(super) name: String,
    pub(super) out: &'o mut Vec<ApiChange>,
    /// The pairs of `$ref`s compared already for this operation (each compared once).
    pub(super) visited: HashSet<(String, String, Dir)>,
}

/// A schema being compared: where it is in the body or parameter (`a.b[].c`), what holds it
/// (`Request body`, `Response 200`, `Query parameter limit`).
#[derive(Clone)]
pub(super) struct At {
    pub(super) owner: String,
    pub(super) path: String,
}

impl At {
    pub(super) fn child(&self, key: &str) -> At {
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
    pub(super) fn name(&self) -> String {
        if self.path.is_empty() {
            self.owner.clone()
        } else {
            crate::i18n::message(
                "repo-core-1996",
                &[
                    ("owner", self.owner.to_string().into()),
                    ("path", self.path.to_string().into()),
                ],
            )
        }
    }
}

impl<'a> Cx<'a, '_> {
    pub(super) fn breaking(&mut self, what: String, old: Option<u32>, new: Option<u32>) {
        let c = change(Severity::Breaking, &self.name, what, old, new);
        self.out.push(c);
    }

    pub(super) fn operation(
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
            let label = crate::i18n::message(
                "repo-core-1997",
                &[
                    ("place", cap(&p.place).to_string().into()),
                    ("name", p.name.to_string().into()),
                ],
            );
            match old_params.iter().find(|o| o.is(p)) {
                None if p.required => self.breaking(
                    crate::i18n::message(
                        "repo-core-1998",
                        &[
                            ("place", p.place.to_string().into()),
                            ("name", p.name.to_string().into()),
                        ],
                    ),
                    None,
                    Some(p.line),
                ),
                Some(o) if p.required && !o.required => self.breaking(
                    crate::i18n::message(
                        "repo-core-1999",
                        &[(
                            "label",
                            crate::repo::messages::display_operation(&label).into(),
                        )],
                    ),
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
                    crate::i18n::message(
                        "repo-core-2000",
                        &[
                            ("place", cap(&o.place).to_string().into()),
                            ("name", o.name.to_string().into()),
                        ],
                    ),
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
                    crate::i18n::message("repo-core-2003", &[("m", m.to_string().into())]),
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
                        crate::i18n::message(
                            "repo-core-2007",
                            &[("key", o.key.to_string().into())],
                        ),
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
                    owner: crate::i18n::message(
                        "repo-core-2008",
                        &[("key", o.key.to_string().into())],
                    ),
                    path: String::new(),
                };
                self.schema(&at, Dir::Response, os, ns, 0);
            }
        }
    }

    /// Compares the schema at `at`, travelling `dir`; each `$ref` pair once per operation.
    pub(super) fn schema(
        &mut self,
        at: &At,
        dir: Dir,
        old: (&'a Node, u32),
        new: (&'a Node, u32),
        depth: usize,
    ) {
        schema::compare(self, at, dir, old, new, depth);
    }
}
