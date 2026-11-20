//! The inventory table itself (see `sentinel_inventory_tests`): one entry per sentinel, with
//! its owner, its role, whether a producer persists it, and a producer-shaped sample.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::repo::operations) enum Role {
    /// The first word of a step that runs no git, or that gitcraft dispatches itself.
    Step,
    /// Stands inside a git argument until settlement binds it.
    Placeholder,
    /// Starts an operation's output; never a step.
    Output,
    /// A graph row's stand-in hash; never in an operation.
    Hash,
}

pub(in crate::repo::operations) struct Sentinel {
    pub constant: &'static str,
    /// The source file that declares it.
    pub owner: &'static str,
    pub value: &'static str,
    pub role: Role,
    /// Whether a current producer writes it into an `UndoEntry` the timeline file keeps.
    pub persisted: bool,
    /// Step markers: the operands their producer adds after the marker. Placeholders: the
    /// whole git step, holding the placeholder. Others: nothing.
    pub sample: &'static [&'static str],
    /// `Plan::decode`'s classification of the sample (see `classify`); empty for no step.
    pub decodes: &'static str,
    /// For a step marker that decodes as `Git`: what handles it before any git runs.
    pub handled_by: &'static str,
}

impl Sentinel {
    /// The sample as the step its producer builds.
    pub(in crate::repo::operations) fn sample_step(&self) -> Vec<String> {
        let marker = (self.role == Role::Step).then_some(self.value);
        marker
            .into_iter()
            .chain(self.sample.iter().copied())
            .map(str::to_owned)
            .collect()
    }
}

const SHA: &str = "1111111111111111111111111111111111111111";
const DESTINATION: &str = r#"{"name":"fork","fetch":["https://github.com/o/r.git"],"push":["https://github.com/o/r.git"]}"#;
const EXPECTATIONS: &str = "src/repo/operation_plan/expectations.rs";
const GUARD: &str = "src/repo/forks/guard.rs";

const fn step(
    constant: &'static str,
    owner: &'static str,
    value: &'static str,
    persisted: bool,
    sample: &'static [&'static str],
    decodes: &'static str,
) -> Sentinel {
    Sentinel {
        constant,
        owner,
        value,
        role: Role::Step,
        persisted,
        sample,
        decodes,
        handled_by: "",
    }
}

const fn other(
    constant: &'static str,
    owner: &'static str,
    value: &'static str,
    role: Role,
) -> Sentinel {
    Sentinel {
        constant,
        owner,
        value,
        role,
        persisted: false,
        sample: &[],
        decodes: "",
        handled_by: "",
    }
}

pub(in crate::repo::operations) const INVENTORY: &[Sentinel] = &[
    step(
        "EXPECT",
        EXPECTATIONS,
        "\u{0}expect",
        true,
        &["refs/stash", SHA],
        "Expectation::Ref",
    ),
    step(
        "EXPECT_SYMREF",
        EXPECTATIONS,
        "\u{0}expect-symref",
        true,
        &["HEAD", "refs/heads/main"],
        "Expectation::Symref",
    ),
    step(
        "EXPECT_TREE",
        EXPECTATIONS,
        "\u{0}expect-tree",
        true,
        &["stash@{0}^2", SHA],
        "Expectation::Tree",
    ),
    step(
        "TRY",
        EXPECTATIONS,
        "\u{0}try",
        false,
        &["Couldn't fetch main", "fetch", "origin", "main"],
        "Try",
    ),
    other("TRIED", EXPECTATIONS, "\u{0}tried\u{0}", Role::Output),
    step(
        "EXPECT_FOLD_SAFE",
        EXPECTATIONS,
        "\u{0}expect-fold-safe",
        false,
        &[SHA],
        "Expectation::FoldSafe",
    ),
    step(
        "EXPECT_FOLD_IGNORED",
        EXPECTATIONS,
        "\u{0}expect-fold-ignored",
        false,
        &[SHA],
        "Expectation::FoldIgnored",
    ),
    step(
        "EXPECT_HTTPS_ROUTE",
        "src/repo/operations.rs",
        "--gitcraft-expect-https-route",
        false,
        &[r#"{"args":["push"],"routes":[],"configuration":""}"#],
        "Expectation::HttpsRoute",
    ),
    step(
        "CHECK",
        "src/gerrit.rs",
        "\0gerrit-review",
        false,
        &[SHA, SHA],
        "Expectation::FeatureGuard",
    ),
    step(
        "AMEND",
        "src/gerrit.rs",
        "\0gerrit-amend",
        false,
        &["commit", "--amend", "-m", "Fix"],
        "GerritAmend",
    ),
    step(
        "APPROVED",
        "src/push_preflight.rs",
        "\0push-preflight",
        false,
        &["1"],
        "ReviewedPush",
    ),
    step(
        "FIX",
        "src/push_preflight.rs",
        "\0push-preflight-fix",
        false,
        &["1", "author"],
        "PreflightFix",
    ),
    step(
        "MARK",
        GUARD,
        "\0fork-destination",
        true,
        &[DESTINATION],
        "Expectation::FeatureGuard",
    ),
    step(
        "CHECKOUT_MARK",
        GUARD,
        "\0fork-checkout",
        true,
        &[DESTINATION],
        "Expectation::FeatureGuard",
    ),
    Sentinel {
        role: Role::Placeholder,
        sample: &[
            "push",
            "--force-with-lease=refs/heads/topic:\0fork-pushed-tip",
            "--",
            "fork",
            "1111111111111111111111111111111111111111:refs/heads/topic",
        ],
        decodes: "Git",
        ..other("PUSHED_TIP", GUARD, "\0fork-pushed-tip", Role::Placeholder)
    },
    Sentinel {
        handled_by: "Execution::step (operation_plan/executor/steps.rs) runs [APPLY, json] \
                     through tracking::execute before dispatch",
        ..step(
            "APPLY",
            "src/git/repository_tools/tracking.rs",
            "\0tracking",
            true,
            &[r#"{"branch":"topic"}"#],
            "Git",
        )
    },
    other(
        "GHOST_MERGE",
        "src/repo/ghost/plan.rs",
        "\u{0}ghost-merge",
        Role::Hash,
    ),
    other("WIP_HASH", "src/repo/mod.rs", "\u{0}wip", Role::Hash),
    other(
        "PREFIX",
        "src/repo/resolver/graph.rs",
        "\u{0}parked:",
        Role::Hash,
    ),
];
