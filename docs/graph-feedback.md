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
The narrow system adapter honors GNOME enable-animations, KDE animation duration, macOS
reduceMotion, Windows MinAnimate and GITCRAFT_REDUCE_MOTION (1/true/yes/on or 0/false/no/off).
Queries run once on a worker using the shared bounded process runner, with a one-second
deadline and 4 KiB limits per query. Pending, unknown and failed answers keep new motion quiet.

Implementation status: UNVALIDATED. Runtime, formatting, tests and independent review are
pending the working execution handoff.

## Hear the graph

Enable **Walk graph topology** above the graph. Down follows the first parent, up follows
a loaded child, right finds the nearest descendant merge, and left finds the nearest reachable
branch tip. These follow parent edges even when an unrelated commit occupies the next row.
When several children are possible, the first in Git's displayed topological order is used.
Saved Views retain their filters and include the selected commit as context. No walk downloads
objects or runs Git; working changes and stash pseudo-commits are excluded from topology. At a loaded-history boundary, the status explains why no step was taken.

Graph rows expose merge, root, divergence, parent and loaded-child counts to AccessKit, with
flow relationships between mounted parent rows. Topology navigation moves keyboard and
screen-reader focus to the destination, while Tab remains free to leave the graph. Branch tips include their upstream's actual
ahead/behind counts. A selected commit also gets a background comparison with main, master or
the checked-out branch: ahead/behind within loaded ancestry, shared ancestor's author date and
merge count. Incomplete ancestry is explicitly identified; author date is not claimed to be
the date the branch was created.

**Graph sounds** is separately opt-in and defaults off. Soft harmonic earcons distinguish lane
crossings and merges. Rodio's native backend supports desktop platforms without subprocesses;
only playback is enabled, with no media decoders. The device is released after five idle
seconds. Device initialization is deferred until a
requested cue. Queue length is bounded, sound failure leaves navigation usable and offers Retry
audio. Test builds record dispatches without opening a sound device.

Native dependency note: Linux playback requires the ALSA development package when building.
Cargo.lock must be resolved in the working execution handoff; no dependency installation,
compilation, protocol fixture or real audio playback has run for this source cut.

## Quiet celebrations

Settings → General → Quiet celebrations has separate **Short visual flourish after success**
and **Soft sound after success** switches. Both default off. A 900 ms status flourish can
acknowledge a repository's first confirmed commit, a completed merge that moved HEAD, or a
successfully created tag explicitly marked **This is a release tag** in the tag dialog.
Arbitrary release names work; the app does not infer a release from a version-number pattern.

The operation must finish successfully and a subsequent repository read must confirm the new
root commit, tag target or moved HEAD. Failed operations, aborted merges, an up-to-date merge,
ordinary reloads and existing tags produce no celebration. A tag created locally followed by
a failed push also stays quiet. Motion follows the system preference and graph animation
policy does not change the separate celebration switches. Sound can be enabled without
motion. Neither feature tracks commit counts or rewards changed lines.
