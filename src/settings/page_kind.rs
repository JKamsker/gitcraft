/// The pages of the settings, in the order of their tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Page {
    Profile,
    #[default]
    General,
    Appearance,
    Graph,
    Git,
    Signing,
    Network,
    HttpsSignins,
    Terminal,
    Identities,
    Ai,
    CustomCommands,
    ContextMenus,
    Agents,
    Shortcuts,
}

impl Page {
    pub const ALL: [Page; 15] = [
        Page::Profile,
        Page::General,
        Page::Appearance,
        Page::Graph,
        Page::Git,
        Page::Signing,
        Page::Network,
        Page::HttpsSignins,
        Page::Terminal,
        Page::Identities,
        Page::Ai,
        Page::CustomCommands,
        Page::ContextMenus,
        Page::Agents,
        Page::Shortcuts,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Page::Profile => "Profile",
            Page::General => "General",
            Page::Appearance => "Appearance",
            Page::Graph => "Commit graph",
            Page::Git => "Git",
            Page::Signing => "Commit signing",
            Page::Network => "Network",
            Page::HttpsSignins => "HTTPS sign-ins",
            Page::Terminal => "Terminal",
            Page::Identities => "Identities",
            Page::Ai => "AI",
            Page::CustomCommands => "Custom commands",
            Page::Agents => "Agents",
            Page::ContextMenus => "Context menus",
            Page::Shortcuts => "Keyboard shortcuts",
        }
    }

    pub(super) fn icon(self) -> &'static str {
        match self {
            Page::Profile => "circle-user-round",
            Page::General => "settings",
            Page::Appearance => "palette",
            Page::Graph => "git-branch",
            Page::Git => "git-merge",
            Page::Signing => "key-round",
            Page::Network => "network",
            Page::HttpsSignins => "key-round",
            Page::Terminal => "square-terminal",
            Page::Identities => "user-round",
            Page::Ai => "sparkles",
            Page::CustomCommands => "command",
            Page::Agents => "bot",
            Page::ContextMenus => "list",
            Page::Shortcuts => "keyboard",
        }
    }
}
