# Wave 1 — security and guardrails cluster verification report

> **What this is.** Gate evidence for the Wave 1 security and guardrails cluster — BR-21 policy
> engine slice 1, BR-22 tool-output guardrails, BR-23 central secret redaction, BR-64 macOS
> Seatbelt slice 1 and BR-65 managed policy tier — with the design decisions observed in each
> slice.
> **Status:** Historical record — this cluster cleared the gate and merged into the campaign's
> `agent-loop-integration` branch at Wave 1. `security/policy/`, `managed/`,
> `guardrails/tool_output.rs`, `biorouter-mcp/src/secret_guard.rs` and
> `biorouter-sandbox/src/seatbelt.rs` all exist in the tree today. The verification run itself
> is undated in the original record.
> **Audience:** maintainers auditing what the campaign shipped and on what evidence.

The agent-loop fix campaign implemented 67 numbered proposals (`BR-1` … `BR-67`) from
[the master improvement-proposals list](../../agent-loop-review/improvement-proposals.md).
Related proposals were grouped into **clusters**, each built in its own git worktree by a
dedicated verifier agent, and clusters shipped in dependency-ordered **waves**. Every wave had
to clear a **gate**: a full per-crate test run admitting zero new failures against a recorded
baseline. This file is the security cluster's gate evidence, produced by the campaign's
`VERIFY-security` verifier agent. Campaign conventions and the wave table are in
[the campaign overview](../README.md).

> **Warning.** This cluster is permission gating, command policy and sandboxing — the most
> security-sensitive code the campaign touched. A green gate is **not** sufficient sign-off.
> Per `HOWTOAI.md`, and as [the campaign outcome report](../outcome-report.md) records, BR-20,
> BR-21, BR-64 and BR-65 all warrant human review regardless of the passing suite.

> **Note.** Paths beginning `~/` or `.worktrees/` are on the verifier's machine as it was
> configured during the campaign; they are not repository paths.

Worktree `.worktrees/security`, branch base `agent-loop-integration` (`db202388`).
**Gate: GREEN.**

## Proposals shipped

BR-64 appears twice because its design document and its first code slice landed as separate
commits.

| BR | Title | Status | Commit | Key files | Tests |
|----|-------|--------|--------|-----------|-------|
| BR-22 | Scan tool output on the main loop for injection + PII | done | `ba9b8596` | `biorouter/src/guardrails/tool_output.rs` (new), `guardrails/mod.rs`, `agents/agent.rs`, `agents/large_response_handler.rs` | in-module unit tests within `biorouter` lib suite (829 passed) |
| BR-23 | Central secret-redaction boundary across all extensions | done | `fc4e5ae6` | `biorouter-mcp/src/secret_guard.rs` (new), `biorouter-mcp/src/lib.rs`, `developer/rmcp_developer.rs`, `biorouter/src/agents/extension_manager.rs` | in-module unit tests within `biorouter-mcp` lib suite (594 passed) |
| BR-21 | Auditable command policy engine (Slice 1) atop the BR-20 floor | done | `afa11aa8` | `biorouter/src/security/policy/{mod,command,rule,baseline}.rs` + `baseline.policy.yaml` + `tests.rs` (new), `security/mod.rs`, `security/security_inspector.rs`, `Cargo.toml` | `security/policy/tests.rs` within `biorouter` lib suite (829 passed) |
| BR-65 | Managed/enterprise policy tier (first mergeable slice) | done | `3862995a` | `biorouter/src/managed/{mod,settings,trust}.rs` (new), `permission/managed_inspector.rs` (new), `permission/{mod,permission_inspector}.rs`, `hooks/mod.rs`, `config/paths.rs`, `agents/agent.rs`, `lib.rs`, managed-policy guide | `tests/managed_policy_tests.rs` — 4 passed |
| BR-64 (design) | Design doc: OS-level tool-execution sandbox | done | `1459b100` | [macOS Seatbelt sandbox design](../../../agent-loop/designs/macos-seatbelt-sandbox.md) | n/a (docs) |
| BR-64 (Slice 1) | macOS Seatbelt sandbox for the developer shell tool | done | `b1407965` | `biorouter-sandbox/src/seatbelt.rs` (new), `biorouter-sandbox/src/lib.rs`, `biorouter-mcp/src/developer/shell.rs` | seatbelt unit tests within `biorouter-mcp` lib suite (594 passed) |

Two of these were designed before implementation:
[the command policy engine](../../../agent-loop/designs/command-policy-engine.md) for BR-21 and
[the managed policy tier](../../../agent-loop/designs/managed-policy-tier.md) for BR-65. The
user-facing guide BR-65 shipped is now
[the managed policy guide](../../../security/managed-policy.md).

Every proposal is its own coherent commit with a well-formed `BR-NN:` message.
`git status --porcelain` was clean at start — no orphaned work, no junk to investigate. The
`biorouter-sandbox` leaf crate (`docker.rs`, `local.rs`, tests) pre-existed in the base
(`db202388`); BR-64 Slice 1 only added `seatbelt.rs` and one `lib.rs` line, so nothing needed
reconstructing.

## Design decisions observed in the merged slices

- **BR-64 two-axis containment.** The OS sandbox — what is technically possible — is kept
  deliberately separate from the approval policy — when to ask. Slice 1 ships only the macOS
  Seatbelt (`sandbox-exec -p`) path wired into the developer shell tool; Linux
  (Landlock/seccomp/bubblewrap) is left for a later slice. The design explicitly frames BR-64
  as complementary to BR-20, BR-21 and BR-65, not a replacement.
- **BR-21 layers on the BR-20 floor.** The auditable allow/ask/deny command policy engine is
  additive to the always-on catastrophic denylist; it ships a declarative
  `baseline.policy.yaml` rule catalog rather than hard-coding rules.
- **BR-65 managed tier defaults.** Enterprise and managed settings plus trust live under a new
  `managed/` module and plug in via a dedicated `managed_inspector` in the permission chain and
  via `hooks/mod.rs`, keeping the managed-policy read path isolated from per-session permission
  logic.
- **BR-23 single redaction boundary.** Secret redaction is centralized in
  `biorouter-mcp/src/secret_guard.rs` and applied at the extension-manager boundary so all
  extensions inherit it, rather than each extension redacting independently.

## Verification steps

1. **Commits and status** — 6 commits in `agent-loop-integration..HEAD`, all BR-tagged; working
   tree clean. No fmt or junk commits needed.
2. **`cargo fmt --all -- --check`** — clean (exit 0). No `style: cargo fmt` commit required.
3. **`./scripts/clippy-lint.sh`** — the only baseline reds are pre-existing
   `clippy::too_many_lines` warnings in files this cluster never touched:
   `agent_drafter/render.rs::serve_mjs` and `control.rs::validate_widget` (the two explicitly
   whitelisted as pre-existing), plus `biorouter/src/system.rs`, `biorouter-bench` and several
   `biorouter-cli` functions. **Zero cluster-introduced lints** — none of the flagged functions
   are in the diff. Four sibling wave reports record the same stale allowlist independently;
   see [Wave 0 — foundation](wave-0-foundation.md),
   [Wave 1 — checkpoints](wave-1-checkpoints.md), [Wave 1 — compaction](wave-1-compaction.md)
   and [Wave 1 — processes](wave-1-processes.md).
4. **OpenAPI** — no `biorouter-server` routes touched. The diff is `crates/biorouter`,
   `crates/biorouter-mcp`, `crates/biorouter-sandbox`, docs and `Cargo.lock`, so
   `just generate-openapi` was not required.
5. **Per-crate regression** — see the evidence below. Zero new failures.
6. **`ui/desktop`** — not touched; npm test and lint skipped.

## Per-crate test evidence

Command:

```bash
CARGO_TARGET_DIR=~/.cache/br-targets/security cargo test -p <crate> --no-fail-fast
```

| Crate | Exit | Result |
|-------|------|--------|
| biorouter | as expected | `unittests src/lib.rs`: `829 passed; 0 failed`. `tests/agent.rs`: `22 passed; 0 failed`. `tests/managed_policy_tests.rs`: `4 passed; 0 failed` (BR-65). `tests/mcp_integration_test.rs`: `4 passed; 0 failed`. `tests/providers.rs`: `FAILED. 14 passed; 1 failed` — **`test_anthropic_provider`**, the known live-API failure present in baseline `workspace-test.log`. All other suites `0 failed`. |
| biorouter-mcp | 0 | `unittests src/lib.rs`: `594 passed; 0 failed; 2 ignored`; all other suites `0 failed`. |
| biorouter-server | 0 | `main.rs`: `49 passed`; `knowledge_routes`: `31 passed`; `knowledge_routes_e2e`: `1 passed`; `llamacpp_routes`: `6 passed`; `0 failed` throughout. |
| biorouter-cli | 0 | `unittests src/lib.rs`: `173 passed; 0 failed`. |
| biorouter-acp | 0 | `lib`: `16 passed`; `server_test`: `11 passed`; `ws_transport_test`: `1 passed`; `0 failed`. Not touched by this cluster; run for completeness. |

The baseline (`~/.cache/br-baseline/workspace-test.log`) has exactly one failing test,
`test_anthropic_provider`. This run reproduces that one and introduces **no new failures**, so
the gate is GREEN.

## Regression findings

**No code regressions.** The only red test is the pre-existing live-API
`test_anthropic_provider`, matching baseline.

## Environment: disk exhaustion, not a cluster defect

The shared APFS data volume hit 100% during verification, with multiple cluster target
directories in play: processes 50 G, security 35 G, checkpoints 31 G, compaction 16 G,
integration 14 G. This first blocked clippy and then caused a spurious `biorouter-acp` link
failure (`ld: write() failed, errno=28 (No space left on device)` on `ws_transport_test`).
Regenerable space was reclaimed — the default `target/` directories and the security target's
12 G `debug/incremental` — and on re-run `biorouter-acp` linked and passed clean. This is a
coordination and disk-space issue, not a code fault.

> **Note.** The shared build volume was chronically near-full while five clusters built in
> parallel. Prune stale `~/.cache/br-targets/*` between waves.

Four sibling reports record the same campaign-wide disk pressure from their own runs, with
differing mitigations; see [Wave 1 — processes](wave-1-processes.md),
[Wave 1 — checkpoints](wave-1-checkpoints.md), [Wave 1 — compaction](wave-1-compaction.md) and
[Wave 2 — loop detection](wave-2-loop-detection.md).

## Verdict

**GREEN.** All six BR proposals are individually committed, formatted and lint-clean for
cluster-introduced code. Every touched crate's test suite passes with zero new failures versus
baseline; the sole red is the known live-API `test_anthropic_provider`. No server routes or
desktop UI were touched, so OpenAPI regeneration and npm checks were not applicable.

## Related documentation

- [Agent-loop fix campaign overview](../README.md) — the wave table, cluster conventions and
  merge status this report is evidence for.
- [Master improvement proposals](../../agent-loop-review/improvement-proposals.md) — the
  definition of BR-21, BR-22, BR-23, BR-64 and BR-65.
- [macOS Seatbelt sandbox design](../../../agent-loop/designs/macos-seatbelt-sandbox.md) — the
  design BR-64 Slice 1 implements.
- [Command policy engine design](../../../agent-loop/designs/command-policy-engine.md) — the
  design BR-21 Slice 1 implements.
- [Managed policy guide](../../../security/managed-policy.md) — the user-facing documentation
  BR-65 shipped.
- [Wave 2 — hooks and permissions cluster](wave-2-hooks-and-permissions.md) — the sibling
  security-sensitive cluster, covering permission scoping and auto-approve.
