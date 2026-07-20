# Pre-run baseline gates for the Apps SDK v2 test drive

> **What this is.** The point-in-time record of the green state the 100-app Agent Drafter test drive
> started from: the worktree, branch, baseline commit, pinned provider model, and the five test gates
> that passed before any app was authored.
> **Status:** Historical record — a gate capture for baseline commit `a443f2c4` on the now-merged
> `feat/apps-sdk-v2` branch. The test counts below are frozen at that commit and have since been
> superseded by the post-remediation counts in [remediation-results.md](remediation-results.md).
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

A baseline exists so that any failure observed during the test drive can be attributed to an
authored app rather than to a platform that was already broken. Everything below was captured before
the first `create_app` call, so a gate that is green here and red later is a regression the run
introduced or exposed.

## Run identity

| Field | Value |
|---|---|
| Worktree | `biorouter-sdk-v2-wt`, a machine-local git worktree of this repository |
| Branch | `feat/apps-sdk-v2` |
| Baseline commit | `a443f2c4` |
| Required provider/model | `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI) |

## Gates that passed at baseline

| Gate | Result | Notes |
|---|---|---|
| `node scripts/agent-drafter/ui-control-harness.mjs` | PASS | All SDK v2 scenarios passed: state, bindings, patching, actions, signals, KB/model gates, themes/layout, presence, errors, profiles, token auth. |
| `cargo test -p biorouter-mcp --lib agent_drafter::` | PASS | 202 passed; 0 failed. Isolated target `/tmp/br-testdrive-target`. |
| `cargo test -p biorouter-mcp --test ui_example_apps` | PASS | 5 passed; 0 failed. |
| `cargo test -p biorouter-mcp --test agent_drafter_registered` | PASS | 2 passed; 0 failed. |
| `cargo test -p biorouter-server --lib routes::apps` | PASS | 74 passed; 0 failed. |

> **Note.** These counts are the baseline, not the current figures. After the remediation campaign
> the same crates report 719 (`biorouter-mcp`) and 101 (`biorouter-server`) passing tests; see the
> test table in [remediation-results.md](remediation-results.md).

## Sandbox isolation

The isolated BioRouter environment is rooted at `.br-testdrive/runtime` via `BIOROUTER_PATH_ROOT`.
Its app store is `.br-testdrive/runtime/config/biorouter/agent_drafter`, so it cannot mix with the
user's pre-existing global Agent Drafter applications.

> **Warning.** That isolation did not in fact hold at baseline. Finding 2 in the
> [audit findings register](audit-findings-register.md) records that `agent_drafter::default_root()`
> ignored `BIOROUTER_PATH_ROOT` and wrote the first draft into the user's global store; the harness
> was corrected mid-run to also set `XDG_CONFIG_HOME`.

## Related documentation

- [Test drive README](README.md) — the index for this campaign and the reading order.
- [Audit findings register](audit-findings-register.md) — the 22 findings the run produced against
  this baseline.
- [Remediation results](remediation-results.md) — the post-fix test counts that supersede the table
  above.
- [App test-drive runbook](../../agent-drafter/testing/app-test-drive-runbook.md) — the procedure
  that required this baseline capture.
