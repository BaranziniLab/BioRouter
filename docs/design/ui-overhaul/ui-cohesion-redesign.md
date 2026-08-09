# UI cohesion redesign

> **What this is.** The written half of the app-wide UI cohesion inspection spec (rev 2):
> the forensics that explain why the shipped app never matched the design sketch, and the
> specifications for the markdown layer, the preview panel, the terminal, tabbed chat
> groups, and every floating surface.
> **Status:** Current — a design specification only; nothing was committed
> to the app at the time of writing. Execution is tracked in
> [`execution-status.md`](execution-status.md).
> **Audience:** developers working on the BioRouter desktop UI, and agents implementing
> the change list.

The redesign's thesis is in its title: **fewer boxes, one ink.** Rev 1 aligned the
sidebar. Rev 2 strips the preview panel and terminal back to a single surface, rebuilds
the markdown layer, and fixes the reason the real app never looked like the sketch — the
chat's prose is painted in Tailwind's cold grey, not the Parchment tokens.

> **The rendered mockups live in [`ui-cohesion-redesign.html`](ui-cohesion-redesign.html)
> and must be opened in a browser to be seen.** That page carries a full interactive shell
> mockup, side-by-side markdown specimens, live component frames, and colour swatches —
> none of which Markdown can reproduce. This companion carries the reasoning, not the
> pixels.

**Identifier key.** Single letters and Greek letters (`A`–`Z`, `Θ`, `Σ`, `Ω`, `Ψ`, `Π`,
`∇`, `Æ`, `Ø`) are this document's own change tags, used to accept or reject each item
individually. `§N.N` and `D-NN` (numbered design decisions), `DR-NN` (drift register) and
`P1`–`P6` (principles) all refer to [`design.md`](../../../design.md) at the repo root.
Every value on the page is taken from design.md's Parchment palette, signed off
2026-07-09, so it is replicable 1:1 in-app.

## What the interactive page adds

> **Rendered in the HTML.** A full-window mockup of the BioRouter shell — sidebar, chat
> transcript, composer, terminal dock and preview panel — with toggles for Theme
> (Light / Dark), View (Current / Redesigned), Sidebar collapse, Show split, Mid-drag,
> Terminal, and Highlight, so each change can be seen appearing and disappearing in place.

The **Current** view shows the shell as it ships today: floating wordmark, no chat glyphs,
boxed preview chips over a bordered card, and a terminal framed inside a gutter of its own
colour. The **Redesigned** view shows aligned 52px headers, labelled zones, underline tabs,
and preview/terminal content sitting directly on the panel ground.

## Why the app never looked like the sketch

Measured in the running build, not inferred. The mismatch is not taste — it is a defect.
The font *stack* is identical in both (`ui-sans-serif` → SF Pro), and so is the smoothing.
The difference is that **the chat's prose is not painted with the design tokens at all.**
`@tailwindcss/typography` ships its own `--tw-prose-*` variables, and nothing in the
codebase ever remaps them — so every element the class list forgets falls back to
Tailwind's stock cold blue-grey ramp, on a warm parchment ground.

| Element | What the app paints | What the token says | Verdict |
|---|---|---|---|
| Paragraph | `#2a2520` | `--text-default` (`#2a2520`) | warm — correct |
| Bold text (`<strong>`) | `#101828` (gray-900) | `--text-default` (`#2a2520`) | cold |
| Headings (h3 / h4) | `#101828` (gray-900) | `--text-default` (`#2a2520`) | cold |
| Rules & table hairlines | `#e5e7eb` (gray-200) | `--border-subtle` (`#e8e1d2`) | cold |
| Markdown links | `#101828` (gray-900) | `--text-accent` (`#b85a32`) | not accent |

Those greys sit at hue 264 — **blue**. The tokens sit at hue ≈30 — warm. So in every
assistant answer, **a bold word is a different hue than the sentence around it**, and every
rule is a cold line on a warm page. `#e5e7eb` is, word for word, one of the hexes
design.md's anti-patterns list names as a *"foreign body"* — and the chat paints it on
every table.

**The glyphs disagree too.** One screen carries three stroke weights where the system
mandates 1.5 everywhere: the sidebar nav at 1.5, the Recent-Chats glyphs at 1px — visibly
lighter, immediately beside them — and a `lucide-gauge` at 1.75 from a direct import that
bypasses the icon wrapper. Two sizes (16 and 18) coexist as well.

> **Rendered in the HTML.** The three stroke weights drawn side by side at the same size,
> so the 1px Recents glyph is visibly lighter than the 1.5 nav icon next to it.

## The markdown layer, rebuilt

The app's most-read surface.

> **Rendered in the HTML.** Two panes of identical content — "Today · as measured" against
> "Redesigned" — where the left pane uses the real values measured from the build rather
> than a caricature: heading, paragraph with bold and link, a three-row table, a display
> formula, a blockquote and a rule.

### Tab labels are sans, not mono

**Overridden by the user, 2026-07-16 — and rightly.** Tab labels were originally set in
the mono face, borrowing otty.sh's trick of using a monospace for UI labels, on the
argument that design.md calls monospace "a first-class citizen". In the real app it reads
as a *special thin font* that belongs to nothing else on screen: the sidebar, the nav, the
chat and the session title are all `--font-sans`, so the tab strip — sitting on the same
52px bar as all of them — was the only place speaking a different language. That is the
exact fault this whole pass exists to remove.

Mono keeps the jobs it earns: code, the terminal, paths, and figures where columns must
align. A tab label is none of those — it is a name.

- **Was:** `--font-mono` 12px, tracking -.01em
- **Now:** `--font-sans` 13px (§3.2 metadata), normal tracking
- Reference: user override, supersedes the otty mono-label idea. Tag `Θ₂`.

### Mono for data, sans for chrome

**The other half of `Θ₂`, 2026-07-17 — the font was still wrong in the chat *and* the
preview.** Both tab strips were already fixed by `Θ₂` in one line, because the preview
panel shares chat's `.br-tabstrip` contract. But the preview's *status strip* still set
mono on the container, so every word in it inherited a face it had not earned — the same
drift, one surface over.

The rule that settles it is a *per-usage* test, not a per-file one: **monospace is a claim
that the glyphs matter.** Either you will read this character by character, or the digits
must not jitter. Ask that of each item and the strip sorts itself.

- **Keeps mono:** the path (l vs 1 vs I), the git ref, a tabular-nums count
- **Goes sans:** the language chip "TYPESCRIPT", the status legend "Modified"
- Reference: `D-33`, generalises `D-31`, `P6` unchanged — this is what "the jobs it earns"
  means. Tag `Θ₃`.

### Remap the prose variables

One block of `--tw-prose-*` overrides retires the entire cold-grey ramp: body, bold,
headings, links, hr, and both table hairline vars. This is the highest-leverage fix in the
whole pass — nothing else changes as much for as little.

- **Now:** Tailwind gray-200/700/900
- **New:** Parchment tokens, both themes
- Reference: §3.1, anti-pattern "cold neutrals". Tag `K`.

### Tables stop being grids

Today every cell carries a 1px box on four sides, the header takes a solid fill, and
numbers sit left-aligned with no tabular figures. §4.17 asks for the opposite: hairline
rows only, no vertical rules, an 11px caps header, numerics right-aligned.

- **Now:** full grid + header fill + 12px
- **New:** row hairlines, caps header, tabular-nums
- Reference: §4.17, 11 violations found. Tag `L`.

### Math sits in the text, not on it

KaTeX's 1.21em is already overridden to 1em — but the shorthand also carries
`line-height:1.2` and a Times-ish serif, neither of which is reset, and display math is
centred and margined *twice* by two different files. Set the leading, keep the math serif
deliberately (it's a formula, not prose), and centre it once.

- **Now:** lh 1.2, double-centred, double-margined
- **New:** lh 1.45, centred once, one margin
- Reference: the `main.css` KaTeX block. Tag `M`.

### A heading scale that steps

h3 and h4 are both 14px/600 today — the same object twice, so a four-level answer reads as
three. h4 becomes a 13px muted label, and the blockquote stops injecting the curly quotes
the plugin adds and nothing suppresses.

- **Now:** h3 = h4 = 14/600; "quotes" injected
- **New:** 18 / 15 / 13 steps; no injected quotes
- Reference: §3.2 type scale. Tag `N`.

## Unboxing the preview and the terminal

After otty.sh, and Claude Code's own review pane.

Both panels wrap their content in a card that adds nothing. The preview goes *panel →
padding → bordered rounded card → header → code*; the terminal goes *dock → gutter →
bordered rounded box* where **the box and its gutter are the same colour** — a hairline
drawn between a surface and itself. Claude Code's review pane, and otty, both do the
opposite: the content *is* the surface.

### Tabs become tabs, not chips

The preview's boxed 8px chips with a drop shadow become text tabs with a 2px accent
underline — which is what `D-07` already chose for horizontal navigation, and what the
panel quietly ignored. Labels are set in the app's own sans, the same face as every other
label in BioRouter.

- **Now:** 8px chip + border + shadow
- **New:** underline tab, sans 13px label
- Reference: design.md `D-07`. Tag `G`.

### One status strip, no card

The inner card, its border, its 8px radius and its popover shadow all come out. What's
left is a status strip — language, path, Preview/Raw — over code that sits directly on the
panel ground. Three nested boxes become none.

- **Now:** panel → p-3 → card → head → code
- **New:** panel → strip → code
- Reference: `P1` surfaces-not-elevation. Tag `H`.

### The terminal loses its frame

Today the xterm host is a `rounded-md` bordered box painted `bg-background-muted` — sitting
inside a gutter painted the same `bg-background-muted`. The border separates a colour from
itself. Drop the box; let the terminal bleed to the dock edges, with its horizontal tabs
underlined like every other horizontal tab.

- **Now:** bordered box inside same-colour gutter
- **New:** full-bleed terminal, one top hairline
- Reference: `D-07`, `D-11` terminal ground. Tag `I`.

### One glyph contract

Every icon in the shell renders at stroke 1.5 and one of two sizes (16 dense, 20 default),
`currentColor` always. The direct `lucide-react` imports that bypass the wrapper get routed
through it, so a glyph can never again be heavier than its neighbour.

- **Now:** strokes 1 / 1.5 / 1.75; sizes 16 / 18
- **New:** stroke 1.5 everywhere; 16 or 20
- Reference: §3.9 iconography. Tag `J`.

## The chat becomes tabbed — and splittable

Future-proofing, on the VS Code editor-group model. This doesn't need a new visual
language — **BioRouter's layout already *is* VS Code's layout**, and naming that makes the
rest fall out:

| VS Code | BioRouter | Consequence |
|---|---|---|
| Primary sidebar | The rail — menu + recents | already collapsible, offcanvas |
| Editor group | **Chat group** — tabs + transcript + composer | splittable into a grid |
| Tab | One chat session | drag to reorder, move, or split |
| Panel (bottom) | The terminal dock | stays global, spans all groups |
| Secondary side bar | The preview panel | follows the **active** group |

### Drop zones

> **Rendered in the HTML.** A five-cell diagram of a chat group showing the left, top,
> centre, bottom and right landing regions, with the left and right edges lit as hot zones.

Hovering a group's edge with a tab in hand tints that half in the accent at 18% with a
dashed edge — the same overlay VS Code shows. Drop in the **centre** to move the chat into
that group; drop on an **edge** to split. Hold `⌥` to duplicate instead of move.

### Tab states

All four states earn their look.

- **Preview (italic).** Single-clicking a chat in Recents opens it *italic* and reuses that
  one tab — so browsing history never leaves twelve tabs behind. Typing in it, or
  double-clicking, pins it upright. Straight from VS Code's `enablePreview`, and it is the
  exact answer to a sidebar full of one-off chats.
- **Running.** VS Code swaps the close × for a dirty dot. BioRouter already owns that
  vocabulary: a live chat shows the coral pulse where its × would be, and the × returns on
  hover. One glyph, one meaning.
- **Active group.** Only the focused group's tab carries the coral underline; an unfocused
  group's active tab keeps a neutral rule. Focus costs zero extra chrome.
- **Overflow.** Tabs shrink to a floor, then scroll, then collapse into a `▾` menu — never
  wrap. A wrapped second row moves every tab under the cursor.

### What this buys the researcher

The real use case isn't tab hoarding — it's **two chats side by side**: a SPOKE query next
to the cohort it feeds, each with its own composer, both live at once. That is why each
group keeps its own composer rather than sharing one at the bottom.

A tab in hand is the chat's own glyph plus its title — the same speech / branch / app marks
the sidebar uses, so a row dragged out of Recents and the tab it becomes are visibly the
same object.

### The header becomes the tab strip

Today's 52px header carries one session title and a `···` menu. With tabs, **the tab is the
title** — so the title row retires and the per-session menu moves to the tab's context
menu. The strip is the same 52px on the same hairline, so the top edge still reads as one
bar across sidebar, chat and preview.

- **Now:** one session title + `···` menu
- **New:** tab strip + per-group actions
- Reference: §4.12, replaces `SessionNamePill`. Tag `Y`.

### One tab, three places

This is why the tab matters beyond looks: **the chat strip, the preview panel and the
terminal dock all run the same component**. Build it once, and a fourth tabbed surface
costs nothing. Had the preview kept its boxed chips and the terminal its left-bar, the
window would ship three tab languages.

- **Now:** boxed chips · left-bar · (no chat tabs)
- **New:** one fill-ladder tab, three surfaces
- Reference: `D-07`, `G`, `I`. Tag `Z`.

### Tabs, after Safari

**Revised again in rev 5.** Rev 4 made the strip *darker* than the sidebar to float the
tabs off it — which bought legibility by putting the heaviest slab in the window along its
top edge. Safari does the opposite and it is the better trade: **the strip is the sidebar's
own colour**, so the whole 52px top edge is one continuous ground, and only the **active
tab is painted** — a floating white pill. One thing is emphasised instead of everything
being separated.

- **Rev 4:** dark strip + a block per tab
- **Rev 5:** strip = `--sidebar`; active = card pill
- Reference: Safari, `P1` surfaces. Tag `Ω`.

### The divider lives in the gap

With inactive tabs unpainted, a Safari hairline sits *between* them — in the gap, never as
a border on the tab — and **retracts around whatever you point at**, so the thing under the
cursor is never boxed in. Boundaries when you're scanning; nothing in the way when you're
aiming.

- **Rev 4:** a fill per tab to imply edges
- **Rev 5:** 1px in the gap, retracts on hover
- Reference: Safari, `D-18` hairlines. Tag `Ψ`.

### The collapse overlap — a real bug

With the sidebar collapsed, the traffic lights landed *on top of* the first tab. Cause:
`.cgroup:first-child` stopped matching the moment the drop-zone layer became the first
child of the group container, so the 172px reserve silently never applied. It is
`:first-of-type` now. Worth noting for the build: **a reserve that fails silently is worse
than no reserve** — this one should carry a test that asserts the first tab's left edge
clears the strip.

- **Bug:** `:first-child` matched nothing
- **Fix:** `:first-of-type` + assert the clearance
- Reference: `TitlebarControls.tsx`. Tag `Π`.

### The split button goes; the drag shows itself

Dragging a tab *is* the split gesture, so the button was a second way to do one thing —
cut. And the drag can't be invisible until you let go. The tab lifts under the cursor at 2°
with a popover shadow, the one it left dims to 35%, and the landing half tints in the
accent *while the tab is still in the air*. You aim, then commit — never commit, then
discover.

- **Rev 3:** split button; drop lands only on release
- **Rev 4:** drag ghost + live drop zone, no button
- Reference: §3.6 motion. Tag `∇`.

> **Rendered in the HTML.** The **✥ Mid-drag** toggle freezes the drag mid-flight so the
> lifted ghost tab, the dimmed source tab and the tinted landing half are all visible at
> once.

### The preview follows the active group

With two chats live, "which chat does the preview belong to?" has to have an answer. It
follows the **active group** — focus a group and the panel shows that chat's artifacts,
exactly as VS Code's secondary side bar tracks the focused editor. The terminal, by
contrast, stays global: it's a machine, not a conversation.

- **Now:** one chat, question doesn't arise
- **New:** preview = active group; terminal = global
- Reference: new — needs a decision. Tag `Æ`.

### What the titlebar strip owes the tabs

One real constraint falls out of `O`: with the sidebar collapsed, the control strip floats
over the canvas — so the *first* group's tab strip has to clear the same 172px reserve the
session title does today.

- **Now:** title clears 204px
- **New:** first tab clears 172px
- Reference: `TitlebarControls.tsx`. Tag `Ø`.

## Every floating thing, one surface

Popovers, menus, toasts and tooltips. There are **four different elevation recipes** in the
app for the same idea. The shared `.biorouter-popover-surface` hand-rolls a two-layer
shadow and a **44%-diluted border** — while a proper `--shadow-popover` token already
exists, correctly authored for all three themes. The hand-rolled one needed its own bespoke
Alma Mater override *precisely because* it bypasses the token. Meanwhile the diluted border
breaks `D-18`, which says hairlines are one value at full strength.

> **Rendered in the HTML.** Three live component stages — a Session review popover with its
> four metric tiles, an Extensions menu with enabled/disabled groups and toggles, and the
> toast family (two-line, title-only, danger) plus a tooltip — followed by a button row
> where the sole outline button is flagged as "the only one wearing a box".

### One surface recipe, one shadow

Every floating thing — popover, dropdown, select, mention picker, toast, tooltip — gets the
same recipe: `--background-default`, a **full-strength** 1px `--border-subtle`, 12px
radius, and the `--shadow-popover` token. Collapsing the hand-rolled shadow onto the token
deletes a whole bespoke Alma Mater override for free.

- **Now:** 4 recipes; 16px radius; 44% border
- **New:** 1 recipe; 12px; full hairline
- Reference: §4.5, `D-18` hairlines. Tag `S`.

### Metric tiles stop being tiles

The review popover puts four filled, rounded boxes inside an already-rounded box — boxes in
a box again — and sets the values at 14px semibold sans. §4.13 asks for a **30px mono-light
readout** over an 11px caps label. Drop the fills, let the numbers do the work, and the
panel reads like an instrument rather than a form. (The CODE tile is also a copy-paste of
the metric component instead of a use of it.)

- **Now:** filled tiles, 14px semibold sans
- **New:** no fill, 30/34 mono 300, tabular
- Reference: §4.13 metric tiles. Tag `T`.

### Menu rows become rows

The extensions rows are plain `<div>`s — no radius, no keyboard semantics, no focus state —
at 14px with ad-hoc padding. They become real 32px/13px menu items on `--radius-md`,
hovering to `--background-medium`, grouped under 11px caps labels instead of an underlined
text link.

- **Now:** divs, 14px, no focus ring
- **New:** 32px items, 13/18, caps group labels
- Reference: §4.5, `P4` one-control-one-look. Tag `U`.

### Put the z-scale to work

The six-stop scale exists and is almost entirely unused. `--z-dropdown` (200) is referenced
by **nothing** — every Radix surface sits at 500 — while `--z-toast` (600) is used by a
heatmap tooltip and the actual toasts sit at **9999**, outside the scale entirely.
Alongside them: `z-[1210]`, `z-[190]`, `z-[100]`, `z-50`.

- **Now:** 11 raw values; scale unused
- **New:** dropdown 200 · modal 400 · toast 600
- Reference: §3.7 z-index. Tag `V`.

### The toast already won — update the doc

§4.3 asks for a 3px left status bar. The shipped toast instead carries a **tinted icon
chip** and a neutral surface, and it is the better answer: it reads at a glance, survives
both themes, and doesn't paint a coloured stripe on a calm surface. Here the *code* should
win and the spec should change — the only real fix is the radius (12px, matching every
other floating surface) and one close-button geometry.

- **Spec:** 3px left status bar, radius 8px
- **New:** tinted chip (as shipped), radius 12px
- Reference: §4.3 — spec yields to code. Tag `W`.

### Buttons take off the box

**Revised in rev 4.** Make workflow and Diagnostics wear a 1px `--border-strong` box on a
white popover — the heaviest lines in the panel, drawn around its quietest actions. They
become **secondary**: a `--background-medium` fill, no border, hover one step to
`--background-strong`. §4.1 already specified exactly that; the popover just reached for
*outline* instead. Outline survives only for a secondary action on an already-tinted
ground.

- **Now:** outline — 1px `--border-strong` box
- **New:** secondary — fill, no border
- Reference: §4.1 buttons, the fills-not-outlines rule. Tag `Θ`.

### Toast: one geometry, two densities

A toast is sometimes *"primekgagent / Extension removed"* and sometimes just *"Extension
removed"*. Both are the same object: the icon chip and the close stay optically centred on
the **first line**, so the title-only toast is a tidy 48px bar and the two-line one grows
downward from the same top edge. Nothing re-centres, nothing jumps between the two.

- **Now:** close pinned top-4; single-line sits off-centre
- **New:** both densities, one top edge
- Reference: §4.3 toasts. Tag `Σ`.

### One close button

Three geometries are in play: the sheet complies with §4.2 (32px, `right-4 top-4`), the
dialog doesn't (`p-2`, no fixed size), and the toast invents a third (`right-2.5 top-4`,
20px). Two contexts, two sizes, no exceptions: 32px on modals, 20px optically centred on
toasts.

- **Now:** 3 geometries across 3 components
- **New:** 32px modal · 20px compact
- Reference: §4.2 close affordance. Tag `X`.

## Carried over from rev 1

The sidebar pass, unchanged.

### The wordmark gets a row of its own

**Revised in rev 3.** Rev 1 put the wordmark in a 52px band — wrong: that space belongs to
the traffic lights and the control strip. Instead the band stays the titlebar, and the
wordmark drops into a proper 32px row whose mark and text align to the *same 48px text edge
as every nav label below it*. The 40px of dead air above it goes.

- **Now:** floats at `pt-10`, 40px above / 0 below
- **New:** 32px row under the band, on the nav grid
- Reference: §4.11, §3.3. Tag `A`.

### The menu is a labelled zone

The nine destinations sit under a quiet `MENU` caption, so built-in functions read as a
group distinct from your conversations. Reference: §4.11. Tag `B`.

### Menu and history are visibly separate

A 32px hairline sliver becomes a full-width rule, and Recents gets its own inset well — one
calm step deeper, the same two-tone device the sidebar uses against the canvas. Reference:
§3.1, `P2`. Tag `C`.

### Recents retracts

The section label becomes a disclosure: one click folds the whole history away and the rail
is just the menu — with a small count so you know what's behind it. For anyone who lives in
the built-ins and doesn't want twelve conversations staring back, the sidebar goes quiet.
State persists like the sidebar's own collapse does.

- **Now:** history is always open, no way to fold
- **New:** `▾ Recents` disclosure + count badge
- Reference: §4.11. Tag `R`.

### History rows carry a glyph

A speech mark for chats, a branch mark for diverged sessions, a tile for apps — so history
stops reading as a wall of identical text. **Still an open call:** this overrides §4.11's
"no leading icon". Reference: revisits §4.11. Tag `D`.

### One type rhythm, sidebar to chat

Both already use the system stack, so the mismatch was rhythm, not family. One scale: 14/20
rows, 11px caps labels, 12px dates. Reference: §3.2, `D-06`. Tag `E`.

### Three headers, one bar

The sidebar's titlebar band, the chat header and the preview header are all 52px, each
closed by the same hairline — so the window has one continuous top edge instead of three
that nearly line up. Today the sidebar band has no hairline at all. Reference: §4.12, `P1`.
Tag `F`.

### The titlebar controls keep their strip

Collapse and New window stay exactly where they are — a floating `no-drag` strip at
`left:100px`, clear of the traffic lights. They must live outside the sidebar: inside it,
collapsing would take the un-collapse button with it. Dashboard comes out, so the strip
narrows 96→64px and the session-title reserve 204→172px — the title moves 32px left.

- **Now:** collapse · new window · dashboard
- **New:** collapse · new window
- Reference: `TitlebarControls.tsx`, §4.11. Tag `O`.

### One glyph, one meaning

The titlebar's *New window* button and the sidebar's *New Session* row both draw the same
`Plus` icon for two different actions — the same glyph promising two things. New window
takes a distinct window-with-arrow mark; Plus stays with New Session.

- **Now:** Plus = new session AND new window
- **New:** Plus = new session; `⧉` = new window
- Reference: §3.9, found while mapping the strip. Tag `P`.

## Caveat: the drift register is stale

While auditing, `design.md`'s drift register was found to be **stale**: it still lists the
terminal as having a single hardcoded light theme (`DR-41`), a bronze tab (`DR-32`) and a
font mismatch (`DR-07`). All three were fixed on main months ago; the register has no
status column, so it reads as live. Everything in this spec was verified against the code
and the running build, not the doc. The register itself needs a pass.

## How to read this

This is a faithful static sketch on the real tokens, meant to be replicated 1:1 in
`AppSidebar.tsx`, `BaseChat.tsx`, `ArtifactViewer.tsx`, `InAppTerminalDock.tsx` and
`MarkdownContent.tsx`. Nothing in the page is committed to the app — each tagged change is
offered to be kept, dropped, or tweaked individually.

## Parchment token reference

The values the mockup paints with, taken from design.md §3.1 (palette), §5.1 (syntax) and
the `InAppTerminalDock` `TERMINAL_THEMES` parchment entry. They are reproduced here so the
numbers survive outside the browser.

> **⚠ Historical — these are the mockup's numbers, not the app's.** As of **2026-08-08** the
> backgrounds, greys and borders below are no longer Parchment's: all three families share one
> neutral set (light `#ffffff` canvas / `#f4f4f2` ground / `#ecece9` medium / `#dcdcd8` strong /
> `#f7f7f5` sidebar; dark `#131312` / `#1b1b19` / `#232320` / `#2c2c29`). Parchment's identity is now
> its warm **ink** (`#2a2520` / `#f4f0e6`) and its dark-orange **accent**, both of which survive
> below unchanged. This table is left as written because it records what the mockup painted; do not
> read a value from it into code. Current values: `ui/desktop/themes/*.theme.mjs` and
> [theme system architecture §8](../theming/theme-system-architecture.md#8--shared-neutrals--one-scaffolding-three-inks).

| Token | Light | Dark |
|---|---|---|
| `--app` | `#ffffff` | `#0d0a06` |
| `--card` | `#ffffff` | `#1e1810` |
| `--ground` | `#faf8f3` | `#14110b` |
| `--medium` | `#f4f0e6` | `#2a2318` |
| `--strong` | `#e8e1d2` | `#362d1f` |
| `--focus-fill` | `#e4dcc9` | `#4d4430` |
| `--sidebar` | `#f3ede1` | `#100d08` |
| `--sidebar-hover` | `#ece4d4` | `#1c160d` |
| `--sidebar-active` | `#e4d9c3` | `#28200f` |
| `--sidebar-border` | `#e9e1d0` | `#2a2318` |
| `--accent-bar` | `#cf6d47` | `#e8895f` |
| `--accent-fill` | `#b85a32` | `#e8895f` |
| `--on-accent` | `#ffffff` | `#16120c` |
| `--text` | `#2a2520` | `#f4f0e6` |
| `--muted` | `#635c54` | `#b0a892` |
| `--subtle` | `#6e6760` | `#9c937b` |
| `--border` | `#e8e1d2` | `#282217` |
| `--border-strong` | `#d4cab6` | `#403928` |
| `--ok` | `#1f7a3d` | `#7ac87c` |
| `--info` | `#1e5fbf` | `#7aabf5` |
| `--danger` | `#b3261e` | `#f07575` |
| `--warn` | `#8a5a00` | `#d9a441` |

Syntax colours (§5.1, light on `#faf8f3`):

| Token | Light | Dark |
|---|---|---|
| `--sx-plain` | `#2a2520` | `#e8e1d2` |
| `--sx-comment` | `#6f6659` | `#8d8266` |
| `--sx-kw` | `#a94f2a` | `#e8895f` |
| `--sx-str` | `#22784f` | `#7fbf6a` |
| `--sx-num` | `#8a5a00` | `#d9a441` |
| `--sx-fn` | `#255fb5` | `#8fb8e8` |
| `--sx-type` | `#7847b8` | `#b98ad6` |
| `--sx-op` | `#6e6760` | `#b0a892` |
| `--diff-add-bg` | `rgba(31,122,61,.09)` | `rgba(122,200,124,.10)` |
| `--diff-add-bar` | `#1f7a3d` | `#7ac87c` |

Terminal colours (`InAppTerminalDock` `TERMINAL_THEMES`, parchment):

| Token | Light | Dark |
|---|---|---|
| `--term-bg` | `#faf8f3` | `#16120c` |
| `--term-fg` | `#2d2a26` | `#e8e1d2` |
| `--term-cursor` | `#b85a32` | `#e8895f` |
| `--term-green` | `#22784f` | `#7fbf6a` |
| `--term-blue` | `#255fb5` | `#6f9fd8` |
| `--term-yellow` | `#9b6818` | `#d9a441` |
| `--term-red` | `#b63f3f` | `#e2665c` |
| `--term-dim` | `#6f6659` | `#8d8266` |
| `--term-magenta` | `#7847b8` | `#b98ad6` |

Radii and shadows:

| Token | Value |
|---|---|
| `--r-sm` | `4px` |
| `--r-md` | `8px` |
| `--r-lg` | `12px` |
| `--r-xl` | `16px` |
| `--r-full` | `9999px` |
| `--shadow-composer` (light) | `0 2px 6px -1px rgba(32,25,15,.09), 0 1px 2px rgba(32,25,15,.05)` |
| `--shadow-popover` (light) | `0 8px 24px rgba(32,25,15,.10), 0 0 1px rgba(32,25,15,.14)` |
| `--shadow-window` (light) | `0 30px 70px -30px rgba(32,25,15,.35), 0 8px 24px -18px rgba(32,25,15,.22)` |
| `--shadow-composer` (dark) | `0 2px 8px -1px rgba(0,0,0,.5), 0 1px 2px rgba(0,0,0,.4)` |
| `--shadow-popover` (dark) | `0 8px 24px rgba(0,0,0,.55), 0 0 1px rgba(255,255,255,.06)` |
| `--shadow-window` (dark) | `0 30px 70px -28px rgba(0,0,0,.7)` |

The cold Tailwind defaults the app paints today, kept for the forensics comparison:
`gray-200` `#e5e7eb`, `gray-700` `#364153`, `gray-900` `#101828`.

## Related documentation

- [`ui-cohesion-redesign.html`](ui-cohesion-redesign.html) — the rendered, interactive
  version of this spec; open it in a browser for the mockups and swatches.
- [`execution-status.md`](execution-status.md) — the status record for this branch: the
  20-step list, commits, gates, and what is still open.
- [`home-screen-redesign.html`](../../history/ui-overhaul-2026-07/home-screen-redesign.html) — the sibling redesign pass for
  the home screen.
- [`knowledge-view-redesign.html`](../../history/ui-overhaul-2026-07/knowledge-view-redesign.html) — the sibling redesign
  pass for the Knowledge view.
- [`design.md`](../../../design.md) — the design system itself: the Parchment palette, the
  `D-NN` decision register, and the `DR-NN` drift register this spec references.
- [Documentation index](../../README.md) — the top-level map of the `docs/` tree.
