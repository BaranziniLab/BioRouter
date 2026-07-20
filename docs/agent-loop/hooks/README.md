# Agent lifecycle hooks

This folder documents Biorouter's hook system: the user-configured shell commands and
LLM judges that run at specific points in the agent lifecycle — before a tool call,
after a prompt is submitted, around context compaction, when a session starts or ends,
and when the agent is about to finish a turn. Hooks are how you enforce guardrails,
inject context, log activity, or trigger notifications without modifying the agent
itself, and they apply everywhere the agent runs: the desktop app, the CLI, scheduled
runs, and subagents.

Come here when you are writing or debugging a `hooks:` entry in
`~/.config/biorouter/config.yaml` or a project's `.biorouter/hooks.yaml`, or when you
need the exact stdin/exit-code/JSON contract a hook script must obey. This is not the
place for the permission system — approval prompts and permission modes are a separate
mechanism documented under [`docs/security/`](../../security/permission-modes.md), and
hooks only intersect with it through the `PermissionRequest` event. Nor is it the place
for the other ways of shaping agent behaviour: subagents, memory, and reusable
instruction sets live in the parent [`docs/agent-loop/`](../README.md) folder. Note
also that the hook scripts themselves ship in the repository under `scripts/hooks/` —
this folder holds only the prose that explains them.

| Document | What it covers |
|---|---|
| [Hooks reference](hooks-reference.md) | The full reference for the hook system — the lifecycle events you can hook, how matchers select them, the two hook types (`command` and `prompt`), the stdin/exit-code/JSON contract, and the blocking and rewriting semantics. Current. |
| [Verify-and-checkpoint Stop hook](verify-and-checkpoint-stop-hook.md) | A guide to `scripts/hooks/verify-and-checkpoint.sh`, the opt-in Stop hook that refuses to let the agent finish a turn until its work is committed and — optionally — the project builds and its tests pass. Current, verified 2026-07-18 against the shipped script. |

Read the reference first: the verify-and-checkpoint page is one concrete hook and
assumes the event list, matchers, and contract that the reference establishes.

## Related documentation

- [Permission modes](../../security/permission-modes.md) — the approval system hooks
  can override through `PermissionRequest`, and the precedence rules that decide which
  wins.
- [Config file reference](../../configuration/config-file-reference.md) — where the
  `hooks:` section sits within the rest of `config.yaml`.
- [Subagents](../subagents.md) — the other way to control what the agent does with a
  turn, and one of the contexts hooks fire in.
- [Shadow git checkpoints](../designs/shadow-git-checkpoints.md) — the built-in
  checkpointing design that the verify-and-checkpoint hook complements.
