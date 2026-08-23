# Preview panel expansion plan

> **What this is.** The plan to widen the artifact side panel along five axes — image formats,
> Office documents, live websites, an annotation channel into the chat, and agent access to what
> the panel is showing. Decision records, five workstreams, the security analysis, and how each
> piece is tested.
> **Status:** Proposed — awaiting approval. Nothing here has shipped. Branch
> `design/preview-panel-expansion`, worktree `biorouter-preview-panel-wt`.
> **Audience:** the reviewer approving this, and whoever executes it afterwards.

Read the [current-state survey](current-state.md) first, or at least §1 below. The panel is
substantially more capable than the brief assumed, and the plan is smaller and differently shaped
as a result.

---

## 1. What the measurement changed

Five things were asked for. Measured against the code, they are not five equal pieces of work.

| # | The ask | What is actually true |
|---|---|---|
| 1 | More image formats | Six work today. **Three more already decode in Chromium and are excluded for no reason.** Two more (HEIC, TIFF) genuinely need a decoder — and the Rust crates for TIFF are already in the dependency tree. |
| 2 | View-only Word / Excel / PowerPoint | **Already shipped.** All four of PDF, DOCX, XLSX and PPTX render today via bundled offline renderers. The real work is the *gap* around them: legacy `.doc`/`.xls`/`.ppt`, OpenDocument, and RTF. |
| 3 | Browse a real website | **Genuinely absent, and deliberately so.** Frame navigation is a closed set of two literals with a test asserting an `https` URL is refused. This is the one item that changes the security model. |
| 4 | Annotation → chat | Genuinely absent. No selection, range, crop or overlay code exists. But every delivery mechanism it needs already exists. |
| 5 | Agent access to the panel | Genuinely absent. But the channel it needs already exists and already does blocking round-trips. |

Two of the five are mostly done. Two are new features on existing plumbing. **One — live
websites — is a real architectural decision**, and most of this document is about it.

### Three facts I verified rather than assumed

These are load-bearing, so they were measured rather than researched.

**Half of real sites cannot be framed, including the example in the brief.** Measured with
`curl` on 2026-08-23:

| Refuses framing | Allows framing |
|---|---|
| `google.com` (`SAMEORIGIN`) — the brief's own example | `ucsf.edu` |
| `pubmed.ncbi.nlm.nih.gov` (`DENY`) | `uniprot.org` |
| `ncbi.nlm.nih.gov/gene` (`SAMEORIGIN`) | `useast.ensembl.org` |
| `clinicaltrials.gov` (`SAMEORIGIN`) | `genome.jp/kegg` |
| `nature.com` (`DENY` + `frame-ancestors 'none'`) | `gnomad.broadinstitute.org` |

The refusers are exactly the destinations this audience uses most. An `<iframe>` implementation
would appear to work — `ucsf.edu` renders fine — and fail on PubMed. That is worse than not
shipping it.

**`capturePage(rect)` composites sandboxed iframes.** Verified against the exact shipping
Electron (39.8.10 darwin-arm64, extracted from the local cache): a host page painted white with a
`sandbox="allow-scripts"` `srcdoc` iframe painting lime returned `255,255,255` at a host pixel and
`0,255,0` at an iframe pixel; a rect capture returned exactly the requested box. This matters
because the artifact preview is sandboxed *without* `allow-same-origin`, so `html2canvas` and
every DOM-walking screenshot library structurally cannot see into it. **One main-process
primitive serves both the annotation crop and the agent screenshot, and no screenshot library is
needed at all.**

**`avif`, `bmp` and `ico` already decode.** Verified by loading a real file of each format.
Chromium 142 (which Electron 39 ships) has exactly eight raster decoders; `heic` and `tiff` are
not among them and `jxl` is flag-gated until Chrome 145. ⚠️ My own probe initially reported a
JPEG XL pass — ImageMagick had silently written a PNG when asked for `.jxl`, and `file(1)`
caught it. Two of seventeen rows were wrong before that check.

---

## 2. Decision records

### PP-01 — The panel stays the single display surface

Nothing here adds a second renderer, a second CSP, or a second action channel.
[`artifact-display-surfaces.md`](../artifact-display-surfaces.md) records what the last one cost:
two policies that failed independently, a prompt bar whose Send was a *truthy* no-op, two
read-only surfaces giving different answers to the same guest action, and a fabricated session
id. Live web content gets its own *container* (PP-02) but it is still presented in, and owned by,
the one panel.

### PP-02 — Live web content never travels through the artifact iframe

It gets a `WebContentsView` in its own session partition, with no preload.

Three independent reasons, any one sufficient:

1. **It would not work.** Half of real sites refuse framing (§1), including the brief's example.
2. **It would reverse a ruling.** `isAllowedArtifactFrameNavigation` is `about:srcdoc` or
   `about:blank` and nothing else, and `permissionPolicy.test.ts:48` asserts
   `https://example.test/exfiltrate` is `false`. That guard exists to stop agent-authored HTML
   phoning home. It stays exactly as it is.
3. **The policies are opposites.** The artifact CSP is `default-src 'none'; connect-src 'none'`.
   A live website needs the network by definition. Loosening the artifact policy to admit
   websites would loosen it for every figure too.

**The cost is real and must be accepted knowingly.** A native view paints above the DOM, so it
does not respect stacking, scrolling, border radius or modals. VS Code itemizes the bill in its
own source — placeholder-screenshot masking during show/hide swaps, an overlay-pause system that
hardcodes a list of overlay class names with hit-test exclusions, a focus dance between the
workbench DOM and the view, native key-event forwarding, and a pixel-snap layout contribution —
and notes that an in-DOM iframe "would need none of the above." We are choosing the expensive
renderer because the cheap one does not work, not because it is nicer.

Concretely for this app: the view must be hidden whenever a modal, the tab-overflow dropdown, a
toast or the artifact resize shield is up, and its bounds must track the panel through resize and
the overlay/side rung switch in `useArtifactPanel`.

### PP-03 — Browsing is a user capability; the agent's access to it is separate, visible and revocable

This is the single most valuable pattern in the competitive research, and both mature
implementations landed on it independently: Codex has `in_app_browser` (a pane a person uses) and
`browser_use` (an agent capability) as separate flags with separate policy; VS Code's tabs are
private by default and the agent cannot read one until the user picks **Share with Agent**.

For us that means:

- **The agent cannot open a live page.** Today an MCP resource *link* with an `http(s)` URI
  becomes an artifact through Path A with no transcript card at all, and can auto-open. If
  in-panel rendering were simply wired onto the existing `externalUrl` kind, any MCP server could
  cause an arbitrary site to load and execute in the user's app. So `externalUrl` **keeps
  rendering its card**, and gains an explicit "Open here" control. One deliberate click is the
  whole boundary, and it makes agent-initiated navigation structurally impossible.
- **The agent cannot read a live page** until the user shares that tab. Sharing is per-tab,
  visible on the tab, and revocable.

### PP-04 — Per-origin policy, with turn-scoped approval

Modelled on Codex's `requirements.toml`: separate axes (`access`, `downloads`, `uploads`) rather
than one on/off, and `access_approval_lifetime = "turn" | "thread"` so an approval can expire.

And stated with the honesty Cursor's own docs use: an origin allowlist is **best-effort**, not a
boundary — link navigation, redirects and `window.location` from an allowed origin all defeat it.
That is the same "privacy is safety, not security" line this repo already takes. Do not describe
it as a boundary in the UI.

### PP-05 — The agent reads the panel over the existing `WorkspaceBridge`

Not a new channel. `/ui/workspace` is the renderer's only WebSocket to the daemon, it is
session-routable (`bridge_for_session`), and it already does blocking round-trips
(`emit_and_wait`, 10 s). `workspace_list` already reads GUI state this way.

Copy `WorkspaceBridge`, **not** `UiBridge`: the former fuses the generation counter with the
sender under one lock; the latter splits them and has a documented, 200/200-reproducible severing
race.

Two changes are needed on the renderer side and none on the daemon: the inbound command handler
is **synchronous** and its reply is `{ok, detail: string}` only. A capture is inherently async and
needs a wider envelope. The daemon already resolves with the whole frame, so extra reply fields
survive today.

### PP-06 — Two observation channels, with an explicit division of labour

Text for acting, pixels for judging. VS Code encodes this in the tool descriptions themselves —
`read_page` says *"This is better than screenshot"*, and `screenshot_page` says *"You can't
perform actions based on the screenshot"*. Replit reached the same conclusion independently and
priced it: a DOM+ARIA snapshot per step costs about $0.20 per session against about $0.50 for a
*single* form under vision.

So the default is text. The screenshot is for "does this look right", which is exactly what the
brief asks for and exactly what text cannot answer.

### PP-07 — Pixels go out of band; the echo frame carries only a descriptor

`MAX_INBOUND_FRAME_BYTES` is 128 KiB and its own comment explains why: the stored echo is handed
to the model verbatim, making it both a memory sink and an injection vector. A base64 PNG does
not belong there. The capture is written to the temp dir and the frame carries the path.

The panel descriptor — what kind of artifact, its title, its path or origin — is small and *is*
pushed in the existing echo, so "what is on screen?" costs no round-trip at all.

### PP-08 — `capturePage(rect)` is the only capture primitive

Verified in §1. No `html2canvas`, no `html-to-image`, no `modern-screenshot`. Those libraries
re-render the DOM, often get it wrong, and structurally cannot enter the sandboxed frame that
holds most of what is worth capturing.

### PP-09 — An annotation crop reuses the existing image attachment path, unchanged

`canvas → saveDataUrlToTemp → { path, kind: 'image' } → createUserMessage → { type: 'image',
data, mimeType }`. **No schema change on either side, and no Rust change.** The existing caps
apply and are the right ones: 4 MiB data-URL string, 3 MiB decoded, 5 images per message, and a
MIME allowlist of `png|jpeg|jpg|gif|webp`. Crops are emitted as PNG, which is on that list.

### PP-10 — A text annotation reuses the `<biorouter-ref>` chip grammar

Issue #65 already solved "attach a non-prose payload to a message and draw it as a chip", and
solved it by putting the payload **inside the message string** — because draft save/restore, the
`?prompt=` deep link, the queue, steering and local history each key off that one string, "and
each one forgotten is a reference the user attached and the agent never sees." A new `RefKind`
inherits all of that. A parallel mechanism would have to re-earn it.

Selections are clamped like `RENDER_ERROR_TEXT_LIMIT` (2,000 chars). There is no cap on composer
text today, and an unbounded selection from a large document would otherwise go straight into the
prompt.

### PP-11 — Annotation works on every preview kind, not just the browser

Codex shipped "Comment with Codex" in its browser but not in its Markdown/HTML preview pane, and
the open issue against it names our exact case: users read a generated report and cannot comment
on it. For a research tool the report *is* the artifact. Annotation is defined once against the
panel, and each preview kind supplies a locator.

### PP-12 — Freeze the frame before annotating

Cursor: *"the annotation sits over a frozen frame of the viewport, so the agent sees the exact
page state you were responding to."* On an animated figure or a live page, an annotation over a
moving target describes something the agent will never see. The freeze is a `capturePage` taken
at the moment the mode is entered; the overlay is drawn on that image.

### PP-13 — The picker overlay lives in a shadow root at maximum z-index

Replit does this so the previewed app's CSS can never clobber the picker. This codebase has
already been bitten by the general form of that problem — Prism's unprefixed `token` classes
colliding with Tailwind's `.table` utility — so the precedent is local as well as external.

### PP-14 — Formats Chromium already decodes are a list change, not a feature

`avif`, `bmp`, `ico` (plus `apng`, `cur`, `jfif`, `pjpeg` for completeness). Zero dependencies,
zero bytes. Four lists must be edited together or the format half-works; two adjacent lists
(`ItemIcon`, `MentionPopover`) already *offer* `bmp`/`tiff`/`ico` the panel cannot render, so this
also closes a pre-existing inconsistency.

`jxl` is **not** added: Chromium 142 has no JPEG XL, and it is flag-gated even in 145.

### PP-15 — TIFF decodes in Rust with crates already in the tree; HEIC uses a separately-loaded WASM

`image` and `tiff` are already dependencies, both MIT/Apache-2.0. The `tiff` crate supports
BigTIFF and real multi-page (`seek_to_image`/`next_image`/`more_images`) — which is what
microscopy needs. ⚠️ Use the `tiff` crate **directly**: `image`'s `TiffDecoder` exposes only the
first IFD. Honest gaps to document rather than paper over: no CCITT fax, no JPEG 2000, no LERC.

HEIC is the Mac-user pain point (every iPhone photo). Decoding needs only `libheif` + `libde265`,
both **LGPL** — `x265` is GPL but encode-only, and we do not encode. Ship `libheif-js` loading
its `.wasm` as a **separate file**, not the inlined bundle: that separability is what satisfies
LGPL cleanly, and it avoids dragging a system `libheif` through the notarized macOS build and two
Docker cross-compiles.

Two traps to avoid, both verified: `heic2any` declares MIT on npm but bundles LGPL `libheif`; and
`sharp` deliberately excludes HEIC from its prebuilts, with rebuilds baking absolute Homebrew
paths into the `.node` that break after packaging.

### PP-16 — Large images move from data URLs to blob URLs

The panel builds `data:image/…;base64,…` in the main process, so an image costs about 4/3 of its
size as a JS string, twice. Chromium struggles with data URLs past a couple of MB for IPC
reasons; blob URLs handle 100 MB. Also enforce a 32,767 px downsample ceiling — past it Chromium
renders **blank**, which a converted whole-slide TIFF will hit.

### PP-17 — Privacy: one genuinely new channel, one that only looks new

- **The agent screenshotting the panel is not a new exposure class.** A public-capable session
  with `developer__shell` can already read the same file. That is §9.5's general filesystem
  read-deny, explicitly **descoped for v1** (DR-14). The screenshot tool sits behind Gate C like
  every other tool and should be documented as the same known gap, not dressed up as novel.
- **A live browser *is* new: it is a network egress surface inside the app.** The asymmetry this
  repo already uses applies exactly — *a user may proceed past a warning; the agent never does the
  same thing automatically.* A user browsing in a private-tier session gets a disclosure and
  proceeds. **The agent navigating that session's browser to an arbitrary URL is an exfiltration
  channel and is refused.** PP-03 already makes agent-initiated navigation impossible; this is the
  reason it must stay impossible rather than becoming a permission.

### PP-18 — Never `postMessage(payload, '*')`, and always check `event.source`

bolt.diy posts element data with `targetOrigin: '*'` in both directions and never validates
origin on receipt. Replit checks inbound but still broadcasts source paths and a PNG of the UI
outbound to whatever frames the app. Both are five-line fixes they did not make.

The existing gate is already correct and must be reused: `ArtifactViewer` accepts a `message`
only when `event.source === trustedFrameRef.current?.contentWindow`, because an untrusted frame
"would otherwise be able to inject instructions into a session that holds shell and file tools."

---

## 3. The five workstreams

### A — Image formats

| Step | Change | Cost |
|---|---|---|
| A1 | Add `avif`, `bmp`, `ico`, `apng`, `cur`, `jfif`, `pjpeg` to all four lists that must agree | 4 files, no deps |
| A2 | Reconcile `ItemIcon` and `MentionPopover` with the panel's list | pre-existing bug |
| A3 | Blob URLs for images over a threshold; 32,767 px ceiling | main process |
| A4 | TIFF → PNG in Rust via the `tiff` crate, multi-page aware | crates already present |
| A5 | HEIC via `libheif-js`, `.wasm` as a separate file, lazy-loaded | ~6.4 MB, LGPL notice |
| A6 | EXIF orientation via `kamadak-exif` for anything we rasterize ourselves | BSD-2 |

Deferred with reasons, not silently: DICOM (`dwv` is **GPL-3.0** and disqualifying; Cornerstone3D
is the MIT alternative if demand appears), NIfTI, OME-TIFF/`viv`, RAW (LibRaw is LGPL/CDDL and
`libraw-wasm`'s ISC label covers only the wrapper), EPS/PostScript (needs **AGPL** Ghostscript —
ImageMagick's own maintainer refused it on exactly these grounds), FITS.

### B — Office documents: gap-fill only

PDF, DOCX, XLSX and PPTX already render. This workstream is about what happens at the edges:

| Step | Change |
|---|---|
| B1 | Audit the four shipped renderers against real files — tables, images, embedded charts, multi-sheet workbooks, speaker notes. Fix or document what breaks. |
| B2 | Decide legacy `.doc`/`.xls`/`.ppt`: these are a different binary format entirely. `calamine` reads `.xls`. `.doc` and `.ppt` have no viable path — decline explicitly with a clear card. |
| B3 | OpenDocument `.odt`/`.ods`/`.odp` and `.rtf` — decide, don't drift. `calamine` covers `.ods`. |
| B4 | An unsupported document must say *which* format and *why*, not "can't be previewed here". |

**The Rust converters are not the answer here** and reusing them would be a downgrade: `docx.rs`
matches only paragraphs so tables and images silently vanish, and `pptx.rs` writes the literal
string `*[image omitted]*`. They exist to feed an LLM cheaply and deliberately discard appearance.

### C — Live websites

| Step | Change |
|---|---|
| C1 | A `WebContentsView` host bound to the panel's geometry, its own partition, no preload |
| C2 | Hide/show against modals, dropdowns, toasts and the resize shield (the itemized bill in PP-02) |
| C3 | Minimal chrome: back, forward, reload, URL, loading and failure states |
| C4 | Deny every permission by default; `setWindowOpenHandler`; block `file:` and `127.0.0.1` from that partition |
| C5 | Per-origin policy with turn-scoped approval (PP-04) |
| C6 | The "Open here" control on the existing `externalUrl` card — the only way a live page loads |
| C7 | Tighten `frame-src` in both CSP sources once nothing needs remote framing |

C7 is worth calling out: `frame-src 'self' blob: https: http:` is in both CSPs today and **nothing
uses the remote part**. It is a latent hole that this workstream should close rather than inherit.

### D — Annotation

| Step | Change |
|---|---|
| D1 | An annotate mode on the panel: freeze the frame (PP-12), overlay in a shadow root (PP-13) |
| D2 | Region select → `capturePage(rect)` → temp file → existing image attachment (PP-09) |
| D3 | Text select → clamped payload → a new `<biorouter-ref>` kind → composer chip (PP-10) |
| D4 | Per-preview-kind locators: file path + line for text, page + rect for PDF, cell for a sheet, URL + selector for a live page |
| D5 | Batch: stay in the mode, accumulate, send together |

**The payload format follows VS Code's**, which is the best template found: labelled markdown
sections — element, URL, ancestor path, outer HTML, dimensions, computed CSS — not a JSON blob
concatenated onto the message, which is what bolt.diy does with the same information and far worse
ergonomics.

One UX rule from three separate Codex bug reports: **the annotation composer's Enter key must
agree with the chat composer's.** Theirs disagree and it is their most-complained-about detail.

### E — Agent access to the panel

| Step | Change |
|---|---|
| E1 | Extend `buildEchoFrame` with a small panel descriptor — free, no round-trip, already debounced |
| E2 | Widen the renderer's command handler to async and its reply beyond `{ok, detail}` (PP-05) |
| E3 | `panel_read` — text/structure of what is displayed. The default. |
| E4 | `panel_capture` — `capturePage(rect)` → temp file → `Content::image(base64, "image/png")` |
| E5 | Tool descriptions that encode the division of labour (PP-06), in the tools themselves |

The image return shape already exists and is copied verbatim from `screen_capture`:
`Content::text(note).with_audience(vec![Role::Assistant])` alongside
`Content::image(data, "image/png").with_priority(0.0)`. Note `with_audience([Assistant])` means the
note reaches the model but not the transcript.

If these hang off the `workspace` platform extension, four edits are needed and three are
test-guarded: `get_tools()`, the match arm, `WORKSPACE_TOOL_NAMES` in `agent.rs`, and
`workspace_parity.rs`. `PLATFORM_EXTENSIONS.len() == 6` is asserted, so adding a *new* extension
fails that test by design.

---

## 4. Sequencing

Ordered so that each phase ships something usable and the risky phase comes after the cheap wins.

| Phase | Contents | Why here |
|---|---|---|
| **1** | A1, A2, A3 | Hours, no dependencies, fixes real files today |
| **2** | B1–B4 | Audit before extending; may prove B2/B3 unnecessary |
| **3** | E1–E5 | Independent of C and D; the channel exists |
| **4** | A4, A5, A6 | New decoders, contained blast radius |
| **5** | C1–C7 | The architectural change; needs its own review |
| **6** | D1–D5 | Wants C in place so the browser is one of its surfaces |

Phase 5 is the one to stop and re-review before merging. Phases 1–4 are additive to a surface
that already works.

---

## 5. Testing

The existing suite is **330 files / 3,224 tests, green** on this branch at `edfd6325`; the
artifact subset is 7 files / 197 tests. That is the baseline every phase is measured against.

**Three coverage holes to close first**, because they are the surfaces being changed: no test
renders `ArtifactViewer`'s `image` branch, its `binary` branch, or its `document` branch, and
there is no `useArtifactPanel.test.ts` at all.

**What jsdom cannot see, and what covers it instead.** jsdom has no layout engine, never runs
Tailwind, does not evaluate `@container`, and does not implement `canvas.getContext('2d')`. So:

| Concern | Instrument |
|---|---|
| Render branches, list agreement, payload shapes, policy predicates | vitest |
| Panel geometry, the native-view overlay bill (PP-02), picker overlay | `ui/desktop/.artifact-harness/` in a real browser |
| Format decode support | a real file per format loaded in the shipping Electron — **not** a synthesized one; my own probe was wrong twice until `file(1)` checked it |
| `capturePage` behaviour | the shipping Electron binary, as in §1 |
| Live-site framing and policy | measured against real origins, re-measured periodically |

**Mutation-test every new guard.** The repo has a documented history of tests that pass against
the bug they were written for. A guard that does not fail when its fix is reverted is not a guard.

---

## 6. Risks and open questions

| Risk | Assessment |
|---|---|
| The native view's overlay bill (PP-02) is larger than estimated | Highest-confidence risk. VS Code itemized it from experience. Mitigation: build C1+C2 first and evaluate before C3–C7. |
| An origin allowlist is mistaken for a boundary | Documented honestly in the UI, per PP-04. |
| HEIC's LGPL obligation | Manageable — separate `.wasm` file. Worth a second pair of eyes before merge. |
| `libheif-js` adds ~6.4 MB | Lazy-loaded on first `.heic` only. |
| Widening the panel's reach outruns its guards | `.biorouterignore` is **not** consulted on this path and fully-automatic mode admits any non-sensitive path. Any widening should state whether that is still acceptable. |

**Questions for the reviewer:**

1. **Should live browsing be gated behind a setting, default off?** PP-03 makes agent-initiated
   navigation impossible, but the feature still puts a full browser in a clinical-data
   application. I lean toward shipping it on, with the private-tier disclosure from PP-17.
2. **B2/B3 — how far into legacy and OpenDocument formats?** `.xls` and `.ods` are nearly free via
   `calamine`. `.doc` and `.ppt` have no viable path and I would decline them explicitly.
3. **Is DICOM in scope?** Deferred here, and the obvious library is GPL-3.0 and disqualifying. A
   biomedical audience may want it enough to justify Cornerstone3D.
4. **Phase 5 review gate** — should the live-browser work land as its own PR with a separate
   security review, rather than inside this branch?

---

## 7. What this amends

- [`artifact-display-surfaces.md`](../artifact-display-surfaces.md) gains a paragraph: the panel is
  still the one surface, but it now hosts a native view as well as DOM previews, and the
  "if you are about to add a second surface" warning should name that explicitly.
- [`current-state.md`](current-state.md) becomes the *before* picture once any phase lands and
  should be re-measured, not edited from memory.

## Related documentation

- [The preview panel as it stands today](current-state.md) — the measured evidence base for every claim above.
- [Where a generated artifact is displayed](../artifact-display-surfaces.md) — the one-surface rule and what the last second renderer cost.
- [How an Auto Visualiser figure's libraries reach the renderer](../artifact-cdn-assets.md) — why displayed content never reaches the network today.
- [Privacy tiers](../../security/privacy-tiers.md) — the capability/classification lattices and the gates PP-17 reasons about.
- [Renderer testing traps](../renderer-testing-traps.md) — why a frontend test passes while the code it covers is broken.
