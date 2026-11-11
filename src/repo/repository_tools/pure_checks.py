#!/usr/bin/env python3
"""Compile real owner/effect policy using inert external payloads, without Cargo.

This compiles production owner/cache/context adapter with inert domain leaves.
Scheduling adapters and mutation settlement ordering also have source checks;
it does not compile domain implementations or the complete application.
Existing Rust unit tests also run in the full application at integration time.
"""
from pathlib import Path
import re
import subprocess
import tempfile
import pure_fixture

with tempfile.TemporaryDirectory(prefix="repository-tools-pure-") as temporary:
    directory = Path(temporary)
    p = Path(__file__).resolve().parent
    source = (p / 'mod.rs').read_text()
    owner = re.search(r'pub\(crate\) struct Owner \{.*?\n\}', source, re.S)[0]
    tool = re.search(
        r'#\[derive\(Clone, Copy, PartialEq, Eq\)\]\nenum Tool \{.*?\n\}', source, re.S
    )[0]
    # Production declarations and implementation, with only external data leaves stubbed.
    (directory / 'owner.rs').write_text('''#![allow(dead_code)]
    mod egui { pub struct Context; }
    mod connections {
        #[derive(Clone, Default, PartialEq, Eq)] pub struct Context { pub choices: Choices }
        #[derive(Clone, Default, PartialEq, Eq)] pub struct Choices(pub u64);
        pub fn current() -> Context { Context::default() }
    }
    mod settings {
        pub struct Settings;
        pub struct Profile { pub id: u64, pub connections: crate::connections::Choices }
        impl Settings {
            pub fn get(_: &crate::egui::Context) -> Self { Self }
            pub fn profile(&self) -> Profile { Profile { id: 1, connections: Default::default() } }
        }
    }
    mod git { pub fn profile_config() -> Option<std::path::PathBuf> { None } }
    mod repo {
    use std::sync::atomic::{AtomicU64, Ordering};
    use crate::egui;
    static REQUESTS: AtomicU64 = AtomicU64::new(1000);
    '''+tool+'\n'+owner+'''
    enum Data { Deferred(()), StateReceipts(Vec<()>) }
    enum Dialog { RepositoryTools(Box<Owner>) }
    mod effects { pub(super) struct Read { pub tool: super::Tool, pub query: String } }
    '''+pure_fixture.source(p)+'''
    #[path = "'''+str(p/'lifetime_checks.rs')+'''"] mod lifetime_checks;
    #[path = "'''+str(p/'context.rs')+'''"] mod context;
    #[path = "'''+str(p/'lifecycle.rs')+'''"] mod lifecycle;
    #[path = "'''+str(p/'owner.rs')+'''"] mod owner;
    }
    ''')
    actions = re.search(r'pub\(crate\) enum Act \{.*?\n\}', source, re.S)[0]
    # Replace external domain payloads, retaining the exact action variant schema.
    actions = re.sub(
        r'(?:crate::[\w:]+|subtrees::Job|borrowing::Plan|reftable::Storage|'
        r'replacements::Histories|remote_config::Editor|publishing::Manager|'
        r'routing::Browser|routing::Force|subconflict::Conflict|dependency::Plan)',
        '()', actions,
    )
    parts = ['''#![allow(dead_code)]
    mod repository_tools {
    struct Owner;
    mod guard { pub struct Runner<'a>(pub &'a ()); }
    ''', tool, actions]
    modules = {
        'borrowing': ('Plan', [('independent', '&()')]),
        'subconflict': ('Conflict', [('resolve', '&(), &str')]),
        'routing': ('Browser,Force', [('save', '&(), &str')]),
        'publishing': ('Manager', [('save_group', '&()')]),
        'remote_config': ('Editor', [('save', '&()')]),
        'reftable': ('Storage', [('migrate', '&(), &str')]),
        'replacements': ('Histories', [
            ('graft', '&(), &str, &str'), ('remove', '&(), &str, &str'), ('convert', '&()'),
        ]),
        'dependency': ('Plan', []),
    }
    for module, (types, functions) in modules.items():
        parts.append(f'mod {module} {{')
        for name in types.split(','):
            parts.append(f'pub type {name} = ();')
        for name, params in functions:
            params = ', '.join(f'_: {kind}' for kind in params.split(', '))
            parts.append(
                f'pub fn {name}(_: &super::guard::Runner, {params}) '
                '-> Result<String,String> { Ok(String::new()) }'
            )
        parts.append('}')
    parts.append('#[path = "'+str(p/'effects.rs')+'"] mod effects;')
    parts.append('''
    #[test] fn routing_preserves_queries_and_structured_turn_arguments() {
        for (act, tool, query) in [
            (Act::AllRefs("refs/heads/a".into()), Tool::Refs, "refs/heads/a"),
            (Act::Turn("base".into(), "turn".into(), "session".into()), Tool::Turn, "base\\0turn\\0session"),
            (Act::ForceBranch("topic".into(), "oid".into()), Tool::ForceRoute, "topic\\0oid"),
        ] {
            let effects::Effect::Read(read) = Owner::transition(act) else { panic!("expected read") };
            assert!(read.tool == tool);
            assert_eq!(read.query, query);
        }
    }
    #[test] fn mutations_remain_local_and_preserve_labels() {
        let effects::Effect::Mutation(job) = Owner::transition(Act::SaveGroup(Box::new(()))) else { panic!("expected mutation") };
        assert_eq!(job.label, "Save publication group");
        assert!(!job.network);
        assert!((job.work)(&guard::Runner(&())).is_ok());
    }
    #[test] fn specialized_compatibility_actions_are_not_reinterpreted() {
        assert!(matches!(Owner::transition(Act::Deferred), effects::Effect::Read(_)));
        assert!(matches!(Owner::transition(Act::IsolatedStack(Vec::new())), effects::Effect::None));
    }
    }
    ''')
    (directory / 'effects.rs').write_text('\n'.join(parts))
    for name in ["owner", "effects"]:
        binary = directory / name
        subprocess.run([
            "rustc", "--edition", "2024", "--test",
            str(directory / (name + ".rs")), "-o", str(binary),
        ], check=True)
        subprocess.run([str(binary)], check=True)

# No execution: enforce settlement ordering in the actual production adapters.
for name in ['mutation.rs', 'publication_jobs.rs', 'dependency_jobs.rs']:
    code = (p / name).read_text()
    callback = code[code.index('tab.op_done('):]
    boundary = ('owner::retire(' if name == 'mutation.rs' else 'tab.repository_tool_scope(')
    assert callback.index('tab.op_done(') < callback.index(boundary), name
adapter = (p / 'context.rs').read_text()
invalidate = adapter[adapter.index('fn invalidate_repository_tool_requests'):adapter.index('/// Observe')]
for forbidden in ['busy_op', 'op_started', 'last_op', 'control', 'channel', 'rx', 'tx']:
    assert forbidden not in invalidate, forbidden
for name in ['virtuals.rs', 'turns.rs', 'subtrees.rs', 'outbox.rs']:
    assert '.result = ' not in (p / name).read_text(), name
print('Production source checks: mutation op_done precedes stale UI discard; invalidation preserves accounting; preparation completions use owner.')
