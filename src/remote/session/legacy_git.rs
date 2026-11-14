//! Git before 2.44 rejects `--no-lazy-fetch` ("unknown option"), which gitcraft passes to
//! every passive read on another machine. A shell set up on such a machine puts a private
//! `git` wrapper (`legacy_git.sh`) first in its PATH, so those commands keep working in
//! ordinary repositories and are refused, not silently fetching, in partial clones.

use crate::remote::sh_quote;

const WRAPPER: &str = include_str!("legacy_git.sh");

/// Shell lines (silent, never reading the input the shell's script comes from) that install
/// the wrapper only when the machine's Git doesn't know `--no-lazy-fetch`.
pub(in crate::remote) fn setup() -> String {
    format!(
        concat!(
            "if command -v git >/dev/null 2>&1 && ! git --no-lazy-fetch version >/dev/null 2>&1 </dev/null; then\n",
            "GITCRAFT_LEGACY_GIT=$(command -v git)\n",
            "gitcraft_shim=${{XDG_CACHE_HOME:-$HOME/.cache}}/gitcraft/legacy-git\n",
            "case $GITCRAFT_LEGACY_GIT:$gitcraft_shim in\n",
            "/*:/*)\n",
            "if (umask 077 && mkdir -p \"$gitcraft_shim\" && printf '%s' {wrapper} >\"$gitcraft_shim/git.$$\" && chmod 700 \"$gitcraft_shim/git.$$\" && mv -f \"$gitcraft_shim/git.$$\" \"$gitcraft_shim/git\") >/dev/null 2>&1 </dev/null; then\n",
            "PATH=$gitcraft_shim:$PATH; export PATH GITCRAFT_LEGACY_GIT\n",
            "else rm -f \"$gitcraft_shim/git.$$\" >/dev/null 2>&1; fi\n",
            ";;\n",
            "esac\n",
            "unset gitcraft_shim\n",
            "fi"
        ),
        wrapper = sh_quote(WRAPPER),
    )
}

#[cfg(all(test, unix))]
mod tests;
