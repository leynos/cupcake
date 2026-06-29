# Add Codex Agent Harness Support

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: DRAFT

## Purpose / big picture

Cupcake currently supports Claude Code, Cursor, Factory AI, and OpenCode as
agent harnesses. A harness is the adapter between an AI coding agent's native
hook event format and Cupcake's policy engine. After this change, a Codex user
can run `cupcake init --harness codex`, get a Codex hook configuration, and have
Codex tool use, permission requests, prompt submission, session start, and stop
events evaluated by Cupcake policies.

The observable result is that `cupcake eval --harness codex` accepts Codex hook
JSON on stdin and prints Codex-compatible hook JSON on stdout. A blocking Rego
policy against `input.tool_name == "Bash"` must block a Codex `PreToolUse`
event, and a deny policy must be expressible for Codex `PermissionRequest`
events using Codex's `{ "behavior": "deny" }` decision shape.

Implementation must not start until this draft is explicitly approved.

## Constraints

- Do not modify `../codex` as part of the Cupcake implementation. The adjacent
  Codex checkout is reference material and an optional validation target only.
- Keep Cupcake's policy engine model unchanged: policy routing remains an
  optimization layer, and Rego policies must still self-filter on
  `input.hook_event_name`, `input.tool_name`, or equivalent fields.
- Preserve existing harness behaviour for `claude`, `cursor`, `factory`, and
  `opencode`.
- Do not introduce a new Rust crate. This is a new harness inside the existing
  `cupcake-core` and `cupcake-cli` crate boundaries.
- Do not add a new external dependency unless the implementation cannot parse
  or format Codex JSON with existing `serde`, `serde_json`, and `anyhow`
  dependencies.
- Run Rust tests with the project-required deterministic feature when using the
  workspace test command:

```bash
cargo test --workspace --features cupcake-core/deterministic-tests
```

- If an implementation step needs to rely on current OpenAI Codex behaviour not
  present in the local `../codex` checkout, verify it against official OpenAI
  Codex documentation and record the source in `Decision Log`.

## Tolerances (exception triggers)

- Scope: stop and escalate if the implementation requires touching more than 20
  files or more than 1,200 net lines of code, excluding generated snapshots or
  documentation.
- Interface: stop and escalate if any existing public API for a supported
  harness must change.
- Dependencies: stop and escalate before adding any external dependency.
- Codex contract: stop and escalate if local Codex source and official Codex
  documentation disagree on event or response schemas.
- Behaviour: stop and escalate if supporting Codex requires changing policy
  synthesis priority, currently `Halt > Deny/Block > Ask > Allow`.
- Iterations: stop and escalate if the same focused test still fails after
  three implementation attempts.
- Time: stop and escalate if any single milestone takes more than four hours.
- Ambiguity: stop and present options if "Codex support" could mean either
  Cupcake-side hook support only or changes inside the Codex agent itself.

## Risks

- Risk: Codex hook schemas are similar to Claude Code but not identical.
  Severity: high.
  Likelihood: high.
  Mitigation: model Codex as its own harness, not as an alias for Claude. Use
  `../codex/codex-rs/hooks/src/schema.rs` and generated schema fixtures as the
  source for event and response fields.

- Risk: Codex `PermissionRequest` is a distinct event with a response shape
  different from `PreToolUse`.
  Severity: high.
  Likelihood: high.
  Mitigation: add a Codex-specific response builder that maps Cupcake decisions
  separately for `PermissionRequest`.

- Risk: Codex hook configuration locations or managed-hook rules may change.
  Severity: medium.
  Likelihood: medium.
  Mitigation: implement project-level `.codex/hooks.json` support first and
  document global configuration separately. If managed hooks become required for
  enterprise deployments, handle that in a later plan.

- Risk: Existing built-in policies are harness-specific and may not all apply
  to Codex.
  Severity: medium.
  Likelihood: medium.
  Mitigation: start by porting the Claude-style builtins that depend only on
  common fields (`hook_event_name`, `tool_name`, `tool_input`, `prompt`, and
  `cwd`). Mark incompatible builtins out of scope if their event has no Codex
  equivalent.

- Risk: Tests that spawn the `cupcake` binary may be slower or sensitive to
  local target directory state.
  Severity: low.
  Likelihood: medium.
  Mitigation: add focused library tests first, then a small number of CLI
  integration tests following existing `cupcake-cli/tests` helpers.

## Progress

- [x] (2026-06-29 00:00Z) Read `CLAUDE.md`, `README.md`, harness reference
  docs, and `cupcake-cli/src/harness_config.rs` to identify existing harness
  patterns.
- [x] (2026-06-29 00:00Z) Inspected `cupcake-core/src/harness` modules and
  `cupcake-cli/src/main.rs` to identify dispatch points.
- [x] (2026-06-29 00:00Z) Inspected local Codex hook implementation in
  `../codex/codex-rs/hooks/src/schema.rs` and hook tests in
  `../codex/codex-rs/core/tests/suite/hooks.rs`.
- [x] (2026-06-29 00:00Z) Drafted this ExecPlan.
- [ ] Get explicit approval for the plan.
- [ ] Add red tests for Codex harness type parsing, event parsing, response
  formatting, CLI eval dispatch, and init configuration.
- [ ] Implement the Codex event and response modules.
- [ ] Wire Codex through core harness types, CLI harness selection, init, and
  builtins deployment.
- [ ] Add Codex documentation and examples.
- [ ] Run focused and workspace validation commands.

## Surprises & discoveries

- Observation: Codex already has a dedicated `codex-hooks` crate with generated
  JSON schema fixtures.
  Evidence: `../codex/codex-rs/hooks/src/schema.rs` defines command input and
  output structs, and `../codex/codex-rs/hooks/schema/generated/` contains
  generated schemas.
  Impact: The Cupcake Codex harness should be schema-compatible with Codex, not
  merely Claude-compatible.

- Observation: Codex has a separate `PermissionRequest` hook event.
  Evidence: `../codex/codex-rs/hooks/src/schema.rs` defines
  `PermissionRequestCommandInput` and `PermissionRequestCommandOutputWire`, and
  Codex tests create outputs such as
  `{ "decision": { "behavior": "deny", "message": "..." } }`.
  Impact: Codex response formatting needs event-specific handling that existing
  Claude and Factory response builders do not provide.

- Observation: Codex plugin hooks can be packaged in a plugin under
  `hooks/hooks.json`.
  Evidence: `../codex/codex-rs/core/tests/suite/hooks.rs` includes a plugin hook
  fixture using `${PLUGIN_ROOT}/hooks/pre_tool_use_hook.py`.
  Impact: Cupcake can start with direct `.codex/hooks.json` installation and
  later add an optional Codex plugin package if distribution demands it.

## Decision log

- Decision: Treat Codex as a new first-class harness named `codex`, not as an
  alias for `claude`.
  Rationale: Codex and Claude share several hook response field names, but Codex
  includes `turn_id`, `permission_mode`, `tool_use_id`, and a distinct
  `PermissionRequest` event shape.
  Date/Author: 2026-06-29 / Codex agent.

- Decision: Keep implementation in `cupcake-core` and `cupcake-cli`; do not add
  a new crate.
  Rationale: Existing harness support follows this boundary. Core owns event and
  response translation; CLI owns setup and user-facing commands.
  Date/Author: 2026-06-29 / Codex agent.

- Decision: Add test coverage before production code.
  Rationale: The project already has practical Rust tests, so Red-Green-Refactor
  is available and required by this plan.
  Date/Author: 2026-06-29 / Codex agent.

- Decision: Make Codex `PermissionRequest` response mapping conservative.
  Rationale: Cupcake `Deny`, `Block`, and `Halt` should become Codex
  `{ "behavior": "deny" }`; `Allow`, `Modify`, and `Ask` need explicit mapping
  because Codex permission requests only accept `allow` or `deny`.
  Date/Author: 2026-06-29 / Codex agent.

## Outcomes & retrospective

No implementation has started. This draft captures the intended scope,
constraints, tests, and validation steps. Update this section after each
approved milestone with what changed, what passed, and any gaps.

## Context and orientation

The Cupcake repository is a Rust workspace with `cupcake-core` and
`cupcake-cli` as default members. `cupcake-core` contains the policy engine and
agent harness translators. `cupcake-cli` exposes user commands such as
`cupcake init` and `cupcake eval`.

The existing harness implementation is split across these paths:

- `cupcake-core/src/harness/types.rs` defines the core `HarnessType` enum.
- `cupcake-core/src/harness/events/` contains typed event schemas for each
  harness.
- `cupcake-core/src/harness/response/` contains response builders for each
  harness.
- `cupcake-core/src/harness/mod.rs` exposes parse and format functions such as
  `ClaudeHarness::format_response`.
- `cupcake-cli/src/main.rs` defines the CLI-facing `HarnessType` enum, reads
  stdin for `cupcake eval`, evaluates policies, and dispatches to the selected
  harness response formatter.
- `cupcake-cli/src/harness_config.rs` configures agent hook files during
  `cupcake init`.

The adjacent Codex checkout at `../codex` is reference material. The important
Codex files are:

- `../codex/codex-rs/hooks/src/schema.rs`, which defines hook stdin and stdout
  shapes.
- `../codex/codex-rs/hooks/schema/generated/*.schema.json`, which contains
  generated JSON schemas.
- `../codex/codex-rs/core/tests/suite/hooks.rs`, which demonstrates real hook
  fixtures and response parsing.
- `../codex/codex-rs/plugin/src/provider_tests.rs`, which shows plugin hook
  packaging via `.codex-plugin/plugin.json` and `hooks/hooks.json`.

Codex hook command input is JSON sent on stdin. For `PreToolUse`, the important
fields are:

```json
{
  "session_id": "thread-id",
  "turn_id": "turn-id",
  "transcript_path": null,
  "cwd": "/path/to/project",
  "hook_event_name": "PreToolUse",
  "model": "gpt-5-codex",
  "permission_mode": "on-request",
  "tool_name": "Bash",
  "tool_input": { "command": "git status" },
  "tool_use_id": "call-123"
}
```

For `PermissionRequest`, Codex uses the same common fields but omits
`tool_use_id`. A deny response has this shape:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PermissionRequest",
    "decision": {
      "behavior": "deny",
      "message": "Policy blocked this permission request"
    }
  }
}
```

For `PreToolUse`, Codex accepts Claude-style permission output:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "Policy blocked this tool call"
  }
}
```

For `PostToolUse`, Codex accepts top-level block output and additional context:

```json
{
  "decision": "block",
  "reason": "Policy blocked after observing tool output"
}
```

and:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PostToolUse",
    "additionalContext": "Observed useful output"
  }
}
```

## Plan of work

Stage A is a contract spike. Confirm whether the implementation should support
the Codex hook events already present in `../codex`: `PreToolUse`,
`PermissionRequest`, `PostToolUse`, `UserPromptSubmit`, `SessionStart`,
`SubagentStart`, `Stop`, `SubagentStop`, `PreCompact`, and `PostCompact`.
Record the final list in `Decision Log`. If implementation scope must be
reduced, start with `PreToolUse`, `PermissionRequest`, and `PostToolUse` because
those directly control tool execution and permission approval.

Stage B is red tests. Add tests before production code:

- In `cupcake-core/src/harness/types.rs`, add tests proving `"codex"`,
  `"openai-codex"`, and `"CODEX"` parse to the new core harness type and
  `policy_dir()` returns `"codex"`.
- Add `cupcake-core/src/harness/events/codex/` tests proving each supported
  Codex event deserializes from Codex-shaped JSON and exposes `event_name()`,
  common data, and tool data where relevant.
- Add `cupcake-core/src/harness/response/codex/` tests proving `FinalDecision`
  maps to Codex-compatible JSON for `PreToolUse`, `PermissionRequest`,
  `PostToolUse`, `UserPromptSubmit`, `SessionStart`, and `Stop`.
- Add a CLI test in `cupcake-cli/tests/harness_integration_test.rs` proving
  `cupcake eval --harness codex` blocks a Bash `PreToolUse` event when a test
  policy denies it.
- Add an init test in `cupcake-cli/tests/init_command_test.rs` proving
  `cupcake init --harness codex` creates `.cupcake/policies/codex/` and a
  Codex hook configuration file.

The red command for focused core tests is:

```bash
cargo test -p cupcake-core codex
```

Before implementation, this command should fail with unresolved Codex symbols
or missing enum variants. That is the expected red failure.

Stage C is the minimal implementation. Add:

- `HarnessType::Codex` to `cupcake-core/src/harness/types.rs`, with
  `as_str() == "codex"`, `display_name() == "Codex"`, and
  `policy_dir() == "codex"`.
- `cupcake-core/src/harness/events/codex/` containing event structs that mirror
  Codex hook input names and casing.
- `cupcake-core/src/harness/response/codex/` containing a Codex response
  builder. This builder should prefer Codex's hook-specific output when an
  event supports it and should preserve top-level block output where Codex
  expects that form.
- `CodexHarness` in `cupcake-core/src/harness/mod.rs` with `parse_event` and
  `format_response`.
- CLI enum and dispatch updates in `cupcake-cli/src/main.rs`.
- Codex setup support in `cupcake-cli/src/harness_config.rs`.

The recommended project-level Codex hook file is `.codex/hooks.json` unless the
contract spike proves Codex only reads `~/.codex/hooks.json` or plugin-managed
hook files for the target version. The generated project hook should evaluate
at least `PreToolUse`, `PermissionRequest`, and `PostToolUse`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "cupcake eval --harness codex --policy-dir .cupcake"
          }
        ]
      }
    ],
    "PermissionRequest": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "cupcake eval --harness codex --policy-dir .cupcake"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "cupcake eval --harness codex --policy-dir .cupcake"
          }
        ]
      }
    ]
  }
}
```

If Codex supports plugin-bundled hooks but not project `.codex/hooks.json` in
the targeted version, replace the file generation step with a bundled plugin
layout under `.codex/plugins/cupcake/` and record that decision.

Stage D is builtins and docs. Port compatible Claude-style builtins to Codex by
creating Codex-specific Rego templates in the same location as existing
harness-specific policy constants. Each policy must self-filter on Codex event
and tool fields. Add:

- `docs/docs/getting-started/usage/codex.md`.
- `docs/docs/reference/harnesses/codex.md`.
- Updates to `docs/docs/reference/harnesses/index.md`.
- Updates to `README.md` supported harnesses table.

Stage E is refactor and validation. Remove duplication only where a shared
helper is already natural. Do not prematurely merge Codex and Claude response
builders if `PermissionRequest` handling would make the shared abstraction
harder to understand.

## Concrete steps

1. From `/data/leynos/Projects/cupcake`, confirm the working tree:

```bash
git status --short
```

Expected output before implementation may include this ExecPlan only:

```plaintext
?? docs/execplans/codex-agent-support.md
```

1. Add red tests for the core harness type and Codex event/response modules.
Run:

```bash
cargo test -p cupcake-core codex
```

Expected red output is a compile failure that names missing Codex event modules
or missing `HarnessType::Codex`.

1. Implement the core Codex harness. Re-run:

```bash
cargo test -p cupcake-core codex
```

Expected green output:

```plaintext
test result: ok.
```

1. Add red CLI tests for `cupcake eval --harness codex` and
`cupcake init --harness codex`. Run:

```bash
cargo test -p cupcake-cli codex
```

Expected red output is either clap rejecting `codex` as an unknown harness or
the init/eval dispatch failing because `CodexHarness` is not wired.

1. Implement CLI enum, eval dispatch, init deployment, and hook config
generation. Re-run:

```bash
cargo test -p cupcake-cli codex
```

Expected green output:

```plaintext
test result: ok.
```

1. Add documentation and update harness tables. Then run format and lint:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --features cupcake-core/deterministic-tests
```

Expected output has no formatting diff and no clippy errors.

1. Run the full Rust validation gate:

```bash
cargo test --workspace --features cupcake-core/deterministic-tests
```

Expected output:

```plaintext
test result: ok.
```

1. Optional Codex-repo smoke validation if `../codex` has a built Codex binary
or can build one within tolerance. Create a temporary Codex home and project,
write a hook that invokes the local Cupcake binary, run a Codex command that
would invoke `Bash`, and observe the policy block. If this step requires
changing `../codex`, stop and skip it.

## Validation and acceptance

Red-Green-Refactor evidence must be recorded in this plan during
implementation:

- Red: `cargo test -p cupcake-core codex` fails before production code because
  Codex core harness symbols do not exist.
- Green: `cargo test -p cupcake-core codex` passes after adding the minimal core
  harness implementation.
- Red: `cargo test -p cupcake-cli codex` fails before CLI wiring because `codex`
  is not a recognized CLI harness or eval dispatch target.
- Green: `cargo test -p cupcake-cli codex` passes after CLI wiring.
- Refactor: `cargo fmt --all`, `cargo clippy --workspace --all-targets
  --features cupcake-core/deterministic-tests`, and `cargo test --workspace
  --features cupcake-core/deterministic-tests` all pass.

Acceptance criteria:

- `cupcake init --harness codex` creates `.cupcake/policies/codex/` and a Codex
  hook configuration without creating unrelated harness policy directories.
- `cupcake eval --harness codex` accepts a Codex `PreToolUse` event and returns
  a Codex `PreToolUse` response.
- A Cupcake deny/block decision for Codex `PreToolUse` produces
  `permissionDecision: "deny"` with a non-empty reason.
- A Cupcake allow decision for Codex `PreToolUse` produces an allow response or
  an empty response accepted by Codex.
- A Cupcake deny/block/halt decision for Codex `PermissionRequest` produces
  `{ "behavior": "deny" }` with a message.
- A Cupcake allow decision for Codex `PermissionRequest` produces
  `{ "behavior": "allow" }`.
- Existing Claude, Cursor, Factory, and OpenCode tests still pass.

Quality criteria:

- Tests: focused core and CLI Codex tests pass; full workspace Rust tests pass
  with `cupcake-core/deterministic-tests`.
- Lint/typecheck: `cargo clippy --workspace --all-targets --features
  cupcake-core/deterministic-tests` passes.
- Documentation: Codex appears in user setup docs and harness reference docs.
- Security: hook setup must fail closed where Codex supports blocking; generated
  configuration must not disable Codex sandboxing or approvals.

## Idempotence and recovery

All implementation steps are file additions or deterministic edits. Re-running
`cargo fmt`, focused tests, and full workspace tests is safe.

`cupcake init --harness codex` must be idempotent in the same way as existing
harness setup: if a hook file already exists, merge Cupcake hooks without
deleting unrelated user configuration. If a test leaves a temporary project, it
must use `tempfile::TempDir` so cleanup is automatic.

If hook configuration generation writes an invalid JSON file during
development, delete only the temporary test directory. Never delete a user's
real `.codex` or `~/.codex` directory during tests.

## Artifacts and notes

Useful Codex response examples from the local Codex tests:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "do not run that"
  }
}
```

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow",
    "updatedInput": {
      "command": "echo rewritten"
    }
  }
}
```

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PermissionRequest",
    "decision": {
      "behavior": "allow"
    }
  }
}
```

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PermissionRequest",
    "decision": {
      "behavior": "deny",
      "message": "permission denied by policy"
    }
  }
}
```

## Interfaces and dependencies

At the end of implementation, these interfaces must exist:

```rust
pub enum HarnessType {
    ClaudeCode,
    Cursor,
    Factory,
    OpenCode,
    Codex,
}
```

```rust
pub struct CodexHarness;

impl CodexHarness {
    pub fn parse_event(input: &str) -> anyhow::Result<events::codex::CodexEvent>;

    pub fn format_response(
        event: &events::codex::CodexEvent,
        decision: &crate::engine::decision::FinalDecision,
    ) -> anyhow::Result<serde_json::Value>;
}
```

```rust
pub enum CodexEvent {
    PreToolUse(PreToolUsePayload),
    PermissionRequest(PermissionRequestPayload),
    PostToolUse(PostToolUsePayload),
    UserPromptSubmit(UserPromptSubmitPayload),
    SessionStart(SessionStartPayload),
    SubagentStart(SubagentStartPayload),
    Stop(StopPayload),
    SubagentStop(SubagentStopPayload),
    PreCompact(PreCompactPayload),
    PostCompact(PostCompactPayload),
}
```

The concrete payload structs may use shared common structs where that reduces
duplication without hiding the Codex wire format. Fields must preserve Codex
JSON names via serde attributes instead of custom string manipulation.

Revision note: Initial draft created from local Cupcake documentation, Cupcake
harness source, local Codex hook source, local Codex hook tests, and official
Codex hook documentation. It defines a Cupcake-only implementation path and
keeps Codex-repo changes out of scope pending explicit approval.
