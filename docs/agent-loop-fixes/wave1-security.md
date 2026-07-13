# Wave 1 — Security & guardrails cluster — verification report

**Verifier:** VERIFY-security · **Worktree:** `.worktrees/security` ·
**Branch base:** `agent-loop-integration` (`db202388`) · **Gate:** GREEN

## BR proposals — status / commit / files / tests

| BR | Title | Status | Commit | Key files | Tests |
|----|-------|--------|--------|-----------|-------|
| BR-22 | Scan tool output on the main loop for injection + PII | done | `ba9b8596` | `biorouter/src/guardrails/tool_output.rs` (new), `guardrails/mod.rs`, `agents/agent.rs`, `agents/large_response_handler.rs` | in-module unit tests within `biorouter` lib suite (829 passed) |
| BR-23 | Central secret-redaction boundary across all extensions | done | `fc4e5ae6` | `biorouter-mcp/src/secret_guard.rs` (new), `biorouter-mcp/src/lib.rs`, `developer/rmcp_developer.rs`, `biorouter/src/agents/extension_manager.rs` | in-module unit tests within `biorouter-mcp` lib suite (594 passed) |
| BR-21 | Auditable command policy engine (Slice 1) atop the BR-20 floor | done | `afa11aa8` | `biorouter/src/security/policy/{mod,command,rule,baseline}.rs` + `baseline.policy.yaml` + `tests.rs` (new), `security/mod.rs`, `security/security_inspector.rs`, `Cargo.toml` | `security/policy/tests.rs` within `biorouter` lib suite (829 passed) |
| BR-65 | Managed/enterprise policy tier (first mergeable slice) | done | `3862995a` | `biorouter/src/managed/{mod,settings,trust}.rs` (new), `permission/managed_inspector.rs` (new), `permission/{mod,permission_inspector}.rs`, `hooks/mod.rs`, `config/paths.rs`, `agents/agent.rs`, `lib.rs`, `docs/guides/managed-policy.md` | `tests/managed_policy_tests.rs` — 4 passed |
| BR-64 | Design doc: OS-level tool-execution sandbox | done | `1459b100` | `docs/agent-loop-fixes/designs/BR-64-design.md` | n/a (docs) |
| BR-64 | macOS Seatbelt sandbox for the developer shell tool (Slice 1) | done | `b1407965` | `biorouter-sandbox/src/seatbelt.rs` (new), `biorouter-sandbox/src/lib.rs`, `biorouter-mcp/src/developer/shell.rs` | seatbelt unit tests within `biorouter-mcp` lib suite (594 passed) |

Every proposal is its own coherent commit with a well-formed `BR-NN:` message.
`git status --porcelain` was clean at start — no orphaned work, no junk to
investigate. The `biorouter-sandbox` leaf crate (docker.rs / local.rs / tests)
pre-existed in the base (`db202388`); BR-64 Slice 1 only added `seatbelt.rs`
and one `lib.rs` line, so nothing needed reconstructing.

## Design-decision records (choices observed in the merged slices)

- **BR-64 two-axis containment.** OS sandbox (what's technically possible) is
  kept deliberately separate from the approval policy (when to ask). Slice 1
  ships only the macOS Seatbelt (`sandbox-exec -p`) path wired into the
  developer shell tool; Linux (Landlock/seccomp/bubblewrap) is left for a later
  slice. Design explicitly frames BR-64 as complementary to BR-20/BR-21/BR-65,
  not a replacement.
- **BR-21 layers on the BR-20 floor.** The auditable allow/ask/deny command
  policy engine is additive to the always-on catastrophic denylist; it ships a
  declarative `baseline.policy.yaml` rule catalog rather than hard-coding rules.
- **BR-65 managed tier defaults.** Enterprise/managed settings + trust live
  under a new `managed/` module and plug in via a dedicated
  `managed_inspector` in the permission chain and via `hooks/mod.rs`, keeping
  the managed-policy read path isolated from per-session permission logic.
- **BR-23 single redaction boundary.** Secret redaction is centralized in
  `biorouter-mcp/src/secret_guard.rs` and applied at the extension-manager
  boundary so all extensions inherit it, rather than each extension redacting
  independently.

## Verification steps

1. **Commits/status** — 6 commits `agent-loop-integration..HEAD`, all BR-tagged;
   working tree clean. No fmt/junk commits needed.
2. **`cargo fmt --all -- --check`** — clean (exit 0). No `style: cargo fmt`
   commit required.
3. **`./scripts/clippy-lint.sh`** — the only baseline reds are pre-existing
   `clippy::too_many_lines` warnings in files this cluster never touched:
   `agent_drafter/render.rs::serve_mjs` and `control.rs::validate_widget` (the
   two explicitly whitelisted as pre-existing), plus `biorouter/src/system.rs`,
   `biorouter-bench` and several `biorouter-cli` functions. **Zero
   cluster-introduced lints** — none of the flagged functions are in the diff.
4. **OpenAPI** — no `biorouter-server` routes touched (diff is `crates/biorouter`,
   `crates/biorouter-mcp`, `crates/biorouter-sandbox`, docs, `Cargo.lock`), so
   `just generate-openapi` was not required.
5. **Per-crate regression** — see evidence below. Zero new failures.
6. **ui/desktop** — not touched; npm test/lint skipped.

## Per-crate test-result evidence

Command: `CARGO_TARGET_DIR=/Users/wanjun/.cache/br-targets/security cargo test -p <crate> --no-fail-fast`

- **biorouter** — exit as expected; all suites green **except** the known
  pre-existing live-API failure:
  - `unittests src/lib.rs`: `829 passed; 0 failed`
  - `tests/agent.rs`: `22 passed; 0 failed`
  - `tests/managed_policy_tests.rs`: `4 passed; 0 failed` (BR-65)
  - `tests/mcp_integration_test.rs`: `4 passed; 0 failed`
  - `tests/providers.rs`: `FAILED. 14 passed; 1 failed` — **`test_anthropic_provider`**
    (known live-API failure, present in baseline `workspace-test.log`)
  - all other suites `0 failed`
- **biorouter-mcp** — exit 0; `unittests src/lib.rs`: `594 passed; 0 failed; 2 ignored`; all other suites `0 failed`.
- **biorouter-server** — exit 0; `main.rs`: `49 passed`; `knowledge_routes`: `31 passed`; `knowledge_routes_e2e`: `1 passed`; `llamacpp_routes`: `6 passed`; `0 failed` throughout.
- **biorouter-cli** — exit 0; `unittests src/lib.rs`: `173 passed; 0 failed`.
- **biorouter-acp** — exit 0; `lib`: `16 passed`; `server_test`: `11 passed`; `ws_transport_test`: `1 passed`; `0 failed`. (Not touched by this cluster; run for completeness.)

Baseline (`/Users/wanjun/.cache/br-baseline/workspace-test.log`) has exactly one
failing test — `test_anthropic_provider`. Our run reproduces that one and
introduces **no new failures** ⇒ gate GREEN.

## Regression findings

- **No code regressions.** The only red test is the pre-existing live-API
  `test_anthropic_provider`, matching baseline.
- **Environmental (not a cluster defect): disk exhaustion.** The shared APFS
  data volume hit 100% during verification (multiple cluster target dirs:
  processes 50G, security 35G, checkpoints 31G, compaction 16G, integration
  14G). This first blocked clippy and then caused a spurious `biorouter-acp`
  link failure (`ld: write() failed, errno=28 (No space left on device)` on
  `ws_transport_test`). Reclaimed regenerable space (removed default
  `target/` dirs and the security target's 12G `debug/incremental`) and
  re-ran — acp then linked and passed clean. This is a coordination/disk-space
  issue, not a code fault. **Human note:** the shared build volume is
  chronically near-full while five clusters build in parallel; prune stale
  `~/.cache/br-targets/*` between waves.

## Verdict

**GREEN.** All six BR proposals are individually committed, formatted, and
lint-clean for cluster-introduced code. Every touched crate's test suite passes
with zero new failures versus baseline (the sole red is the known live-API
`test_anthropic_provider`). No server routes or desktop UI were touched, so
OpenAPI regen and npm checks were not applicable.
