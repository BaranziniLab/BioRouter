# Knowledge browser harness

> **What this is.** A standalone Vite page that mounts the **real** Knowledge-section
> React components in a real browser, against checked-in fixture JSON, with no Electron
> and no running `biorouterd`.
> **Status:** Current.
> **Audience:** anyone changing `ui/desktop/src/components/knowledge/**` or the primitives it uses.

## Run it

```bash
cd ui/desktop
npx vite --config .knowledge-harness/vite.config.mts --port 5200
```

Then open <http://localhost:5200/>, or drive it with the agent-browser MCP tools.
Nothing else is required — the fixtures live in `fixtures/` next to this file.

## Why it exists

`npm run test:run` runs in jsdom, which has **no layout engine, no canvas, does not run
Tailwind, and does not evaluate `:has()` or `:focus-visible`**. Three real defects in this
section were invisible to the 2700-test suite and visible here in seconds:

| Defect                                                                                                                                                      | Why jsdom could not see it                                                                                                                                               |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| The KB picker's keyboard highlight parked on "Manage bases…" while the base list was still in flight, so Enter as the first keystroke opened the manager    | Needs a genuinely asynchronous list plus a real `aria-activedescendant` read. Reproducible in jsdom once you know to look — but nobody looked, because nothing rendered. |
| The graph canvas caching its label ink at mount                                                                                                             | jsdom has no canvas and no computed `color` to resolve from.                                                                                                             |
| `KBSelectorTrigger` having no visible focus indicator, because a Tailwind `bg-*` utility beats the design system's `:where(button):focus-visible` base rule | jsdom does not evaluate `:focus-visible` and never runs Tailwind, so it has no cascade layers to get wrong.                                                              |

The **`ui-spec.md` §2.2 / §7.6** requirement that a browser harness is "not severable" from
this work is what this file discharges.

## What it mounts

Pick a surface from the left rail. Every one of them is the shipping component:

| Rail entry            | Component                                     |
| --------------------- | --------------------------------------------- |
| Knowledge view        | `KnowledgeView` — the whole section shell     |
| KB picker (open)      | `KBSelectorTrigger` with its popover open     |
| KB manager dialog     | `KBManagerDialog`                             |
| Sources rail (ingest) | `SourcesRail` → `KbTierPanel` + `IngestPanel` |
| Change log drawer     | `ChangeLogDrawer`                             |
| Graph panel           | `KnowledgeGraphPanel` → `ForceGraphCanvas`    |

`ThemeProvider`, `ConfigProvider`, `ModelAndProviderProvider` and `KnowledgeProvider` are
the real ones too. **Only two boundaries are faked**: the Electron IPC bridge
(`window.electron`) and `fetch`, which is routed to the fixtures in `main.tsx`'s `ROUTES`
table. An unstubbed request logs `[knowledge-harness] unstubbed <METHOD> <path>` and
answers 501 rather than letting the dev server hand back `index.html` and produce a JSON
parse error three frames later.

## Runtime controls

- **Theme family** — parchment / alma-mater / roche-limit, via the real `useTheme()`
  setter, so `data-theme` moves on `<html>` exactly as it does in the app.
- **Mode** — light / dark, likewise via the real setter (`.dark` on `<html>`).
  ⚠ Switch these **without reloading**. A live toggle is what catches anything that
  resolved a theme value once at mount and cached it — the canvas ink bug's shape.
- **slow base list** — delays `GET /knowledge/bases` by 600 ms. This is not decoration:
  the picker's highlight defect only exists while the collection is in flight, so a
  harness that answers instantly would certify the bug as absent.
- **remount surface** — remounts the surface (and its `KnowledgeProvider`) without
  reloading the page, so the live theme survives.

## Fixtures

`fixtures/` is checked in so the harness runs standalone.

| File             | Serves                                                                                                                              |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `bases.json`     | `GET /knowledge/bases` — four bases, two private, one with a default model                                                          |
| `graph.json`     | `GET /knowledge/bases/{id}/graph` — 15 nodes / 17 edges covering every `PageKind`, three credibility tiers and one retracted source |
| `history.json`   | `GET /knowledge/bases/{id}/history` — one entry per `ChangeKind`                                                                    |
| `pages.json`     | `GET /knowledge/bases/{id}/page` — page bodies with `[[knowledge-link]]` markers                                                    |
| `providers.json` | `GET /config/providers` — three configured providers with curated `known_models`, so the ingest model picker has a populated list   |

## Conventions

- **The harness shell uses inline styles, deliberately.** Tailwind v4 does not scan
  dot-directories, so a utility class written in `main.tsx` would never be generated and
  the chrome would render unstyled. Everything inside a mounted surface still gets its
  classes from `src/`, via `@source '../src'` in `harness.css`.
- `.knowledge-harness/` is outside `tsconfig.json`'s `include`, ESLint's glob and
  vitest's `include`, so it is never type-checked, linted or collected as a test —
  the same arrangement as `.artifact-harness/`.

## Related documentation

- [`.artifact-harness/`](../.artifact-harness/) — the precedent this is built on.
- `docs/knowledge-base/okf-migration/ui-spec.md` §2.2, §5.11, §7.6.
- `src/styles/focusSurface.test.ts` and `src/styles/composerFocus.test.ts` — why a focus
  rule is authored CSS rather than a Tailwind variant, and why it is asserted at the source.
