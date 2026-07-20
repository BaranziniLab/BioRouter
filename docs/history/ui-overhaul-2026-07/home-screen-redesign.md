# Home screen redesign

> **What this is.** The written half of the Home-page redesign spec: why the Home column
> was realigned to the chat column, what the token and session numbers on Home actually
> meant, and the eight decisions that produced the usage heatmap that replaced the tiles.
> **Status:** Historical record — signed off 2026-07-08, steps 1–7 implemented, shipped.
> **Audience:** developers working on the BioRouter desktop Home page, session accounting,
> and the sessions/insights routes.
> **Identifier key.** `H-01`…`H-08` are the eight decisions in this document (see
> [Eight decisions](#eight-decisions)). `DR-52` is a drift-register entry in the design
> system, [`design.md`](../../../design.md).

Two asks drove this work: make the Home page share the chat column's edges, and replace
the flat "tokens in the last N days" tiles with a real usage heatmap. Doing the second
honestly required auditing how BioRouter counts tokens and sessions in the first place.
It does not count them the way the tiles imply.

> **The visuals live in [`home-screen-redesign.html`](home-screen-redesign.html) and must
> be opened in a browser to be seen.** That page carries the live before/after mockups of
> the Home page, the interactive heatmap with its hover and keyboard tooltips, the
> intensity-formula histograms, the width-comparison bars, and the theme-switchable colour
> swatches. This companion carries the reasoning, not the pixels.

## Headline numbers

| Measure | Value |
|---|---|
| Home column before | 1120px |
| Home column now | 760px |
| Accounting defects fixed | 7 |
| Refuted — do not "fix" | 4 |
| Per-day token rows seeded | 816 |
| Decisions signed off | 8 / 8 |

## The Home page was 47% wider than the rest of the app

`Hub.tsx` renders the composer inside `max-w-[760px]`, exactly matching the chat column.
But the content above it — `SessionsInsights.tsx` — was wrapped in `<ReadableContent>`,
whose default size is `max-w-[1120px]`, with a further `px-8` of padding. So the
statistics started 148px to the left of the box you type into.

| Region | Width | Source |
|---|---|---|
| Chat column | 760px | `BaseChat`, `ChatInput`, composer |
| Home content | 1120px | `ReadableContent size="text"` |
| Knowledge graph | 1440px | `ReadableContent size="graph"` |

> **Rendered in the HTML.** Three proportional bars showing the 760 / 1120 / 1440px
> columns side by side, the two over-wide ones marked as defects.

This is one instance of a broader defect already logged as `DR-52` in the design system
([`design.md`](../../../design.md)): the content column is 1120 / 1280 / 1440 / uncapped
depending on the view, so switching tabs visibly reflows the page. Home is the most
jarring case because the composer sits right there for comparison.

### The fix

Add a `chat` size to `ReadableContent` and use it for the six wrappers in
`SessionsInsights.tsx`. Drop the `px-8`, which would otherwise inset the content *inside*
the 760px column and misalign it again.

```ts
// ui/desktop/src/components/Layout/ReadableContent.tsx
const WIDTH_BY_SIZE = {
  chat:  'max-w-[760px]',   // NEW — the composer / chat column
  text:  'max-w-[1120px]',
  wide:  'max-w-[1280px]',
  graph: 'max-w-[1440px]',
};
```

## What the numbers on Home actually mean

The accounting path was traced from the provider response, through the agent loop, into
SQLite, out through the HTTP route, to the JSX. Every candidate defect was then handed to
an adversarial reviewer whose job was to refute it. **Four were refuted.** Seven survived.

### How a token becomes a number

| Stage | What happens |
|---|---|
| 1. Provider | Returns `Usage { input, output, total }`, all `Option<i32>`. Anthropic folds `cache_creation` + `cache_read` into `input`, which is *correct* — its plain `input_tokens` excludes cached tokens. |
| 2. Agent loop | Per turn, `update_session_metrics` *overwrites* `total/input/output_tokens` with this turn's snapshot (the live context-window size) and *adds* the same values into `accumulated_*`. |
| 3. SQLite | One row per session. `accumulated_total_tokens` = Σ(per-turn totals) = tokens actually processed and billed. **No per-turn timestamp exists.** |
| 4. Insights SQL | A single aggregate over `sessions`, windowed on `updated_at`. |
| 5. UI | Six numbers in two grids of tiles. |

### The seven confirmed defects

| Sev | Defect | Direction | Consequence |
|---|---|---|---|
| med | **Insights count every session type** — `get_insights` has no `session_type` filter, but `list_sessions` does. | overcount | Every `SubAgent`, `Hidden` and `Terminal` session inflates *Total sessions* and *Total tokens*. One user task spawning three sub-agents shows as four sessions. The tile and the list below it disagree. |
| med | **"Tokens · past 7 days" is not a 7-day figure** — the window keys on `updated_at` and then sums the session's *entire lifetime* total. | overcount | A 60-day-old session holding 2,000,000 tokens that receives *one* reply today contributes all 2,000,000 to "past 7 days". The number is unbounded relative to the truth. |
| med | **`messages.tokens` is declared but never written** — both `INSERT INTO messages` statements omit the column. | — | The column is NULL for every row that has ever existed. **There is no per-day token series anywhere in the database.** This is what blocks the heatmap. |
| low | **Cancelled or errored streams drop the turn** — usage is yielded only on the terminal chunk. | undercount | Stop a generation and the provider still billed you for the full input plus partial output. BioRouter records zero. |
| low | **Non-atomic `i32` read-modify-write** — accumulation happens in Rust, not SQL. | both | A lost update drops a turn under concurrent writes. And `i32` overflows at ~2.147e9 accumulated tokens — in release that *wraps negative* and then *subtracts* from the insights `SUM`. |
| low | **A usage-less turn blanks the live gauge** — `Usage::default()` writes `NULL` over the current-turn columns. | — | The context-window readout blanks for that turn. Home totals are unaffected (the SQL `COALESCE`s to `accumulated_*`). |
| low | **Multi-chunk usage would multiply** — every usage-bearing chunk is added. | overcount | Zero effect on Anthropic / OpenAI / llama-server / Ollama, which emit usage exactly once per turn. A non-conformant proxy that emits cumulative usage per chunk would multiply the turn ~N×. |

### What was refuted — do not "fix" these

1. **`accumulated_total_tokens` grows super-linearly. That is correct.** Each turn resends
   the whole conversation, so turn *N*'s `input_tokens` already contains turns 1…*N−1*.
   Summing per-turn totals therefore grows quadratically — and that is exactly what the
   provider processed and billed. It is *tokens consumed*, not *tokens in the transcript*.
   Changing it would make the number wrong.
2. **The 7/30-day windows are rolling UTC, not calendar days.** True, and intentional —
   documented in a comment. Not a defect, though a heatmap needs calendar days.
3. **`sessions_last_7_days` keys on `updated_at`.** Also intentional: "an active session
   counts toward the recent window even if it was started earlier." Defensible for a
   *sessions active* tile. It is the *token* sum on the same key that is wrong.
4. **The `COALESCE` mixes cumulative and last-turn units.** The triggering row (NULL
   `accumulated_*` with a positive `total_tokens`) is not producible by any write path.

## Can the heatmap even be built?

| Dimension | Available today? | Source |
|---|---|---|
| Sessions started per day | Yes — exact | `sessions.created_at`, a fixed UTC timestamp that never moves |
| Messages per day | Yes — exact | `messages.created_timestamp` (unix *seconds*) |
| Tokens per day | No | Tokens exist only as one lifetime total per session. `messages.tokens` is always NULL. |

> **The honest position.** A session that ran from Monday to Friday holds a single
> `accumulated_total_tokens` and a single `created_at`. Its tokens cannot be spread across
> those five days, because nothing records when they were spent.

### Two ways forward

| Approach | What it gives | Cost |
|---|---|---|
| **Interim** — zero backend change | Attribute a session's whole token total to its `created_at` day. Computed client-side from the `listSessions` response the page already fetches. Sessions-per-day is exact; tokens-per-day is anchored, not distributed. | Ship today. Tooltip must say *"tokens are attributed to the day the session started."* |
| **Correct** — new table | Append-only `token_events(session_id, ts, input, output, total)`, one row per turn, written from `update_session_metrics` where the per-turn delta is already in hand. True per-day tokens — *and* it fixes the "past 7 days" overcount at the same time. | A migration, one insert, one endpoint. Historical days seed from `created_at`. |

> **Warning.** Do not repurpose `messages.tokens`, even after populating it.
> `replace_conversation_inner` `DELETE`s and re-inserts the entire message list, which
> would drop or re-stamp historical token rows on every edit. An append-only side table is
> the robust choice.

## The shading formula, decided by measurement

Token counts are heavy-tailed: one marathon day can be 30× a normal day. The candidate
formulas were run over a simulation of 150 days of plausible usage — 63 idle, ordinary
days of 20k–150k tokens, and one 1.8M-token outlier.

### Measured level distribution

Counts are days per shading level; `L0` is the idle days.

| Formula | L0 | L1 | L2 | L3 | L4 | Verdict |
|---|---|---|---|---|---|---|
| Linear ÷ max | 69 | 80 | 0 | 0 | 1 | everything collapses to L1 |
| Log ÷ max | 69 | 0 | 1 | 56 | 24 | saturated; L1 unused |
| **Log + quartiles** | 69 | 21 | 20 | 20 | 20 | even spread; ordinary days span L1–L3 |

> **Rendered in the HTML.** Three side-by-side bar charts of that distribution, painted in
> the actual heatmap ramp colours so the collapse and the saturation are visible as shade.

**Linear scaling is unusable.** All 44 ordinary working days collapse into the faintest
shade; only the outlier is visible. This is the obvious first implementation and it must
be avoided.

**Log-with-max-normalisation saturates.** Because `ln(1+x)` compresses so hard, every
active day lands at 0.75–1.0 of the maximum: 56 of 81 active days become level 3, level 1
is *empty*, and a 4,200-token day renders as dark as a 250,000-token day.

**Log score + quartiles over active days** spreads the shades evenly (21/20/20/20) and
keeps ordinary days distinguishable across three levels. It is also what GitHub does, so
it will read correctly to anyone who has seen a contribution graph. Absolute values live
in the tooltip, where they belong.

```text
score(d) = ln(1 + tokens(d)) + 0.5 · ln(1 + sessions(d))     // 0 when the day is idle

// thresholds = the 25th / 50th / 75th percentile of score over the
// ACTIVE days in the visible window. Recomputed per window, server-side.
level(d) = 0 if score == 0
         | 1 if score <= q25 | 2 if score <= q50
         | 3 if score <= q75 | 4 otherwise
```

Tokens drive the shade; sessions act as a tiebreaker so a day with three short sessions
still outranks a day with one. The `0.5` weight is the one knob — see [H-04](#h-04--what-the-shade-actually-weighs).

## The new Home page

> **Rendered in the HTML.** A live, hoverable mockup of the redesigned Home page inside a
> 760px column — greeting panel, streak header, the 22-week heatmap grid with month and
> weekday labels, the Less/More legend, three stat tiles, recent-session rows and the
> composer — plus a "before" mockup showing 1120px content sitting over the 760px
> composer. It is the real interaction, not a picture of it: hovering or keyboard-focusing
> any cell opens the tooltip.

The page, top to bottom:

- **The greeting panel stays.** It is the page's only line of voice — an "UCSF Biorouter"
  kicker over "Hello! What insights will your data reveal today?"
- **Streak header** — "3 day streak" on the left, "Longest streak · 21 days" on the right,
  over the subtitle "Daily usage intensity — sessions started and tokens processed."
- **The heatmap grid** — weekday labels (Sun / Tue / Thu / Sat shown, alternate rows
  blank), month labels above, cells as `<button>`s so they are focusable. Days in the
  current streak carry an inset 2px ring; hover scales a cell to 1.12.
- **Footer row** — the five-swatch Less→More legend (13px swatches) on the left, and the
  standing caveat on the right: "Tokens attributed to the day each session started."
- **Three stat tiles** — Sessions · 30 days, Tokens · 30 days, Tokens · all time.
- **Recent sessions** — the existing rows, each showing a token count and a relative time.
- **The composer**, unchanged, at 760px — now sharing every edge with the content above it.

### The tooltip

Opens on hover *and* on keyboard focus. For an active day it lists Sessions started,
Tokens processed, Messages and Top model, then the note "Tokens attributed to the day each
session started." An idle day reads "No activity — —".

### Grid metrics

The mockup pins one source of truth for the grid: `--cell: 24px`, `--gap: 6px`,
`--labels: 34px`, giving 22 weeks × (24 + 6) − 6 = 654px, which sits comfortably inside
the 760px column with its 8px gutter. Below 820px viewport width it drops to
`--cell: 15px`, `--gap: 4px`, `--labels: 28px`. In the app itself, H-07 sizes 22 weeks to a
760px column at 13px cells.

### The heatmap ramp

A warm sequential ramp, monotonic in luminance, with a separate dark-theme scale.

| Token | Light | Dark | Meaning |
|---|---|---|---|
| `--h0` | `#ece5d6` | `#241f16` | level 0 — idle day |
| `--h1` | `#e9c9ab` | `#4a3524` | level 1 |
| `--h2` | `#dda27a` | `#7a4d2e` | level 2 |
| `--h3` | `#c6774c` | `#b0653a` | level 3 |
| `--h4` | `#a04a27` | `#e8895f` | level 4 — heaviest day |

The mockup renders against the app's own surface tokens rather than the document's:

| Token | Light | Dark |
|---|---|---|
| `--app-canvas` | `#faf8f3` | `#0d0a06` |
| `--app-surface` | `#ffffff` | `#16120c` |
| `--app-fill` | `#f4f0e6` | `#282217` |
| `--app-strong` | `#d4cab6` | `#403928` |
| `--app-ink` | `#2a2520` | `#f4f0e6` |
| `--app-muted` | `#635c54` | `#b0a892` |
| `--app-subtle` | `#6e6760` | `#9c937b` |
| `--app-line` | `#e8e1d2` | `#282217` |
| `--app-accent` | `#b85a32` | `#e8895f` |
| `--app-focus-fill` | `#e4dcc9` | `#4d4430` |

Focus is not a colour-only signal: `.cell:focus-visible` swaps the background to
`--app-focus-fill` *and* draws an inset 1.5px `--app-ink` ring.

### What the "before" mockup shows

The before state is four tiles — 142 total sessions, 18 past 7 days, 4.2M total tokens,
2.9M tokens · 7 days — laid out at 1120px over a 760px composer. Two of those numbers are
the audit's defects made visible: *142 total sessions* counts sub-agent and hidden sessions
the list below never shows, and *2.9M tokens in 7 days* includes the whole lifetime of any
old session merely touched this week.

## The endpoint

```json
GET /sessions/activity?days=155        // ~5 months, server-clamped

{
  "range":       { "start": "2026-02-09", "end": "2026-07-09" },
  "maxSessions": 9,
  "maxTokens":   512000,
  "currentStreak": 3,
  "longestStreak": 21,
  "days": [                            // only days WITH activity; client fills level-0
    { "date": "2026-05-08", "sessions": 3, "tokens": 128402,
      "inputTokens": 96000, "outputTokens": 32402, "level": 3 }
  ]
}
```

`date` is a local calendar day. `level` is computed **server-side** — the server holds the
window's percentile thresholds, so every client agrees on the shading. Days without
activity are omitted to keep the payload small.

> **Warning — timezone.** SQLite's `'localtime'` modifier resolves against the
> `biorouterd` process's timezone, not the renderer's. For a desktop app they are the same
> machine, so this is fine — but the client should pass its UTC offset if we ever run the
> daemon remotely.

## Eight decisions

Signed off 2026-07-08 — every recommendation was accepted, and all eight are implemented.
Each decision below lists its options with the accepted one marked **Chosen**.

> **Rendered in the HTML.** The decisions are live radio-button cards with an export bar
> that serialises the current selection to text and copies it; they now record what was
> chosen rather than what might be.

### H-01 — Home column width

*The ask.* Match the composer exactly, or give the statistics a little more room?
Touches `ReadableContent.tsx` and `SessionsInsights.tsx` (6 wrappers).

- **760px — exact match. Chosen.** Every edge on Home lines up with the box you type into,
  and with every chat message. New `ReadableContent size="chat"`.
- 880px — a little wider. The heatmap gets 22 weeks across comfortably; the composer stays
  760px and is visibly narrower.

### H-02 — Where do per-day tokens come from?

*Blocks everything.* There is no per-day token data in the database today. Either ship an
approximation now, or add the table first. Touches a migration + `reply_parts.rs` + a new
route, or nothing at all.

- **Both, in that order. Chosen.** Ship the interim heatmap client-side now (sessions
  exact, tokens anchored to the start day, stated in the tooltip). Land `token_events`
  next, which also fixes the "past 7 days" overcount. Swap the data source; the UI never
  changes.
- Correct first. Add `token_events` before any UI work. Slower, but the heatmap is never
  approximate. Historical days still seed from `created_at`.
- Interim only. Never add the table. Accept that tokens are attributed to the session's
  start day, forever.

### H-03 — Intensity scale

Measured on 150 simulated days with one 1.8M-token outlier. See
[the shading formula](#the-shading-formula-decided-by-measurement). Touches the level
computation (server-side).

- **Log score + quartiles. Chosen.** Even 21/20/20/20 spread. Ordinary days occupy three
  distinct levels. Shading is relative to your own window — the same convention as GitHub.
- Log ÷ window max. Preserves an absolute sense of volume, but saturates: 56/81 active days
  land on one level and level 1 goes unused. A 4k-token day looks like a 250k day.
- Linear ÷ window max. Every ordinary day collapses to the faintest shade. Recorded only so
  the trap is on the record.

### H-04 — What the shade actually weighs

Tokens are the work; sessions are the intent. The score blends them. Touches one constant.

- **Tokens lead, sessions break ties. Chosen.** A day of deep work outranks a day of three
  trivial sessions. `ln(1+tokens) + 0.5·ln(1+sessions)`.
- Equal weight: `0.5·ln(1+tokens)ₙ + 0.5·ln(1+sessions)ₙ`.
- Sessions only. A pure "did you show up" graph. Exact today, with no backend change at all.

### H-05 — Which sessions count?

*Correctness.* `get_insights` counted `SubAgent`, `Hidden` and `Terminal` sessions; the
list beneath it does not. The two disagreed on screen. Touches one `WHERE` clause — and
changes numbers users already see.

- **User + Scheduled. Chosen.** Matches `list_sessions`. The tile and the list finally
  agree. **Total sessions will drop** — that is the bug being fixed, not a regression.
- User only. Cron runs vanish from the graph. Cleaner, but a scheduled overnight run is
  real usage.
- All types (keep today's behaviour). Numbers stay big and stay wrong.

### H-06 — The "Tokens · past 7 days" tile

It sums the whole lifetime of every session touched this week. Touches the insights SQL,
and possibly a label.

- **Fix it with `token_events`. Chosen.** Sum real per-turn deltas inside the window.
  Requires H-02·A or B.
- Relabel it honestly: "Tokens in sessions active this week." Free, immediate, and no longer
  a lie.
- Drop the tile. The heatmap says it better anyway.

### H-07 — Window and streaks

The reference shows five months and a streak counter. Touches the endpoint's default
`days`.

- **~5 months + streaks. Chosen.** 22 weeks fits a 760px column at 13px cells. Streak =
  consecutive days with any activity.
- 12 months, scrollable. Needs 8px cells or horizontal scroll inside 760px.
- 5 months, no streak counter. A streak is a gamification device. This app's thesis is
  "calm".

### H-08 — The tiles beneath

Keep a compact stat row under the heatmap, or let it stand alone? Touches
`SessionsInsights.tsx` layout.

- **Three tiles + recent sessions. Chosen.** Sessions · 30d, Tokens · 30d, Tokens · all
  time. Then the recent-session rows, as today.
- Heatmap + recent sessions only. The heatmap already carries the volume story. Maximum
  calm.

## What was done, in order

| Step | Change | Verified by | State |
|---|---|---|---|
| 1 | Add `size="chat"` (760px) to `ReadableContent`; switch the wrappers in `SessionsInsights.tsx`; drop `px-8`. The greeting panel stays. | Existing tests + a new width assertion | done |
| 2 | Add `WHERE session_type IN ('user','scheduled')` to `get_insights` so the tiles match the list. | `insights_exclude_internal_session_types` | done |
| 3 | Accumulate atomically in SQL (`COALESCE(col,0) + ?`) and widen the token columns to `i64`. | `accumulate_tokens_is_additive_and_atomic`, `accumulated_tokens_exceed_i32` | done |
| 4 | New `token_events` table (schema v10) + one insert per turn; seed history from `created_at`. | `token_events_drive_the_windowed_totals`; migration run against the live DB | done |
| 5 | `GET /sessions/activity`; regenerate the OpenAPI client. | `cargo test -p biorouter-server` + 2 read-only route tests | done |
| 6 | Replace the token tiles with the heatmap; tooltip on hover *and* on keyboard focus. | 9 Vitest cases + the 64-assertion contrast guard | done |
| 7 | Flush usage on cancelled/errored streams (accumulate once per turn, after the loop closes). | Anthropic format now yields a running usage snapshot | done |

**Shipped.** 744 `biorouter` lib tests, 44 `biorouter-server` tests, 614 Vitest tests, 64
contrast assertions; `cargo fmt`, clippy, `tsc` and eslint all clean.

Migration 10 was verified against the real session database before shipping: schema 9 → 10,
**816 historical events seeded** spanning 2026-01-27 to 2026-07-07, and the seeded sum
(`733,615,032`) matches the sessions' lifetime sum exactly. Days before the migration are
attributed to the day a session was *created*, because that is the only timestamp the old
schema kept; from here forward every turn is stamped when it is spent (H-02).

## Related documentation

- [`home-screen-redesign.html`](home-screen-redesign.html) — the rendered mockups, live
  heatmap and formula histograms this page describes
- [UI overhaul execution status](../../design/ui-overhaul/execution-status.md) — the branch-wide step list, gates
  and open items for the UI cohesion pass
- [`ui-cohesion-redesign.html`](../../design/ui-overhaul/ui-cohesion-redesign.html) — the approved visual spec for
  the app-wide cohesion pass, with Current ⇄ Redesigned toggles
- [Knowledge view redesign](knowledge-view-redesign.md) — the sibling redesign
  spec for the Knowledge view, from the same overhaul
- [`design.md`](../../../design.md) — the design system; `DR-52` (the content-width drift)
  lives in the drift register
- [Documentation index](../../README.md) — the map of everything under `docs/`
