# QA round 1 — first sweep over the merged streaming tracks

> **What this is.** The first of three QA rounds over the merged streaming tool-call work: a
> driven sweep of the desktop GUI covering the named "Recent Chats" bug, the sidebar, chat
> tasks, tabs, visualizations and creative probes.
> **Status:** Historical record (completed 2026-07-19).
> **Audience:** developers working on the desktop transcript and tool-card rendering, and anyone
> tracing an `R1-NN` finding.

Findings raised in this round carry `R1-NN` identifiers, defined where each is first stated. Two
were confirmed: a sidebar highlight desync, and — the significant one — failed tool calls
rendering as green successes because `rmcp` serializes `isError` in camelCase while the UI read
snake_case. Twelve areas passed clean. The observations at the end are deliberately not promoted
to findings; they seeded [round 2](qa-round-2-results.md).

Date: 2026-07-19
Driver: Electron GUI via Playwright gui-driver (tmux), vite renderer on :5173, staged debug backend (Jul 19 03:30).
Provider: app default (us.anthropic.claude-sonnet-4-6 / versa_bedrock).
Console error/warn hook installed on launch (`window.__errs` / `window.__warns`), active the whole run.

Merged tracks under test: streaming providers, pending tool cards, per-tool response emission, tool-card status/turnActive, trailing activity indicator, tab system + cargo fixes, Home submit transport, dashboard REMOVAL, boot splash, icon alignment, theme work.

---

## Setup notes
- Launch initially loaded `chrome-error://chromewebdata/` because no vite dev server was running; the gui-driver requires a standalone vite on :5173. Started it (`BIOROUTER_NO_HMR=1 npx vite --port 5173 --strictPort`) and relaunch loaded `http://localhost:5173/#/?` correctly. Not a product bug — a driver prereq.

---

## Checklist 1 — NAMED BUG: "Recent Chats buttons are currently not behaving correctly"

Exercised the sidebar Recents and Home "Recent chats" and the /sessions history page hard:
- Single click on a sidebar recent → navigates `/pair?resumeSessionId=<id>`, opens a tab named from the row, no console errors. CORRECT.
- Opening a second, different recent → second tab, both preserved. CORRECT.
- Clicking an already-open session (from sidebar OR Home OR /sessions history) → dedupes, activates the existing tab, no duplicate. CORRECT.
- Re-clicking the currently-active session → no-op, `aria-current="page"` stays. CORRECT.
- Rapid triple-click on a not-yet-open recent → exactly ONE new tab, no duplicate (dedupe survives the async-nav race). CORRECT.
- Home "Recent chats" rows use a different path (`resumeSession` → `setView('pair',{resumeSessionId})`) than the sidebar (`navigate('/pair?resumeSessionId=')`), but both dedupe to the same tab. CORRECT.
- /sessions "See all" page: tab strip is hidden on /sessions but tabs are preserved; clicking a history item restores the strip and opens/activates the tab. CORRECT.

The Recents open/dedupe behavior itself is solid. The real defect is in the ACTIVE-STATE derivation — see R1-01.

### R1-01 (minor) — sidebar active-highlight + URL desync when a "New Session" (empty) tab is active
The sidebar computes the active recent from the URL query param:
`currentSessionId = currentPath === '/pair' ? searchParams.get('resumeSessionId') : null` (AppSidebar.tsx:128), passed as `activeSessionId` to RecentChats.

When you create/switch to an empty **New Session** tab, the URL is NOT cleared — it keeps the previous `resumeSessionId`. Result:
- Selected tab (strip) = "New Session", but URL = `#/pair?resumeSessionId=20260719_40`.
- The sidebar Recents keeps highlighting the *previous* session ("HOMEREG repeat requests") with `aria-current`/active background, even though that chat is NOT the one on screen.
- Switching between real-session tabs DOES sync the URL + highlight correctly; only the empty New Session tab leaves them stale.

Repro: open a real recent chat (tab + sidebar highlight both correct) → click sidebar "New Session" (or the `+` in the strip) → the empty New Session tab is selected, but the sidebar still shows the old chat highlighted and the URL still carries its `resumeSessionId`. Screenshot: /tmp/br-shots/newsession-urlmismatch.png
Impact: user cannot tell from the sidebar which chat is active; a reload/deeplink off that stale URL would resume the old session instead of the empty one.
Suspected files: ui/desktop/src/components/BioRouterSidebar/AppSidebar.tsx (128, 194-203, 302), the tab reducer / URL-sync for new-session tabs (grep `resumeSessionId` in the tab strip + navigationUtils.ts).

---

## Checklist 2 — Basic sidebar / control sweep

All nav items load, correct hash routes, ZERO console errors:
- Workflows (#/workflows) — Create/Import + built-in "Meditation" workflow. OK.
- Scheduler (#/schedules) — OK.
- Extensions (#/extensions) — OK.
- Skills (#/skills) — OK.
- Knowledge (#/knowledge) — OK.
- Applications (#/applications) — OK.
- Settings (#/settings) — Models/Chat/App tabs. OK.
- Home (#/) — hero + usage heatmap + Recent chats. OK. (Dashboard REMOVED — no dashboard route/pane remains; Home is the streak/heatmap surface.)

Theme work (Settings > App > Theme):
- Palettes Parchment / Alma Mater / Roche Limit, Modes Light / Dark / System all present and functional.
- Dark mode applies cleanly (`html.dark`, body bg rgb(13,10,6)); wordmark recolors navy→teal on dark as designed; no unstyled/contrast-broken regions in the settings surface. Palette switch (Parchment↔Alma Mater) under dark works, zero errors. Reset to Parchment/Light clean.

Driver note (NOT a product bug): Radix `role="tab"` triggers (Settings Models/Chat/App) do not react to a synthetic `el.click()`; they DO activate on real keyboard focus+Enter. Verified the tabs work; just a driver-injection limitation.

---

## Checklist 3 — Chat tasks

### (a) Web search — session 20260719_48 "Australia capital search"
Prompt: "Search the web: what is the capital of Australia? One sentence." (provider us.anthropic.claude-sonnet-4-6)
Streaming behavior observed (all GOOD):
- Pending tool card "Ran Search Modules · 1 result ready" appeared early with green status dot.
- "Working on the result" sub-indicator under the card; trailing "Biorouter is working on it…" above the composer.
- Running dot on the "New Session" tab in the strip; Stop button (red square) in composer during the turn.
- Turn completed: answer "Canberra is the capital of Australia" (correct), tab auto-renamed "Australia capital search", sidebar recent updated, cost $0.00 → $0.08. Zero console errors.
- Per-tool emission is CORRECT: DB shows 3 real `code_execution__search_modules` toolRequest/toolResponse pairs → 3 distinct cards (not a render duplication).

### R1-02 (minor) — 3 identical success tool cards while a loop-guard treated the tool as failed
The model called `code_execution__search_modules` 3× (no web-search tool exists here). Each toolResult carries `status:"success"` so all three cards render identically as "Ran Search Modules · 1 result ready" (green/success). But after the 3rd, the backend injected a `<biorouter-loop-guard>` user message: "No progress: 'code_execution__search_modules' has now failed 3 times…", and the assistant then answered "I don't have a web search tool available… Based on my training knowledge: Canberra…".
Issue: the UI shows three identical **success** cards ("1 result ready") with no differentiation and no surfaced indication that a loop-guard intervened / that the calls made no progress. A user reads three green successes then a "no tool available" answer — contradictory. The card faithfully mirrors `toolResult.status=success`, so the defect is at the semantics/labelling layer (a search that returns "1 result" but no usable answer should not read as an unqualified success, and the loop-guard event is invisible in the transcript).
Repro: ask any fresh session to "search the web" for something; watch 3 identical success cards + a give-up answer.
Suspected files: tool-card renderer (grep `result ready` / `Ran ` in ui/desktop/src/components), the loop-guard emission in crates/biorouter/src/agents/agent.rs (biorouter-loop-guard), search_modules tool status in crates/biorouter-mcp.

### (d) Multi-step project task — session "File creation task" (20260719_49)
Prompt: "In /tmp/qa-r1 make calc.py with an add function plus test_calc.py, run pytest, fix failures, summarize in one line."
This is a STRONG PASS for the streaming tracks:
- Pending tool cards appear early; each has a distinct, descriptive label ("Ran Create /tmp/qa-r1 directory", "Coordinating 3 tool steps", "Coordinating 4 tool steps", "Locate pytest and check Python version", "Install pytest via pip3", "Install pytest with --break-system-packages", "Run pytest on test_calc.py") — per-tool response emission works, cards are NOT duplicated.
- Assistant prose streams INTERLEAVED between tool cards ("pytest isn't installed. Let me install it…", "pytest is now installed. Let me run the tests.") — the per-tool + streaming interplay is smooth.
- Trailing "Biorouter is working on it…" indicator + Stop button present throughout; running dot on the tab.
- Cost ticked $0.06 → $0.25 live; final summary correct ("All 3 tests passed… created in ~/Desktop/qa-r1… pytest 9.1.1 / Python 3.14.6"). Files verified on disk (calc.py, test_calc.py, 3 tests). Tab auto-renamed. Zero console errors/warns.
- Note (expected, not a bug): developer tools are sandboxed to the working dir, so the agent created the files under ~/Desktop/qa-r1 instead of /tmp/qa-r1 and said so.

Graceful-degradation bonus: the earlier accidental partial submit ("In /tmp/qa-r1 create a") — a DRIVER timing artifact where send fired before async typing finished, NOT a product bug — was handled well by the agent: "It looks like your message was cut off! You were saying 'In /tmp/qa-r1 create a…' — what would you like me to create there?"

---

## Checklist 5 — Visualization — session "Chromosome length bar chart" (20260719_50)
Prompt: "Make a bar chart of the top 5 longest human chromosomes … using an autovisualiser tool."
CLEAN PASS:
- Model used `show_chart` type "bar"; tool card "Ran Bar chart of top 5 longest human chromosomes by length in Mb · 2 results ready".
- Inline artifact card ("Bar Chart · text/html") in the transcript AND the artifact side panel auto-opened on the newest artifact, rendering the chart correctly (chr1 248.96 → chr5 181.54, GRCh38). Data table + prose also correct. Cost $0.16, zero errors.
- Tab-switch away and back: transcript fully preserved (tool card, inline card, table, prose); the side panel closes on switch (per-tab) — reasonable.
- Close tab + reopen the session fresh from the sidebar: transcript restored from DB; the panel did NOT auto-open (reasonable — this is the "reloading a saved session" case, distinct from a live turn), but the inline card's "Open Bar Chart in the artifact viewer" RE-RENDERS the figure perfectly from the persisted blob. No "Chart is not defined" / blank-figure regression.

Not driven (budget): a second viz type (network/heatmap). The autovisualiser inline+panel+persist+re-render pipeline is verified via the bar chart; a second type mainly exercises a different tool's template. Deferred to conserve paid turns.

---

## Checklist 4 — Tabs (concurrency + stress)

Stress: opened 6 new tabs rapidly via the `+` (chat-tab-new) → 8→13 tabs. Observations:
- One of the 6 `+` clicks was deduped (clicking `+` while already on a fresh empty New Session reuses it) — reasonable.
- At 13 tabs the strip shrinks tabs to icon-only labels and shows the overflow menu (`chat-tab-overflow-trigger` "Show all chats"). Layout holds, no wrapping/overflow of the window.
- Closed in mixed order — close ACTIVE (focus shifts to neighbor), close BACKGROUND (active session unchanged, only index shifts), close FIRST, close LAST (active): 13→12→11→10→9. Every step: correct focus, NO zombie spinners, NO lost sessions, ZERO console errors.
- The R1-01 URL/sidebar desync recurs here too: with an empty New Session active the URL stayed `resumeSessionId=20260719_50`.

Concurrent streaming across tabs was implicitly covered by the earlier live turns (each ran in its own tab with its own running dot; switching tabs mid-turn preserved the transcript and elapsed state). No duplicate submissions seen in the DB (each session carries exactly the user messages I sent; the "File creation task" session legitimately has 2 user turns — the partial + the full — not a double-submit).

---

## Checklist 6 — Creative probes (4 driven)

1. **Empty / whitespace message**: typing only spaces leaves the composer value empty and the Send button `disabled`. Double-clicking Send on empty = NO submission, no turn, no errors. PASS.
2. **Cancel mid-tool-call then immediately resubmit** (session "sleep 8" shell run): sent a slow `sleep 8 && echo` shell task; while the tool ran, clicked Stop → the tool card flipped to "Stopped Run: sleep 8 && echo hello-from-shell · No result" (a clean, distinct stopped state — NOT a stuck spinner; tool-card status track working). Composer restored immediately. Then resubmitted "Now just say the word ready." → agent replied "ready" cleanly. Cost $0.02, zero errors. PASS — the cancel/turnActive path is solid.
3. **Narrow-window resize**: `window.resizeTo` clamped the Electron window to its 800px minimum width; at that width the left sidebar rail collapses responsively, the composer goes full-width, tabs shrink to icons with an overflow menu — layout holds, nothing clips or overlaps. Widening back restores the sidebar. PASS.
4. **Rapid double-click Send**: covered under (1) for empty; and the earlier triple-click on a Recents row (checklist 1) proved the submit/open path dedupes under rapid repeat clicks. PASS.

## Checklist 7 — Cross-cutting

**ZERO `console.error` and ZERO `console.warn` captured across the ENTIRE round** (hook installed at launch, verified at the end: `__errs.length === 0`, `__warns.length === 0`). No uncaught errors, no unhandled rejections, no React warnings surfaced during: sidebar sweep, theme toggling (incl. dark mode + palette swap), 3 live LLM turns, tab open/close stress (13 tabs), artifact render + re-render, cancel/resubmit, and window resize.

Polish observations: none rising above R1-01/R1-02. The streaming tool cards, trailing "working on it" indicator, per-tool descriptive labels, live cost ticking, tab auto-rename, and running-dot indicators are all cohesive and well-styled in both light and dark.

---

## Not driven (be honest)
- Chat task (b) standalone fibonacci and (c) standalone file-organize-by-extension: folded into the richer multi-step task (d), which exercised file creation + shell + pytest run + fix + summary. The dedicated (b)/(c) prompts were not run separately to conserve paid turns.
- Second visualization type (network / heatmap): deferred (budget); bar-chart verified the full inline+panel+persist+re-render pipeline.
- Three concurrent LIVE streaming turns in 3 tabs simultaneously: partially covered (tabs each stream independently with their own running dot + preserved transcript on switch), but I did not fire 3 paid turns at once.
- Kill the backend process mid-turn (error UX): skipped — destructive to the shared dev backend / other worktrees.
- Switch model mid-session via the chip; open Settings while a turn actively streams; boot-splash (app was already warm when the driver attached): not driven this round.

## Summary
Strong round. The merged streaming/tool-card/tab tracks are working well: pending tool cards appear early, per-tool responses stream with distinct descriptive labels (verified against the DB — no card duplication), the trailing activity indicator + running-dot + live cost all behave, cancel yields a clean "Stopped · No result" state, and the tab system survives heavy open/close stress with correct focus and zero console noise. Dashboard removal is complete; Home is the heatmap/recents surface. Theme (light/dark + 3 palettes) is solid. Two findings, both minor: R1-01 (sidebar/URL active-state desync on empty New Session tabs — the likely core of the "Recent Chats buttons not behaving correctly" report, since the active highlight lands on the wrong row) and R1-02 (three identical success tool cards while a loop-guard treated the tool as failed).

---

## Round 1 RESULTS — regression verification (2026-07-19)

Verifier re-drove both fixed findings in the real GUI (fresh vite + gui-driver, staged debug backend — both fixes are frontend-only so no backend restage was needed) and re-ran the full suites. Fresh vite was restarted before driving so the renderer served the committed fix code (the prior BIOROUTER_NO_HMR server predated the edits).

### Per-finding verdicts

**R1-01 (fix 185578cc) — FIXED. Verified live, verbatim repro.**
- Baseline: open sidebar recent `20260719_48` → `hash=#/pair?resumeSessionId=20260719_48`, active highlighted row = `recent-chat-20260719_48`. URL + highlight agree.
- Sidebar "New Session" → `hash=#/pair` (stale param cleared), active highlighted row = **none**. FIXED.
- Strip "+" (`chat-tab-new`) → `hash=#/pair`, active row = **none**. FIXED.
- Switch back to a real tab (`recent-chat-20260719_49`) → `hash=#/pair?resumeSessionId=20260719_49`, active row = `recent-chat-20260719_49`. URL mirrors back correctly. FIXED.
- Screenshot: /tmp/br-shots/r1-02-verify.png (sidebar highlight lands on the active tab, not a stale row).

**R1-02 (fix dfa6dc32, frontend half) — FIXED. Verified live by reopening session 20260719_48 (no paid turn).**
- Transcript now reads: `problem=3, ran=0, failed=3, resultReady=0` — three red "Problem with Search Modules · Tool call failed" cards, zero green "Ran … · 1 result ready" success cards, then the coherent "I don't have a web search tool available… Canberra is the capital of Australia." answer. The green-success/give-up contradiction is gone.
- Screenshot: /tmp/br-shots/r1-02-verify.png.
- Skipped backend remainder (R1-02-backend-remainder) confirmed still out of scope and unnecessary for coherence: with the frontend fix the transcript already reconciles the loop-guard's failure count with the rendered cards.

### Revert-proof (gates not vacuous)
Reverted ONLY the three source files (`useChatGroupsUrlSync.ts`, `ToolCallWithResponse.tsx`, `BaseChat.tsx`) via `git apply -R`, keeping the tests. The 3 gate files then reported `Tests 4 failed | 48 passed (52)` — exactly the 2 R1-01 tests ("activating an empty New Session tab clears the stale resumeSessionId", "re-activating a real tab after an empty one mirrors its id back") and the 2 R1-02 tests ("shows a camelCase isError result … as a failure", "ignores files named by a tool call that failed with the camelCase isError flag"). Re-applying the source returned `52 passed (52)`. The gates cannot pass vacuously.

### Standing-guarantee spot-checks (live)
- **Home submit lands exactly 1 message** — PASS. Home composer submit created session `20260719_53`; DB shows exactly one genuine user-typed message (id 17330) plus one `toolResponse` row (also role=user, id 17332) — one real user turn, no double-submit.
- **3 tab open/close cycles do not duplicate** — PASS. From a real tab, `+` → close ×3: tab count `4→5→4` each cycle, no accumulation, no zombie, focus restored. No phantom DB sessions created by empty tabs.
- **Multi-tool turn shows pending cards early + complete args** — PASS (with caveat). The composite file task rendered a card with complete args ("Ran Create dir, write hi.txt, and cat it back · 1 result ready"); a running tool card + Stop button were observed live during the sleep turns (pending state appears before completion). The richer 7-distinct-card case was already covered in Checklist 3(d) this round; the spot-check turn collapsed into one composite developer call, so distinct-multi-card early-render was not re-exercised here.
- **Cancel mid-tool leaves no orphan** — PASS. Sent `sleep 45 && echo finalprobe` (session `20260719_54`), waited for the running card, clicked Stop mid-run: card flipped to "Stopped Run: sleep 45 && echo finalprobe · No result", Stop→Send, running=false. DB session 54 holds only the user message — the aborted turn was not half-committed (no dangling toolRequest without a response). Screenshot: /tmp/br-shots/cancel-stopped-verify.png.

### Suite results (verbatim)
- `cargo test -p biorouter --lib` → `test result: ok. 1454 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`
- `cargo test -p biorouter-mcp --lib` → `test result: ok. 807 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out`
- `cd ui/desktop && npm run test:run` → `Test Files 160 passed (160) | Tests 1353 passed (1353)`
- `npx tsc --noEmit` → exit 0
- `npm run lint:check` → exit 0 (incl. `OK — all 228 contrast assertions pass`)

### NEW observations (seed next round — none rise to a confirmed product bug)
- **Possible unintended active-tab switch**: while driving the paid-turn spot-checks, a `sleep 30` send I intended for session 53 landed in session 49 (the active tab had become 49 without an explicit navigate on my part). I could not deterministically reproduce it and it overlapped heavy eval/click driver traffic + the documented "DOM lies across tab switches" caveat, so this is UNCONFIRMED and more likely driver/interaction noise than a product defect — but tab-focus stability under rapid programmatic interaction is worth a deliberate probe next round.
- **Driver-only (not product)**: the `/pair` chat composer occasionally dropped typed input even with `document.activeElement === chat-input` immediately after a turn settled; a second click+type landed. This is a gui-driver focus race, not an app bug (real keyboard focus works).

## Related documentation

- [Streaming tool-call UI campaign](README.md) — the campaign index this round belongs to.
- [QA round 2 results](qa-round-2-results.md) — the next round, which took the observations above as its seed.
- [Streaming implementation status](streaming-implementation-status.md) — the merged tracks this round was sweeping.
- [Campaign final report](campaign-final-report.md) — the closing summary of all three rounds.
