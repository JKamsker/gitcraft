# Graph feedback

Settings → Layout → Commit graph → Animate graph changes controls brief incoming highlights,
ref movement and smooth scrolling. A successful fetch marks commits newly discovered in the
loaded history as **new** for twelve seconds. Opening a repository, ordinary reloads and
loading an older page do not create markers. At most 512 incoming commits and moving refs are
retained. A marker remains readable when motion is disabled.

Merge, reset, pull and rebase operations move a visible ref pill from its previous projected
row to its new row for 350 ms. Identity includes the full ref namespace, so a tag and branch
with the same name cannot borrow each other's movement. Saved Views keep raw commit identity
while transitions use projected positions. Graph navigation scrolls into view over 350 ms;
manual scrolling or a pointer press cancels it. Disabling animation, including egui's shared
zero-duration policy, finishes scrolling immediately and suppresses highlights and sliding.

Implementation status: UNVALIDATED. Runtime, formatting, tests and independent review are
pending the working execution handoff.
