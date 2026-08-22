# Thinking indicator redesign

> **What this is.** The design of record for the affordance that tells the user Biorouter is
> working on their turn — the breathing dot currently sitting above the chat composer. Seven
> measured findings, six candidate replacements, and a recommendation.
> **Status:** Proposed. No implementation has started; nothing here is committed to.
> **Audience:** Anyone changing turn-state narration near the composer, and anyone reviewing
> whether these proposals hold.

The live companion is [`thinking-indicator-studio.html`](thinking-indicator-studio.html), where
each specimen animates at 1:1 inside a real chat column. Read this file for the argument; open
that one to judge the motion, which is the part prose cannot carry.

The brief was "the breathing dot above the composer is becoming redundant now that there is one
by the tool calls too, and the two do not line up". Both halves are true. The measurements below
say why, and why the obvious fix — delete one of them — is wrong.

## What is actually wrong

### 1. Two identical dots, on screen together for most of a turn

`LoadingBioRouter` (above the composer) and `TurnActivityIndicator` (in the transcript) render
byte-identical dot markup: the same three spans, the same 16 px / 10 px / 6 px stack, the same
two keyframes. The match was deliberate — commit `26a23fd7`, "Align working status with
composer". What was not intended is how often both are visible.

`deriveTrailingActivity` suppresses the transcript pill for `WaitingForUserInput`,
`LoadingConversation`, and while prose is streaming; its comment reads "Do not double-narrate."
That list misses `Thinking`, `Compacting`, and the whole of tool execution — which is most of a
turn. During those, both pills breathe at 1.8 s, 8 px out of line.

### 2. They are 8 px apart, and no padding value fixes it

| | Inset from the 760 px column edge | Source |
|---|---|---|
| Transcript pill | **4 px** (the pill's own `px-1`; wrapper adds nothing) | `ProgressiveMessageList.tsx:306` |
| Composer pill | **12 px** (`pl-2` on the row + the same `px-1`) | `BaseChat.tsx:2219` |

`pl-2` is not a mistake to delete. 12 px is exactly where the composer's own context row sits
(`pl-3`, `ChatInput.tsx:2150`), so the composer pill is correctly aligned — to the *composer's*
grid. The transcript pill is also correctly aligned, to the *transcript's*. **Two indicators
anchored to two different grids cannot be brought into line while both exist.** One has to go.

### 3. Three copies of the dot, and three unrelated pulse vocabularies

The three spans appear verbatim in `TurnActivityIndicator.tsx`, `LoadingBioRouter.tsx` and
`BioRouterSidebar/RecentChats.tsx`. Nothing stops a fourth copy and nothing keeps the three in
step.

Worse, three different pulse languages run at once:

| Affordance | Animation | Period |
|---|---|---|
| Breathing dot | custom `biorouter-working-ring` + `-glow` (scale + opacity) | 1.8 s |
| Tool-call status badge (`ToolCallStatusIndicator.tsx:29`) | Tailwind stock `animate-pulse` (opacity only) | 2 s |
| Message-queue dot (`MessageQueue.tsx:160`) | Tailwind stock `animate-pulse` | 2 s |

So the composer's dot genuinely does not match the one by the tool calls. They are not the same
component, not the same rhythm, and not the same mechanism.

### 4. 1.8 s is off the motion scale

The duration ladder tops out at `--dur-slow: 525ms`, documented in `main.css` as "the period of
an ambient loop that never ends (the spinner)" — the one tier reserved for exactly this. It has
**zero call sites**. The indicator hardcodes `1.8s` in a Tailwind arbitrary-value class, twice
per copy, six times in the app.

### 5. The status row costs a layout shift every turn

`renderWorkingStatus()` returns `null` when idle, so the row appears and disappears with each
turn — the composer jumps roughly 34 px (a ~24 px row plus `mb-2.5`) the moment you press Send,
which is exactly when your eye is on the composer.

### 6. The design system already specified two of these, and neither was built

`design.md` §4.20 fixes a canonical spinner: "1.5 px stroke, `currentColor`, 700 ms linear
rotation, sizes 14/16/20/24". There is no `Spinner.tsx`; 31 call sites use bare `animate-spin`
(1 s) or lucide's `Loader2`. §4.18 fixes a streaming caret — "2 px × 1em in `--text-muted`,
blinking at 1 s; removed on completion" — which does not exist either.

Three canonical periods therefore disagree on paper before any of them reaches the screen:
700 ms (§4.20), 525 ms (`--dur-slow`), 1.8 s (what ships). Whatever lands should pick one and
retire the others, or it becomes the fourth.

### 7. But the composer indicator cannot simply be deleted

The moment assistant prose starts streaming, the transcript indicator switches itself off —
`trailingActivity.ts:140`, "the visible text IS the feedback" — and there is no caret to replace
it (`MarkdownContent` has no streaming prop; this is deliberate). During prose, and during every
gap the transcript indicator suppresses itself for, **the composer pill is the only thing on
screen saying the turn is alive.**

So the two are not duplicates. The transcript one is *intermittent and per-step*; the composer
one is *continuous and turn-scoped*. They only look identical because they were drawn
identically. The fix is not to remove one — it is to stop them pretending to be the same thing.

## What replicating the composer changed

The first pass sketched the composer rather than reproducing it. Drawing it from the
source — `rounded-container` at `py-2.5 pr-3 pl-4`, a 54 px card, a 32×32
`rounded-element` Send/Stop box, three rows 6 px apart, and the one ink column the
differing row paddings exist to preserve (folder 236 px / placeholder 237 px /
reasoning glyph 236 px) — changed two of the six directions and added one finding.

**The composer already carries two accent signals during a turn.** The card's border is
coral *at rest*, because the composer autofocuses; and Stop passes no `variant`, so it
takes `default` — a solid accent square. The breathing dot is the third object in that
box and the only one in neutral ink. The composer is not short of signals; it is short
of *distinguishable* ones.

That has two consequences:

- **A had to change mechanism.** An accent arc travelling over an accent edge is
  invisible. The working state now *modulates* the edge it already has: the whole
  border drops to 32 % for the duration of the turn, and a full-strength segment
  travels round it. Same hue, same 1 px, same border — the only variable is where the
  ink is concentrated, which is the one thing focus never does. It still has to survive
  sitting 12 px from the accent-filled Stop button.
- **E got worse, not better.** An accent arc orbiting a solid accent square has almost
  no contrast to work with; it would need a third colour, which is a new signal rather
  than a reuse. The earlier mock drew Stop as a grey circle, which flattered the idea.

## The six directions

Full specimens, with live motion and per-option CSS, are in the studio page. Summarised:

| | Direction | Height | Kills the duplicate | Ends the two-grid clash | Keeps the label |
|---|---|---|---|---|---|
| **A** | **Live edge** — the composer's own 1 px border dims to 32 % and a full-strength segment travels round it | 0 px | yes | n/a — nothing to align | no |
| **B** | **Hairline sweep** — the row becomes a 1 px rule running the existing `.br-progress__fill--indeterminate` | ~30 px | yes | yes | yes |
| **C** | **The mark is the clock** — the BR monogram's split navy/coral underline travels | ~34 px | yes | no | yes |
| **D** | **Type carries it** — no dot; a slow light sweep across the label itself | ~26 px | yes | yes | yes |
| **E** | **The control is the indicator** — an arc orbits the Stop button; no status row | 0 px | yes | n/a | no |
| **F** | **The composer recedes** — a state, not a loop; layers onto any of the above | 0 px | partial | n/a | via placeholder |

Two carry blocking objections worth recording rather than rediscovering:

- **C runs into D-14.** The design system has already deleted animated brand decoration once —
  the staggered sidebar entrance and the flying-bird marks — on the reasoning "the nav stops
  performing. Honours *calm*." `design.md`'s first adjective is that nothing "pulses, glows, or
  gradients without a reason grounded in state." An animating logo is what that decision closed.
  The mark is also `<text>` in Inter measured at runtime with `getBBox`, so animating it means
  animating around a live measurement, untestable in jsdom.
- **E collides twice.** Pressing Stop already fires a one-shot `animate-ping` for
  `STOP_ACK_MS` = 450 ms to say "received" (`ChatInput.tsx:2524`), deliberately distinct from
  looping pulses that mean "still working" — an orbiting ring sits on top of the
  acknowledgement at the moment it has to read clearly. And the button is already
  accent-filled, so the orbit has no contrast to work with.

## Recommendation

**A + D together, with F as a follow-up.** They fix different halves and neither does it alone.

**D** removes the duplicated atom rather than restyling it: with no dot above the composer there
is nothing to be redundant with and nothing to be 8 px out of line, and it keeps every word the
row carries today while being a net deletion of code. Its weakness is peripheral read — if you
are not looking at the words, nothing moves.

**A** is exactly that missing peripheral signal, at zero height, so the composer stops jumping on
Send. Together the composer says "Biorouter has the floor" at the edge of vision and "here is
what it is doing" in prose, and the transcript dot goes back to being the only dot in the app.

**F** afterwards, because it is the only piece that still says anything under
`prefers-reduced-motion`, where both A and D flatten to a static frame. That is a real gap in
every option here and F is the cheapest way to close it.

## Implementation order

1. **Extract the dot into one primitive** before touching anything visual. Three verbatim copies
   is what let the two get out of step; the sidebar keeps the dot, so the primitive is needed
   either way.
2. **Move 1.8 s onto the scale.** `--dur-slow` exists for ambient loops and has no call sites.
   Whatever ships should be its first, and the arbitrary `animate-[…_1.8s_…]` values go with it —
   they are exactly the newly-written arbitrary utilities `main.css` warns can silently fail to
   reach the stylesheet under `BIOROUTER_NO_HMR`.
3. **Delete the dot from the composer row** (D). *Keep* `pl-2` — it is what aligns the row with
   the composer's context chip at 12 px, and once the dot is gone there is no competing round
   thing for it to be out of line with.
4. **Add the travelling edge** (A) as authored CSS in `main.css`, keyed off a `data-working`
   attribute on the composer card — not a Tailwind arbitrary value, for the reason the
   focus-edge comment directly above it already gives.
5. **Look at all three families and both modes.** Only ink and accent vary — surfaces are one
   shared neutral ramp — but the accent is the whole of A: Parchment `#b85a32` / `#e8895f`,
   Alma Mater `#14828c` / `#60d0da`, Roche Limit `#ee6c1a` in both. An edge that reads as
   "working" in coral may read as "focused" in teal.
6. **Design the reduced-motion resting frame deliberately.** The global reset nulls duration and
   clamps iteration count, which parks A's arc at a fixed angle and D's gradient at a fixed
   offset. Both need an explicit resting appearance, the way
   `.br-progress__fill--indeterminate` already declares one.

## Related documentation

- [Thinking indicator studio](thinking-indicator-studio.html) — the live specimens.
- [Biorouter Design System](../../../design.md) — D-14, D-15, §4.18, §4.20.
- [Theming](../theming/README.md) — the per-family accent values step 5 refers to.
- [Design](../README.md) — the parent folder index.
