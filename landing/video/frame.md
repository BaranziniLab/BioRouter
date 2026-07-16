---
format: 1920x1080
fps: 30
project: Biorouter motion design system
message: "One integrated research environment for biomedical discovery."
tone: "warm, confident, scientific — calm motion, never frantic; editorial not flashy"
palette:
  bg: "#faf8f3"        # warm cream — primary canvas / app surface
  bg_2: "#ffffff"      # pure white — cards, panels
  bg_3: "#f4f0e6"      # neutral-100 — soft hover / chips
  bg_4: "#ece5cf"      # neutral-200 — strong hover
  ink: "#2a2520"       # warm near-black — primary text
  ink_2: "#7a736c"     # muted secondary text
  ink_3: "#b6ae93"     # light tertiary text
  accent: "#cf6d47"    # coral — the Biorouter brand accent
  accent_hover: "#ba6240"
  accent_soft: "rgba(207,109,71,0.10)"
  accent_soft2: "rgba(207,109,71,0.18)"
  ucsf: "#052049"      # UCSF navy — institutional / secondary accent
  ucsf_hover: "#083570"
  border: "rgba(40,30,15,0.07)"
  border_2: "rgba(40,30,15,0.12)"
typography:
  display: "-apple-system, 'Helvetica Neue', Arial, sans-serif — weight 700"
  body: "-apple-system, 'Helvetica Neue', Arial, sans-serif — weight 400/600"
  mono: "'SF Mono', Menlo, Consolas, 'Courier New', monospace"
radii:
  sm: "6px"
  lg: "12px"
  xl: "16px"
shadow:
  card: "0 1px 3px rgba(0,0,0,0.06), 0 0 1px rgba(0,0,0,0.12)"
  lg: "0 8px 24px rgba(32,25,15,0.10), 0 0 1px rgba(0,0,0,0.15)"
---

# Biorouter Motion Design Spec

This file is the **single source of visual truth** for every Biorouter video.
It mirrors the landing site's design system (`shared.css` `:root`) so motion
assets feel native to the site rather than bolted on. Read this before
authoring or editing any composition under `video/`. Do **not** invent colors,
fonts, or pacing — adjust the tokens here and let compositions inherit them.

## 1. Brand truth (do not drift)

- **The canvas is warm cream `#faf8f3`, never pure black/white full-bleed.**
  Pure white (`#ffffff`) is reserved for cards/panels that sit *on* the cream.
- **Coral `#cf6d47` is the only brand accent.** Use it for emphasis, active
  states, the caret, progress, and the result of the BAAM formula. Use UCSF
  navy `#052049` only for the institutional / secure-models story.
- **Text is warm near-black `#2a2520`, muted is `#7a736c`.** Never use #000.
- The feel is **anti-slop, editorial, scientific-warm** — generous whitespace,
  soft shadows (`shadow.lg`), 16px card radii, no gradients, no neon glows.

## 2. Canvas & safe area

- Master format: **1920×1080, 30 fps** (landscape). The BAAM motif may also be
  exported as a transparent WebM overlay.
- **Safe margins:** keep all essential text/UI inside a 120px margin (a
  1680×840 safe box). The hero plays in a rounded card on the page, so edges
  may be visually clipped by `border-radius`.
- Every composition root is `1920×1080` with `overflow:hidden` and the cream bg.

## 3. Typography scale (px @ 1080p)

| Role            | Size | Weight | Color        |
|-----------------|------|--------|--------------|
| Hero display    | 112  | 700    | ink          |
| Scene title     | 64   | 700    | ink          |
| Subtitle / lede | 36   | 400    | ink_2        |
| Eyebrow / label | 22   | 700    | accent (uppercase, +2px letter-spacing) |
| UI body         | 26   | 400/600| ink / ink_2  |
| Caption rail    | 30   | 600    | ink on bg_2  |
| Mono / code     | 24   | 400    | ink_2        |

System font stack only (`-apple-system, 'Helvetica Neue', Arial`) — matches the
site and avoids render-time webfont flicker (a determinism hazard).

## 4. Motion language

- **Calm and deliberate.** Default tween `0.6–0.9s`, ease `power3.out` for
  entrances, `power2.inOut` for moves. Nothing snaps; nothing bounces hard.
- **Enter:** fade + 40px rise (`opacity 0→1, y 40→0`). **Stagger** lists/grids
  at `0.08–0.12s`.
- **Scene transitions:** a clean cut on the track, with the incoming scene's
  content rising in over `0.5s`. Hold the resolved end state for ≥0.4s before
  the next cut (the final frame of a clip is captured).
- **Accent reveals:** the coral underline/chip wipes in via `scaleX 0→1`
  (transform-origin left), `0.5s power2.out`.
- **The caret** blinks via a finite GSAP repeat (never `repeat:-1` at render;
  compute `repeat = floor(duration/1.1)-1`).
- **Allowed properties only:** `opacity, x, y, scale, rotation, color,
  backgroundColor, borderRadius`. Never animate `width/height/top/left/display`.

## 5. Reusable scene grammar

Compositions are authored **monolithic** (one HTML file, one paused GSAP
timeline, scenes as non-overlapping `.clip` sections on track 1) for render
robustness. Each scene follows the same grammar:

- **Eyebrow** (accent, uppercase) → **scene title** (ink) → one supporting line.
- A **device card** (`.device`) = white panel, `radius.xl`, `shadow.lg`,
  optional 38px window bar with three macOS dots. This is the Biorouter window.
- **Chips** (`.chip`) for extensions/skills/models: `bg_3`, `radius.lg`,
  `ink_2`; active chip = `accent_soft` bg + accent text.
- **The composable formula** is the signature motif:
  `Extensions + Skills + Knowledge + Model = Workflow`.

## 6. The six product pillars (chapter order)

Used by the hero (#1) and the chaptered tour (#3), in this order:

1. **Home** — sessions, tokens, recent chats; the calm dashboard.
2. **Chat** — reasoning + live tool calls against data.
3. **Dashboard mode** — many agents as folded cards on a canvas.
4. **Knowledge** — ingest → LLM-maintained KB + force graph.
5. **Workflows** — portable Extensions+Skills+Knowledge+Model, federated.
6. **Models** — Local → Institutional (secure UCSF) → Commercial routing.

## 7. Compositions in this project

| File              | Deliverable | Length | Purpose                                   |
|-------------------|-------------|--------|-------------------------------------------|
| `index.html`      | #1 Hero     | ~32s   | "Biorouter in motion" overview montage    |
| `tour.html`       | #3 Tour     | ~48s   | Chaptered product walkthrough (6 pillars) |
| `baam.html`       | #4 BAAM     | ~14s   | Composable-agent formula motion graphic   |
| `release.html`    | #2 Release  | ~18s   | Variable-driven "What's new" generator    |

Render targets land in `../assets/videos/` as `.mp4` + `.webm` + a poster
`.jpg`. See `video/README.md` for the regeneration commands.

## 8. Output & accessibility

- Export **MP4 (H.264)** for compatibility + **WebM (VP9)** for size; both
  referenced in `<video>` with the MP4 last as the broad fallback.
- Always generate a **poster JPG** (first meaningful frame) so the page paints
  instantly before the video loads.
- Hero/tour autoplay **muted + loop + playsinline**; provide a visible caption
  layer in-frame (no audio is assumed). Respect `prefers-reduced-motion` on the
  page by falling back to the poster.
