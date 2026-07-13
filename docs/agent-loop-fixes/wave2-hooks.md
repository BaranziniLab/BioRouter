# Wave 2 — Hooks & permissions cluster

Verification report for the `hooks` worktree (branch base: `agent-loop-integration`,
merge base `be342632`). Verdict: **GREEN — ready to integrate.**

## Proposals shipped

One commit per proposal, in dependency order.

| BR | Commit | What landed |
|----|--------|-------------|
| BR-27 | `70b0b73b` | Hook matchers can match on `tool_input` **content**, not just tool name; compiled regexes are cached instead of recompiled per event. New `hooks/config.rs` matcher config surface. |
| BR-28 | `00d90ca9` | `fire()` now **returns aggregates** from hook events instead of discarding them, so callers (agent loop, subagent handler, CLI session, scheduler) can act on what the hooks decided. |
| BR-19 | `01834165` | Hooks reach the **tool path**: `PreToolUse` can rewrite tool input, `PostToolUse` can block a result, and hook context flows through `tool_execution` / `tool_inspection`. |
| BR-18 | `01c49a7f` | Revives read-only auto-approve. New `permission/tool_risk.rs` grades each call `Low/Medium/High/Unknown` from MCP `ToolAnnotations`, so **`SmartApprove` is no longer a second name for `Approve`**. |
| BR-63 | `a445269d` | Richer tool-confirmation card: risk grade + rendered call preview. New `conversation/tool_preview.rs`, `ToolCallPreview.tsx`, regenerated OpenAPI + TS client. |

## Key decisions / notes for the integrator

- **BR-18 is the substantive behavioural change.** Previously `PermissionInspector`
  held two `HashSet`s (`readonly_tools` / `regular_tools`) that were constructed
  empty with a "will be populated from extension manager" comment and had no
  setter — so the read-only short-circuit could *never* fire and both gating modes
  prompted on every call, including a plain file read. The replacement grades tools
  from the `ToolAnnotations` extensions already publish. It is **fail-closed** in
  every ambiguous direction: a tool claiming both `read_only_hint` and
  `destructive_hint` → `High`; a writer that never disclaimed destructiveness →
  `High`; no/unseen annotations → `Unknown` → confirmed by default. `ToolRisk`'s
  `Ord` derive is load-bearing (`Low < Medium < High < Unknown`), so a naive
  `>= threshold` comparison already fails closed.
- **`ToolRisk` is a wire type** (serialises `"low"|"medium"|"high"|"unknown"`);
  BR-63 ships it to the confirmation card, so changing the variants is an API break.
- This is **security-sensitive code** (permission gating). Per `CLAUDE.md` /
  `HOWTOAI.md`, it warrants human review regardless of the green gate.
- OpenAPI was regenerated in BR-63 and re-verified here: `just generate-openapi`
  reproduces the committed `ui/desktop/openapi.json` + `src/api/types.gen.ts`
  byte-for-byte (working tree stayed clean).

## Regression findings

**No cluster-introduced regressions.** No fixup commits were needed.

Two environmental issues surfaced and are worth knowing:

1. **The build host ran out of disk (ENOSPC), not lint.** The first
   `clippy-lint.sh` run exited 1 with errors like
   `couldn't create a temp dir: No space left on device (os error 28)` from
   `tungstenite` / `sqlx-core`. These were **not** lint findings. The three cluster
   cargo target dirs under `~/.cache/br-targets/` totalled **189 GB**
   (`hooks` 49 G, `server` 63 G, `loopdet` 77 G) against ~146 MB free. Removing
   *this cluster's own* `hooks/debug/incremental` (9 G) freed enough to proceed;
   sibling clusters' target dirs were left untouched. Re-running clippy with
   `CARGO_INCREMENTAL=0` gave a clean **exit 0**. Future waves should watch disk
   headroom before blaming a red gate on the diff.
2. **`clippy-baselines/too_many_lines.txt` needed no edit** — no cluster function
   grew past the limit.

## Evidence

### Style / lint
- `cargo fmt --all -- --check` → clean (exit 0).
- `./scripts/clippy-lint.sh` → **exit 0**, zero errors, zero ENOSPC (second run).
- `just generate-openapi` → no drift; `git status --porcelain` empty.

### Rust, per crate (`cargo test -p <crate> --no-fail-fast`)

| Crate | Result |
|-------|--------|
| `biorouter` | 47 suites overall; the only red is `test_anthropic_provider` (`tests/providers.rs:251`) — `test result: FAILED. 14 passed; 1 failed` — the **known allowed live-API failure**, red in the GATE-1 baseline too. |
| `biorouter-mcp` | all suites ok |
| `biorouter-server` | all suites ok |
| `biorouter-cli` | all suites ok |
| `biorouter-acp` | all suites ok |

Totals across the five crates: **2078 passed, 1 failed** (47 suites ok, 1 failed).

Versus **GATE-1** (`~/.cache/br-baseline/gate1-summary.txt`): 2024 passed, 1 failed
(the same `test_anthropic_provider`). The +54 passing tests are the cluster's own new
coverage (`smart_approve_tests.rs`, extended `hooks_agent_loop_tests.rs` /
`hooks_integration_tests.rs`, `ToolCallConfirmation.test.tsx`). The baseline's 55 "ok"
suites cover the whole workspace; this run scoped to the five crates named in the gate,
hence the lower suite count. **Zero new failures.**

### Frontend (`ui/desktop`)
- `npm run test:run` → **2 failed | 715 passed (717)**; failures are exactly the two
  pre-existing reds: `src/biorouterd.test.ts` and
  `src/components/settings/extensions/modal/ExtensionModal.test.tsx`. No new failures.
- New BR-63 suite `ToolCallConfirmation.test.tsx` → **8/8 pass**.
- `npm run lint:check` → 40 errors / 9 warnings, matching the pre-existing baseline;
  **none** of them touch cluster-introduced files (`ToolCallConfirmation.tsx`,
  `ToolCallPreview.tsx`, `api/types.gen.ts`, `api/index.ts`).
