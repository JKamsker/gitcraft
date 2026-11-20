# Test coverage in the diff's gutter (#173 item 8).

diff-coverage-toggle = Test coverage in diffs: show or hide
diff-coverage-show = Show the test coverage of the changed lines
diff-coverage-hide = Hide the test coverage of the changed lines
diff-coverage-mark-covered = { $hits ->
    [one] Covered: ran { $hits } time
   *[other] Covered: ran { $hits } times
}
diff-coverage-mark-uncovered = Not covered: no test ran this line
diff-coverage-mark-unknown = Coverage unknown: no current evidence for this line
diff-coverage-error = The coverage report couldn't be read (hover for why)
diff-coverage-current = Test coverage from { $report } at { $sha }
diff-coverage-legend = Green: covered. Red: not covered. Hollow: unknown, the report has no current evidence for the line. Changed lines without a mark aren't executable.
diff-coverage-stale-other = Coverage from { $sha }, stale: not applied to this revision
diff-coverage-stale-edited = Coverage of HEAD ({ $sha }), stale: files changed after it was measured
diff-coverage-undetermined = Coverage of HEAD ({ $sha }): can't tell whether it includes the uncommitted edits, not applied
diff-coverage-stale-index = Coverage of the working tree: the staged version of this file differs from it, not applied
diff-coverage-no-revision = Coverage revision unknown: no { $report }.sha records the commit tested, not applied
diff-coverage-stale-tip = { $report } can't be shown to be of { $sha }, the revision shown, so every changed line is marked unknown. After a coverage run, write the full id of the commit tested into { $report }.sha.
diff-coverage-setting-label = Test coverage in diffs
diff-coverage-setting = Mark changed lines covered, uncovered or unknown
diff-coverage-setting-tip = Reads the coverage report in the usual places (lcov.info, coverage/lcov.info, coverage.xml, cobertura.xml, *.lcov). Its marks are applied only when a .sha file beside it records the revision the diff shows; otherwise the lines show unknown and the diff says why.
diff-coverage-checking = Checking the test coverage report again…
diff-coverage-checking-tip = The marks show unknown until the report is checked against the revision shown.
