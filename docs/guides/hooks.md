# Hooks

Hooks let you run your own shell commands — or an LLM judge — at specific
points in Biorouter's agent lifecycle: before a tool runs, after a prompt is
submitted, around context compaction, when a session starts or ends, and more.
Use them to enforce guardrails, inject context, log activity, or trigger
notifications.

Hooks work everywhere the agent runs: the desktop app, the CLI, scheduled
runs, and subagents.

## Configuration

Hooks live under a `hooks:` section in your global config
(`~/.config/biorouter/config.yaml`):

```yaml
hooks:
  PreToolUse:
    - matcher: "developer__shell"
      hooks:
        - type: command
          command: "$HOME/hooks/shell-guard.sh"
          timeout: 30
  PreCompact:
    - matcher: "auto"
      hooks:
        - type: command
          command: "cp ~/.config/biorouter/sessions/* ~/backups/ 2>/dev/null || true"
```

Projects can ship their own hooks in `.biorouter/hooks.yaml` at the session
working directory, using the same schema under a top-level `hooks:` key.
**Project hooks are disabled by default** — a repository should not be able to
run commands on your machine just because you opened it. Opt in globally with:

```yaml
hooks:
  allow_project_hooks: true
```

(or `BIOROUTER_ALLOW_PROJECT_HOOKS=1`). Project hooks run *in addition to*
global hooks, never instead of them.

## Events

| Event | Fires | Can block? | Matcher matches on |
|---|---|---|---|
| `PreToolUse` | before a tool call executes | yes — `deny` / `ask` | tool name |
| `PostToolUse` | after a tool call succeeds | no (context only) | tool name |
| `PostToolUseFailure` | after a tool call fails | no | tool name |
| `PermissionRequest` | when a tool would show an approval prompt | yes — `allow` / `deny` | tool name |
| `UserPromptSubmit` | when a user prompt is submitted | yes | — |
| `Stop` | when the agent is about to finish its turn | yes (capped at 5) | — |
| `SessionStart` | first prompt of a session in this process | no (context only) | `startup` \| `resume` |
| `SessionEnd` | CLI exit, headless completion, scheduled-run completion | no | exit reason |
| `Notification` | when a permission prompt is shown | no | `permission_prompt` |
| `PreCompact` / `PostCompact` | around context compaction | no | `manual` \| `auto` |
| `SubagentStart` / `SubagentStop` | around a subagent task | no | — |

Matchers: omit (or use `""` / `"*"`) to match everything; otherwise exact
match, `a|b` alternation, or a full regex (anchored). Tool names follow the
`extension__tool` convention, e.g. `developer__shell`, `developer__.*`.

## Hook types

**`command`** — a shell command. It receives the event as JSON on stdin and
runs in the session working directory with `BIOROUTER_HOOK_EVENT`,
`BIOROUTER_SESSION_ID`, and `BIOROUTER_PROJECT_DIR` set. Default timeout 60s
(`timeout` overrides, in seconds).

**`prompt`** — an LLM judge. The rule you write is evaluated against the event
payload by your configured provider (its fast model when available, or an
explicit `provider:` + `model:` pair). The judge answers
`{"ok": true|false, "reason": "..."}`; `ok: false` blocks. Default timeout 30s.

```yaml
hooks:
  PreToolUse:
    - matcher: "developer__shell"
      hooks:
        - type: prompt
          prompt: "Block any command that deletes files outside the project directory."
```

## Command hook contract

Input on stdin (snake_case, Claude Code-compatible):

```json
{
  "session_id": "…",
  "cwd": "/path/to/project",
  "hook_event_name": "PreToolUse",
  "tool_name": "developer__shell",
  "tool_input": {"command": "rm -rf build"}
}
```

Exit codes:

- **0** — success. stdout may contain a JSON decision (below). For
  `UserPromptSubmit` and `SessionStart`, plain (non-JSON) stdout is injected
  as context for the model.
- **2** — block. stderr is used as the reason: for `PreToolUse` it is fed back
  to the model, for `UserPromptSubmit` it is shown to the user, for `Stop` it
  becomes feedback the agent must address before finishing.
- anything else — non-blocking error; the event proceeds (failure-open).

Optional JSON on stdout (camelCase, Claude Code-compatible):

```json
{
  "decision": "block",
  "reason": "tests have not been run",
  "systemMessage": "shown to the user as a yellow notice",
  "hookSpecificOutput": {
    "permissionDecision": "allow | deny | ask",
    "permissionDecisionReason": "…",
    "additionalContext": "injected for the model"
  }
}
```

When several hooks match one event they run in parallel and the most
restrictive decision wins (`deny` > `ask` > `allow`).

## Semantics worth knowing

- **Failure-open everywhere.** A crashing, timing-out, or misconfigured hook
  never blocks the agent; only explicit decisions do.
- **PreToolUse `deny`** turns the tool call into an error result containing
  your reason, so the model can adapt. **`ask`** routes the call through the
  normal approval dialog with your reason attached.
- **PermissionRequest `allow`** auto-approves a call that would otherwise
  prompt the user — useful for trusted commands in trusted projects.
- **Stop blocks are capped** at 5 consecutive blocks per session; the payload
  field `stop_hook_active` is `true` on re-checks so well-behaved hooks can
  exit early.
- **GUI sessions do not fire `SessionEnd`** in v1 (there is no reliable
  session-close signal in the desktop app).
- Hook activity (blocks, judge verdicts, `systemMessage`s) appears as yellow
  inline notices in both the CLI and the desktop app chat.

## Examples

Block destructive shell commands:

```bash
#!/bin/bash
# ~/hooks/shell-guard.sh — PreToolUse, matcher developer__shell
input=$(cat)
command=$(echo "$input" | jq -r '.tool_input.command // ""')
if echo "$command" | grep -qE 'rm -rf|mkfs|dd if='; then
  echo "Destructive command blocked by shell-guard" >&2
  exit 2
fi
```

Require a clean test run before the agent finishes:

```yaml
hooks:
  Stop:
    - hooks:
        - type: command
          command: "cargo test --quiet 2>/dev/null || { echo 'tests are failing; fix them before finishing' >&2; exit 2; }"
          timeout: 300
```

Inject lab context at session start:

```yaml
hooks:
  SessionStart:
    - hooks:
        - type: command
          command: "cat ~/.config/biorouter/lab-context.md 2>/dev/null"
```
