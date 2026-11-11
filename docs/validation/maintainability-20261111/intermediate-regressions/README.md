# Intermediate regression evidence

These are historical WIP checks, not final-candidate validation. Result records bind
immutable commits and application/test binary hashes. Build targets and scripts are
on E:. Runtime scratch uses private native Linux directories for filesystem fidelity.

* `baf6cc14efc6a4974562de3e1bddc555b18ac2dd`: combined library run failed:
  5,161 passed, five failed, 27 ignored, four filtered. Isolated filtered tests and
  doc tests were not reached. Failures exposed CRLF shell checkout and diff UI issues.
* `651a673f9a1334fac5d3a648493e4897501556a7`: test build passed. Linux and
  Windows cross-target lint failed on 54 fixture diagnostics. Targeted execution
  passed six pipe, three outcome and one resolver recording checks, 54 repository
  Insights tests, five checkpoint tests and the owned-host remote repair test.
  Two diff-layout tests, keyboard navigation and the layout zoom test failed,
  motivating passive navigation restoration and correct egui/overflow fixtures.
* `638bc66e9d94116c3ba61d8a5559c8ead10fe192`: all 61 Python tests passed in
  an immutable E: clone with private E: fixtures. This is not execution on later heads.
* `5107972275102b712d2e435ea315f57453209ba0`: strict Linux all-target
  Clippy passed for all three tracked Cargo packages, including the isolated
  recovery-recording profile. Later application changes require new gates.

Later targeted runners use the same pinned 651a executable. The shell runner checks
the original recording log hashes/counts before owned-child tests. Real children
run in a probed PID/proc/dev namespace, without host PID searches or host signals.

Gzip files preserve original bytes. The manifest records original/compressed hashes;
saved runners match result-record hashes. Final gates must be bound separately.
