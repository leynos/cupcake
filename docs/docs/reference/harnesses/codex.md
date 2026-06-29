---
title: "Codex Harness"
description: "Technical reference for OpenAI Codex harness integration"
---

# Codex Harness

Codex integrates with Cupcake through external command hooks configured in
`.codex/hooks.json` for a project or `~/.codex/hooks.json` globally.

## Setup

```bash
cupcake init --harness codex
```

The generated hook commands call:

```bash
cupcake eval --harness codex --policy-dir .cupcake
```

Policies belong in `.cupcake/policies/codex/`.

## Supported Events

Cupcake parses these Codex hook events:

| Event                 | Policy Use                                      |
| --------------------- | ----------------------------------------------- |
| `PreToolUse`          | Approve, deny, ask, or modify tool input        |
| `PermissionRequest`   | Participate in Codex's native approval flow     |
| `PostToolUse`         | Inspect tool results and block follow-up flow   |
| `UserPromptSubmit`    | Inspect submitted prompts                       |
| `SessionStart`        | Add context at session start                    |
| `SubagentStart`       | Add context for subagent lifecycle              |
| `Stop`                | Block or allow stop processing                  |
| `SubagentStop`        | Block or allow subagent stop processing         |
| `PreCompact`          | Stop compaction with Codex universal output     |
| `PostCompact`         | Stop post-compaction flow with universal output |

`SubagentStart` and `SubagentStop` require non-empty `agent_id` and
`agent_type` fields.

## Response Formats

`PreToolUse` uses Codex hook-specific permission output:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "Blocked by policy"
  }
}
```

`PermissionRequest` uses Codex's nested decision shape:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PermissionRequest",
    "decision": {
      "behavior": "deny",
      "message": "Denied by policy"
    }
  }
}
```

When Cupcake returns `Ask` for `PermissionRequest`, it emits no hook verdict so
Codex can continue its normal approval flow.

`PostToolUse`, `UserPromptSubmit`, `Stop`, and `SubagentStop` use Codex's
top-level block response when policies block:

```json
{
  "decision": "block",
  "reason": "Blocked by policy"
}
```

`PreCompact` and `PostCompact` do not support top-level block decisions. Cupcake
uses Codex universal output for compact blocking:

```json
{
  "continue": false,
  "stopReason": "Do not compact yet"
}
```

## Policy Example

```rego
# METADATA
# scope: package
# custom:
#   routing:
#     required_events: ["PreToolUse"]
#     required_tools: ["Bash"]
package cupcake.policies.codex.shell_policy

import rego.v1

deny contains decision if {
    input.hook_event_name == "PreToolUse"
    input.tool_name == "Bash"
    contains(input.tool_input.command, "--no-verify")
    decision := {
        "rule_id": "CODEX-GIT-001",
        "reason": "Do not bypass git hooks",
        "severity": "HIGH"
    }
}
```
