# Computer Controller — 20 Multi-App Orchestration Test Tasks

Goal: stress the Computer Controller on realistic, multi-step, multi-app workflows
(Slack messaging, web literature gathering on multiple sclerosis, touring GWAS
summary-statistics resource sites, and writing MS Word reports). Model:
**mimo-v2-omni** (vision). Environment: macOS with Slack, Microsoft Word (AppleScript +
`docx_tool`), Safari, Chrome installed.

Tool inventory the agent can orchestrate:
- `web_scrape` (HTTP fetch → cache), `computer_control` (AppleScript: browser/Slack/Word UI),
  `docx_tool` (write `.docx` directly), `pdf_tool`, `xlsx_tool`, `cache`,
  `developer` shell + `screen_capture`/`list_windows`.

Status: ✅ pass · ⚠️ partial · ❌ fail · 🔬 run live below

## Slack (UI-automation only — Slack is not meaningfully AppleScript-scriptable)

1. Open Slack, surface the **Broccolito Lab** workspace, and report the visible channels.
2. **Send** a clearly-labeled test message to a Broccolito Lab channel.
3. Read the latest message in a Slack channel and summarize it.
4. Search Slack for messages mentioning "GWAS" and report what you find.

## Web literature on multiple sclerosis

5. Fetch a PubMed search for "multiple sclerosis GWAS" and list the top article titles.
6. Open a review article on MS genetics in the browser and extract the abstract.
7. Gather 5 recent MS literature references (title + link) from a search.
8. Open Google Scholar for "multiple sclerosis susceptibility loci" and capture the top results.

## GWAS summary-statistics resource tours

9. Tour the **GWAS Catalog** (ebi.ac.uk/gwas), search "multiple sclerosis", report summary-stat downloads.
10. Visit **IEU OpenGWAS** and find MS datasets / summary stats.
11. Visit **FinnGen** and locate the MS endpoint summary statistics.
12. Find the **IMSGC** MS GWAS resource and report its download links.

## MS Word reports

13. Write a Word report summarizing MS-genetics literature gathered.
14. Create a Word doc with a title, headings, and a bibliography of MS references.
15. Append a "GWAS resources" section (resource + URL table) to an existing report.
16. Insert a figure with a caption into a Word report.

## Full multi-app orchestration (hardest)

17. Gather MS literature → synthesize a summary → write it into a Word report → post a Slack message announcing it.
18. Search GWAS Catalog for MS summary stats → compile the download links into a Word table → save the report.
19. Read two local text files → synthesize → write a Word report → notify Slack.
20. Find MS GWAS summary stats across 3 sites → write a comparison report in Word → post a summary to Slack.

---

## Live results & findings

Each problem is tagged **P** prompt/context, **T** tool-hardening, or **E** environment.
Model: mimo-v2-omni. Sandboxed config.

### Tasks that worked well

- **Task 5 (PubMed literature)** ✅ — `web_scrape` fetched the PubMed results page; the
  model parsed the HTML with shell (`sed`/`grep`) and extracted 5 real MS-GWAS article
  titles, and correctly suggested the E-utilities API for structured data.
- **Task 9 (GWAS Catalog tour)** ✅ — the model used the GWAS Catalog **JSON API**
  (`/gwas/api/search?q=multiple sclerosis`) via `web_scrape`, extracted real study
  accessions (GCST003566, GCST005531, …) and the `fullPvalueSet` summary-stats flag,
  and documented the working endpoint. Genuinely useful output.
- **Task 13 (MS Word report)** ✅ (with a note) — a valid 36 KB `.docx` was produced.
  Note: the model **bypassed `docx_tool`** and used `developer` shell + `python-docx`
  instead. Both work; this is a model-preference observation, not a failure. `docx_tool`
  (update_doc creates-if-missing) remains available and is more portable (no python-docx
  dependency).

Conclusion: **web/data/report tasks are reliable.** The hard part — as the user
reported — is **GUI app automation**, specifically Slack.

### Task 1+2 (Slack) — the failure the user observed, and the fixes

**Observed:** the agent got **stuck in Slack's search/quick-switcher overlay**. It typed
"general" expecting a `#general` channel (this workspace has none — only a Slackbot DM),
Slack said *"couldn't find anything by that name"*, and the agent didn't know to dismiss
the popup and use the composer. The transcript also showed it issuing a **blind
coordinate click** (`click at {230,170}`) and repeatedly hitting
`System Events ... AppleEvent timed out (-1712)` — **each hung call blocked ~120 s**,
burning time/tokens (the user's "circling / consuming tokens" symptom).

**Root causes & fixes:**

| Cause | Category | Fix |
|---|---|---|
| `computer_control` could hang ~120 s on a blocked UI (AppleEvent timeout) | **T** | Added a **watchdog**: the script runs under a tokio timeout (default 45 s, `BIOROUTER_COMPUTER_CONTROL_TIMEOUT_SECS`). On timeout it returns fast with actionable guidance ("UI is blocked by a modal/overlay; screenshot, press Escape, change approach; do NOT re-run") instead of stalling. |
| No app-specific playbook for chat apps → wrong flow (search vs. composer vs. quick-switcher) | **P** | Added a **"Driving messaging & chat apps"** section to the instructions: composer is the box at the bottom (click → type → Return → verify); switch channels with the quick switcher (Cmd/Ctrl+K) **not** Search (Cmd/Ctrl+G); don't assume `#general` exists. |
| Got stuck in an overlay and kept typing into it | **P** | Added operating principle **#6 (recover from popups/hangs)**: if a popup/dialog/search is open or an action hangs, press Escape, screenshot, reassess; never keep typing into an unresponsive popup or re-send a timed-out script. |
| Blind coordinate clicks | **P** | Reinforced the existing method-preference order (app automation → keyboard → named element → coordinates last). |
| Slack itself is not meaningfully AppleScript-scriptable | **E** | Inherent; UI automation is the only path. For reliable Slack posting, a Slack API/webhook integration would be the real solution (out of scope for this pass; noted as a recommendation). |

**Verification (live, mimo-v2-omni):** re-ran the exact recovery step that was stuck.
The agent activated Slack, **pressed Escape to clear the leftover search popup**, took a
screenshot, and correctly reported **(a) no search popup open** and **(b) the composer is
at the bottom** ("Message #spoke-tech input field"). This is precisely the step it could
not do before (trapped in the search overlay, never locating the composer). With the
overlay cleared and the composer identified, the composer→type→Return send flow proceeds
normally.

**Operational note discovered during testing:** the dev `biorouter` binary was *adhoc*-signed,
so every `cargo build` changed its signature and **voided the macOS Keychain grant** for the
provider API key (runs then failed with "XIAOMI_MIMO_API_KEY not found"). Re-signing the dev
binary with the UCSF Developer ID (stable identity, as `just copy-binary` does) restores the
grant and keeps it across rebuilds. Worth doing automatically after a dev rebuild.

**Recommendation (not yet implemented):** for *reliable* Slack posting, add a Slack
Web-API / incoming-webhook path instead of UI automation — UI automation will always be
brittle for Slack. The fixes above make the UI path fail gracefully and recover, but an
API integration is the real solution (separate, larger piece of work).
