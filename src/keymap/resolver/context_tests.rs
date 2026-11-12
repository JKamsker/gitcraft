//! Context identifiers must be computed before entering egui's nonreentrant lock closures.
use super::*;
use syn::visit::Visit;

#[derive(Default)]
struct ContextCapture(bool);

impl<'ast> Visit<'ast> for ContextCapture {
    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        self.0 |= node.path.is_ident("ctx");
        syn::visit::visit_expr_path(self, node);
    }
}

#[derive(Default)]
struct LockClosures(Vec<String>);

impl<'ast> Visit<'ast> for LockClosures {
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if matches!(&*node.receiver, syn::Expr::Path(path) if path.path.is_ident("ctx"))
            && matches!(
                node.method.to_string().as_str(),
                "data" | "data_mut" | "input" | "input_mut" | "memory" | "memory_mut"
            )
        {
            for arg in &node.args {
                if let syn::Expr::Closure(closure) = arg {
                    let mut capture = ContextCapture::default();
                    capture.visit_expr(&closure.body);
                    if capture.0 {
                        self.0.push(node.method.to_string());
                    }
                }
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}

fn context_captures(source: &str) -> Result<Vec<String>, String> {
    let syntax = syn::parse_file(source).map_err(|error| error.to_string())?;
    let mut closures = LockClosures::default();
    closures.visit_file(&syntax);
    Ok(closures.0)
}

#[test]
fn resolver_lock_closures_do_not_reenter_context_through_identifier_helpers() -> Result<(), String>
{
    // Detect the original indirect reentry without deliberately deadlocking a GUI thread.
    let old = r#"
        fn old(ctx: &Context) {
            ctx.data(|d| d.get_temp::<Resolver>(id(ctx, Scope::Window)));
            ctx.data(|d| d.get_temp::<Resolver>(id(ctx, command.scope())));
        }
    "#;
    assert_eq!(context_captures(old)?, vec!["data", "data"]);
    let captured_first = r#"
        fn good(ctx: &Context) {
            let key = id(ctx, Scope::Window);
            ctx.data(|d| d.get_temp::<Resolver>(key));
        }
    "#;
    assert!(context_captures(captured_first)?.is_empty());
    assert!(context_captures(include_str!("../resolver.rs"))?.is_empty());
    Ok(())
}

#[test]
fn permission_and_chord_overlay_queries_preserve_scope_and_viewport_ownership() -> Result<(), String>
{
    let ctx = Context::default();
    let viewports = [
        egui::ViewportId::ROOT,
        egui::ViewportId::from_hash_of("first-popout"),
        egui::ViewportId::from_hash_of("second-popout"),
    ];
    let scopes = [
        Scope::Window,
        Scope::Repository,
        Scope::Files,
        Scope::Diff,
        Scope::Changes,
        Scope::Graph,
        Scope::Rebase,
        Scope::Settings,
        Scope::Find,
        Scope::Selection,
    ];
    let mut keys = Vec::new();
    for viewport in viewports {
        let mut viewport_key = None;
        for scope in scopes {
            let selected = config::commands()
                .find(|command| command.scope() == scope)
                .ok_or_else(|| format!("missing command for {scope:?}"))?;
            let mut input = egui::RawInput {
                viewport_id: viewport,
                time: Some(0.0),
                ..Default::default()
            };
            input.viewports.insert(viewport, Default::default());
            ctx.run_ui(input, |_| {
                assert_eq!(ctx.viewport_id(), viewport);
                let key = id(&ctx, scope);
                viewport_key = Some(key);
                assert_eq!(key, id(&ctx, Scope::Window));
                let prefix = Stroke::new(Key::K, egui::Modifiers::COMMAND);
                let resolver = Resolver {
                    active: vec![scope],
                    allowed: Some(vec![selected]),
                    prefix: vec![prefix.clone()],
                    deadline: 10.0,
                    config: [(
                        selected.id().into(),
                        vec![prefix, Stroke::new(Key::F8, egui::Modifiers::NONE)],
                    )]
                    .into_iter()
                    .collect(),
                    ..Default::default()
                };
                ctx.data_mut(|data| data.insert_temp(key, resolver));
                for command in config::commands() {
                    assert_eq!(permitted(&ctx, command), command == selected);
                }
                let shown = std::cell::RefCell::new(Vec::new());
                which_key(&ctx, |command| {
                    // Callers may query the Context while describing refusal reasons.
                    assert_eq!(ctx.viewport_id(), viewport);
                    shown.borrow_mut().push(command);
                    Some("fixture refusal".into())
                });
                assert_eq!(*shown.borrow(), vec![selected]);
                assert!(
                    permitted(&ctx, selected),
                    "overlay must not consume commands"
                );
            })
            .drop_without_applying_deltas();
        }
        keys.push(viewport_key.ok_or("viewport did not run")?);
    }
    assert_ne!(keys[0], keys[1]);
    assert_ne!(keys[0], keys[2]);
    assert_ne!(keys[1], keys[2]);
    Ok(())
}
