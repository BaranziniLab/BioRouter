# Apps benchmark v2 — baseline

The pinned v1 baseline for the apps SDK v2 work (plan Phase 6, "Benchmark v2").
These numbers are scored from the **raw store files** (`manifest.json` +
`index.html` + `src/main.ts`) of every app in the local store — never
served/rendered HTML, the same honesty rule the v1 variety benchmark used.

## Reproduce

```bash
source bin/activate-hermit
node ui/desktop/scripts/appcheck/benchmark.mjs          # human table + summary
node ui/desktop/scripts/appcheck/benchmark.mjs --json   # machine-readable
```

Store dir defaults to `~/.config/biorouter/agent_drafter`; override with
`BIOROUTER_APPS_DIR`. A missing/empty store exits 0 with a "no apps installed"
note (CI-safe).

Live health check (needs a running `biorouterd`):

```bash
node ui/desktop/scripts/appcheck/check-all.mjs [base-url]   # default http://127.0.0.1:3000
```

## Baseline measured 2026-07-12

Store: `~/.config/biorouter/agent_drafter`, **111 apps** (all v1).

```
v2-score: 61.3% non-chat, 0 avg bound paths, 0% typed-calls
```

Archetype distribution (heuristic, from raw `index.html` structure):

| archetype | apps | %     |
|-----------|------|-------|
| chat      | 43   | 38.7% |
| other     | 68   | 61.3% |
| explorer / dashboard / workbench / wizard / canvas | 0 | 0% |

v2 feature usage:

| axis                              | value |
|-----------------------------------|-------|
| bound state paths (total)         | 0     |
| bound state paths (avg/app)       | 0     |
| apps with any binding             | 0 (0%) |
| declared surface (actions/signals/components) | 0 / 0 / 0 |
| typed calls (`br.call` / `actions.register`) | 0 apps (0%) |
| catalog components (network/plot/table/kpi/log/component) | 0 apps (0%) |
| theme packs                       | 0 apps (0%) |
| structured archetypes             | 0 apps (0%) |

## Reading the baseline

Every app in the store today is a v1 app, so all six v2 axes read **zero**: no
`data-br-bind*` bound state, no `manifest.surface` declarations, no `br.call`
typed RPC, no catalog components, no theme packs, and no structured archetype
(`explorer`/`dashboard`/`workbench`/`wizard`/`canvas`).

"Non-chat" here means an app **not** classified as the `chat` archetype — this
includes the `other` bucket (v1 form/tool apps with a custom UI but no chat
region and no recognized structured layout). The stricter "structured
archetypes" row (0%) is the number to watch climb as v2 archetype starters land;
`% non-chat` and `avg bound paths` / `% typed-calls` in the summary line track
the v2 protocol-adoption goals of the plan.
