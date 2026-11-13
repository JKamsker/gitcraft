//! Places and field attribution: which expressions are the tab (or a reference to it),
//! and whether a field of it is read, written, borrowed mutably or mutated by a method.

use syn::Expr;
use syn::visit::Visit;

use crate::model::Ns;
use crate::refs::{Access, RefKind};
use crate::resolve::Def;
use crate::syntax::{self, line_of};
use crate::tab::Mode;
use crate::visit::{Local, Place, Visitor};

impl Visitor<'_, '_> {
    pub(crate) fn is_tab(&self, defs: &[Def]) -> bool {
        self.tab.item.is_some_and(|t| defs.contains(&Def::Item(t)))
    }

    /// `Some(mutable)` when a type is the tab or a reference to it.
    pub(crate) fn tab_type(&self, ty: &syn::Type) -> Local {
        match ty {
            syn::Type::Reference(r) => self.tab_type(&r.elem).map(|_| r.mutability.is_some()),
            syn::Type::Paren(p) => self.tab_type(&p.elem),
            syn::Type::Path(p) if p.qself.is_none() => {
                let segs = syntax::path_idents(&p.path);
                let defs = self.res.resolve(
                    self.module,
                    &segs,
                    Ns::Type,
                    p.path.leading_colon.is_some(),
                    &self.self_ty,
                );
                self.is_tab(&defs).then_some(true)
            }
            _ => None,
        }
    }

    /// Whether an initializer evaluates to the tab: another tab binding, a tab constructor,
    /// or a function whose every definition returns the tab.
    pub(crate) fn tab_value(&self, expr: &Expr) -> Local {
        match expr {
            Expr::Reference(r) => self.tab_value(&r.expr).map(|m| m && r.mutability.is_some()),
            Expr::Paren(p) => self.tab_value(&p.expr),
            Expr::Path(_) => self.tab_root(expr),
            Expr::Call(c) => match &*c.func {
                Expr::Path(p) => {
                    let segs = syntax::path_idents(&p.path);
                    let defs =
                        self.res
                            .resolve(self.module, &segs, Ns::Value, false, &self.self_ty);
                    let named = segs.last().is_some_and(|n| self.tab.returning.contains(n));
                    (self.is_tab(&defs) || named).then_some(true)
                }
                _ => None,
            },
            Expr::MethodCall(m) if m.method == "unwrap" || m.method == "expect" => {
                self.tab_value(&m.receiver)
            }
            Expr::MethodCall(m) => self
                .tab
                .returning
                .contains(&m.method.to_string())
                .then_some(true),
            _ => None,
        }
    }

    /// The tab binding a place expression starts from.
    pub(crate) fn tab_root(&self, expr: &Expr) -> Local {
        match expr {
            Expr::Path(p) if p.qself.is_none() && p.path.segments.len() == 1 => {
                self.local(&p.path.segments[0].ident.to_string()).flatten()
            }
            Expr::Paren(p) => self.tab_root(&p.expr),
            Expr::Unary(u) if matches!(u.op, syn::UnOp::Deref(_)) => self.tab_root(&u.expr),
            _ => None,
        }
    }

    pub(crate) fn access(&mut self, field: &syn::Ident, mode: Mode) {
        let name = field.to_string();
        if self.tab.fields.contains_key(&name) {
            let site = self.at(line_of(field));
            self.out.accesses.push(Access {
                module: self.module,
                field: name,
                mode,
                site,
            });
        }
    }

    /// Visit a place (assignee, borrowed or method receiver), attributing tab fields.
    pub(crate) fn place(&mut self, expr: &Expr, how: &Place) {
        match expr {
            Expr::Field(f) => {
                if let syn::Member::Named(name) = &f.member {
                    if let Some(mutable) = self.tab_root(&f.base) {
                        let mode = match how {
                            Place::Plain(mode) => *mode,
                            Place::Call(method) => self.tab.call_mode(method, mutable),
                        };
                        self.access(name, mode);
                        return;
                    }
                    if self.tab.fields.contains_key(&name.to_string())
                        && self.tab_root(chain_root(&f.base)).is_none()
                    {
                        self.out.stats.tab_field_unattributed += 1;
                        *self
                            .out
                            .unattributed
                            .entry(self.site.file.clone())
                            .or_default() += 1;
                    }
                }
                self.place(&f.base, how);
            }
            Expr::Index(i) => {
                self.place(&i.expr, how);
                self.visit_expr(&i.index);
            }
            Expr::Paren(p) => self.place(&p.expr, how),
            Expr::Group(g) => self.place(&g.expr, how),
            Expr::Unary(u) if matches!(u.op, syn::UnOp::Deref(_)) => self.place(&u.expr, how),
            Expr::MethodCall(m) => self.method_call(m),
            other => self.visit_expr(other),
        }
    }

    pub(crate) fn method_call(&mut self, m: &syn::ExprMethodCall) {
        self.out.stats.method_calls += 1;
        let method = m.method.to_string();
        // A field read for a non-mutating method, through any binding: `tab.history.len()`.
        let how = if self.tab.is_mutating(&method) {
            Place::Call(method)
        } else {
            Place::Plain(Mode::Read)
        };
        self.place(&m.receiver, &how);
        if let Some(turbofish) = &m.turbofish {
            self.visit_angle_bracketed_generic_arguments(turbofish);
        }
        for arg in &m.args {
            self.visit_expr(arg);
        }
    }

    pub(crate) fn visit_struct_literal(&mut self, s: &syn::ExprStruct) {
        if let Some(q) = &s.qself {
            self.visit_type(&q.ty);
        }
        let defs = self.path(&s.path, Ns::Type, RefKind::Value);
        let tab = self.is_tab(&defs);
        for field in &s.fields {
            if let (true, syn::Member::Named(name)) = (tab, &field.member) {
                self.access(name, Mode::Init);
            }
            self.visit_expr(&field.expr);
        }
        if let Some(rest) = &s.rest {
            self.visit_expr(rest);
        }
    }

    /// An assignment target; destructuring (`(t.a, [t.b, _]) = ..`) writes every element.
    fn assignee(&mut self, expr: &Expr) {
        match expr {
            Expr::Tuple(t) => t.elems.iter().for_each(|e| self.assignee(e)),
            Expr::Array(a) => a.elems.iter().for_each(|e| self.assignee(e)),
            Expr::Paren(p) => self.assignee(&p.expr),
            other => self.place(other, &Place::Plain(Mode::Write)),
        }
    }

    pub(crate) fn place_expr(&mut self, expr: &Expr) -> bool {
        match expr {
            Expr::Assign(a) => {
                self.assignee(&a.left);
                self.visit_expr(&a.right);
            }
            Expr::Binary(b) if is_assign(&b.op) => {
                self.place(&b.left, &Place::Plain(Mode::Write));
                self.visit_expr(&b.right);
            }
            Expr::Reference(r) if r.mutability.is_some() => {
                self.place(&r.expr, &Place::Plain(Mode::MutBorrow))
            }
            Expr::Field(_) | Expr::Index(_) => self.place(expr, &Place::Plain(Mode::Read)),
            Expr::MethodCall(m) => self.method_call(m),
            Expr::Struct(s) => self.visit_struct_literal(s),
            _ => return false,
        }
        true
    }
}

/// The expression a place chain (`a.b[i].c()`) starts from.
fn chain_root(expr: &Expr) -> &Expr {
    match expr {
        Expr::Field(f) => chain_root(&f.base),
        Expr::Index(i) => chain_root(&i.expr),
        Expr::MethodCall(m) => chain_root(&m.receiver),
        Expr::Paren(p) => chain_root(&p.expr),
        Expr::Group(g) => chain_root(&g.expr),
        Expr::Try(t) => chain_root(&t.expr),
        other => other,
    }
}

fn is_assign(op: &syn::BinOp) -> bool {
    use syn::BinOp::*;
    matches!(
        op,
        AddAssign(_)
            | SubAssign(_)
            | MulAssign(_)
            | DivAssign(_)
            | RemAssign(_)
            | BitXorAssign(_)
            | BitAndAssign(_)
            | BitOrAssign(_)
            | ShlAssign(_)
            | ShrAssign(_)
    )
}
