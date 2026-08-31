# Chat summary and academic figure acceptance

## Contract

The chat summary is a compact contextual popover, not a dashboard: use the
application's small typography, neutral surfaces, spacing and standard buttons.
Show persisted To Do state as an ordered stepper with explicit status text and a
completion count. Connections indicate list order, not inferred dependencies.
Hide the section when there are no tasks. Task updates, reopening, renaming,
replacement and chat switching must not show another chat's or an obsolete list.

Generated figures prioritize the data: meaningful titles, units, source or
assumption notes when relevant, restrained theme-aware colors, and large legible
labels fitted to the available space. No decorative gradient banners or generic
subtitles. Explicit user styling remains supported. Accessible data and controls
must remain useful without relying on color alone.

## 2026-08-30 development evidence

These checks used the isolated development profile, not the installed release.
Sol owned design, implementation and review; Luna drove the desktop through
Computer Use. The daemon was rebuilt before the following fresh-artifact run.

| Check | Result |
| --- | --- |
| Existing chat with two completed tasks | Compact metrics and 2/2 completed rows visible |
| Empty new chat | Metrics visible; no To Do section |
| Three-step fictional comparison | 1/3 complete, one in progress, one pending |
| Finish verification and present a chart | 3/3 complete; fresh chart rendered |
| Reopen comparison and rename verification | 2/3 complete; correct name/status; no duplicates |
| Tool activity names | Semantic task operations, including starting, completing, reopening and renaming |
| Figure aesthetics | Neutral background, informative heading, labeled axes, series controls, data disclosure |
| Comparison accuracy | With an explicit 30-day month, 15/20 hours saved, $8/$9 per hour, $12 per incremental hour |
| Summary keyboard | Escape from inside the popover closes it and returns focus to its trigger |
| Desktop artifact resizing | Approximately 300/440-pixel previews remained rendered; hover tooltip overlap observed, not formal collision coverage |
| To Do disabled in Settings → Chat → Capabilities | Fresh chat reported unavailable and made no checklist call (~12 seconds) |
| To Do restored to its original enabled setting | New chat created/completed two items through real tools (~22 seconds); installed extension count stayed at two |

The chart maps monthly cost to x and daily time saved to y. Its plotted points
and labels agree. The test prompt did not prescribe the reverse mapping, so an
initial tester report of reversed axes was not a product defect. Native HTML
data disclosure is not a modal: Escape dismissal is not its acceptance contract.
Test Escape/focus return on the summary popover itself.

The three-step run's final summary showed 10 tool calls, 283k billed tokens and
one artifact. This is observed UI accounting, not evidence that all tokens were
uncached or that the artifact caused that usage. Investigate the provider path
before attributing or optimizing it.

The stored accounting separates 99,500 input, 1,804 output and 181,248 cached
input tokens (282,552 total); the current context was 23,928 tokens. Do not
describe this as 283k uncached input or a 283k live context. Figure generation
now also returns a small structured receipt while retaining the complete HTML
resource for the desktop. The receipt says `created`, not `rendered`: successful
HTML generation is not visual verification. Provider conversion and live usage
still need to be checked separately from that serialization regression.

### Soul evidence and retest boundary

In the isolated profile, a fresh chat selected Soul without a user selection.
The live ingestion used the shipped `update-soul` skill and the conversation
ingestion tool, creating two curated pages with raw-source provenance, index
and log updates. The synthetic QA convention was explicitly scoped as test
material, not a real user preference. Read-only lint reported zero errors and
four warnings: all four pointed at existing raw evidence files. A regression
test reproduced those false warnings before the bounded lint correction;
missing files, symlinks and bundle escapes must continue to warn.

A subsequent manual Meditation run staged raw input but the test driver quit
the development app while it was still running. It is **not a passed schedule
test**. Startup recovery, successful completion and cursor advancement remain
required; no manual schedule-file or knowledge-data repair is test evidence.

Automated evidence at this checkpoint:

- Summary parsing, state and rendering: 27 focused tests passed, including two
  review regressions observed failing before their fixes.
- Full desktop suite: 3,824 passed before those two final review regressions;
  focused tests and typechecking passed after their fixes.
- Metadata-only session read: red-first test then passed, preserving private
  session reach checks while omitting conversation history.
- Auto Visualiser Rust module: 102 passed, two ignored.
- Browser fixture harness: 26 checks passed, including 320/480/1200-pixel
  layouts, light/dark examples, explicit styles and keyboard/mouse legends.

### First academic-template batch, final automated checkpoint

- Core library: **3,297 passed, zero failed, one ignored**.
- MCP library: **1,545 passed, zero failed, seven ignored**.
- Specialized browser harness: **90 checks passed** (22 static/VM, 36 layouts,
  32 interactions including keyboard scrolling).
- Generic chart harness: **27 checks passed** (five static/VM, 12 layouts,
  ten legend/tooltip interactions).

Both Rust libraries ran in one two-worker batch. The resource-sweep and
long-title partial-dashboard regressions were observed failing before their
fixes. Dashboard receipts retain independent recovery guidance, exact created/
failed counts and up to eight bounded failure entries with explicit omissions
and truncation flags. The complete artifact retains all error cards.

The browser matrix found and fixed oversized tooltips, transition-time tooltip
positioning, rapid map-resize animation, compressed table values, and unexplained
blank all-zero donuts. Luna independently rechecked narrow/wide donut captures:
readable scrolling tables, explicit zero state and no decorative banner.

Evidence: `/tmp/biorouter-academic-resource-core-green.log`,
`/tmp/biorouter-resource-and-dashboard-red.log`, and
`/tmp/biorouter-specialized-figures.Xs09P6/` (`keyboard-final.log`,
`generic-tooltip-green.log`, plus 48 screenshots). These are local development
checks, not final repository-gate or newly rebuilt desktop evidence.

### Rebuilt desktop checkpoint

The daemon built from `05e79cfb` was launched through a PTY-backed Forge session.
The initial pipe-backed launch exited before a daemon started and is not test
evidence. The running binary's SHA-256 was
`7619ed5cbe1bc747397159f62da14de0e381686a869e2174cdd7968b51baf83a`.

Luna drove a fresh fictional clinic comparison in session `20260831_1`:
three meaningful To Do steps finished 3/3; ten tool calls produced one fresh
artifact, with no repeated render call. A costs $120/month for 30 minutes/day;
B costs $180/month for 40 minutes/day. With the stated 30-day month, average
costs were $0.133 and $0.150 per minute, and incremental cost was $0.20 per
additional minute. The parent independently inspected the final screenshot:
compact summary, quiet figure styling, correct cost/time axes and data.

The stored figure response retains the complete HTML plus a bounded structured
receipt (`status: created`, `uri: ui://scatter/chart`). This verifies persistence,
not token savings in an uninstrumented external provider. No resource-discovery
`Method not found` errors appeared in this daemon's log during the run.

Fresh Soul lint in session `20260831_2` loaded `knowledge-lint` and returned zero
errors, warnings, informational findings or truncated diagnostics. No repair
was requested.

The subsequent manual Meditation actually completed from 17:31:00 to 17:34:04
PDT in `20260831_3`. It made one recent-mode recall call with the exact prior
successful-run cursor, then ingested three explicit non-scheduled session IDs:
`20260831_2`, `20260831_1`, and `20260830_24`. Raw staging was `3fc95ec` and curated
updates were `7b0b098`; index/log and provenance links were retained. The parent
verified `currently_running: false`, no error, and `last_run` advancing to
`2026-08-31T00:34:04.581369Z` without editing schedule storage. The initial
apparently-stalled observation was an intermediate snapshot, not a failed run.
Only this isolated test corpus was used; this does not validate all possible
real-user preference inferences.

Two follow-ups emerged: startup wrote cache-only "Soul recovery" commits,
reproduced by an executed Rust regression before its fix; and `BIOROUTER_PATH_ROOT` isolates the
backend but not Electron user data. Future dev launches must also pass the
existing Electron `--user-data-dir` option and verify its resolved log/config
path. Do not copy or clean the installed application's data for a test.

The shipped schema prompt audit also found obsolete `kb_ingest_source`/
`kb_query` names and nonexistent `kb_lint` repair parameters. All three template
files failed a direct contract check before correction, then passed. The added
Rust regression also passed; existing customized schemas are not overwritten.

The manual-run HTTP request waits for completion, while its UI initially only
disabled the run button. A bounded UI fix adds accessible pending feedback,
without claiming completion or changing that API contract. Its two failing
tests passed after the fix; all nine schedule UI tests passed together.
Live pending-message verification and richer in-place run progress remain
separate checks. This patch does not add polling or new cancellation behavior.

## Remaining release evidence

- Finish the scientific-renderer groups below; generic charts and the first six
  specialized templates do not establish all-renderer coverage.
- Finish capability-toggle, subagent and Soul/Meditation live scenarios.
- Run integrated server regressions and all repository gates on the final diff.
- Resolve active-request external catalog refresh and approval-actor questions
  recorded in the coding-agent bridge documentation.
- Remote synchronization and merge remain subject to the outstanding approval
  and review gates. This checkpoint is not a release approval.

### Scientific-renderer follow-up matrix

| Group | Renderers | Required preservation and edge cases |
| --- | --- | --- |
| Quantitative color | Heatmap, calendar, choropleth, shared sequential scale | Monotonic luminance, honest legends, observed zero versus missing, constant ranges, long labels, leap day/timezone boundaries, explicit map view |
| Outcome axes | Kaplan–Meier, forest | Step-after survival geometry, probabilities/censoring, explicit colors; log ratios around 1 versus linear differences around 0, confidence intervals and weights |
| Hierarchies | Network, dendrogram | Directed/weighted edges, isolated nodes, dragging, category cues, long internal/leaf labels, hierarchy and leaf counts; do not imply quantitative branch lengths |

The pre-fix source audit confirmed that the shared sequential scale had relative
luminance 0.127 → 0.657 → 0.266 at normalized values 0/0.5/1, and is used by
heatmap, calendar and choropleth. It is not a monotonic sequential encoding.
Source inspection also found label-to-HTML interpolation in network/heatmap
tooltips and choropleth text. The new templates use a monotonic scale and literal
text; regression fixtures exercise malicious-looking labels without execution.

The follow-up browser matrix is seven renderers × three widths × two themes.
Preserve data semantics, add bounded readable tables and keyboard scrolling, and
measure labels/tooltips rather than applying an indiscriminate font increase.
The new scientific harness baseline has ten passes and 14 failures. Executing
the actual calendar template in a VM confirmed that a March 7–9 fixture drops
March 9 in Los Angeles and shifts labels to March 6–8 in Tokyo; UTC passes.
The final scientific matrix passed 119 checks: 33 static/VM checks, 42 layouts,
and 44 interaction/invariant/edge checks. Evidence is in
`/tmp/biorouter-scientific-figures.L7JK7J/matrix-final.log`. Browser tests caught
buried network arrowheads, fixed by accounting for target-node radius. VM tests
caught zero forest weights being replaced by default weights, and duplicate
calendar dates influencing the scale despite their earlier values being
discarded. Zero remains zero in the table with a documented minimum marker;
calendar duplicates retain the existing last-entry-wins behavior consistently.
Forest invalid weights and calendar expanded/negative years now have Rust
validation regressions pending the final serial run.

Luna reviewed the final corrected forest close-value and network screenshots.
Ticks remain distinct, arrows are visible, and narrow figures explicitly scroll
instead of shrinking labels or overflowing the page. Earlier four browser
harness failures (selectors, resize timing and document reuse) were corrected
in the harness, not misreported as application defects.

The final inventory also includes area, boxplot, bubble, gauge, histogram,
Manhattan, Mermaid, sunburst, volcano, word cloud and dashboard templates.
Those eleven need a follow-on consistency/semantics audit and targeted
fixtures; completing the seven-renderer matrix is not all-renderer coverage.
Do not change statistical or spatial encodings just to unify their appearance.

Luna's lightweight VM probes executed the current gauge and histogram template
scripts with stubbed canvas/Chart hosts. Accepted gauge input
`{value:150,min:0,max:100}` produced arc fractions `[1,0]` and center text `100`,
silently replacing the actual measurement. Histogram input
`{values:[1.00001,1.00002],bins:2}` produced counts `[1,1]` but two identical
`1.00–1.00` labels. These are confirmed template regressions, not browser tests.
A mixed empty/nonempty boxplot was accepted by the backend; source inspection
and an isolated quantile probe found undefined/NaN statistics for the empty
group. The follow-up hierarchy matrix now covers this case.

### Follow-up verification checkpoints

- Local commit `c926829a` contains Soul cache-only history suppression, corrected
  shipped schema tool guidance, and accessible pending feedback for manual
  schedules. The serial Rust run passed 3,298 core and 1,550 MCP tests in
  `/tmp/biorouter-scientific-distributions-core-rerun.log`; the schedule UI set
  passed nine tests. The new pending message still needs a rebuilt live-app check.
- Gauge/histogram: 43 checks passed (11 static/VM, 12 layouts, 20 interaction and
  numeric edge checks) in `/tmp/biorouter-distributions-browser-final.log`.
  Three numeric probes first failed: extreme finite gauge spans, overflowing
  histogram spans, and zero-width bins between adjacent floats. Histogram axis
  titles also clipped at 320/480px before measured wrapping. Counts and exact
  measurements remain unchanged; reduced precision-limited bins are disclosed.
  Luna reviewed five final captures without finding overlap or misleading data.
- Boxplot/sunburst/wordcloud: 61 checks passed (17 static/VM, 18 layouts, 26
  interaction/edge checks) in
  `/tmp/biorouter-hierarchies-figures.1BqgzX/matrix-final.log`. Actual browser tests
  caught boxplot hover interception and wordcloud font-box collisions. Empty
  groups, zero/missing values, duplicate labels, negative hierarchy values,
  omitted words and safe literal labels have explicit handling. Original numeric
  boxplot statistics are retained in the table. Luna reviewed the final captures.
- Area/bubble/volcano/Manhattan: the initial executed VM matrix failed 12 checks;
  the first browser matrix passed 48 and failed eight narrow tooltip checks.
  Responsive layouts fixed the tooltips, but Luna correctly found visible
  ResizeObserver error cards that the first harness had missed. The harness now
  explicitly checks rendered alerts. Both chart layout and shared overflow-hint
  reporting defer/coalesce mutations outside observer delivery; each path has an
  executed failing-then-passing VM regression. The final matrix passed 58 checks
  (14 static/VM, 24 layouts, 20 interactions) in
  `/tmp/biorouter-cartesian-browser-final.log`. Luna rechecked six overwritten
  area/bubble captures and confirmed the banners were gone. The earlier 56-pass
  run is not acceptance evidence. Data fixes preserve exact bubble radii, CSS
  color channels and prototype-like chromosome labels as literal data. A new
  numeric-validation Rust test first failed (one pass/one fail); the backend fix
  is written but its rerun is pending.
- The shared resize repair was then rerun serially through every previously
  completed figure harness: generic 27, specialized 90, scientific 119,
  distribution 43 and hierarchy 61 checks all passed. Including Cartesian 58,
  that checkpoint is 398 checks across 144 layouts. Logs are
  `/tmp/biorouter-*-shared-resize-rerun.log` and the Cartesian final log above.
- Mermaid/dashboard template checks covered all 60 planned layouts. The actual
  typed-tool regressions then failed in three places before repair: colliding
  IDs, lost definitions/references/labels, and unknown/ambiguous Gantt
  dependencies. `/tmp/biorouter-typed-mermaid-red.log` includes a concrete
  three-node flow collapsing into one node with self-loops. The optional export
  test did not export in this red run; actual generated-HTML browser acceptance
  still follows the compiler repair.
- Latest hierarchy and Cartesian Rust tests, subsequent template edits, final UI
  typechecking/gates, and Mermaid/dashboard checks are not covered by the earlier
  3,298/1,550 Rust checkpoint. No release or remote synchronization is implied.

### Final integrated regression run

The full desktop suite passed **3,829 tests in 373 files** using two workers
(`/tmp/biorouter-final-ui-suite-run.log`). An initial invocation rejected the
unsupported `--minWorkers` option before running tests; only the corrected run
is acceptance evidence.

The subsequent combined Rust run passed **3,298 core tests and 1,563 MCP tests**,
with zero failures, one core ignore and seven MCP ignores
(`/tmp/biorouter-final-core-mcp.log`). This includes the latest scientific,
distribution, hierarchy, Cartesian and typed Mermaid regressions. The actual
tool outputs for nine diagram kinds were exported to the explicit temporary
directory `/tmp/biorouter-mermaid-typed.VgAfVh/` for browser verification.

The typed compiler now uses collision-free internal IDs while preserving visible
labels, implicit referenced nodes and the state start/end sentinel. Gantt
dependencies resolve exact IDs before unambiguous ID lists and reject unknown,
duplicate or ambiguous inputs. A separate Sol review found no concrete blocker
in the Cartesian validation or gauge/histogram changes. These checks do not
replace final browser, repository-gate or rebuilt desktop evidence.

The consolidated Mermaid/dashboard browser matrix then passed **94 checks**:
12 static/VM checks, 54 layouts of actual tool outputs, 12 parsed/rendered
identity and dependency checks, two security probes, eight dashboard interaction
checks and six dashboard layouts. Evidence is
`/tmp/biorouter-mermaid-final.8UxajD/matrix-final.log`. Two earlier failures came
from inspecting Mermaid state before its extraction/render phase; the final
checks inspect rendered identities, visible labels and compiled transitions,
and explicitly reject a collapsed-identity negative fixture. A dark-text issue
was confined to a hand-authored tall-panel test fixture that omitted the shared
theme runtime; no production theme change was needed. Luna rechecked the final
dark dashboard and wide class diagram and found no current visual defect.

A final copy review found sunburst guidance promising every node in a table that
is deliberately bounded to 500 rows. The wording now describes that bound and
points to the row-limit caption. Its executed VM regression failed before the
correction (17 passes, one failure), then passed (18 passes, zero failures).
This last sentence change follows the full Rust checkpoint above; the follow-up
hierarchy browser and repository gates provide subsequent validation.

The hierarchy rerun passed **62 checks** with the final guidance
(`/tmp/biorouter-hierarchies-final-caption.log`). Combined with the earlier
matrices and the final Mermaid/dashboard run, the accepted figure checkpoint is
**493 checks across 204 layouts**. This is renderer/browser coverage, not a
claim that every live provider or packaged application has been retested.

The original Destiny archive digest was rechecked as
`a95600b77f0b6a5b5744cacd6cedfa3d19bd16d8ac59662d3be2dc4b727eb529`.
Its retained audit identified repository-skill installation delegated to a child
without shell/editor tools, followed by an invalid Developer-inheritance retry.
The final core run passed
`destiny_style_repository_install_is_routed_to_the_audited_skills_manager`
and the bridge roster, live-grant and dispatch-revocation regressions. The
earlier successful Skills-importer dry-run does not establish approval-dependent
installation or permanent-removal/residue coverage. Those remain distinct from
the separate live skill hot-load/use/hot-unload evidence.

A no-window probe of this worktree's Electron runtime confirmed that the launch
flag resolves `app.getPath('userData')` to
`/private/tmp/biorouter-goal-gui.gsVSPC/state/desktop`. The first comparison used
the equivalent `/tmp` alias and reported a mismatch; the canonical-path check
passed. BioRouter was not launched by this probe. The real Forge launch must
still confirm the forwarded flag and resolved log path before live testing.

The first full repository gate stopped at three prohibited UTF-8 string slices
in the Gantt resolver. Sol replaced them with character iteration and splits at
match-proven boundaries, without suppressing lints, and added multibyte
whitespace/Unicode-ID regressions. The entire combined Rust suite then passed
again: **3,298 core and 1,563 MCP tests**, with the same ignores and zero failures
(`/tmp/biorouter-final-core-mcp-unicode.log`). All nine newly exported diagram
HTML files matched the SHA-256 manifest captured before that correction, so the
actual-output browser evidence applies unchanged.

The separate UI lint command passed typechecking, ESLint, generated-theme
consistency, all **332 contrast assertions**, and semantic-token mirror checks
(`/tmp/biorouter-final-ui-lint.log`). Luna owns the serial full-gate rerun; any
source fixes remain Sol's responsibility.

The all-target Clippy rerun also found a slice in the new test assertion itself;
Sol replaced it with `strip_prefix` without changing the contract. The subsequent
**complete `just check-everything` passed**, including strict all-target Clippy,
UI checks, OpenAPI regeneration/comparison, version, branding, cross-compile,
registry generation and privacy-registry checks
(`/tmp/biorouter-final-repository-gates-final.log`). Generated schema/SDK files
had no diff. The exact edited identity assertion then passed as one MCP test
(`/tmp/biorouter-final-mermaid-assertion-tests.log`); the filtered core target
ran zero tests and is not additional core evidence.

### Remaining ingestion runtime guarantees

A read-only follow-up at `879ca4c1` confirmed that the successful Meditation run
does not establish a whole-operation timeout or streamed chat-ingest progress.
`knowledge/macros/ingest.rs` acquires locks, refreshes and stages input before
the bounded model loop, then settles/verifies afterward. Refresh still awaits
blocking work without an overall deadline. Both chat ingest helpers pass
`event_sink: None`; the bridge waits for the complete response. Its advisory
600-second task budget is not an enforced dispatch deadline, and transport can
wait 31 minutes.

These are larger follow-ups, alongside safe active-request catalog refresh.
Implementing a timeout by dropping the outer future could leave blocking work
or staged changes unsettled; the design needs cancellation, draining and cleanup
coverage. User triage was requested before expanding that runtime work. No
bounded-total-latency or streaming-progress guarantee is claimed here.

### Full server validation

The first server run passed 547 library tests and failed one stale source-order
assertion looking for the former `get_session(&session_id, true)` call. The route
now supports metadata-only reads, and source inspection confirmed the privacy
gate still precedes both modes. The runtime private metadata/history regression
also passed. Sol updated the assertion to locate the first session read
independently of its history argument, retaining the gate-before-read check,
and corrected the history-only explanatory comment.

The full rerun passed **1,219 tests with zero failures** across library, binary
and integration targets (`/tmp/biorouter-final-server-tests-rerun.log`). Ten
opt-in real Claude/Codex tests remained ignored because they require signed-in
CLIs and spend provider quota; zero-test targets and ignored cases are not
additional coverage. The subsequent complete repository gate rerun also passed
(`/tmp/biorouter-post-server-repository-gates.log`, exit zero).

### Final production-default desktop checkpoint

The production-default daemon and CLI built successfully from `502930f5` in
4m54s. The daemon SHA-256 was
`b55d66a4298ebbe091f72e7facce95e285b8e9192c9821840d9267369f5636f3`.
Actual Electron arguments, renderer working-directory metadata, profile files,
and daemon open-file paths confirmed isolation under the test profile and
`/tmp/biorouter-final-live.cqHHQg`. No installed-user profile was used.

Luna's natural SQLite/comparator request in parent `20260831_4` successfully
spawned child `20260831_5`, but the child correctly reported that its actual
tools could neither create the project nor execute tests. Its grants were the
audited Knowledge, Skills and Extension Manager subset. Developer, Code
Execution, Computer Controller and native host tools are intentionally absent
from this subscription bridge; configured enablement is not callable access.
No SQLite fixture, comparator or executed tests were produced. The attempted
live steer targeted the same child but arrived after it stopped, returning
`target child has no turn in flight; live steering cannot be delivered`.
Spawn is proven; useful executable delegation is blocked on this provider, and
this terminal-child attempt does not prove live steering. Four blocked checklist
items correctly remained pending, and the failed activity was named
`Workspace Send Prompt`. The parent reported the blocker without fabricating
work. No daemon warning, error or resource-method-not-found line appeared.

The unnecessary spawn despite an unavailable execution surface exposed a
planning-guidance weakness. A new focused prompt-contract regression failed
before the wording correction (`/tmp/biorouter-child-preflight-red.log`). The
full green rerun passed 3,299 core and 1,563 MCP tests with zero failures
(`/tmp/biorouter-child-preflight-full-green.log`); all repository gates passed
again (`/tmp/biorouter-child-preflight-repository-gates.log`). The nine exported
Mermaid fixtures still match the previously browser-verified hashes. A supported
follow-on live scenario remains a separate acceptance check.
Do not weaken the credential boundary to make the SQLite scenario pass.

Manual Meditation in `20260831_6` completed from 19:13:56 to 19:16:23 PDT.
Luna observed the accessible pending message and disabled run button, followed
by completion and a re-enabled button. Parent screenshot inspection confirmed
both states; read-only schedule inspection confirmed `currently_running: false`,
no error and `last_run: 2026-08-31T02:16:23.603801Z`. No manual schedule or
knowledge-store repair was made. The development app was then quit normally
after completion, not interrupted mid-run.
