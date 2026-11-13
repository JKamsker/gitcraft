//! The `syn::visit` hooks of the reference pass: each sets the name space and reference
//! kind of the paths below it, and opens a scope wherever Rust introduces bindings.

use syn::visit::{self, Visit};

use crate::model::Ns;
use crate::refs::RefKind;
use crate::visit::Visitor;

impl<'ast> Visit<'ast> for Visitor<'_, '_> {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        self.item(item);
    }

    // Attribute and visibility paths (`derive`, `pub(in ..)`) are not dependencies.
    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {}

    fn visit_visibility(&mut self, _: &'ast syn::Visibility) {}

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let (ns, kind) = self.current();
        self.path(path, ns, kind);
        for segment in &path.segments {
            let saved = self.ns_kind(Ns::Type, RefKind::Type);
            visit::visit_path_arguments(self, &segment.arguments);
            self.restore(saved);
        }
    }

    fn visit_type_path(&mut self, ty: &'ast syn::TypePath) {
        if let Some(q) = &ty.qself {
            self.visit_type(&q.ty);
        }
        let kind = match self.current().1 {
            RefKind::Impl => RefKind::Impl,
            _ => RefKind::Type,
        };
        let saved = self.ns_kind(Ns::Type, kind);
        self.visit_path(&ty.path);
        self.restore(saved);
    }

    fn visit_expr_path(&mut self, expr: &'ast syn::ExprPath) {
        if let Some(q) = &expr.qself {
            self.visit_type(&q.ty);
        }
        let saved = self.ns_kind(Ns::Value, RefKind::Value);
        self.visit_path(&expr.path);
        self.restore(saved);
    }

    fn visit_trait_bound(&mut self, bound: &'ast syn::TraitBound) {
        let saved = self.ns_kind(Ns::Type, RefKind::Type);
        visit::visit_trait_bound(self, bound);
        self.restore(saved);
    }

    fn visit_pat_struct(&mut self, pat: &'ast syn::PatStruct) {
        if let Some(q) = &pat.qself {
            self.visit_type(&q.ty);
        }
        let saved = self.ns_kind(Ns::Type, RefKind::Value);
        self.visit_path(&pat.path);
        self.restore(saved);
        for field in &pat.fields {
            self.visit_pat(&field.pat);
        }
    }

    fn visit_pat_tuple_struct(&mut self, pat: &'ast syn::PatTupleStruct) {
        if let Some(q) = &pat.qself {
            self.visit_type(&q.ty);
        }
        let saved = self.ns_kind(Ns::Value, RefKind::Value);
        self.visit_path(&pat.path);
        self.restore(saved);
        for elem in &pat.elems {
            self.visit_pat(elem);
        }
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.path(&mac.path, Ns::Macro, RefKind::Macro);
        self.visit_macro_body(mac);
    }

    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        if !self.place_expr(expr) {
            visit::visit_expr(self, expr);
        }
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let Some(init) = &local.init {
            self.visit_expr(&init.expr);
            if let Some((_, diverge)) = &init.diverge {
                self.visit_expr(diverge);
            }
        }
        self.visit_pat(&local.pat);
        self.bind_local(local);
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.with_frame(|v| {
            v.block_items(block);
            for stmt in &block.stmts {
                if !matches!(stmt, syn::Stmt::Item(syn::Item::Use(_))) {
                    v.visit_stmt(stmt);
                }
            }
        });
    }

    fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
        self.with_frame(|v| {
            for input in &closure.inputs {
                v.visit_pat(input);
                v.bind_closure_input(input);
            }
            v.visit_return_type(&closure.output);
            v.visit_expr(&closure.body);
        });
    }

    fn visit_arm(&mut self, arm: &'ast syn::Arm) {
        self.with_frame(|v| {
            v.visit_pat(&arm.pat);
            v.bind_pattern(&arm.pat);
            if let Some((_, guard)) = &arm.guard {
                v.visit_expr(guard);
            }
            v.visit_expr(&arm.body);
        });
    }

    fn visit_expr_for_loop(&mut self, expr: &'ast syn::ExprForLoop) {
        self.visit_expr(&expr.expr);
        self.with_frame(|v| {
            v.visit_pat(&expr.pat);
            v.bind_pattern(&expr.pat);
            v.visit_block(&expr.body);
        });
    }

    fn visit_expr_let(&mut self, expr: &'ast syn::ExprLet) {
        self.visit_expr(&expr.expr);
        self.visit_pat(&expr.pat);
        self.bind_pattern(&expr.pat);
    }

    fn visit_generics(&mut self, generics: &'ast syn::Generics) {
        self.bind_generics(generics);
        visit::visit_generics(self, generics);
    }

    fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
        self.trait_fn(f);
    }
}
