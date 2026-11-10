//! Route metadata must use the operation's repository and command-line config.
//! Syntax belongs to the shared parser; this narrower allowlist is proof policy.

pub(super) struct RouteQueries<'a> {
    prefix: &'a [&'a str],
    operation: &'a [&'a str],
}

impl<'a> RouteQueries<'a> {
    pub(super) fn new(args: &'a [&'a str]) -> Result<Self, String> {
        if args.is_empty() {
            return Ok(Self {
                prefix: &[],
                operation: &[],
            });
        }
        let (at, _) = crate::git::parse_command_envelope(args).map_err(
            |_| "Cannot prove Git routing for an unknown or incomplete command envelope",
        )?;
        let prefix = &args[..at];
        let mut rest = prefix;
        while let Some(word) = rest.first() {
            let consumed = match *word {
                "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" => 2,
                "--bare"
                | "--no-pager"
                | "-P"
                | "--no-replace-objects"
                | "--no-lazy-fetch"
                | "--no-optional-locks"
                | "--literal-pathspecs"
                | "--glob-pathspecs"
                | "--noglob-pathspecs"
                | "--icase-pathspecs" => 1,
                w if (w.starts_with("-C") || w.starts_with("-c")) && w.len() > 2 => 1,
                w if ["--git-dir=", "--work-tree=", "--namespace="]
                    .iter()
                    .any(|p| w.starts_with(p)) =>
                {
                    1
                }
                // --config-env depends on ambient variables which normalized reads
                // may remove. Pager/super-prefix and future globals have no proof.
                _ => return Err("Cannot prove Git routing for this global option".into()),
            };
            rest = &rest[consumed..];
        }
        Ok(Self {
            prefix,
            operation: &args[at..],
        })
    }

    pub(super) fn operation(&self) -> &'a [&'a str] {
        self.operation
    }

    pub(super) fn command<'q>(&'q self, query: &[&'q str]) -> Vec<&'q str> {
        self.prefix
            .iter()
            .copied()
            .chain(query.iter().copied())
            .collect()
    }
}
