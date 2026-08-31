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

Source audit confirmed that the current shared sequential scale has relative
luminance 0.127 → 0.657 → 0.266 at normalized values 0/0.5/1, and is used by
heatmap, calendar and choropleth. It is not a monotonic sequential encoding.
Source inspection also found label-to-HTML interpolation in network/heatmap
tooltips and choropleth text; regression fixtures must exercise literal
malicious-looking labels before accepting these renderers.

The follow-up browser matrix is seven renderers × three widths × two themes.
Preserve data semantics, add bounded readable tables and keyboard scrolling, and
measure labels/tooltips rather than applying an indiscriminate font increase.
The new scientific harness baseline has ten passes and 14 failures. Executing
the actual calendar template in a VM confirmed that a March 7–9 fixture drops
March 9 in Los Angeles and shifts labels to March 6–8 in Tokyo; UTC passes.
Its 42-layout browser matrix has not run. Constant choropleth ranges remain a
concern to test, not a confirmed failure.
