# Cargo compiler-lint cap protection

Validated source: `fb24ffb36db04c7983f4ed388de8fd61f72622f2`; its complete `ci`
tree is `47dd9883cb101d8efe19ba3088ed8a9767deed7b`. The tested working CI files
were verified unchanged against that commit before recording this evidence.
`cargo-cap-policy-tests.log.gz` records all 43 Python regression methods passing:

```sh
PATH=/home/jonas/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  python3 -m unittest discover -s ci -p 'test_*.py' -v
```

The real compiler fixture first rejects score 26 at threshold 25 under manifest
`forbid` and `-D warnings`. Both Cargo config filenames, build string/array flags,
and target flags can suppress that diagnostic through `--cap-lints`; the fixture
demonstrates the bypass, requires the snapshot policy to reject it, and checks
that removing the cap restores compiler failure. Forty static configuration
variants cover target, encoded flags, environment tables, TOML escapes, nested
packages, and Windows filename casing. Ordinary cfg, codegen, and target-feature
flags remain accepted. Working/index/revision cases cover both config filenames
and uppercase Windows spellings. The source-size checker's new case-insensitive
selection is opt-in for policy inputs; the physical Rust size default is unchanged.

The actual compiler fixture deliberately makes no claim that Cargo's `[env]`
values alter its first invocation: on the installed Cargo they affect children.
The static policy still rejects lint caps there to protect nested invocations.
Working and staged source-size checks passed (2,615 Rust files, zero errors),
and working/staged complexity policy checks each reported zero errors. These
checks change CI tooling only; application source remains the assembled `3c2af37d`
snapshot and its final runtime/lint validation remains separate.

# Later source compilation inventory

`clippy-84a15b24.log.gz` records all-target source compilation with this command on
`84a15b24af458c14e81f68068b1dce13b0d90999`, exit 0:

```sh
CARGO_TARGET_DIR=/home/jonas/gitcraft-target \
  /home/jonas/.cargo/bin/cargo clippy --locked --all-targets -- \
  -W clippy::cognitive_complexity
```

This snapshot includes the Insights refactors and prompt budget regression. Those
scopes introduce no complexity warnings; the known backend warnings remain
because the inventory predates `ccc4aefe`. This is compiler evidence, not a
passing combined lint gate or application runtime test run.
