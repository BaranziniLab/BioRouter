# Stop hook: verify build/tests + git checkpoint

`scripts/hooks/verify-and-checkpoint.sh` is an **opt-in** Biorouter
[Stop hook](../../crates/biorouter/src/hooks) that makes the agent's output
**reproducible from a clean checkout** before it finishes a turn.

It exists because, in practice, agents (especially smaller models) routinely:

- declare a C++ project "done" without ever running `cmake` (broken build);
- run tests, see red, and finish anyway;
- leave everything **uncommitted**, or use a `src/` layout that only works after
  an editable install — i.e. "works in my session, broken on a clean checkout".

This hook turns "hope it's reproducible" into "checked".

## What it does

When the agent is about to stop **inside a git repository**:

1. **Commit / reproducibility check (always on, cheap).** If `git status` shows
   uncommitted changes, the hook **blocks** the stop and tells the agent to add a
   `.gitignore` for build artifacts and commit its work in logical commits.
2. **Build/test check (opt-in, `BIOROUTER_VERIFY_BUILD=1`).** Detects the
   toolchain and runs it, blocking the stop on failure:
   - `Cargo.toml` → `cargo test`
   - `CMakeLists.txt` → `cmake -S . -B build && cmake --build build`, then `ctest`
     — and, because C++ projects often forget `add_test()`, it falls back to
     running any built `*test*` executable when `ctest` finds none.
   - `pyproject.toml` / `setup.py` / `tests/*.py` → `pytest`
   - `package.json` → `npm test`

A block prints `{"decision":"block","reason":"…"}` on stdout; Biorouter feeds the
reason back to the agent so it fixes/commits, then re-evaluates. The runtime
**caps consecutive Stop-hook blocks** (`STOP_HOOK_BLOCK_CAP`), so this can never
loop forever — if the agent truly can't get to green, it finishes anyway with the
reason surfaced. The hook is **failure-open**: outside a git repo, or on any
internal error, it allows the stop.

## Enable it

Add to your Biorouter hooks config (e.g. `~/.config/biorouter/config.yaml` or the
project hook config):

```json
{
  "hooks": {
    "Stop": [
      { "hooks": [
        { "type": "command",
          "command": "/absolute/path/to/BioRouter/scripts/hooks/verify-and-checkpoint.sh" }
      ] }
    ]
  }
}
```

Because hooks run in Biorouter's shared core, this applies to **both the CLI and
the desktop GUI**.

## Tuning

| Env var | Effect |
|---|---|
| `BIOROUTER_VERIFY_BUILD=1` | Enable the build/test check (off by default — full test runs on every turn-end can be slow; the commit check is always on). |
| `BIOROUTER_SKIP_VERIFY_HOOK=1` | Disable the hook entirely for a run. |

**Cost note:** with `BIOROUTER_VERIFY_BUILD=1` the hook may run your full test
suite each time the agent would otherwise stop. That's the point for
build-heavy QA, but for large suites you may prefer to leave it off and rely on
the (cheap) commit check, enabling the build check only for the final push.

## Relationship to the built-in git context (Plan A)

The developer extension also now injects a **git status + commit policy** into its
instructions when the working directory is a repo (branch, uncommitted count, and
"commit logical units / never rewrite history without asking"). That nudges good
git behavior *during* the turn; this hook *enforces* a reproducible, green result
*at the end*. Use the context alone for a light touch, or add this hook when you
want the result guaranteed.
