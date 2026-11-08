use super::*;
fn state(v: logic::View) -> State {
    State {
        id: 1,
        query: String::new(),
        result: Some(Ok(Data::Subtrees(v))),
        filter: String::new(),
        tool: Tool::Subtrees,
        acknowledged: true,
        compatible: false,
        parent: String::new(),
        effective: true,
    }
}
fn frame(
    ctx: &egui::Context,
    s: &mut State,
    acts: &mut Vec<Action>,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1900., 2300.),
            )),
            events,
            ..Default::default()
        },
        |ui| show(ui, s, acts),
    )
}
fn text_pos(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
    output.shapes.iter().find_map(|s| match &s.shape {
        egui::Shape::Text(t) if t.galley.text() == label => Some(t.pos + t.galley.size() / 2.),
        _ => None,
    })
}
fn click(pos: egui::Pos2) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}
#[test]
fn subtree_pure_headless_acceptance_and_conflict_routes_are_reachable() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut s = state(logic::View::subtree_test_view());
    let mut acts = vec![];
    let mut out = frame(&ctx, &mut s, &mut acts, vec![]);
    let pos =
        text_pos(&out, "Accept this subtree update").ok_or("reviewed Ready import acceptance")?;
    assert!(text_pos(&out, "Fetch registered branch").is_some());
    assert!(text_pos(&out, "Split committed folder history").is_some());
    out.textures_delta.clear();
    let mut out = frame(&ctx, &mut s, &mut acts, click(pos));
    out.textures_delta.clear();
    assert!(acts.iter().any(|a|matches!(a,Action::RepositoryTools(Act::SubtreeJob(v,Job::Accept(id))) if id=="attempt-1" && v.head=="b".repeat(40))));
    let mut v = logic::View::subtree_test_view();
    v.state.candidates[0].stage = logic::Stage::Conflict;
    v.state.candidates[0].result = None;
    v.candidate_diff = None;
    let mut s = state(v);
    let mut out = frame(&ctx, &mut s, &mut vec![], vec![]);
    assert!(text_pos(&out, "Continue / inspect resolved candidate").is_some());
    assert!(text_pos(&out, "Accept this subtree update").is_none());
    out.textures_delta.clear();
    Ok(())
}
#[test]
fn subtree_pure_headless_split_requires_renewed_approval_and_pins_connection() -> Result<(), String>
{
    let ctx = egui::Context::default();
    let mut v = logic::View::subtree_test_view();
    v.state.candidates[0].kind = logic::Kind::Split;
    v.state.candidates[0].stage = logic::Stage::Paused;
    let mut s = state(v);
    let mut acts = vec![];
    let mut out = frame(&ctx, &mut s, &mut acts, vec![]);
    let pos = text_pos(&out, "Prepare a renewed publication review")
        .ok_or("renewed publication route")?;
    out.textures_delta.clear();
    let mut out = frame(&ctx, &mut s, &mut acts, click(pos));
    out.textures_delta.clear();
    assert!(acts.iter().any(|a|matches!(a,Action::RepositoryTools(Act::SubtreePreparePush(_,id,true)) if id=="attempt-1")));
    let profile = crate::settings::profiles::UserProfile::default();
    let context = crate::connections::Context {
        choices: profile.connections.clone(),
        ..Default::default()
    };
    assert!(selection(&context, &profile, 7, &context, &profile, 7));
    assert!(!selection(&context, &profile, 7, &context, &profile, 8));
    let mut changed = profile.clone();
    changed.id = profile.id + 1;
    assert!(!selection(&context, &profile, 7, &context, &changed, 7));
    Ok(())
}
