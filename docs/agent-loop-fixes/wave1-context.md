# Wave 1 — "Context & prompts" cluster verification report

Worktree: `/Users/wanjun/Desktop/BioRouter/.worktrees/context`
Branch: `agent-loop-context` (base: `agent-loop-integration`)
Verifier gate: **GREEN**

## Proposal → status / commit / files / tests

| BR | Title | Status | Commit | Key files | Test evidence |
|----|-------|--------|--------|-----------|---------------|
| BR-1 | gitignore-aware cached workspace file map in MOIM | done | `86d2acd7` | `agents/workspace_summary.rs` (new), `agents/extension_manager.rs`, `agents/mod.rs` | biorouter lib 829 passed |
| BR-2 | total context budget with ranking/truncation for injected blocks | done | `12f02dcc` | `context_budget.rs` (new), `agents/moim.rs`, `agents/prompt_manager.rs`, `hints/load_hints.rs`, `lib.rs` | biorouter lib 829 passed |
| BR-3 | per-model system-prompt variants (strong default + small-local overlay) | done | `0717bb5b` | `agents/prompt_manager.rs`, `agents/reply_parts.rs`, `prompt_template.rs`, `prompts/system_small_local.md` (new) | biorouter lib 829 passed (incl. overlay/variant tests) |
| BR-5 | dedup MOIM and refresh the system-prompt clock | done | `2e6c7a9d` | `agents/moim.rs`, `agents/prompt_manager.rs`, `agents/extension_manager.rs` | biorouter lib 829 passed |
| BR-8 | cap and cache eager skill-body inlining | done | `8d946378` | `agents/agent.rs`, `context_budget.rs` | biorouter lib 829 passed |
| BR-9 | frame project hints/AGENTS.md as lower-trust untrusted context | done | `1e740bc4` | `hints/load_hints.rs` | biorouter lib 829 passed |
| BR-60 | structured per-item todo list + living plan artifact | done | `bfaea95e` (+ fix `6e101107`) | `agents/todo_extension.rs`, `session/extension_data.rs`, `session/mod.rs`, `prompts/system.md` | biorouter lib 829 passed after snapshot fix |

Every proposal is its own commit. Working tree clean; no orphaned or junk changes.
(`docs/agent-loop-fixes/CAMPAIGN.md` shows as differing vs the base tip only
because the `agent-loop-integration` branch advanced after the branch point — no
cluster commit touched it, and the worktree is clean.)

## Design-decision records (open-question choices made)

- **BR-60 changed the system-prompt todo wording.** BR-60 rewrote the "Maintain a
  todo list when tools for one are available" clause in `prompts/system.md` into a
  three-line "living plan + per-item checklist (in progress → completed), confirm
  every item before yielding" instruction. This is an intended behavioural change
  aligned with the structured todo extension. Consequence: the three
  `prompt_manager` insta snapshots (`basic`, `typical_setup`, `one_extension`) had
  to be regenerated. Accepted the new expected output (the prompt text is the
  intended contract), committed as `6e101107`.
- **BR-3 small-local overlay** ships as an additive overlay (`system_small_local.md`)
  on top of the strong default, gated so an explicit system-prompt override skips
  the overlay (covered by `test_small_local_variant_skips_overlay_under_override`).
- **BR-9** frames project hints / AGENTS.md as lower-trust untrusted context rather
  than dropping them — preserves usefulness while reducing injection blast radius.

## Regression findings & fix

- **Introduced regression (fixed):** BR-60 edited `prompts/system.md` but left the
  three `prompt_manager` insta snapshots stale → `--lib` target failed with 3
  snapshot mismatches (`test_basic`, `test_typical_setup`, `test_one_extension`).
  Fix: regenerated the snapshots via `INSTA_UPDATE=always` (cargo-insta is not
  installed in this environment); diff is exactly the intended 2-line→3-line prompt
  text change, nothing else. Committed `6e101107 "BR-60: fix regression - update
  prompt_manager snapshots for new todo/plan wording"`. Re-ran `cargo test -p
  biorouter` → lib green (829 passed, 0 failed).

## Style / lint

- `cargo fmt --all -- --check`: clean (exit 0).
- `./scripts/clippy-lint.sh`: baseline "fail" is entirely pre-existing stale-allowlist
  `too_many_lines` reds in files **this cluster never touched** —
  `agent_drafter/render.rs::serve_mjs`, `agent_drafter/control.rs::validate_widget`
  (both explicitly whitelisted as pre-existing), plus `biorouter-cli
  doctor.rs::handle_doctor`, `biorouter-cli tui/mod.rs::drive_response`, and
  `biorouter/system.rs::install_info` (107/100, pre-existing on the base — not in
  this cluster's diff). The Context cluster introduced **zero** new clippy findings
  in `crates/biorouter`.

## OpenAPI / TS client / UI

- No `biorouter-server` route changes and no `ui/desktop` changes in this cluster →
  `just generate-openapi` and the npm test/lint steps are not applicable (skipped).

## Per-crate test-result evidence (`CARGO_TARGET_DIR=.../context`, `--no-fail-fast`)

- **biorouter**: lib `test result: ok. 829 passed; 0 failed; 0 ignored` (baseline
  was 755 — cluster added tests). All integration tests green. Only failure is
  `tests/providers.rs::test_anthropic_provider` (`14 passed; 1 failed`) — the
  KNOWN pre-existing live-API failure per baseline. No new failures.
- **biorouter-mcp**: `test result: ok. 584 passed; 0 failed; 2 ignored` (lib) plus
  all integration suites green.
- **biorouter-server**: `50 passed`, `49 passed`, `31 passed`, `1 passed`,
  `6 passed` — all 0 failed.
- **biorouter-cli**: `test result: ok. 173 passed; 0 failed`.
- **biorouter-acp**: `16 passed`, `11 passed`, `1 passed` — all 0 failed.

## Environment note

The shared disk hit 100% (`No space left on device`) during clippy. Reclaimed
space by removing the stale sibling build-cache dir
`/Users/wanjun/.cache/br-targets/processes` (45G, last touched hours earlier;
target dirs are pure build cache — source lives in the git worktrees, so only a
future rebuild is affected). Context cluster verification completed with ~21G free.

## Verdict

GATE GREEN. All seven proposals landed as distinct, well-formed BR commits; the one
cluster-introduced regression (stale insta snapshots from BR-60's prompt edit) was
fixed and re-proven green. Zero new test failures across all five crates versus the
baseline; the sole red is the known live-API `test_anthropic_provider`.
