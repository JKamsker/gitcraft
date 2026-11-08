# Bounded independent UI source review

Candidate: `1994266ee33ef98baca7ecb29493454f0b7f3e2d`. Result: **two static findings; changes required**.

- **UI-1 (P2), `src/repo/graph_view/mod.rs:440-441`: minimap clipped by the new horizontal ScrollArea.** The minimap strip is reserved outside the graph viewport, but history_minimap is now called using the horizontal content UI, whose X clip ends before that strip. Its painter and pointer hit target inherit that clip. Trigger: ordinary non-ghost history at a map-enabled width/height. Move minimap drawing/interaction to the parent UI after horizontal.show and carry out the vertical offset/height.
- **UI-2 (P2), `src/settings/page.rs:89-93`: compact page selector bypasses localization.** Below 760 logical points it directly displays raw Page::title() for selected text and options, giving English page names under German. Project both captions through labels::display while preserving raw enum/salt/search identities.

The inspected graph projection/motion/keyboard/a11y/deferred-badge paths, raw action/selection keys, grouped-menu raw salts, and private HunkSummary usage remain present. This is a bounded source review, not full product/native/runtime acceptance. No compiler, tests, runtime, network, Git mutation, CI, PR, or agents were used. The parent runs exact inert gates. `review.json` records the evidence, requested follow-up checks, and SHA-256 hashes for the requested source/catalog scope.
