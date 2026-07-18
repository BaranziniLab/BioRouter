# Wave 2 — hooks and permissions cluster verification report

> **What this is.** Gate evidence for the Wave 2 hooks and permissions cluster — BR-27 content
> matchers, BR-28 hook aggregates, BR-19 hooks on the tool path, BR-18 SmartApprove revival,
> BR-24 scoped permissions and BR-63 reasoning effort — plus the fail-closed reasoning behind
> the two security-critical diffs.
> **Status:** Historical record — re-verified 2026-07-13, cleared the gate, and merged into the
> campaign's `agent-loop-integration` branch at Wave 2. `permission/tool_risk.rs`,
> `permission/permission_scope.rs` and `agents/effort.rs` exist in the tree today. One finding
> here may still be live: the `scripts/clippy-lint.sh` jq-parser bug, which three sibling
> reports also record.
> **Audience:** maintainers auditing what the campaign shipped and on what evidence.

The agent-loop fix campaign implemented 67 numbered proposals (`BR-1` … `BR-67`) from
[the master improvement-proposals list](../../agent-loop-review/improvement-proposals.md).
Related proposals were grouped into **clusters**, each built in its own git worktree, and
clusters shipped in dependency-ordered **waves**. Every wave had to clear a **gate**: a full
per-crate test run admitting zero new failures against a recorded baseline. This file is the
hooks cluster's gate evidence, for the `hooks` worktree (branch base `agent-loop-integration`,
merge base `be342632`). Campaign conventions and the wave table are in
[the campaign overview](../README.md).

**Verdict: GREEN — ready to integrate.**

> **Note.** Re-verified 2026-07-13 after BR-24 (`309bafc9`) and BR-63 (2) (`30f3b1e1`) landed on
> top of the first green run. All figures below are from the re-run; the earlier numbers
> (2078 passed / 47 suites) are superseded.

> **Warning.** BR-18 and BR-24 are permission gating. A green gate is **not** sufficient
> sign-off — see the security-critical decisions below.

## Proposals shipped

One commit per proposal, in dependency order. Working tree clean; no orphaned work. BR-63
landed in two parts, written here as **BR-63 (1)** and **BR-63 (2)**. Bold rows are the two
commits that landed after the first green run and prompted the 2026-07-13 re-verification.

| BR | Commit | What landed | Key files | Tests |
|----|--------|-------------|-----------|-------|
| BR-27 | `70b0b73b` | Hook matchers can match on `tool_input` **content**, not just tool name; compiled regexes are cached instead of recompiled per event. | `hooks/config.rs`, `hooks/matcher.rs` | `hooks_integration_tests.rs` 9/9 |
| BR-28 | `00d90ca9` | `fire()` now **returns aggregates** from hook events instead of discarding them, so callers (agent loop, subagent handler, CLI session, scheduler) can act on what the hooks decided. | `hooks/outcome.rs`, `hooks/mod.rs` | `hooks_agent_loop_tests.rs` 10/10 |
| BR-19 | `01834165` | Hooks reach the **tool path**: `PreToolUse` can rewrite tool input, `PostToolUse` can block a result, and hook context flows through `tool_execution` / `tool_inspection`. | `agents/tool_execution.rs`, `tool_inspection.rs`, `hooks/inspector.rs` | `hooks_agent_loop_tests.rs` |
| BR-18 | `01c49a7f` | Revives read-only auto-approve. New `permission/tool_risk.rs` grades each call `Low/Medium/High/Unknown` from MCP `ToolAnnotations`, so **`SmartApprove` is no longer a second name for `Approve`**. | `permission/tool_risk.rs`, `permission/permission_inspector.rs` | `smart_approve_tests.rs` 10/10 |
| BR-63 (1) | `a445269d` | Richer tool-confirmation card: risk grade + rendered call preview. | `conversation/tool_preview.rs`, `ToolCallConfirmation.tsx`, `ToolCallPreview.tsx` | `ToolCallConfirmation.test.tsx` 8/8 |
| **BR-24** | **`309bafc9`** | **Per-directory / per-command-prefix permission scoping.** Adds the two intermediate grades between "this exact call" and "this whole tool": `PermissionScope::Directory` and `PermissionScope::CommandPrefix`. | `permission/permission_scope.rs` (new, 790 L), `permission_store.rs`, `permission_inspector.rs` | `scoped_permission_tests.rs` 6/6 |
| **BR-63 (2)** | **`30f3b1e1`** | **Per-turn reasoning-effort control** (`Quick` / `Normal` / `Deep`). One knob moves provider effort + thinking budget + exploration caps together. | `agents/effort.rs` (new, 313 L), `agents/agent.rs`, `providers/formats/{anthropic,openai,openai_responses,databricks}.rs`, `routes/reply.rs`, `store/reasoningEffort.ts`, `BottomMenuReasoningEffort.tsx` | `tests/agent.rs` 12/12, `reasoningEffort.test.ts` 5/5, `BottomMenuReasoningEffort.test.tsx` |

## Security-critical decisions, and what needs human review

- **BR-18 is the substantive behavioural change.** Previously `PermissionInspector` held two
  `HashSet`s (`readonly_tools` / `regular_tools`) that were constructed empty with a "will be
  populated from extension manager" comment and had no setter — so the read-only short-circuit
  could *never* fire and both gating modes prompted on every call, including a plain file read.
  The replacement grades tools from the `ToolAnnotations` extensions already publish. It is
  **fail-closed** in every ambiguous direction: a tool claiming both `read_only_hint` and
  `destructive_hint` → `High`; a writer that never disclaimed destructiveness → `High`; no or
  unseen annotations → `Unknown` → confirmed by default. `ToolRisk`'s `Ord` derive is
  load-bearing (`Low < Medium < High < Unknown`), so a naive `>= threshold` comparison already
  fails closed.
- **BR-24 is the other security-critical one, and the highest-risk diff in the cluster.** It
  widens what a *remembered* approval can cover. The previous grades were "byte-identical
  repeat call" (blake3 of tool name plus exact JSON args) or "whitelist the whole tool name" —
  so `AlwaysAllow` on `shell` blessed every future shell command including `rm -rf`. BR-24 adds
  Directory and CommandPrefix grades in between. The matching deliberately fails **closed**:
  - Directory containment is **component-wise on canonicalized paths**, so a grant for
    `/work/a` does not authorize `/work/ab` (a string prefix would), nor
    `/work/a/../../etc/passwd`, nor a symlink inside `/work/a` pointing out of it.
  - CommandPrefix matches an exact sequence of shell **tokens**, not a substring.
  - Anything the matcher cannot fully parse simply does not match, so the user is prompted.

  An over-broad remembered grant is worse than no feature — it is a silent, persistent grant the
  user believes is narrow — so this module warrants the closest human read.
- This is **security-sensitive code** (permission gating). Per `CLAUDE.md` and `HOWTOAI.md` it
  warrants human review regardless of the green gate — BR-18 and BR-24 especially.
  [The campaign outcome report](../outcome-report.md) carries the same flag for the campaign as
  a whole.

## Other decisions for the integrator

- **BR-63 (2)'s `Normal` is a strict no-op** — it touches neither model config nor the
  exploration caps, so any session that never sets an effort behaves exactly as before. The new
  behaviour is opt-in; that is why the default is safe to merge. `Quick` halves the exploration
  caps and disables thinking; `Deep` doubles the caps, sets provider effort `high` and a 16k
  thinking budget. Providers take what they understand and ignore the rest, so an unsupported
  provider still gets the provider-agnostic cap changes.
- **`ToolRisk` and `ReasoningEffort` are wire types.** `ToolRisk` serialises
  `"low"|"medium"|"high"|"unknown"` and BR-63 (1) ships it to the confirmation card;
  `ReasoningEffort` is on the OpenAPI surface via `routes/reply.rs`. Changing either set of
  variants is an API break.
- **OpenAPI is in sync.** BR-63 (2) changed `routes/reply.rs` and `openapi.rs`; re-running the
  generator reproduced the committed `ui/desktop/openapi.json` byte-for-byte
  (`git status --porcelain` stayed empty).

## No regressions: environmental and tooling notes

**No cluster-introduced regressions. No fixup commits were needed** — neither in the first pass
nor in this re-verification. What follows is environmental and tooling context, not a list of
regressions.

1. **`scripts/clippy-lint.sh` exit 0 is NOT trustworthy on its own.** The baseline checker's jq
   parser does `.message.spans[0].text[0].text | split("fn ")[1]`. One pre-existing violation
   in `biorouter-bench` (`eval_suites/core/developer/simple_repo_clone_test.rs:22`) has an
   `#[async_trait]` span with **no `"fn "` in it**, so jq dies with
   `split input and separator must be strings` — and because it dies mid-stream, every violation
   *after* that record is silently dropped from the comparison. The script then prints
   `✅ too_many_lines: ok` having compared a truncated list. Observed verbatim in this run.

   **Mitigation used here:** the `too_many_lines` set was re-derived by hand with a robust
   parser, in the script's exact scope (`cargo clippy` *without* `--all-targets`), and diffed
   against `clippy-baselines/too_many_lines.txt`. Result: the 13 baseline entries match
   **exactly**; the only extra record is the `#[async_trait]` one that triggers the bug, absent
   from the baseline because baseline *generation* choked on the same record.
   `biorouter-bench` is untouched by this cluster. **No new `too_many_lines` violations.**
   Fixing that jq parser is a good standalone chore.

   > **Note.** Three sibling reports write up the same tooling defect independently, each from
   > its own run: [Wave 2 — loop detection](wave-2-loop-detection.md),
   > [Wave 3 — polish](wave-3-polish.md) and
   > [the cross-platform parity verification report](../../../agent-loop/cross-platform/parity-verification-report.md).
   > It was never fixed inside the campaign.

2. **The strict gate did pass on its own merits**: `cargo clippy --all-targets -- -D warnings`
   is a separate step in the same script and returned clean.
3. **Disk headroom** — earlier waves hit ENOSPC from the shared `~/.cache/br-targets/`
   directories and it *looked* like a lint failure. 15 GB free at the start of this run;
   `CARGO_INCREMENTAL=0` throughout.
4. `clippy-baselines/too_many_lines.txt` needed **no edit** — no cluster function grew past the
   limit.

## Evidence

### Style and lint

- `cargo fmt --all -- --check` → **clean** (exit 0).
- `./scripts/clippy-lint.sh` → exit 0; strict `-D warnings` clean; `too_many_lines`
  hand-verified against the 13-entry baseline (see tooling note 1).
- `cargo run -p biorouter-server --bin generate_schema` → **no drift**;
  `git status --porcelain ui/desktop/openapi.json` empty.

### Rust, per crate

Command: `cargo test -p <crate> --no-fail-fast`. The `Exit` column is the process exit code;
`cargo test` exits 101 when any test in the run fails.

| Crate | Exit | Result |
|-------|------|--------|
| `biorouter` | 101 | Only red is `test_anthropic_provider` (`tests/providers.rs:251`, "Expected error when context window is exceeded") — **known-allowed live-Anthropic-API failure**, red in the GATE-1 baseline too. |
| `biorouter-mcp` | 0 | all suites ok |
| `biorouter-server` | 0 | all suites ok (incl. `tunnel::lapstone_test`, which passed this run) |
| `biorouter-cli` | 0 | all suites ok |
| `biorouter-acp` | 0 | all suites ok |

New and most-relevant suites, all green:

```text
tests/scoped_permission_tests.rs   test result: ok. 6 passed;  0 failed   (BR-24)
tests/agent.rs                     test result: ok. 12 passed; 0 failed   (BR-63 part 2)
tests/smart_approve_tests.rs       test result: ok. 10 passed; 0 failed   (BR-18)
tests/hooks_agent_loop_tests.rs    test result: ok. 10 passed; 0 failed   (BR-28/BR-19)
tests/hooks_integration_tests.rs   test result: ok. 9 passed;  0 failed   (BR-27)
tests/managed_policy_tests.rs      test result: ok. 4 passed;  0 failed
tests/providers.rs                 test result: FAILED. 14 passed; 1 failed  (known live-API red)
```

**Totals across the five crates: 49 suites, 2135 passed, 1 failed.**

Versus **GATE-1** (`~/.cache/br-baseline/gate1-summary.txt`): 56 suites, **2024 passed,
1 failed** — the *same* `test_anthropic_provider` line appears verbatim in the baseline file.
The **+111** passing tests are the cluster's own new coverage. The baseline's higher suite count
is because GATE-1 ran the whole workspace (including `biorouter-bench` and `biorouter-test`)
while this run scoped to the five crates named in the gate. **Zero new failures.**

### Frontend (`ui/desktop`)

- `npm run test:run` → **1 failed | 724 passed (725), 90 files**. The single failure is
  `src/biorouterd.test.ts` ("Could not find biorouterd binary…"), a known pre-existing red —
  the worktree has no compiled `target/debug/biorouterd`. Not a code defect.
  (`ExtensionModal.test.tsx`, red in the earlier run, now passes.)
- New BR-63 suites: `store/reasoningEffort.test.ts` **5/5**,
  `bottom_menu/BottomMenuReasoningEffort.test.tsx` pass.
- `npm run lint:check` → **40 errors / 9 warnings**, matching the pre-existing baseline. One
  flagged file, `ChatInput.tsx`, *is* touched by BR-63 (2) — but its 2 errors are at lines
  **83/88** (`'HTMLImageElement' is not defined`, `'Image' is not defined`, `no-undef`) and are
  pre-existing; BR-63's diff to that file is only an added import and one
  `<BottomMenuReasoningEffort />` JSX element. **No new lint errors.**

## Related documentation

- [Agent-loop fix campaign overview](../README.md) — the wave table, cluster conventions and
  merge status this report is evidence for.
- [Master improvement proposals](../../agent-loop-review/improvement-proposals.md) — the
  definition of BR-18, BR-19, BR-24, BR-27, BR-28 and BR-63.
- [Permission modes guide](../../../security/permission-modes.md) — the current user-facing
  documentation for the gating BR-18 and BR-24 changed.
- [Hooks reference](../../../agent-loop/hooks/hooks-reference.md) — the current documentation
  for the hook system BR-19, BR-27 and BR-28 extended.
- [Wave 1 — security and guardrails cluster](wave-1-security.md) — the other security-sensitive
  cluster, covering command policy and sandboxing.
- [Wave 2 — loop detection cluster](wave-2-loop-detection.md) — the sibling Wave 2 gate that
  independently diagnosed the same `clippy-lint.sh` jq-parser bug.
