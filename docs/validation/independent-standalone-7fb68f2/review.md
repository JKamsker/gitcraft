Standalone file tools independent review — FAIL

Exact application `7fb68f226a1056970d5f722751a1bd2f0b937cbf`, tree `6fb37833541172109ec37ac21089f02e7fb25f04`, relative `c56d0502d7deb59ba85ac8a807c59a8d68729874`.

P2 — `src/standalone/files.rs:35–51`: save guards omit ACL/xattr metadata. If MERGED access metadata tightens after candidate preparation, unchanged identity/bytes/mtime/mode/uid/gid pass the last guard and displaced-inode comparison. The result can be installed successfully with the old ACL. An owned Linux fixture demonstrates a named-user ACL reduction from read to none with unchanged0640mode and all sampled guarded fields, while the prepared candidate keeps the earlier grant. This is a filesystem primitive reproduction plus exact-source control-flow proof, not an injected app-save execution claim. See `acl-drift-reproducer.json`.

Capture and revalidate bounded source access metadata before displacement and on the displaced inode; account for rename changing ctime. Add an actual save_with phase regression for ACL-only drift. Consider equivalent Windows descriptor-only mutation separately.

Independent existing-binary runs passed all22module and3executable cases. All349current application files match source;5author gate logs and names hash match; author full842aggregatepass/3ignored/0filtered and all25new cases verified. No fresh compilation or independent full-suite claim.

Windows helper command protocol static inspection found no filename/code interpolation or unsafe dynamic execution: constant encoded script, literal environment paths, managed ACL APIs, exact descriptor check and bounded process. No native Windows/helper fixture or macOS runtime was run. No real global configuration/provider writes, CI or application source edits occurred. This scoped failure does not replace final combined-source review.
