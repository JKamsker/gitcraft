use super::*;

pub(super) fn operation_template(
    label: &str,
    depth: usize,
    family: PaintFamily,
) -> Option<Painted> {
    use std::sync::OnceLock;
    struct Template {
        pattern: regex::Regex,
        id: &'static str,
        args: &'static [(&'static str, bool)],
    }
    static TEMPLATES: OnceLock<Vec<Template>> = OnceLock::new();
    let templates = TEMPLATES.get_or_init(|| {
        review::TEMPLATES.iter().chain(workflow::TEMPLATES).copied().chain([
            ("(?s)^Inspect\\ shared\\ Git\\ directories\\ and\\ donors;\\ make\\ a\\ clone\\ independent$", "repo-core-0085", &[][..]),
            ("(?s)^Subtrees:\\ register,\\ update,\\ inspect\\ and\\ publish\\ vendored\\ folders…$", "repo-core-0082", &[][..]),
            ("(?s)^Push\\ outbox:\\ queue\\ exact\\ commit\\ or\\ send\\ selected\\ entries…$", "repo-core-0080", &[][..]),
            ("(?s)^Conflicts\\ that\\ can\\ wait:\\ isolated\\ rebase\\ and\\ restack…$", "repo-core-0072", &[][..]),
            ("(?s)^Parallel\\ branches\\ in\\ this\\ folder:\\ hunk\\ ownership…$", "repo-core-0074", &[][..]),
            ("(?s)^Named\\ changelists:\\ messages,\\ commit\\ and\\ shelve…$", "repo-core-0078", &[][..]),
            ("(?s)^Stop\\ remembering\\ conflict\\ resolutions\\ \\(rerere\\)$", "repo-core-0118", &[][..]),
            ("(?s)^Publish\\ submodule\\ dependencies\\ before\\ parent…$", "repo-core-0086", &[][..]),
            ("(?s)^Agent\\ turns:\\ save\\ each\\ turn\\ in\\ this\\ worktree$", "repo-core-0108", &[][..]),
            ("(?s)^Replace\\ refs\\ and\\ temporary\\ history\\ joins…$", "repo-core-0094", &[][..]),
            ("(?s)^Save\\ snapshot\\ \\(stash,\\ keep\\ the\\ changes\\)…$", "repo-core-0048", &[][..]),
            ("(?s)^Remote\\ URLs,\\ refspecs\\ and\\ mirror\\ policy…$", "repo-core-0092", &[][..]),
            ("(?s)^Delete\\ branch\\ locally\\ and\\ on\\ the\\ remote$", "repo-core-3367", &[][..]),
            ("(?s)^Remember\\ conflict\\ resolutions\\ \\(rerere\\)$", "repo-core-0119", &[][..]),
            ("(?s)^Activity:\\ operations\\ and\\ their\\ output$", "repo-core-0066", &[][..]),
            ("(?s)^Download\\ history\\ objects\\ \\(consented\\)$", "repo-core-3615", &[][..]),
            ("(?s)^command\\ output\\ benchmark\\ screenshot$", "repo-core-0039", &[][..]),
            ("(?s)^Inspect\\ state\\ publication\\ receipts…$", "repo-core-0076", &[][..]),
            ("(?s)^Ref\\ storage\\ and\\ reftable\\ migration…$", "repo-core-0096", &[][..]),
            ("(?s)^Working\\-state\\ transaction\\ recovery$", "repo-core-0077", &[][..]),
            ("(?s)^grafts\\ original\\ effective\\ history$", "repo-core-0095", &[][..]),
            ("(?s)^Select\\ stale\\ or\\ merged\\ branches…$", "repo-core-0071", &[][..]),
            ("(?s)^triangular\\ upstream\\ ahead\\ behind$", "repo-core-0089", &[][..]),
            ("(?s)^Push\\ for\\ review,\\ skipping\\ hooks$", "repo-core-0035", &[][..]),
            ("(?s)^publication\\ group\\ retry\\ mirrors$", "repo-core-0091", &[][..]),
            ("(?s)^objects\\ pack\\ delta\\ file\\ version$", "repo-core-0099", &[][..]),
            ("(?s)^All\\ refs\\ and\\ namespace\\ history…$", "repo-core-0101", &[][..]),
            ("(?s)^Pull\\ \\(fast\\-forward\\ if\\ possible\\)$", "repo-core-3093", &[][..]),
            ("(?s)^File\\ versions\\ across\\ branches$", "repo-core-0028", &[][..]),
            ("(?s)^Shared\\ and\\ borrowed\\ objects$", "repo-core-0084", &[][..]),
            ("(?s)^Bake\\-off:\\ compare\\ attempts…$", "repo-core-0107", &[][..]),
            ("(?s)^What\\ a\\ package\\ will\\ publish$", "repo-core-0110", &[][..]),
            ("(?s)^Stash\\ only\\ what\\ is\\ staged…$", "repo-core-0046", &[][..]),
            ("(?s)^Inspect\\ local\\ Git\\ bundle…$", "repo-core-0044", &[][..]),
            ("(?s)^submodules\\ publish\\ remote$", "repo-core-0087", &[][..]),
            ("(?s)^Publish\\ to\\ several\\ hosts…$", "repo-core-0090", &[][..]),
            ("(?s)^Start\\ work\\ from\\ an\\ issue…$", "repo-core-0133", &[][..]),
            ("(?s)^Bring\\ the\\ conflicts\\ back\\?$", "repo-core-0693", &[][..]),
            ("(?s)^Three\\-state\\ staging\\ lens$", "repo-core-0026", &[][..]),
            ("(?s)^Distribute\\ staged\\ fixes…$", "repo-core-0049", &[][..]),
            ("(?s)^fetch\\ push\\ mapping\\ prune$", "repo-core-0093", &[][..]),
            ("(?s)^How\\ an\\ object\\ is\\ stored…$", "repo-core-0098", &[][..]),
            ("(?s)^Pull\\ \\(fast\\-forward\\ only\\)$", "repo-core-3094", &[][..]),
            ("(?s)^Code\\ moved\\ across\\ files$", "repo-core-0022", &[][..]),
            ("(?s)^network\\ offline\\ metered$", "repo-core-0065", &[][..]),
            ("(?s)^Vendored\\ source\\ history$", "repo-core-0083", &[][..]),
            ("(?s)^Forget\\ this\\ resolution\\?$", "repo-core-0688", &[][..]),
            ("(?s)^Worktree\\ dev\\ container…$", "repo-core-4201", &[][..]),
            ("(?s)^Sync\\ \\(pull,\\ then\\ push\\)$", "repo-core-0033", &[][..]),
            ("(?s)^Linked\\ project\\ copies…$", "repo-core-0040", &[][..]),
            ("(?s)^Timeline\\ and\\ undo\\ tree$", "repo-core-0053", &[][..]),
            ("(?s)^graph\\ or\\ visible\\ lines$", "repo-core-4194", &[][..]),
            ("(?s)^Pull\\ and\\ push\\ routes…$", "repo-core-0088", &[][..]),
            ("(?s)^Push\\ reviewed\\ history$", "repo-core-3110", &[][..]),
            ("(?s)^data\\ download\\ remote$", "repo-core-0021", &[][..]),
            ("(?s)^refactor\\ moved\\ block$", "repo-core-0023", &[][..]),
            ("(?s)^First\\ commit\\ created$", "repo-core-0278", &[][..]),
            ("(?s)^Fix\\ outgoing\\ history$", "repo-core-3118", &[][..]),
            ("(?s)^Delete\\ remote\\ branch$", "repo-core-3491", &[][..]),
            ("(?s)^Run\\ Actions\\ locally…$", "repo-core-4197", &[][..]),
            ("(?s)^Behaviour\\ timeline…$", "repo-core-0038", &[][..]),
            ("(?s)^Offline\\ publication$", "repo-core-0081", &[][..]),
            ("(?s)^Release\\ tag\\ created$", "repo-core-0279", &[][..]),
            ("(?s)^Edit\\ commit\\ message$", "repo-core-3172", &[][..]),
            ("(?s)^File\\ migration\\ map$", "repo-core-0024", &[][..]),
            ("(?s)^Export\\ Git\\ bundle…$", "repo-core-0042", &[][..]),
            ("(?s)^refs\\ compatibility$", "repo-core-0097", &[][..]),
            ("(?s)^Interactive\\ rebase$", "repo-core-3337", &[][..]),
            ("(?s)^Stage\\ all\\ changes$", "repo-core-0055", &[][..]),
            ("(?s)^Find\\ lost\\ commits$", "repo-core-0060", &[][..]),
            ("(?s)^Deferred\\ rewrites$", "repo-core-0073", &[][..]),
            ("(?s)^Repository\\ doctor$", "repo-core-0103", &[][..]),
            ("(?s)^Start\\ agent\\ task…$", "repo-core-0105", &[][..]),
            ("(?s)^Update\\ submodules$", "repo-core-0122", &[][..]),
            ("(?s)^Use\\ this\\ decision$", "repo-core-0535", &[][..]),
            ("(?s)^Delete\\ remote\\ tag$", "repo-core-3543", &[][..]),
            ("(?s)^offline\\ exchange$", "repo-core-0045", &[][..]),
            ("(?s)^Virtual\\ branches$", "repo-core-0075", &[][..]),
            ("(?s)^Check\\ CODEOWNERS$", "repo-core-0116", &[][..]),
            ("(?s)^Show\\ CI\\ for\\ HEAD$", "repo-core-0135", &[][..]),
            ("(?s)^Background\\ fetch$", "repo-core-2899", &[][..]),
            ("(?s)^Show\\ that\\ window$", "repo-core-3042", &[][..]),
            ("(?s)^Move\\ commit\\ down$", "repo-core-3175", &[][..]),
            ("(?s)^Toggle\\ terminal$", "repo-core-0061", &[][..]),
            ("(?s)^network\\ offline$", "repo-core-0063", &[][..]),
            ("(?s)^Working\\ changes$", "repo-core-0079", &[][..]),
            ("(?s)^Worktree\\ setup…$", "repo-core-0109", &[][..]),
            ("(?s)^Open\\ in\\ browser$", "repo-core-0115", &[][..]),
            ("(?s)^Merge\\ completed$", "repo-core-0280", &[][..]),
            ("(?s)^Delete\\ new\\ file$", "repo-core-0851", &[][..]),
            ("(?s)^Discard\\ changes$", "repo-core-0856", &[][..]),
            ("(?s)^Remove\\ worktree$", "repo-core-3597", &[][..]),
            ("(?s)^Prune\\ worktrees$", "repo-core-3602", &[][..]),
            ("(?s)^offline\\ backup$", "repo-core-0043", &[][..]),
            ("(?s)^Stash\\ changes…$", "repo-core-0054", &[][..]),
            ("(?s)^Search\\ commits$", "repo-core-0057", &[][..]),
            ("(?s)^refs\\ internals$", "repo-core-0102", &[][..]),
            ("(?s)^Open\\ in\\ editor$", "repo-core-0113", &[][..]),
            ("(?s)^Unity:\\ Set\\-up…$", "repo-core-0124", &[][..]),
            ("(?s)^Finish\\ hotfix…$", "repo-core-0137", &[][..]),
            ("(?s)^Edit\\ the\\ note…$", "repo-core-0547", &[][..]),
            ("(?s)^Move\\ commit\\ up$", "repo-core-3174", &[][..]),
            ("(?s)^Email\\ patches…$", "repo-core-4199", &[][..]),
            ("(?s)^Create\\ branch…$", "repo-core-4203", &[][..]),
            ("(?s)^LFS:\\ Storage…$", "repo-core-0132", &[][..]),
            ("(?s)^Create\\ branch$", "repo-core-0140", &[][..]),
            ("(?s)^Use\\ it\\ anyway$", "repo-core-0534", &[][..]),
            ("(?s)^Stash\\ changes$", "repo-core-1492", &[][..]),
            ("(?s)^Pull\\ \\(rebase\\)$", "repo-core-3095", &[][..]),
            ("(?s)^Delete\\ branch$", "repo-core-3361", &[][..]),
            ("(?s)^Remove\\ remote$", "repo-core-3500", &[][..]),
            ("(?s)^New\\ terminal$", "repo-core-0068", &[][..]),
            ("(?s)^Cut\\ release…$", "repo-core-0139", &[][..]),
            ("(?s)^Add\\ worktree$", "repo-core-1535", &[][..]),
            ("(?s)^Apply\\ patch…$", "repo-core-2630", &[][..]),
            ("(?s)^LFS:\\ Locks…$", "repo-core-0131", &[][..]),
            ("(?s)^Drop\\ commit$", "repo-core-3173", &[][..]),
            ("(?s)^Create\\ tag…$", "repo-core-4204", &[][..]),
            ("(?s)^Go\\ offline$", "repo-core-0064", &[][..]),
            ("(?s)^LFS:\\ Fetch$", "repo-core-0126", &[][..]),
            ("(?s)^Create\\ tag$", "repo-core-0141", &[][..]),
            ("(?s)^Use\\ theirs$", "repo-core-0725", &[][..]),
            ("(?s)^Hard\\ reset$", "repo-core-3355", &[][..]),
            ("(?s)^Delete\\ tag$", "repo-core-3546", &[][..]),
            ("(?s)^Go\\ online$", "repo-core-0062", &[][..]),
            ("(?s)^conflicts$", "repo-core-0120", &[][..]),
            ("(?s)^submodule$", "repo-core-0123", &[][..]),
            ("(?s)^LFS:\\ Pull$", "repo-core-0129", &[][..]),
            ("(?s)^Note\\ why…$", "repo-core-0546", &[][..]),
            ("(?s)^machines$", "repo-core-0041", &[][..]),
            ("(?s)^recovery$", "repo-core-0059", &[][..]),
            ("(?s)^terminal$", "repo-core-0069", &[][..]),
            ("(?s)^settings$", "repo-core-0125", &[][..]),
            ("(?s)^Use\\ mine$", "repo-core-0727", &[][..]),
            ("(?s)^staging$", "repo-core-0027", &[][..]),
            ("(?s)^release$", "repo-core-0138", &[][..]),
            ("(?s)^remote$", "repo-core-0034", &[][..]),
            ("(?s)^absorb$", "repo-core-0050", &[][..]),
            ("(?s)^Reflog$", "repo-core-0058", &[][..]),
            ("(?s)^doctor$", "repo-core-0104", &[][..]),
            ("(?s)^folder$", "repo-core-0112", &[][..]),
            ("(?s)^owners$", "repo-core-0117", &[][..]),
            ("(?s)^builds$", "repo-core-0136", &[][..]),
            ("(?s)^Squash$", "repo-core-2444", &[][..]),
            ("(?s)^Unlock$", "repo-core-3591", &[][..]),
            ("(?s)^Search$", "repo-core-4193", &[][..]),
            ("(?s)^files$", "repo-core-0025", &[][..]),
            ("(?s)^stash$", "repo-core-0047", &[][..]),
            ("(?s)^agent$", "repo-core-0106", &[][..]),
            ("(?s)^issue$", "repo-core-0134", &[][..]),
            ("(?s)^Mixed$", "repo-core-0376", &[][..]),
            ("(?s)^Apply$", "repo-core-0872", &[][..]),
            ("(?s)^above$", "repo-core-2442", &[][..]),
            ("(?s)^below$", "repo-core-2443", &[][..]),
            ("(?s)^Fixup$", "repo-core-2446", &[][..]),
            ("(?s)^Undo$", "repo-core-0051", &[][..]),
            ("(?s)^Redo$", "repo-core-0052", &[][..]),
            ("(?s)^Soft$", "repo-core-0373", &[][..]),
            ("(?s)^Keep$", "repo-core-0380", &[][..]),
            ("(?s)^Hard$", "repo-core-0383", &[][..]),
            ("(?s)^Save$", "repo-core-1062", &[][..]),
            ("(?s)^from$", "repo-core-1101", &[][..]),
            ("(?s)^Move$", "repo-core-2441", &[][..]),
            ("(?s)^into$", "repo-core-2445", &[][..]),
            ("(?s)^Lock$", "repo-core-3593", &[][..]),
            ("(?s)^log$", "repo-core-0067", &[][..]),
            ("(?s)^lfs$", "repo-core-0127", &[][..]),
            ("(?s)^Pop$", "repo-core-0871", &[][..]),
            ("(?s)^Add$", "repo-core-0976", &[][..]),
            ("(?s)^to$", "repo-core-1102", &[][..]),
            ("(?s)^Go\\ to\\ line\\ (.*?)\\ of\\ the\\ file\\ before\\ the\\ change$", "repo-core-2040", &[("o", false)][..]),
            ("(?s)^Remove\\ unused\\ fork\\ remotes\\ \\(([0-9]+)\\)$", "repo-core-0121", &[("unused", true)][..]),
            ("(?s)^Interactive\\ rebase\\ (.*?)\\ onto\\ (.*?)…$", "repo-core-2988", &[("target", false), ("at", false)][..]),
            ("(?s)^Interactive\\ rebase\\ onto\\ (.*?)$", "repo-core-3342", &[("onto-label", false)][..]),
            ("(?s)^Revert\\ a\\ hunk\\ of\\ (.*?)\\ from\\ (.*?)$", "repo-core-3311", &[("path", false), ("hash", false)][..]),
            ("(?s)^Keep\\ both\\ versions\\ of\\ (.*?)$", "repo-core-0590", &[("path", false)][..]),
            ("(?s)^Revert\\ (.*?)\\ as\\ changed\\ in\\ (.*?)$", "repo-core-3309", &[("path", false), ("hash", false)][..]),
            ("(?s)^Revert\\ ([0-9]+)\\ line(.*?)\\ of\\ (.*?)\\ from\\ (.*?)$", "repo-core-3312", &[("n", true), ("", false), ("path", false), ("hash", false)][..]),
            ("(?s)^(.*?)\\ \\(its\\ fixup!\\ commit\\)$", "repo-core-2538", &[("label", false)][..]),
            ("(?s)^Rebase\\ commit\\ (.*?),\\ (.*?),\\ (.*?),\\ (.*?)$", "repo-core-3249", &[("hash", false), ("summary", false), ("author", false), ("label", false)][..]),
            ("(?s)^Edit\\ the\\ message\\ of\\ (.*?)$", "repo-core-3321", &[("hash", false)][..]),
            ("(?s)^Create\\ patch\\ from\\ (.*?)…$", "repo-core-2994", &[("at", false)][..]),
            ("(?s)^Commit\\ fixup!\\ for\\ (.*?)$", "repo-core-2529", &[("hash", false)][..]),
            ("(?s)^Create\\ branch\\ at\\ (.*?)…$", "repo-core-2990", &[("at", false)][..]),
            ("(?s)^Export\\ bundle\\ to\\ (.*?)$", "repo-core-0267", &[("file", false)][..]),
            ("(?s)^Update\\ submodule\\ (.*?)$", "repo-core-2844", &[("path", false)][..]),
            ("(?s)^Reset\\ (.*?)\\ to\\ here…$", "repo-core-2986", &[("target", false)][..]),
            ("(?s)^Create\\ tag\\ at\\ (.*?)…$", "repo-core-2992", &[("at", false)][..]),
            ("(?s)^Open\\ (.*?)\\ in\\ editor$", "repo-core-3001", &[("path", false)][..]),
            ("(?s)^([0-9]+)\\ picked\\ file(.*?)$", "repo-core-1498", &[("n", true), ("", false)][..]),
            ("(?s)^(.*?)\\ notes\\ “(.*?)”\\ (.*?)\\ (.*?)$", "repo-core-1100", &[("Push", false), ("namespace", false), ("", false), ("remote", false)][..]),
            ("(?s)^Rename\\ (.*?)\\ to\\ (.*?)$", "repo-core-1233", &[("old", false), ("new", false)][..]),
            ("(?s)^Go\\ to\\ line\\ ([0-9]+)$", "repo-core-2039", &[("n", true)][..]),
            ("(?s)^(Stage|Unstage|Discard|Revert)\\ ([0-9]+)\\ line(.*?)\\ of\\ (.*?)$", "repo-core-0164", &[("verb", false), ("n", true), ("", false), ("path", false)][..]),
            ("(?s)^Fold\\ into\\ (.*?)$", "repo-core-2530", &[("hash", false)][..]),
            ("(?s)^(.*?)\\ worktree\\ (.*?)$", "repo-core-3595", &[("label", false), ("name", false)][..]),
            ("(?s)^Push\\ tag\\ (.*?)$", "repo-core-3541", &[("name", false)][..]),
            ("(?s)^Doctor:\\ (.*?)$", "repo-core-0114", &[("title", false)][..]),
            ("(?s)^Unstage\\ (.*?)$", "repo-core-3010", &[("path", false)][..]),
            ("(?s)^Create\\ (pull request|merge request)$", "repo-core-0360", &[("noun", false)][..]),
            ("(?s)^Remote\\ (.*?)$", "repo-core-1468", &[("name", false)][..]),
            ("(?s)^(.*?):\\ done$", "repo-core-0269", &[("label", false)][..]),
            ("(?s)^(Soft|Mixed|Keep|Hard)\\ reset$", "repo-core-0387", &[("label", false)][..]),
            ("(?s)^Track\\ (.*?)$", "repo-core-1532", &[("upstream", false)][..]),
            ("(?s)^(Soft|Mixed|Keep|Hard)\\ reset$", "repo-core-2659", &[("label", false)][..]),
            ("(?s)^Stage\\ (.*?)$", "repo-core-3007", &[("path", false)][..]),
            ("(?s)^Fetch\\ (.*?)$", "repo-core-3495", &[("name", false)][..]),
            ("(?s)^(.*?)\\ for\\ (.*?)$", "repo-core-0729", &[("label", false), ("path", false)][..]),
            ("(?s)^Open\\ (.*?)$", "repo-core-3003", &[("path", false)][..]),
            ("(?s)^Pop\\ (.*?)$", "repo-core-3512", &[("name", false)][..]),
            ("(?s)^(Move|Squash|Fixup)\\ (.*?)\\ (above|below|into)\\ (.*?)$", "repo-core-2447", &[("verb", false), ("hash", false), ("word", false), ("target", false)][..]),
            ("(?s)^(.*?)\\ (.*?)$", "repo-core-2999", &[("label", false), ("path", false)][..]),
        ]).map(|(pattern, id, args)| {
            // Invalid source-owned patterns are bugs, never an opaque-diagnostic fallback.
            // The exhaustive pure map test initializes every pattern in both locales.
            #[expect(clippy::expect_used, reason = "fixed source-owned regexes must fail loudly if malformed")]
            let pattern = regex::Regex::new(pattern).expect("fixed operation label pattern");
            Template { pattern, id, args }
        }).collect()
    });
    if depth < 4 {
        'template: for template in templates {
            let source_family = if template.id.starts_with("repo-workflows-review-") {
                PaintFamily::Review
            } else if template.id.starts_with("repo-workflows-") {
                PaintFamily::Workflow
            } else {
                PaintFamily::Legacy
            };
            if family == PaintFamily::WorkflowFixed {
                if source_family != PaintFamily::Workflow || !template.args.is_empty() {
                    continue;
                }
            } else if family == PaintFamily::CoreLines {
                if template.id != "repo-core-0164" {
                    continue;
                }
            } else if family != source_family {
                continue;
            }
            let Some(captures) = template.pattern.captures(label) else {
                continue;
            };
            if matches!(family, PaintFamily::Workflow | PaintFamily::WorkflowFixed) {
                // Config keys/values/files are opaque. Repeated unescaped separators
                // do not identify one removal-label partition, so paint no envelope.
                if template.id == "repo-workflows-doctor-remove-confirm-b0b26eef"
                    && (label.matches(" (").count() != 1 || label.matches(") from ").count() != 1)
                {
                    return Some(Painted::Opaque);
                }
                let mut args = Vec::new();
                for (i, (name, numeric)) in template.args.iter().enumerate() {
                    if name.is_empty() {
                        continue;
                    }
                    let raw = captures.get(i + 1).map_or("", |v| v.as_str());
                    let value: crate::i18n::Value<'_> = if *numeric {
                        let Ok(number) = raw.parse::<u64>() else {
                            return Some(Painted::Opaque);
                        };
                        number.into()
                    } else {
                        let adapted = workflow_argument(template.id, name, raw, depth);
                        let Some(adapted) = adapted else {
                            // The same raw Push-refused envelope also comes from an
                            // ordinary known Push with opaque output. Only a recognized
                            // headline gets the richer adapter; other output stays data.
                            if matches!(
                                workflow::adapter(template.id, name),
                                Some(workflow::Adapter::Headline)
                            ) {
                                continue 'template;
                            }
                            return Some(Painted::Opaque);
                        };
                        adapted.into()
                    };
                    args.push((*name, value));
                }
                // Repeated command-key placeholders must still match the same raw key.
                if template.id == "repo-workflows-timeline-jump-b77dfff9"
                    && captures.get(3).map(|v| v.as_str()) != captures.get(4).map(|v| v.as_str())
                {
                    return Some(Painted::Opaque);
                }
                return Some(Painted::Known(crate::i18n::message(template.id, &args)));
            }
            // A template whose leading value is another operation must recognize it.
            // This prevents generic label/path forms from translating custom commands.
            if !matches!(
                template.id,
                "repo-core-0164" | "repo-core-2447" | "repo-core-0387" | "repo-core-2659"
            ) && template
                .args
                .first()
                .is_some_and(|(name, _)| matches!(*name, "label" | "verb" | "Push"))
            {
                let inner = captures.get(1).map_or("", |v| v.as_str());
                if !matches!(known_operation(inner, depth + 1), Some(Painted::Known(_))) {
                    continue;
                }
            }
            let args: Vec<(&str, crate::i18n::Value<'_>)> = template
                .args
                .iter()
                .enumerate()
                .filter(|(_, (name, _))| !name.is_empty())
                .map(|(i, (name, numeric))| {
                    let raw = captures.get(i + 1).map_or("", |v| v.as_str());
                    let value = if *numeric {
                        raw.parse::<f64>()
                            .map(Into::into)
                            .unwrap_or_else(|_| raw.into())
                    } else if !template.id.starts_with("repo-workflows-")
                        && !matches!(
                            template.id,
                            "repo-core-0164" | "repo-core-2447" | "repo-core-1100"
                        )
                        && matches!(*name, "label" | "verb" | "what" | "to-lowercase")
                    {
                        operation_display(raw, depth + 1).into()
                    } else if matches!(*name, "e" | "error") {
                        crate::git::display_error(&crate::forge::inbox::display_error(raw)).into()
                    } else {
                        raw.into()
                    };
                    (*name, value)
                })
                .collect();
            return Some(Painted::Known(crate::i18n::message(template.id, &args)));
        }
    }
    None
}
