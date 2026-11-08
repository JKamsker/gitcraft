# Fluent tool and signing settings

This incremental unit adds 36 paired English/German messages for external diff
and merge tools, AI settings and signing settings. Whole explanatory patterns
include named arguments for config keys, programs and profile names. AI diff-size
editing uses the existing locale formatter and matching settings parser.

Stored AI prompt payloads and their literal substitution tokens remain unchanged;
only the controls and explanations around them translate. Tool enums, serialized
values, command lines, Git config keys, presets, paths and signing data retain
their identities. External-tool combo IDs still use the raw tool kind. Tool
product names and technical signing-format names remain literal. Raw discovery
and Git errors remain raw pending the owned render-adapter integration.

Inert headless regressions exercise the actual prompt reset button, external-tool
picker and signing field rows in German, then switch to English. They verify the
original prompt payload, config/enum identities and edited signing values.
Neither the AI discovery page nor the signing page's global-config read/write
entry point is called. Only audited pure/headless cases and all-target compilation
are permitted; no application process, tool discovery, global Git config,
credential, network, native or full-suite execution is claimed. Exact immutable
checks are recorded outside the repository in
`/tmp/gitcraft-fluent-settings-tools-validation.json`.

Themes, custom commands, profile/connection/network pages, previews, the full
manual and other interaction modules remain pending. This is writer-validated
localization WIP, not a claim of complete Fluent migration.
