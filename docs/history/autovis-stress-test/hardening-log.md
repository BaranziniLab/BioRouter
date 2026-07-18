# Auto Visualiser stress-test — hardening log

Fixes applied between batches, driven by the problems flagged in
[`results.md`](results.md).

## After Batch 1 (viz 1–10)

**Observed:** 10/10 passed — every request produced one dashboard artifact with the
requested 2–3 figures, a title, a summary, and a caption under each figure; zero build
errors, zero tool failures, libraries inlined once each. The one recurring inefficiency:
`render_dashboard` was called **twice** in the same turn on 2/10 runs (#1, #9), both times
successfully. That is the model drafting then re-rendering an identical report — it burns
tokens and drops a second, redundant card into the chat.

**Fix:** the success message `render_dashboard` returns to the model was neutral
("Combined report 'X' rendered inline … with N figures."). Added a terminal instruction on
the **success** path: *"The report is complete and displayed to the user. Do not call
render_dashboard again unless the user asks to change it."* The failure path already told the
model to retry; it now also says to retry **with the corrected figures only**. Guarded by a
new assertion in `tests_dashboard.rs`. Backend rebuilt and restaged. Batch 2 will confirm the
duplicate-call rate drops.

## After Batch 2 (viz 11–20)

**Observed:** 10/10 passed. The Batch-1 nudge worked — **zero** duplicate `render_dashboard`
calls across all 10 (down from 2/10). No Auto Visualiser correctness defects surfaced; the
diversity swaps (#19 Kaplan–Meier) rendered correctly. The one model round-trip (#12) was
correct behaviour and is mitigated by pre-loading numeric data into prompts.

**The real Batch-2 findings are platform-level, not Auto Visualiser:**
1. **biorouterd killed by Electron, no respawn.** `crates/biorouter-server/src/commands/agent.rs`
   has no idle timeout (SIGINT/SIGTERM only); the dev log shows a clean `exited with code 0`
   ~2.6 min after launch. Source: `ui/desktop/src/main.ts:1211`
   `mainWindow.on('closed', () => biorouterdProcess.kill())` on a module-global backend shared by
   all windows — a transient/secondary window close takes the shared backend down and nothing
   restarts it. **Suggested fix:** respawn-on-unexpected-exit, and/or only kill the shared
   backend when the *last* window closes (ref-count windows) rather than on any window close.
2. **Silent backend-disconnected UX.** With the backend dead, Home renders normally from cache
   and the composer still accepts input; on submit `createSession` throws `Failed to fetch`, the
   composer clears, and **no "backend disconnected / reconnecting" banner appears** — the app
   looks like it silently swallowed the message. **Suggested fix:** surface a connection-lost
   state (disable composer + banner) when backend health checks fail.

**Fix applied (harness):** ran the dev app in **external-backend mode**
(`BIOROUTER_EXTERNAL_BACKEND=true`, `BIOROUTER_EXTERNAL_PORT=3000`, secret `test`) against a
persistent biorouterd I control on port 3000 (same sandbox, CDN mode). This decouples backend
lifetime from Electron's window lifecycle so the 100-viz run proceeds without the ~2.6-min death
cycle. No Auto Visualiser code change was needed for Batch 2. Items (1) and (2) are logged as
product follow-ups (out of scope for the Auto Visualiser stress test, but real robustness gaps
the test surfaced).

## After Batch 3 (viz 21–30)

**Observed:** 10/10 correct. Outstanding chart-type diversity actually rendered — Sankey, two
Leaflet choropleths + a marker map, calendar heatmap, network, forest, boxplot, treemap, gauge,
and a Mermaid **state diagram** (all four asset families exercised). One process flag: **#30
double-called `render_dashboard`** (`ranDashboard:2`) — the third double-call of the run (#1, #9,
#30). Correct final artifact both times; cost is a redundant re-render + a duplicate chat card.

**Root cause (most likely):** the model emits the second `render_dashboard` in the *same*
assistant turn (or before the first tool response returns), so the Batch-1 success-path message
("don't call again") can't intercept it — a message-level nudge can only affect the *next* turn.

**Specified fix — server-side idempotency guard (ready to apply):** make the tool, not the
prompt, enforce single-render. In `crates/biorouter-mcp/src/autovisualiser/`:
- Add to `AutoVisualiserRouter` a field `last_dashboard: std::sync::Mutex<Option<(u64, std::time::Instant, CallToolResult)>>` (hash, when, cached result).
- At the top of `render_dashboard`, hash the normalized `RenderDashboardParams` (e.g. `DefaultHasher` over the serialized args). If the incoming hash equals the stored one **and** `elapsed < ~15s`, return the **cached `CallToolResult`** unchanged plus assistant text "This report was just rendered and is already displayed — returning the existing artifact." Otherwise render normally and store `(hash, Instant::now(), result)`.
- Window kept short (~15 s) so a genuine later "re-render the same thing" (after a tweak that reverts) still works.
This collapses accidental duplicate/near-simultaneous calls into one artifact regardless of model
behaviour, and also hardens the feature for real users (double-submit → one card).

**Activation decision (deliberate):** NOT applied mid-run. It requires rebuilding the debug
`biorouterd` and restarting the persistent backend on :3000 — the same restart path that cost a
long recovery detour earlier. Because the double-call is **cosmetic and does not affect
correctness** (all 30 artifacts are correct), and the primary directive is "100 correctly
generated, don't stop," activating it now would risk the primary goal for a non-correctness fix.
Plan: apply + `cargo build` + smoke-test as a **consolidated hardening at a safe break** (end of
run, or immediately if any single later batch shows ≥2 double-calls). Until then, the Batch-1
message nudge keeps the rate low (~5%).

## After Batch 4 (viz 31–40)

**Observed:** 10/10 correct. Two double `render_dashboard` calls (#31, #40) → 5/40 (12.5%)
overall — which met the "≥2 in a batch → activate" trigger. Before acting I inspected the chat
DOM on #40: `ranCount:2` tool chips but **`dashIframeCount:1`** — the user is shown **one** report
artifact, not two. And the `Figure N.`-prefixed titles on #30/#40 (absent on #31) indicate the
2nd call is sometimes a **refinement** with *different* args, not a byte-identical repeat.

**This corrects the Batch-3 recommendation:**
- A backend idempotency guard **keyed on an args hash would MISS the refinement case** (different
  args → different hash → both still render) and could **wrongly block a legitimate re-render**.
  So the Batch-3 "server-side idempotency guard" is **withdrawn as the primary fix** — it's the
  wrong tool for a refinement.
- The double-call is a **token-only cost with zero correctness/UX impact** (one artifact shown).
  The correct, safe product follow-up is **UI-side**: have `collectArtifactsFromMessages`
  (`ui/desktop/src/components/BaseChat.tsx`) collapse **multiple dashboard artifacts from the same
  assistant turn to the last**, so an intermediate/refine render never surfaces a card. Frontend-
  only, correct whether the 2nd call repeats or refines. Keep the Batch-1 message nudge.
- **Trigger overridden, deliberately:** even though the numeric trigger fired, the *evidence*
  says the risk/benefit doesn't justify a mid-run rebuild — the cost is tokens only, no user-
  visible defect, and the backend fix would be incorrect. Logged as a product follow-up so the
  100-viz completion (the primary directive) isn't jeopardised for a cosmetic optimization.
