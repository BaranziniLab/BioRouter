# Apps SDK v2 baseline

Worktree: `/Users/wanjun/Desktop/biorouter-sdk-v2-wt`  
Branch: `feat/apps-sdk-v2`  
Baseline commit: `a443f2c4`  
Required provider/model: `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

| Gate | Result | Notes |
|---|---|---|
| `node scripts/agent-drafter/ui-control-harness.mjs` | PASS | All SDK v2 scenarios passed: state, bindings, patching, actions, signals, KB/model gates, themes/layout, presence, errors, profiles, token auth. |
| `cargo test -p biorouter-mcp --lib agent_drafter::` | PASS | 202 passed; 0 failed. Isolated target `/tmp/br-testdrive-target`. |
| `cargo test -p biorouter-mcp --test ui_example_apps` | PASS | 5 passed; 0 failed. |
| `cargo test -p biorouter-mcp --test agent_drafter_registered` | PASS | 2 passed; 0 failed. |
| `cargo test -p biorouter-server --lib routes::apps` | PASS | 74 passed; 0 failed. |

The isolated BioRouter environment is rooted at `.br-testdrive/runtime` via
`BIOROUTER_PATH_ROOT`. Its app store is
`.br-testdrive/runtime/config/biorouter/agent_drafter`, so it cannot mix with the user's
pre-existing global Agent Drafter applications.
