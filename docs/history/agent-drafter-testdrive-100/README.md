# Agent Drafter Apps SDK v2 — 100-app test drive

This directory contains the branch-local evidence required by
`docs/agent-drafter-test-driving-guide.md` for all specifications in
`docs/agentic-app-test-ideas-100.md`.

- `results/spec-NNN.md`: per-app functional and aesthetic rubric.
- `shots/`: browser screenshots and before/after evidence.
- `authoring-logs/`: complete named-session Agent Drafter transcripts and static audits.
- `ledger.json`: machine-readable authoring rounds, timings, and audit state.
- `FINDINGS.md`: cumulative de-duplicated findings dashboard.
- `PROVIDER-BLOCKER.md`: current UCSF Azure 403/IP-allowlist evidence and exact resume point.
- `INVENTORY.md`: authored-app, verdict, screenshot, and blocked-spec index.
- `LAYOUT-DIVERSITY.md`: corpus-level layout audit and Agent Drafter diversity probes.
- `layout-probes/`: five no-sidebar archetype probes, 5/5 static audit, browser rubrics, and screenshots.
- `PLATFORM-INTEGRATIONS.md`: requested/configured/available/exercised audit for extensions, connectors, skills, KBs, routes, workflows, figures, and exports.

Current verified progress is 13/100 browser-tested drafts. The VPN/provider outage is
resolved and the sequential run is active.

All app projects live inside this worktree at
`.br-testdrive/runtime/config/biorouter/agent_drafter/<app-id>/`. Authoring and runtime
models are pinned to the UCSF Azure OpenAI deployment
`versa_azure/gpt-5.5-2026-04-24`; audits reject any other model reference.

Run the driver regression suite with:

```bash
python3 -m unittest scripts/agent-drafter-testdrive/test_run.py -v
```
