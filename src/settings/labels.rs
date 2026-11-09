//! Finite renderer for the legacy row/page/section names. Raw names remain preview and
//! search identities; user-supplied names pass through unchanged.
pub(super) fn display(raw: &str) -> &str {
    match raw {
        "History filters" => crate::i18n::text("dev-integration-ui-history-filters"),
        "Global history filters" => crate::i18n::text("dev-integration-ui-global-history-filters"),
        "Context menus" => crate::i18n::text("dev-integration-ui-context-menus"),
        "Optional tools" => crate::i18n::text("dev-baseline-settings-label-0"),
        "History / diagnostics" => crate::i18n::text("dev-baseline-settings-label-1"),
        "Commit surgery" => crate::i18n::text("dev-baseline-settings-label-2"),
        "Branch rehearsal" => crate::i18n::text("dev-baseline-settings-label-3"),
        "Sidebar" => crate::i18n::text("dev-baseline-settings-label-4"),
        "Show Agents section" => crate::i18n::text("dev-baseline-settings-label-5"),
        "Files (GNOME)" => crate::i18n::text("interaction-shell-prose-files"),
        "AI" => crate::i18n::text("interaction-settings-label-ai"),
        "Accent colour" => crate::i18n::text("interaction-settings-label-accent-colour"),
        "Add gitcraft to an agent" => {
            crate::i18n::text("interaction-settings-label-add-gitcraft-to-an-agent")
        }
        "After success" => crate::i18n::text("interaction-settings-label-after-success"),
        "Agents" => crate::i18n::text("interaction-settings-label-agents"),
        "Agents may change repositories" => {
            crate::i18n::text("interaction-settings-label-agents-may-change-repositories")
        }
        "Also fold" => crate::i18n::text("interaction-settings-label-also-fold"),
        "Appearance" => crate::i18n::text("interaction-settings-label-appearance"),
        "Applications menu" => crate::i18n::text("interaction-settings-label-applications-menu"),
        "Avatars" => crate::i18n::text("interaction-settings-label-avatars"),
        "Behaviour" => crate::i18n::text("interaction-settings-label-behaviour"),
        "Branch column width" => {
            crate::i18n::text("interaction-settings-label-branch-column-width")
        }
        "Changed files" => crate::i18n::text("interaction-settings-label-changed-files"),
        "Changes by agents" => crate::i18n::text("interaction-settings-label-changes-by-agents"),
        "Channel" => crate::i18n::text("interaction-settings-label-channel"),
        "Check now" => crate::i18n::text("interaction-settings-label-check-now"),
        "Code colours" => crate::i18n::text("interaction-settings-label-code-colours"),
        "Columns" => crate::i18n::text("interaction-settings-label-columns"),
        "Commit graph" => crate::i18n::text("interaction-settings-label-commit-graph"),
        "Commit prompt" => crate::i18n::text("interaction-settings-label-commit-prompt"),
        "Commit signing" => crate::i18n::text("interaction-settings-label-commit-signing"),
        "Conflict editor" => crate::i18n::text("interaction-settings-label-conflict-editor"),
        "Corners" => crate::i18n::text("interaction-settings-label-corners"),
        "Custom command" => crate::i18n::text("interaction-settings-label-custom-command"),
        "Custom commands" => crate::i18n::text("interaction-settings-label-custom-commands"),
        "Default shell" => crate::i18n::text("interaction-settings-label-default-shell"),
        "Diff algorithms" => crate::i18n::text("interaction-settings-label-diff-algorithms"),
        "Diff context" => crate::i18n::text("interaction-settings-label-diff-context"),
        "Diff layout" => crate::i18n::text("interaction-settings-label-diff-layout"),
        "Diff tool" => crate::i18n::text("interaction-settings-label-diff-tool"),
        "Diffs" => crate::i18n::text("interaction-settings-label-diffs"),
        "Diffs and files" => crate::i18n::text("interaction-settings-label-diffs-and-files"),
        "Editor command" => crate::i18n::text("interaction-settings-label-editor-command"),
        "Email" => crate::i18n::text("interaction-settings-label-email"),
        "Explanatory motion" => crate::i18n::text("interaction-settings-label-explanatory-motion"),
        "External tools" => crate::i18n::text("interaction-settings-label-external-tools"),
        "Fetch in the background" => {
            crate::i18n::text("interaction-settings-label-fetch-in-the-background")
        }
        "Fold generated files" => {
            crate::i18n::text("interaction-settings-label-fold-generated-files")
        }
        "Folder" => crate::i18n::text("interaction-settings-label-folder"),
        "Font size" => crate::i18n::text("interaction-settings-label-font-size"),
        "Forgejo / Gitea login" => {
            crate::i18n::text("interaction-settings-label-forgejo-gitea-login")
        }
        "Format" => crate::i18n::text("interaction-settings-label-format"),
        "General" => crate::i18n::text("interaction-settings-label-general"),
        "Git" => crate::i18n::text("interaction-settings-label-git"),
        "Git author" => crate::i18n::text("interaction-settings-label-git-author"),
        "Git file tools" => crate::i18n::text("interaction-settings-label-git-file-tools"),
        "Git merges" => crate::i18n::text("interaction-settings-label-git-merges"),
        "Graph feedback" => crate::i18n::text("interaction-settings-label-graph-feedback"),
        "HTTPS sign-ins" => crate::i18n::text("interaction-settings-label-https-sign-ins"),
        "Host / namespace" => crate::i18n::text("interaction-settings-label-host-namespace"),
        "Identities" => crate::i18n::text("interaction-settings-label-identities"),
        "In use" => crate::i18n::text("interaction-settings-label-in-use"),
        "Installed" => crate::i18n::text("interaction-settings-label-installed"),
        "Keyboard shortcuts" => crate::i18n::text("interaction-settings-label-keyboard-shortcuts"),
        "Lane colours" => crate::i18n::text("interaction-settings-label-lane-colours"),
        "History map" => crate::i18n::text("interaction-settings-label-history-map"),
        "Lanes" => crate::i18n::text("interaction-settings-label-lanes"),
        "Largest diff sent" => crate::i18n::text("interaction-settings-label-largest-diff-sent"),
        "Merge tool" => crate::i18n::text("interaction-settings-label-merge-tool"),
        "Mergiraf" => crate::i18n::text("interaction-settings-label-mergiraf"),
        "Name" => crate::i18n::text("interaction-settings-label-name"),
        "Needs" => crate::i18n::text("interaction-settings-label-needs"),
        "Network" => crate::i18n::text("interaction-settings-label-network"),
        "New expectation" => crate::i18n::text("interaction-settings-label-new-expectation"),
        "New versions" => crate::i18n::text("interaction-settings-label-new-versions"),
        "Notifications" => crate::i18n::text("interaction-settings-label-notifications"),
        "Offline" => crate::i18n::text("interaction-settings-label-offline"),
        "Outgoing author per host" => {
            crate::i18n::text("interaction-settings-label-outgoing-author-per-host")
        }
        "Pinned" => crate::i18n::text("interaction-settings-label-pinned"),
        "Portable mode" => crate::i18n::text("interaction-settings-label-portable-mode"),
        "Previous version" => crate::i18n::text("interaction-settings-label-previous-version"),
        "Profile" => crate::i18n::text("interaction-settings-label-profile"),
        "Profile's commit signing" => {
            crate::i18n::text("interaction-settings-label-profile-s-commit-signing")
        }
        "Profile's connections" => {
            crate::i18n::text("interaction-settings-label-profile-s-connections")
        }
        "GitHub account" => crate::i18n::text("interaction-settings-label-github-account"),
        "GitLab account" => crate::i18n::text("interaction-settings-label-gitlab-account"),
        "Program" => crate::i18n::text("interaction-settings-label-program"),
        "Pull" => crate::i18n::text("interaction-settings-label-pull"),
        "Quiet celebrations" => crate::i18n::text("interaction-settings-label-quiet-celebrations"),
        "Register gitcraft" => crate::i18n::text("interaction-settings-label-register-gitcraft"),
        "Remote machines" => crate::i18n::text("interaction-settings-label-remote-machines"),
        "Repository commands" => {
            crate::i18n::text("interaction-settings-label-repository-commands")
        }
        "Resident watcher" => crate::i18n::text("interaction-settings-label-resident-watcher"),
        "Review prompt" => crate::i18n::text("interaction-settings-label-review-prompt"),
        "SSH private key" => crate::i18n::text("interaction-settings-label-ssh-private-key"),
        "SSH sign-ins" => crate::i18n::text("interaction-settings-label-ssh-sign-ins"),
        "Shortened branch names" => {
            crate::i18n::text("interaction-settings-label-shortened-branch-names")
        }
        "Sign by default" => crate::i18n::text("interaction-settings-label-sign-by-default"),
        "Sign-ins" => crate::i18n::text("interaction-settings-label-sign-ins"),
        "Signing" => crate::i18n::text("interaction-settings-label-signing"),
        "Signing key" => crate::i18n::text("interaction-settings-label-signing-key"),
        "Stale branches" => crate::i18n::text("interaction-settings-label-stale-branches"),
        "Submodules" => crate::i18n::text("interaction-settings-label-submodules"),
        "Terminal" => crate::i18n::text("interaction-settings-label-terminal"),
        "Theme" => crate::i18n::text("interaction-settings-label-theme"),
        "Tips" => crate::i18n::text("interaction-settings-label-tips"),
        "Title bar" => crate::i18n::text("interaction-settings-label-title-bar"),
        "Tool" => crate::i18n::text("interaction-settings-label-tool"),
        "Topology navigation" => {
            crate::i18n::text("interaction-settings-label-topology-navigation")
        }
        "Updates" => crate::i18n::text("interaction-settings-label-updates"),
        "Variables" => crate::i18n::text("interaction-settings-label-variables"),
        "Version" => crate::i18n::text("interaction-settings-label-version"),
        "Worktrees" => crate::i18n::text("interaction-settings-label-worktrees"),
        "Your themes" => crate::i18n::text("interaction-settings-label-your-themes"),
        "Zoom" => crate::i18n::text("interaction-settings-label-zoom"),
        "Settings" => crate::i18n::text("interaction-settings-label-settings"),
        "Search results" => crate::i18n::text("interaction-settings-label-search-results"),
        "Connection doctor" => crate::i18n::text("interaction-accounts-doctor"),
        "HTTPS accounts" => crate::i18n::text("interaction-accounts-https-accounts"),
        "Repository network" => crate::i18n::text("interaction-accounts-repo-network"),
        "Command palette" => crate::i18n::text("interaction-presentation-palette"),
        "Manual" => crate::i18n::text("interaction-presentation-manual"),
        _ => raw,
    }
}
