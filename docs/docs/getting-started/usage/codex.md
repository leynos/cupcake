---
title: "Codex"
description: "Set up Cupcake with OpenAI Codex"
---

# OpenAI Codex

Cupcake integrates with OpenAI Codex through Codex command hooks. Events are
passed to `cupcake eval --harness codex` on stdin, and Cupcake returns
Codex-compatible hook JSON on stdout.

## Project Setup

```bash
cupcake init --harness codex
```

This creates:

- `.cupcake/policies/codex/` for Codex-specific policies
- `.cupcake/policies/codex/builtins/` with compatible built-in policies
- `.codex/hooks.json` with Cupcake hook commands

## Global Setup

```bash
cupcake init --global --harness codex
```

This creates global Cupcake policy structure and configures Codex hooks at
`~/.codex/hooks.json`.

## Test An Event

```bash
cat > codex-event.json <<'EOF'
{
  "session_id": "thread-1",
  "turn_id": "turn-1",
  "transcript_path": null,
  "cwd": "/path/to/project",
  "hook_event_name": "PreToolUse",
  "model": "gpt-5-codex",
  "permission_mode": "default",
  "tool_name": "Bash",
  "tool_input": { "command": "git status" },
  "tool_use_id": "call-1"
}
EOF

cupcake eval --harness codex --policy-dir .cupcake < codex-event.json
```

## Notes

Codex policies use the native Codex fields, including `hook_event_name`,
`tool_name`, `tool_input`, `prompt`, `cwd`, `model`, and `permission_mode`.
`PermissionRequest` is supported as a distinct Codex approval-flow event.

See the [Codex reference](../../reference/harnesses/codex.md) for event and
response details.
