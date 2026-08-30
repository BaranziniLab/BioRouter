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

## Remaining release evidence

- Normalize and test the remaining specialized figure templates; generic
  bar/line/scatter results do not establish all-renderer coverage.
- Finish capability-toggle, subagent and Soul/Meditation live scenarios.
- Run integrated server regressions and all repository gates on the final diff.
- Resolve active-request external catalog refresh and approval-actor questions
  recorded in the coding-agent bridge documentation.
- Remote synchronization and merge remain subject to the outstanding approval
  and review gates. This checkpoint is not a release approval.
