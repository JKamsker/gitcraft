use super::*;

fn fixture(ctx: &egui::Context) -> (RepoTab, PullRequest) {
    let mut tab = RepoTab::new_deferred("/nonexistent/pr-row-layout-test".into(), ctx);
    tab.hosting = Some(Hosting {
        forge: Forge::GitHub,
        host: "github.com".into(),
        path: "owner/repo".into(),
        web: "https://github.com".into(),
    });
    tab.forks = crate::repo::forks::Forks::fixture(crate::forge::ForkInfo {
        number: 7,
        cross: true,
        repo_path: "contributor/fork".into(),
        maintainers_can_edit: true,
        first_time: true,
        ..Default::default()
    });
    let pr = PullRequest {
        number: 7,
        title: "A long pull request title that must make room for badges".into(),
        branch: "feature".into(),
        base: "main".into(),
        base_oid: String::new(),
        head_repo: Some("contributor/fork".into()),
        from_fork: true,
        sha: "head".into(),
        author: "contributor".into(),
        url: "https://github.com/owner/repo/pull/7".into(),
        draft: true,
    };
    (tab, pr)
}

fn render(tab: &RepoTab, pr: &PullRequest, width: f32, ctx: &egui::Context) -> egui::FullOutput {
    let mut acts = Vec::new();
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 120.0))),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    tab.pr_row(ui, &mut acts, Forge::GitHub, pr, true);
                });
        },
    )
}

/// Assert actual painted galleys and their clip bounds at the default sidebar width.
#[test]
fn default_width_reserves_author_draft_and_badges_and_elides_route() -> Result<(), String> {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let (tab, pr) = fixture(&ctx);
    assert!(tab.refs.local.is_empty() && tab.refs.remotes.is_empty());
    let output = render(&tab, &pr, 270.0, &ctx);
    let painted = |label: &str| {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.text() == label => Some((shape.clip_rect, t)),
                _ => None,
            })
            .ok_or_else(|| format!("Missing painted text: {label}"))
    };
    let labels = [
        "contributor",
        crate::i18n::text("repo-core-3450"),
        crate::i18n::text("repo-workflows-forks-badges-42d0a9e0"),
        "first-time",
    ];
    for label in labels {
        let (clip, text) = painted(label)?;
        assert!(!text.galley.elided, "identity or badge elided: {label}");
        assert!(text.pos.x >= clip.left(), "left-clipped: {label}");
        assert!(
            text.pos.x + text.galley.size().x <= clip.right() + 0.5,
            "right-clipped: {label}"
        );
        assert!(
            text.pos.y >= clip.top() && text.pos.y + text.galley.size().y <= clip.bottom() + 0.5
        );
    }
    let (_, title) = painted(&format!("#{} {}", pr.number, pr.title))?;
    let (_, badge) = painted(crate::i18n::text("repo-workflows-forks-badges-42d0a9e0"))?;
    assert!(title.galley.elided);
    assert!(
        title.pos.x + title.galley.size().x + 7.5 <= badge.pos.x,
        "title must leave space for badges"
    );
    let expected = "contributor/fork:feature → owner/repo:main";
    let (clip, route_text) = painted(expected)?;
    assert!(
        route_text.galley.elided,
        "route should be independently elided"
    );
    assert!(route_text.pos.x + route_text.galley.size().x <= clip.right() + 0.5);
    let (_, author) = painted("contributor")?;
    assert!(route_text.pos.x > author.pos.x + author.galley.size().x);
    assert!(tooltip(&pr, tab.hosting.as_ref(), false).contains(expected));
    assert!(tooltip(&pr, tab.hosting.as_ref(), false).contains(&pr.url));
    output.drop_without_applying_deltas();
    Ok(())
}

#[test]
fn selected_base_and_deleted_fork_routes_never_infer_owner_from_author() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let (tab, mut pr) = fixture(&ctx);
    assert_eq!(
        route(&pr, tab.hosting.as_ref()),
        "contributor/fork:feature → owner/repo:main"
    );
    pr.head_repo = None;
    pr.author = "repository-admin".into();
    assert_eq!(
        route(&pr, tab.hosting.as_ref()),
        "unknown:feature → owner/repo:main"
    );
    assert!(
        tooltip(&pr, tab.hosting.as_ref(), false).contains("unknown:feature → owner/repo:main")
    );
    let output = render(&tab, &pr, 800.0, &ctx);
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Text(t) if t.galley.text() == "unknown:feature → owner/repo:main" && !t.galley.elided)));
    output.drop_without_applying_deltas();
}

#[test]
fn tooltip_status_follows_language_for_draft_and_open_prs() {
    let ctx = egui::Context::default();
    let (tab, mut pr) = fixture(&ctx);
    for (language, draft, open) in [
        ("en-US", "Draft", "Open"),
        ("de", "Entwurf", "Offen"),
        ("en-US", "Draft", "Open"),
    ] {
        crate::i18n::set_language(language);
        for (is_draft, expected) in [(true, draft), (false, open)] {
            pr.draft = is_draft;
            assert_eq!(
                tooltip(&pr, tab.hosting.as_ref(), false),
                format!(
                    "contributor · {expected} · contributor/fork:feature → owner/repo:main\n{}",
                    pr.url
                )
            );
        }
    }
}
