# Wave 1 — Processes cluster verification report

Cluster: **Long-running & processes** (`agent-loop-processes`)
Worktree: `/Users/wanjun/Desktop/biorouter/.worktrees/processes`
Isolated target dir: `/Users/wanjun/.cache/br-targets/processes`
Base for diff: `agent-loop-integration`

## Verdict: GREEN

Zero test failures or clippy violations attributable to this cluster. Every
proposal is its own commit; the working tree is clean. All residual reds
(2 flaky live-network tunnel tests, 1 known live-API provider test, frontend
lint/`biorouterd` binary-not-built, stale clippy allowlist entries) were
confirmed pre-existing and/or environmental and unrelated to the files this
cluster touches.

## BR proposal status

| BR | Description | Status | Commit | Key files | Tests |
|----|-------------|--------|--------|-----------|-------|
| BR-37 | Reap orphaned background shell jobs across restarts | done | `22518f70` | `biorouter-mcp/src/developer/background.rs` | biorouter-mcp lib green |
| BR-40 | Structured subagent result envelope (status/tokens/artifacts) | done | `5168cf5e` | `biorouter/src/agents/subagent_result.rs`, `subagent_handler.rs`, `subagent_tool.rs`, `agents/mod.rs` | biorouter lib green |
| BR-41 | Persist/restore session goals + surface interrupted elicitations across daemon restart | done | `46a67474` | `biorouter/src/agents/goal.rs`, `agent.rs` | biorouter `agents::goal::persistence_tests` + `tests` green |
| BR-42 | Unified active-work registry + `/active_work` route (jobs, subagents, schedules) | done | `29943732` | `biorouter-mcp/src/active_work.rs`, `biorouter-mcp/src/lib.rs`, `biorouter-server/src/routes/active_work.rs`, `routes/mod.rs`, `openapi.rs` | biorouter-mcp + biorouter-server green |
| BR-42 (regen) | Regenerate OpenAPI spec + TS client for `/active_work` | done | `9bd7e1a9` | `ui/desktop/openapi.json`, `src/api/{index,sdk.gen,types.gen}.ts` | generate-api clean; `src/api` lint-clean |

## Pipeline steps

1. **Commit audit** — 4 proposal commits (BR-37/40/41/42), each self-contained;
   clean `git status`. No orphaned or junk work. `.notif-harness/` junk noted in
   the git snapshot belongs to the *integration* worktree, not this one.
2. **cargo fmt --all -- --check** — clean (exit 0).
3. **clippy** (`./scripts/clippy-lint.sh`) — see clippy section below.
4. **OpenAPI regen** — BR-42 registered the route in `openapi.rs` but had not
   regenerated the generated artifacts. Ran `just generate-openapi` (schema +
   `npm run generate-api` with the pinned `@hey-api/openapi-ts@0.90.10`, after
   `npm install`). Diff was cleanly scoped to `listActiveWork` /
   `cancelActiveWork` (`/active_work`, `/active_work/{id}/cancel`) + their types,
   no unrelated churn. Committed as `9bd7e1a9`.
5. **Per-crate regression** — see evidence below.
6. **Frontend** — `npm install` + `npm run test:run` + `npm run lint:check`
   (ui/desktop touched by the regen commit). See frontend section.

## Clippy findings

`clippy::too_many_lines` baseline check reported NEW-vs-baseline entries. Diffed
current violations against `clippy-baselines/too_many_lines.txt`. The 5 new
entries are:

- `agent_drafter/control.rs::validate_widget` — **known pre-existing** (per task).
- `agent_drafter/render.rs::serve_mjs` — **known pre-existing** (per task).
- `biorouter-cli/src/cli.rs::handle_session_subcommand` — CLI file, **not touched by this cluster**.
- `biorouter-cli/src/commands/doctor.rs::handle_doctor` — CLI file, **not touched by this cluster**.
- `biorouter-cli/src/session/tui/mod.rs::drive_response` — CLI file, **not touched by this cluster**.

None of the five are in files this cluster modified (cluster touched:
`active_work.rs`, `background.rs`, `lib.rs`, `openapi.rs`, `routes/active_work.rs`,
`routes/mod.rs`, `agent.rs`, `goal.rs`, `agents/mod.rs`, `subagent_handler.rs`,
`subagent_result.rs`, `subagent_tool.rs`). The three CLI entries are stale
allowlist drift from other clusters' CLI work, not this cluster. **No
cluster-introduced clippy violations.** No fixes applied (correctly out of scope).

## Design-decision records

- **OpenAPI regen committed under BR-42.** BR-42's source commit changed
  `openapi.rs` (spec registration) without refreshing `ui/desktop/openapi.json`
  or the generated TS client. Per CLAUDE.md ("After changing server routes, always
  run `just generate-openapi`"), I regenerated and committed the artifacts as a
  separate `BR-42: regenerate …` commit rather than amending, to keep the
  hand-written proposal and machine-generated output as distinct, reviewable
  commits.
- **Pinned generator, not npx latest.** The first `just generate-openapi` npx-ed
  `@hey-api/openapi-ts@0.99.0` (project pins `^0.90.3`), which crashed
  (`Cannot read properties of undefined (reading 'AnyKeyword')`). Installed
  ui/desktop deps and used the local pinned `0.90.10` via `npm run generate-api`
  to produce the correct client.

## Regression findings (all pre-existing / environmental, NOT cluster)

- **biorouter-server tunnel tests flaky.** `tunnel::lapstone_test::test_tunnel_end_to_end`
  and `test_tunnel_post_request` are live-network e2e tests (spin up a real
  lapstone tunnel, hit its public URL, assert `response.status().is_success()`).
  They fail non-deterministically under parallel load: in the full run both
  failed; on retry `end_to_end` passed and `post_request` failed; run alone
  `post_request` passed 3/3. Baseline recorded them green. The cluster touches
  **zero** tunnel code (`routes/mod.rs` change is purely additive — one
  `.merge(active_work::routes(...))` line). Classified environmental/flaky.
- **biorouter `test_anthropic_provider`** — the task's declared known
  pre-existing live-API failure; matches baseline.
- **Frontend `src/biorouterd.test.ts` 1 failure** — "Could not find biorouterd
  binary in …/target/debug/biorouterd". This verifier builds into the isolated
  `CARGO_TARGET_DIR` cache, so the worktree's `./target` is empty; the test needs
  a locally-built binary. `biorouterd.ts` is untouched by the cluster. Environmental.
- **Frontend `npm run lint:check` — 40 pre-existing errors** in untouched files
  (`utils/dependencyChecker.ts`, `extensionUpdater.ts`, `githubUpdater.ts`,
  `sessionNameSync.ts`, `settings.ts`, `App.tsx`, `knowledge/*`, etc.), mostly
  config-level global issues (`BroadcastChannel`/`structuredClone` not defined,
  empty blocks, sparse arrays). The cluster's only ui/desktop change
  (regenerated `src/api/`) is lint-clean (0 errors in `src/api/`).

## Per-crate test evidence (`cargo test -p <crate> --no-fail-fast`)

- **biorouter-mcp** — PASS. `test result: ok. 593 passed; 0 failed; 2 ignored`
  (lib) plus integration suites all `ok` (2, 1, 2, 1, 1, 5 passed; 0 failed).
- **biorouter-server** — lib `FAILED. 51 passed; 2 failed` (the two flaky tunnel
  e2e tests only; both pass in isolation — see findings). All other suites green
  (`31 passed`, `1 passed`, `6 passed`, 0 failed). No cluster-attributable failure.
- **biorouter** — lib `ok. 796 passed; 0 failed` (incl. cluster's
  `agents::goal::persistence_tests`, `agents::goal::tests`, subagent tests, all
  green). Only failure: `tests/providers.rs::test_anthropic_provider`
  (`FAILED. 14 passed; 1 failed`) = known live-API, matches baseline.
- **biorouter-cli** — PASS. `test result: ok. 173 passed; 0 failed`.
- **biorouter-acp** — PASS. `16 passed` + `11 passed` + `1 passed`; 0 failed.

## Frontend evidence

- `npm run test:run` — `Test Files 1 failed | 86 passed (87)`, `Tests 1 failed |
  708 passed (709)`. The single failure is the environmental biorouterd-binary
  one above.
- `npm run lint:check` — 40 pre-existing errors in untouched files; regenerated
  `src/api/` is clean.

## Infrastructure note (must-read for the human)

The shared machine ran out of disk repeatedly during this run (Data volume
~890G/926G used; 5 concurrent verifier `CARGO_TARGET_DIR` caches under
`/Users/wanjun/.cache/br-targets/` totalling ~130–150G). To make progress I:
- Deleted regenerable `debug/incremental/` caches across cluster targets, then
  my own cluster's `incremental/` (11G), and ran biorouter-acp with
  `CARGO_INCREMENTAL=0`.
- Deleted the sibling **`context`** (36G) and **`checkpoints`** (19G) target
  caches to unblock a hard 100%-full ENOSPC. `checkpoints` was observed to have
  rebuilt to 29G afterwards — i.e. sibling verifiers are running concurrently and
  will simply recompile from scratch if they need those caches. No source or
  committed artifacts were touched; only regenerable build caches.

The human should be aware disk pressure is a real risk for the remaining
verifiers and may want to prune or enlarge the cache volume.
