# Verify-and-checkpoint Stop hook

> **What this is.** A guide to `scripts/hooks/verify-and-checkpoint.sh`, the opt-in
> Stop hook that refuses to let the agent finish a turn until its work is committed
> and — optionally — the project builds and its tests pass.
> **Status:** Current — verified 2026-07-18 against the shipped script
> `scripts/hooks/verify-and-checkpoint.sh` and the hook runtime in
> `crates/biorouter/src/hooks/`.
> **Audience:** developers running Biorouter on code projects.

This is one concrete hook, not the hook system itself. For the event list, matchers,
and the stdin/exit-code contract every hook obeys, read the
[hooks reference](hooks-reference.md) first; this page assumes it.

The hook exists because, in practice, agents (especially smaller models) routinely:

- declare a C++ project "done" without ever running `cmake` (broken build);
- run tests, see red, and finish anyway;
- leave everything uncommitted, or use a `src/` layout that only works after an
  editable install — i.e. "works in my session, broken on a clean checkout".

It turns "hope it's reproducible" into "checked": the agent's output must be
reproducible from a clean checkout before the turn ends.

## What it does

When the agent is about to stop inside a git repository:

1. **Commit / reproducibility check** (always on, cheap). If `git status` shows
   uncommitted changes, the hook blocks the stop and tells the agent to add a
   `.gitignore` for build artifacts and commit its work in logical commits.
2. **Build/test check** (opt-in, `BIOROUTER_VERIFY_BUILD=1`). Detects the toolchain
   and runs it, blocking the stop on failure:
   - `Cargo.toml` → `cargo test`
   - `CMakeLists.txt` → `cmake -S . -B build && cmake --build build`, then `ctest`
     — and, because C++ projects often forget `add_test()`, it falls back to running
     any built `*test*` executable when `ctest` finds none.
   - `pyproject.toml` / `setup.py` / `tests/*.py` → `pytest`
   - `package.json` → `npm test`

A block prints `{"decision":"block","reason":"…"}` on stdout; Biorouter feeds the
reason back to the agent so it fixes/commits, then re-evaluates.

The runtime caps consecutive Stop-hook blocks at `STOP_HOOK_BLOCK_CAP`, so this can
never loop forever — if the agent truly can't get to green, it finishes anyway with
the reason surfaced. The hook is failure-open: outside a git repo, or on any
internal error, it allows the stop.

## Enable it

Add a `Stop` hook entry to your Biorouter hooks config — the global
`~/.config/biorouter/config.yaml`, or the project hook config
`.biorouter/hooks.yaml`:

```yaml
hooks:
  Stop:
    - hooks:
        - type: command
          command: "/absolute/path/to/biorouter/scripts/hooks/verify-and-checkpoint.sh"
```

The equivalent JSON, for a config surface that takes JSON:

```json
{
  "hooks": {
    "Stop": [
      { "hooks": [
        { "type": "command",
          "command": "/absolute/path/to/biorouter/scripts/hooks/verify-and-checkpoint.sh" }
      ] }
    ]
  }
}
```

Because hooks run in Biorouter's shared core, this applies to both the CLI and the
desktop GUI.

> **Note.** Project hooks are disabled by default. If you put this in
> `.biorouter/hooks.yaml`, also set `hooks.allow_project_hooks: true` globally (or
> `BIOROUTER_ALLOW_PROJECT_HOOKS=1`) — see
> [Configuration in the hooks reference](hooks-reference.md#configuration).

## Tuning

| Setting | Effect |
|---|---|
| `BIOROUTER_VERIFY_BUILD=1` | Enable the build/test check (off by default — full test runs on every turn-end can be slow; the commit check is always on). |
| `BIOROUTER_SKIP_VERIFY_HOOK=1` | Disable the hook entirely for a run. |
| `STOP_HOOK_BLOCK_CAP` | The loop guard: the maximum number of consecutive Stop-hook blocks in a session, after which the turn finishes anyway. A built-in constant (`5`), defined in `crates/biorouter/src/hooks/mod.rs` — not a user setting. |

> **Cost.** With `BIOROUTER_VERIFY_BUILD=1` the hook may run your full test suite
> each time the agent would otherwise stop. That's the point for build-heavy QA, but
> for large suites you may prefer to leave it off and rely on the cheap commit check,
> enabling the build check only for the final push.

## Relationship to the built-in git context

The developer extension injects a git status and commit policy into its instructions
when the working directory is a repo (branch, uncommitted count, and "commit logical
units / never rewrite history without asking"). That nudges good git behavior
*during* the turn; this hook *enforces* a reproducible, green result *at the end*.
Use the context alone for a light touch, or add this hook when you want the result
guaranteed.

> **Note.** Earlier revisions of this page called that built-in git context
> "Plan A". The label came from an internal work campaign and has no index anywhere
> in `docs/`; it is dropped here to avoid sending readers looking for a document that
> does not exist.

## Related documentation

- [Hooks reference](hooks-reference.md) — the event list, matchers, and the
  stdin/exit-code contract this hook implements.
- [Shadow git checkpoints](../designs/shadow-git-checkpoints.md) — the complementary
  design for capturing agent work in git without relying on the agent to commit.
- [Environment variables](../../configuration/environment-variables.md) — the
  catalogue of the other `BIOROUTER_*` variables that change agent behaviour.
- [Managed / enterprise policy](../../security/managed-policy.md) — how an admin can
  make a Stop hook like this one mandatory and undisableable.
