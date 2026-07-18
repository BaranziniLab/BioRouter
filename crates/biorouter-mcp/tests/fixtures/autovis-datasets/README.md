# Auto Visualiser — tool-selection fixtures

Ten synthetic datasets for checking that a model picks the **right** Auto
Visualiser tool from a raw file, with **no tool named in the prompt**.

This is deliberately not what the unit tests in
`crates/biorouter-mcp/src/autovisualiser/` cover. Those assert "given these
args, `render_volcano` emits correct HTML" — they exercise the renderer. These
fixtures exercise the *tool descriptions*: give the agent a file path, ask it to
visualize the contents, and see whether it reads the file, infers the domain,
and reaches for the correct one of the 34 tools. A tool description that
degrades — so the model stops choosing `render_kaplan_meier` for survival data,
or `render_heatmap` for a gene × sample matrix — is invisible to the unit tests
and caught here.

## How to run

Point a session at one of these files and ask it to visualize the data without
naming a tool, e.g.:

> Read `<repo>/crates/biorouter-mcp/tests/fixtures/autovis-datasets/09_survival_km.csv`
> and visualize it.

Then check the tool it chose against the **Expected viz** column below. Run it
under the GUI (`just run-ui`) or the CLI; see `.claude/skills/debug-app` for a
scripted driver. This is a manual/agent-driven probe, not a `cargo test` — there
is no automated harness wired to it.

## Baseline

The table below is the recorded result of the original phase-3 run
(`mimo-v2.5-pro`, 2026-06-19): **10/10 correct.** Treat it as the regression
baseline — the tool descriptions were sufficient for the model to pick correctly
in every case, so any future miss is a signal, not noise.

| # | File | What it is / intended for | Expected viz | Biorouter output | Satisfied |
|---|------|---------------------------|--------------|------------------|-----------|
| 1 | `01_scatter_gene_expression.csv` | BRCA1 vs TP53 expression across 40 samples, tumor/normal | Scatter colored by condition | `show_chart` scatter, **two series colored by tumor/normal**, axes labeled, legend | ✅ |
| 2 | `02_bar_drug_efficacy.csv` | Efficacy % of 5 drugs | Bar chart | `show_chart` bar, **sorted ascending**, axes labeled | ✅ |
| 3 | `03_map_trial_sites.csv` | 6 trial sites lat/lng + enrollment | Map with markers | `render_map`, US map, clustered markers, auto-fit, correct dimensions | ✅ |
| 4 | `04_sankey_patient_flow.json` | Trial patient flow (links only, no node list) | Sankey | `render_sankey` "Patient Flow Through Clinical Trial"; **derived node list from links** | ✅ |
| 5 | `05_network_ppi_small.json` | Small PPI graph (`edges`/`id` keys) | Network | `render_network`; mapped `edges`→links, **enriched node groups + direction** | ✅ |
| 6 | `06_network_regulatory_large.json` | 22-node regulatory net w/ groups & weights | Network (complex) | `render_network` "Gene Regulatory Network"; color by group, arrows, sized nodes, weighted edges | ✅ |
| 7 | `07_heatmap_expression_matrix.csv` | 6 genes × 6 samples matrix | Heatmap | `render_heatmap`, diverging color scale + legend, row/col labels | ✅ |
| 8 | `08_timeseries_biomarkers.csv` | CRP & IL6 over 6 months | Line chart | `show_chart` line, two series, axes labeled, legend | ✅ |
| 9 | `09_survival_km.csv` | Survival over time, Treatment vs Control | Kaplan–Meier | `render_kaplan_meier`, proper step curves, % axis, Treatment correctly above Control | ✅ |
| 10 | `10_volcano_differential_expression.csv` | 8 genes: log2FC & -log10 p | Volcano | `render_volcano`, threshold lines, up=red / down=blue / non-sig=grey | ✅ |

## Notable intelligence observed
- **Inference beyond the raw file:** colored the scatter by an unrequested
  condition column; sorted bars; enriched a bare PPI graph with functional
  groups and edge direction; derived Sankey nodes from links alone.
- **Correct domain mapping:** survival→Kaplan–Meier, DE table→volcano,
  matrix→heatmap, flows→Sankey, interactions→force-directed network — all from
  the file contents, with no tool named in the prompt.

## Baseline run method (2026-06-19)
- App launched under Playwright (tmux `gui` driver), absolute file paths.
- Model: `mimo-v2.5-pro` — the model that exposed the stringified-`data` bug.
- Each visualization screenshotted via the gui driver for visual inspection.

## Fixes this run drove (all shipped)
- `de_flexible` — every tool accepts `data` as an object **or** stringified JSON
  (mimo sends a string). Now in `autovisualiser/common.rs`, used across all tool
  modules.
- `render_map` — reports its size to the iframe + `invalidateSize()`, fixing
  dimensions. Now in `templates/map_template.html`.
- Removed the "MCP UI is experimental" note under inline visualizations.
