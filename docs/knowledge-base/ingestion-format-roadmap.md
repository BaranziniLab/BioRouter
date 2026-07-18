# Knowledge Ingestion Modalities — Expansion Plan

**Date:** 2026-06-10
**Status:** Phases 1, 2, 3, and 4.1 implemented and verified 2026-06-10
(xlsx/xls/ods via calamine; pptx via in-house `convert/pptx.rs` with speaker
notes; PDF primary upgraded to pdf-inspector with the legacy chain as
fallback; scanned-PDF quality banner in source.md). Eval on real two-column
papers (BERT, Attention): pdf-inspector 4–35× faster than pdf-extract with
20 headings + 100+ table rows recovered vs zero, no content loss. Remaining:
Phase 4.2/4.3 (vision-LLM fallback, optional Docling sidecar) and Phase 5.
**Scope:** Expand knowledge-base ingestion beyond the current PDF/HTML/DOCX/CSV set to PowerPoint (.pptx), Excel (.xlsx/.xls/.ods), and a higher-fidelity PDF pipeline — based on a survey of the current architecture and a comparison of open-source conversion tools.

---

## 1. Current architecture (as built)

The ingestion pipeline is **conversion-then-digestion**:

```
Frontend (Dropzone / paste / URL)
  → POST /knowledge/bases/{id}/ingest   (multipart or JSON, SSE response)
    → KnowledgeService::add_raw_source
        → convert::convert()             ← deterministic format → markdown
        → raw/{source-id}/{original.*, source.md, meta.yaml}  (immutable, git-committed)
    → ingest macro sub-agent (bounded LLM loop, kb_* tools, txn branch)
        → knowledge/ wiki pages + [[links]] + log.md          (one atomic commit)
```

Key property: **conversion quality bounds digestion quality.** The sub-agent
only ever sees `source.md`; whatever the converter destroys (tables, reading
order, slide notes) is unrecoverable downstream.

### Conversion dispatch today

`crates/biorouter-mcp/src/knowledge/convert/mod.rs` (`convert()`, `normalize_mime()`, `guess_mime()`):

| Format | Converter | Crate(s) | Fidelity notes |
|---|---|---|---|
| PDF | `pdf.rs` | `pdf-extract` → `lopdf` → `python3 pdfminer` subprocess → lossy content-op scan | Text-layer only. No OCR, no tables, no headings, no multi-column reading order. |
| HTML | `html.rs` | `htmd` | Good. |
| DOCX | `docx.rs` | `docx-rs` | Text + basic structure. |
| CSV | `csv.rs` | `csv` | Markdown table. 8 MB cap. |
| MD / TXT | passthrough | — | — |
| URL | `url_fetch.rs` | `reqwest` → re-dispatch | — |

### Known gaps found during this review

1. **`needs_llm_fallback` is dead code.** `pdf.rs:15` sets it for scanned PDFs
   (< 32 chars extracted), `Converted` carries it, and *nothing consumes it* —
   no UI warning, no alternate path. Scanned PDFs silently ingest as empty.
2. **`.xls` is mis-mapped.** `normalize_mime()` maps
   `application/vnd.ms-excel` → `text/csv` (`mod.rs:69`), so a real legacy
   Excel binary would be fed to the CSV parser and produce garbage.
3. **`.pptx`/`.xlsx` are blocked at three layers:** frontend extension
   allowlist (`fileValidation.ts:153`), `guess_mime()` falls through to
   `text/plain`, and `convert()` bails with `unsupported mime`.
4. **`umya-spreadsheet` 2.2.3 is already a workspace dependency** (used by
   `computercontroller/xlsx_tool.rs`) — XLSX support has zero-new-dependency
   options.

Lint (`macros/lint.rs`: deterministic scan + optional sub-agent autofix),
query (`macros/query.rs`: BM25 `kb_search` + page reads, optional file-as-page),
and the CLI (`biorouter knowledge ingest|query|lint|...`, MCP-free, calls the
same macros) all operate downstream of `source.md` and need **no changes** for
new modalities — that is the payoff of the convert-then-digest design.

---

## 2. Tool research summary

Full agent research reports (June 2026 data). License flags matter because
BioRouter dmgs/zips are distributed artifacts.

### 2.1 PDF → Markdown

| Tool | Lang | License | Weight | Quality vs `pdf-extract` |
|---|---|---|---|---|
| **pdf-inspector** (Firecrawl) | Rust, MIT | crate, no models | ~150 ms/doc | Headings (font-size), lists, tables (dual heuristic), **multi-column reading order**, bold/italic, plus a 10–50 ms text-vs-scanned classifier. Best lightweight upgrade. |
| **pdf_oxide** | Rust, MIT/Apache-2 | crate | very fast | Similar feature set, claims 5× faster than pdf-extract; young (Nov 2025), single author, 0.3.x churn — A/B candidate, not primary. |
| **Docling** (IBM/LF AI) | Python, **MIT** | sidecar; ~100s of MB models, CPU-capable | 2–6 s/page CPU | Layout ML + TableFormer + pluggable OCR + formula recognition. Best *cleanly licensed* heavyweight. `docling-serve` HTTP API fits biorouterd's architecture. |
| MinerU | Python, custom (Apache-base) | ~20 GB | — | OmniDocBench pipeline leader (tables, CJK, formulas). Too heavy + non-standard license. |
| Marker / Surya | Python, **GPL-3 + restricted weights** (<$2M commercial cap) | GBs, GPU-preferring | — | olmOCR-bench best pipeline (76.1) but license disqualifies distribution. |
| olmOCR | Python, Apache-2 | 7B VLM, ≥12 GB VRAM | — | Top open scores (82.4); cloud-scale only. |
| PyMuPDF4LLM / mutool | **AGPL** | — | — | Best no-ML quality, license disqualifies. |
| MarkItDown (PDF path) | Python, MIT | — | — | pdfminer plain text — **no better than what we have**. |
| GROBID (CRF image) | Java, Apache-2 | ~470 MB Docker, CPU | 2.5–10 PDF/s | Not a markdown converter; best-in-class scholarly **reference/metadata** extraction (TEI XML). Niche future add-on for biomedical citations. |
| extractous | Rust+GraalVM Tika | Apache-2 | large native libs | Plain text out, stale 18 mo. Pass. |

Benchmarks (olmOCR-bench, OmniDocBench): GPU VLMs > Marker/MinerU > Docling >
heuristic tools > plain text extraction. None of the published benchmarks
cover the young Rust crates — we need a small internal eval.

**Honest delta vs current method:** for digitally-born papers, body *words*
mostly survive today; the real losses are (a) multi-column reading order
(pdf-extract interleaves columns — actively corrupts meaning), (b) tables
(destroyed; the single biggest faithfulness loss for biomedical results),
(c) headings (lost structure the sub-agent could use to split pages),
(d) scanned PDFs (currently 0% — common in older biomedical literature).
pdf-inspector recovers roughly the first three at zero weight; only the
Docling/OCR tier recovers scanned docs and complex tables/equations.

### 2.2 XLSX → Markdown

- **calamine** (2.3k★, MIT, pure Rust, active): the clear winner. Reads
  **xlsx/xlsm/xlsb/xls + ods**; deps are `zip` + `quick-xml` (already in
  tree); `worksheet_range()` returns **cached computed values** (what an LLM
  wants — no formula recalculation problem), `worksheet_formula()` available
  separately; lazy sheet loading, ~1.1M cells/s. Markdown glue is ~60–80
  lines (same shape as `csv.rs`); prior art: `madato` crate (copy the
  pattern, skip the dependency).
- umya-spreadsheet (already in tree): editing-oriented, heavier object model,
  slower reads, xlsx-only. Fine fallback; calamine is better *and* fixes
  `.xls`/`.ods` for free.
- markitdown/pandas sidecar: output essentially identical to the calamine
  path — not worth a Python runtime for this format. (pandoc **cannot** read
  xlsx at all.)

### 2.3 PPTX → Markdown

The sparse one — no Rust crate currently extracts **speaker notes**:

- **pptx-to-md** (MIT/Apache-2, deps already ~all in tree): slide text,
  lists, tables, images; **no speaker notes**, document-order only, 5★
  single-author — vendor/fork material, not a load-bearing dependency.
- **undoc** (MIT, pure Rust, May 2026): pptx + xlsx + docx → md, tables,
  slide markers; notes undocumented; 20★, fast churn — re-evaluate in 6 months.
- **markitdown** (150k★, MIT, Python): the quality gold standard — titles,
  tables, charts, **speaker notes**, image alt-text, (top,left)
  reading-order sort. Cost: Python runtime + python-pptx; viable only as an
  optional subprocess/sidecar, not bundled.
- pandoc: pptx is *writer-only* (issues #4252, #7621) — confirmed no help.
- **In-house extractor feasibility: high.** PPTX = zip of XML. Slide text:
  `ppt/slides/slide{N}.xml` (`<a:t>` runs in `<a:p>` in `<p:txBody>`);
  tables: `<a:tbl>/<a:tr>/<a:tc>`; **speaker notes:
  `ppt/notesSlides/notesSlide{N}.xml`** (resolve via slide `_rels`); slide
  order: `ppt/presentation.xml` `sldIdLst`. ~300–500 lines with `zip` +
  `quick-xml` (both already deps) — and it's the only pure-Rust route to
  speaker notes, which for lab-meeting/conference decks often carry more
  knowledge than the slides.

Known limits of any pure-Rust pptx path: SmartArt text (`diagrams/data{N}.xml`),
chart data series, grouped/rotated shape ordering. Acceptable: the sub-agent
digests content, it doesn't reproduce layout.

---

## 3. Plan

Guiding principles:
- **Pure-Rust, in-process converters as the always-works default** (matches
  pdf-extract/htmd/docx-rs/csv precedent; nothing new for users to install).
- **Optional high-fidelity sidecar later**, never bundled Python.
- Every converter returns `Converted { markdown, title, mime, needs_llm_fallback }`
  and is exercised by fixture tests under `convert/fixtures/`.

### Phase 1 — XLSX/XLS/ODS via calamine (small, ships first)

1. Add `calamine` to `biorouter-mcp/Cargo.toml`.
2. New `convert/xlsx.rs`: per sheet → `## <sheet name>` + markdown table from
   `worksheet_range()` (cached values); escape `|`; skip empty sheets; cap
   output (e.g. 500 rows or ~200k cells per sheet, then
   `> … truncated: N more rows`) using `range.get_size()`; title from filename.
3. Dispatch in `convert/mod.rs`:
   - `application/vnd.openxmlformats-officedocument.spreadsheetml.sheet` → xlsx
   - **fix the `.xls` bug**: route `application/vnd.ms-excel` to calamine
     (it reads legacy xls) instead of the CSV parser; keep `text/csv` → csv.rs
   - `application/vnd.oasis.opendocument.spreadsheet` → calamine ods
   - extend `guess_mime()` for `.xlsx/.xlsm/.xls/.ods`.
4. Frontend: add extensions to `fileValidation.ts` (with a size cap — reuse
   the 8 MB CSV-class limit for spreadsheets) and to the Dropzone label;
   mirror the cap in `validate_ingest_upload()` (`routes/knowledge.rs:74`).
5. Tests: fixtures (multi-sheet xlsx, xls, ods, formula workbook → cached
   values, huge-sheet truncation) in `convert/fixtures/`; run
   `cargo test -p biorouter-mcp --lib knowledge::`.

No HTTP route shape changes ⇒ no OpenAPI regen needed (re-check if any
request fields are added).

### Phase 2 — PPTX via in-house extractor (zip + quick-xml)

1. New `convert/pptx.rs` (~400 lines):
   - slide order from `presentation.xml` + rels;
   - per slide: `# Slide N — <title placeholder>`, paragraphs/lists from
     text bodies, `<a:tbl>` → markdown tables;
   - **speaker notes** from `notesSlides/` rels → `> **Notes:** …` block;
   - skip images/SmartArt with an explicit `*[figure omitted]*` marker so
     the sub-agent knows content was elided (candidate for the Phase 4
     vision path).
   - Reference implementations: `pptx-to-md` crate (vendorable, compatible
     license) and markitdown's `_pptx_converter.py` (reading-order sort).
2. Dispatch + `guess_mime()` for `.pptx`
   (`application/vnd.openxmlformats-officedocument.presentationml.presentation`);
   legacy `.ppt` → explicit error "please re-save as .pptx" (no viable
   pure-Rust OLE2 reader today; litchi is the crate to watch).
3. Frontend + server validation as in Phase 1 (25 MB cap).
4. Fixture tests: titled deck, deck with tables, deck with notes, empty deck.

### Phase 3 — PDF upgrade to structured markdown (pdf-inspector)

1. Internal eval first (cheap and decisive): ~25 PubMed Central PDFs
   (multi-column, results tables, references) through current chain vs
   `pdf-inspector` vs `pdf_oxide`; score heading/table/reading-order
   retention; reuse the lint macro or a one-off harness for judging.
2. Adopt the winner (expected: pdf-inspector, MIT) as **primary** in
   `extract_pdf_text()`'s primary slot; keep the existing
   pdf-extract → lopdf → pdfminer → lossy chain as fallback (the
   `extract_pdf_text_with` plumbing already supports exactly this).
3. Use its text-vs-scanned classifier to set `needs_llm_fallback`
   deterministically instead of the `< 32 chars` heuristic.

### Phase 4 — make `needs_llm_fallback` real (scanned PDFs, figures)

Today the flag is produced and dropped. In order of effort:

1. **Surface it**: when set, emit an SSE warning event during ingest and
   prepend a frontmatter note to `source.md`
   (`extraction_quality: poor — scanned or image-based`), so users and the
   sub-agent stop silently ingesting empty sources.
2. **Vision-LLM fallback**: BioRouter already routes multimodal providers —
   rasterize pages (`pdfium-render`, MIT-licensed bindings over BSD Pdfium)
   and have the existing completer transcribe page images to markdown,
   bounded by page count. This handles scanned biomedical PDFs with zero new
   user-visible dependencies beyond the bundled pdfium dylib.
3. **Optional Docling sidecar** (config-gated, user-installed
   `docling-serve` URL in config.yaml): when configured, scanned/complex
   PDFs POST to it for OCR + TableFormer-grade conversion. Clean MIT
   license; never bundled. GROBID (Apache-2, CPU CRF image) is a further
   optional add-on for reference/metadata extraction if citation-graph
   features materialize.

### Phase 5 — cheap long-tail formats (opportunistic)

- `.epub`: zip + existing `htmd` (~50 lines).
- `.rtf`: `rtf-parser` crate (pure Rust) → text.
- `.odp`: same zip+XML approach as pptx if demand appears.
- Plain images (`.png/.jpg` of figures/posters): vision-LLM path from
  Phase 4.2 makes this nearly free.

### Acceptance criteria

- `.xlsx/.xls/.ods/.pptx` drag-and-drop ingests end-to-end (GUI dropzone,
  `biorouter knowledge ingest --file`, URL path) with faithful tables,
  sheet names, and speaker notes in `raw/*/source.md`.
- No regression in the existing ~122 `knowledge::` tests + ~19 route tests.
- Scanned-PDF ingestion either produces real text (Phase 4) or a visible
  quality warning — never a silent empty source.
- All new dependencies MIT/Apache-2; no Python or model downloads in the
  default install.
