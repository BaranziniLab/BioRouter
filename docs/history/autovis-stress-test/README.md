# Auto Visualiser stress test — 100 combined-visualization requests

> **What this is.** The scenario specification for a 100-request stress test of the Auto
> Visualiser's `render_dashboard` composite-report tool: for each run, the figures it should
> produce, the exact user prompt, the check criteria, and a follow-up prompt that mutates the
> report. It doubles as the index for this folder.
> **Status:** Historical record — the run was executed against the dev desktop app driven by
> GPT-5.5 and completed 100/100 on 2026-07-11. Six scenarios (#19, #34, #48, #78, #84, #90) were
> swapped mid-run for tool diversity, so those entries below describe the *revised* figures and
> carry a **Plan swap** line; the executed outcome of every run is in
> [run-results.md](run-results.md). Treat this file as a reusable prompt corpus, not as work
> awaiting execution.
> **Audience:** maintainers of the Auto Visualiser extension.

Each scenario drives BioRouter in a **separate chat** to produce **one combined artifact** — a
single `render_dashboard` report of **2–3 figures that tell a cohesive story** — rather than a
handful of loose figures. The point of the corpus is breadth: it spans ten subject-matter batches
and reaches for every figure tool the extension registers, so a regression in any one renderer, in
the report's asset de-duplication, or in the model's willingness to reach for `render_dashboard`
shows up somewhere in the 100.

For every run the harness records whether it produced one dashboard artifact, the panel count,
per-figure render success (a `<canvas>` or `<svg>` present, no error card), whether the report has
a title, a summary and per-figure captions, and process notes (hiccups, trial-and-error,
back-and-forth, inconsistencies, inefficiencies, vulnerabilities, issues).

## Files in this folder

| File | What it holds |
|---|---|
| `README.md` (this file) | The up-front scenario specification for all 100 runs, plus the pass criteria, the batch index and the tool-coverage index. |
| [run-results.md](run-results.md) | The per-run results log for all 100 scenarios, the 100/100 final verdict, the tool-coverage evidence, the double-`render_dashboard` rate, and the infrastructure findings the run surfaced. |
| [hardening-log.md](hardening-log.md) | The fix applied after each batch (Batches 1–4), including the withdrawn server-side idempotency guard and the two platform bugs the run exposed. |

> **Note.** This file and `run-results.md` disagree about several scenarios: the six plan swaps
> were recorded in `run-results.md` as they happened and only later reflected here. Where the two
> differ on what a scenario asked for, `run-results.md` records what was actually executed.

## Pass criteria

A run passes when, per visualization:

- Exactly one `ui://dashboard/*` artifact opens in the side panel.
- It contains the expected number of panels.
- Every panel renders a real figure — a `<canvas>` or `<svg>`, or an SVG diagram — with **zero**
  `.panel-error` cards.
- The report carries a title, a summary, and a caption under each figure.
- The continuing prompt then mutates it and it re-renders cleanly.

Tool-coverage target: all 33 figure tools plus `render_dashboard` exercised ≥2× across the 100.

## How to read a scenario entry

Every numbered entry uses the same four fields, plus a fifth on the six swapped scenarios:

| Field | Meaning |
|---|---|
| **Figures** | The figures the report should contain, each naming the Auto Visualiser tool expected to draw it. |
| **Prompt** | The exact text sent as the first message of a fresh chat. Numeric datasets are inlined here deliberately — pre-loading concrete numbers stops the model asking a clarification round for data it should not fabricate. |
| **Check** | What the verifier looks for inside the rendered report: panel count, per-figure shape, captions. |
| **Continue** | The follow-up message that mutates the report; it must re-render cleanly. |
| **Plan swap** | Present only on #19, #34, #48, #78, #84 and #90 — records the figure this scenario originally asked for and why it was changed mid-run. |

## Batch index

| Batch | Theme | Scenarios |
|---|---|---|
| 1 | Genomics & transcriptomics | 1–10 |
| 2 | Clinical trials & survival | 11–20 |
| 3 | Epidemiology, public health & geography | 21–30 |
| 4 | Single-cell, proteomics & multi-omics | 31–40 |
| 5 | Neuroscience, imaging & physiology | 41–50 |
| 6 | Environment, climate & ecology | 51–60 |
| 7 | Business, finance & operations | 61–70 |
| 8 | Social science, demographics & education | 71–80 |
| 9 | Workflows, systems & diagrams | 81–90 |
| 10 | Physics, chemistry, engineering & misc | 91–100 |

## Tool-coverage index

Derived from the **Figures** line of every scenario below, so it reflects the plan after the six
mid-run swaps. Use it to find which runs exercise a given renderer.

| Tool | Runs | Scenarios |
|---|---|---|
| `render_area` | 10 | #7, #15, #24, #33, #49, #51, #56, #57, #61, #100 |
| `render_boxplot` | 8 | #4, #13, #28, #43, #54, #65, #82, #93 |
| `render_bubble` | 2 | #9, #63 |
| `render_calendar_heatmap` | 4 | #16, #26, #55, #79 |
| `render_chord` | 3 | #36, #42, #76 |
| `render_choropleth` | 4 | #22, #27, #52, #72 |
| `render_class_diagram` | 2 | #70, #89 |
| `render_dendrogram` | 2 | #8, #48 |
| `render_donut` | 14 | #3, #10, #18, #25, #38, #44, #52, #57, #63, #67, #72, #79, #83, #87 |
| `render_er_diagram` | 2 | #84, #85 |
| `render_flowchart` | 3 | #1, #81, #82 |
| `render_forest` | 4 | #2, #11, #18, #27 |
| `render_gantt` | 2 | #66, #86 |
| `render_gauge` | 5 | #14, #23, #29, #50, #60 |
| `render_heatmap` | 11 | #3, #8, #17, #32, #36, #39, #41, #58, #68, #96, #97 |
| `render_histogram` | 7 | #5, #10, #19, #37, #43, #46, #74 |
| `render_kaplan_meier` | 2 | #11, #19 |
| `render_manhattan` | 2 | #2, #34 |
| `render_map` | 5 | #23, #55, #59, #77, #90 |
| `render_mindmap` | 2 | #86, #88 |
| `render_network` | 5 | #6, #25, #37, #42, #80 |
| `render_radar` | 6 | #20, #32, #47, #74, #94, #100 |
| `render_sankey` | 3 | #21, #35, #65 |
| `render_sequence` | 2 | #84, #90 |
| `render_state_diagram` | 3 | #30, #62, #83 |
| `render_sunburst` | 2 | #33, #88 |
| `render_timeline` | 4 | #59, #66, #80, #87 |
| `render_treemap` | 5 | #12, #29, #56, #64, #78 |
| `render_volcano` | 3 | #1, #5, #34 |
| `render_wordcloud` | 2 | #73, #78 |
| `show_chart` | 64 | #1, #3, #4, #6, #7, #9, #10, #12, #13, #14, #15, #16, #17, #20, #21, #22, #24, #26, #28, #29, #30, #31, #35, #38, #39, #40, #41, #44, #45, #46, #47, #48, #49, #50, #51, #53, #54, #58, #60, #61, #62, #64, #67, #68, #69, #70, #71, #73, #75, #76, #77, #81, #85, #89, #91, #92, #93, #94, #95, #96, #97, #98, #99, #100 |
| `render_dashboard` | 100 | every scenario — it is the container the other tools render into. |

Two notes on the index:

- It names **31** distinct single-figure tools, each appearing ≥2×. The generic `render_mermaid`
  tool is never named directly; the eight typed Mermaid wrappers (`render_flowchart`,
  `render_gantt`, `render_sequence`, `render_mindmap`, `render_timeline`, `render_er_diagram`,
  `render_state_diagram`, `render_class_diagram`) are used instead. The coverage target above says
  33 figure tools; treat the table as the verified figure.
- 31 single-figure tools plus `render_dashboard` sit inside the 34 tools the extension registers —
  see [the Auto Visualiser extension guide](../../extensions/built-in/auto-visualiser.md).

## Batch 1 — Genomics & transcriptomics (1–10)

### 1. Tumour vs normal RNA-seq overview
- **Figures:** library-size bar (`show_chart`), volcano (`render_volcano`), pipeline flow (`render_flowchart`)
- **Prompt:** "I ran a tumour-vs-normal RNA-seq study on 6 samples. Library sizes (M reads): S1 31, S2 30, S3 33, S4 29, S5 32, S6 30. Top DE genes: MYC log2FC 2.4 (-log10p 4.0), TP53 -1.8 (2.7), CDK4 1.4 (2.9), RB1 -1.1 (1.5). Pipeline: FASTQ → STAR → featureCounts → DESeq2 → report. Please give me ONE combined report with quality-control and results sections and a caption for every figure."
- **Check:** 3 panels; bar has 6 bars; volcano shows 4 points with threshold lines; flowchart renders 5 nodes; summary + captions present.
- **Continue:** "Add a fourth figure: a donut of DE direction (2 up-regulated, 2 down-regulated), and move it into the results section."

### 2. GWAS of a complex trait
- **Figures:** manhattan (`render_manhattan`), effect-size forest (`render_forest`)
- **Prompt:** "Visualise a GWAS for type-2-diabetes risk as one report: a Manhattan plot across chromosomes 1–8 with a few genome-wide-significant peaks (e.g. chr3 -log10p 12, chr8 9.5, chr6 8.1), and a forest plot of the odds ratios for the top 5 lead SNPs (ORs 1.35 [1.2–1.5], 1.22, 0.78, 1.18, 1.41). Give it a title, a summary, and a caption per figure."
- **Check:** 2 panels; manhattan shows chromosome-coloured points crossing a significance line; forest shows 5 rows with CIs and a reference line at OR=1; captions present.
- **Continue:** "Raise the significance threshold line to 7.3 and re-render, and add one sentence to the summary about how many peaks survive."

### 3. Variant landscape of a cancer cohort
- **Figures:** mutated-gene bar (`show_chart`), variant-type donut (`render_donut`), gene–sample heatmap (`render_heatmap`)
- **Prompt:** "For a 20-patient cancer cohort, build ONE report showing: (1) the 6 most frequently mutated genes by patient count (TP53 14, KRAS 9, PIK3CA 7, APC 6, SMAD4 4, BRAF 3), (2) the proportion of variant types (missense 55%, nonsense 18%, frameshift 15%, splice 8%, silent 4%), and (3) a small mutation heatmap of 6 genes × 8 patients (0/1 mutated). Title, summary, per-figure captions."
- **Check:** 3 panels; bar 6 categories; donut 5 slices; heatmap 6×8 grid; captions present.
- **Continue:** "Sort the mutated-gene bar descending and recolour the donut so 'silent' is muted grey."

### 4. Single-gene expression across conditions
- **Figures:** grouped bar (`show_chart`), boxplot (`render_boxplot`)
- **Prompt:** "Show how gene BRCA1 behaves across 3 conditions (control, drug-A, drug-B) as one combined report: a bar chart of mean expression (control 5.1, drug-A 7.8, drug-B 3.2) and a boxplot of the per-replicate spread for each condition (control ~4.5–5.8, drug-A ~7–8.6, drug-B ~2.7–3.9). Add a title, summary and captions."
- **Check:** 2 panels; bar 3 groups; boxplot 3 boxes with whiskers; captions.
- **Continue:** "Add a horizontal reference note in the summary that drug-B is significantly down and flag it."

### 5. Differential methylation summary
- **Figures:** histogram of beta-diffs (`render_histogram`), volcano (`render_volcano`)
- **Prompt:** "Summarise a differential-methylation analysis as one report: a histogram of the distribution of beta-value differences across ~20 CpGs (values roughly -0.4 to 0.5) and a volcano plot of methylation change vs significance for 8 CpGs. Title, summary, captions."
- **Check:** 2 panels; histogram auto-binned; volcano 8 points; captions.
- **Continue:** "Increase the histogram bin count to 12 and add a caption note about the hypermethylation tail."

### 6. Pathway enrichment story
- **Figures:** enrichment bar (`show_chart`), gene–pathway network (`render_network`)
- **Prompt:** "Build ONE report for a pathway-enrichment result: a bar chart of -log10 FDR for the top 6 enriched pathways (Cell cycle 8.2, DNA repair 6.5, Apoptosis 5.1, p53 signalling 4.7, MAPK 3.9, Immune response 3.1) and a small network linking 5 hub genes to 3 of those pathways. Title, summary, captions."
- **Check:** 2 panels; bar 6; network with nodes+edges (force-directed); captions.
- **Continue:** "Make the network node size reflect degree and add a legend note in its caption."

### 7. Expression time-course
- **Figures:** multi-series line (`show_chart`), area (`render_area`)
- **Prompt:** "Show a gene-expression time course (0,2,4,8,12,24 h) for 3 genes as one report: a line chart of the three trajectories and a stacked area chart of their relative composition over time. Give it a title, a summary and captions."
- **Check:** 2 panels; line 3 series over 6 timepoints; area stacked; captions.
- **Continue:** "Add markers at each timepoint on the line chart and note the peak time for each gene in the summary."

### 8. Phylogeny & conservation
- **Figures:** dendrogram (`render_dendrogram`), conservation heatmap (`render_heatmap`)
- **Prompt:** "Present a small phylogenetic story as one report: a dendrogram clustering 6 species by a marker gene, and a heatmap of pairwise sequence identity (6×6, values 0.6–1.0). Title, summary, captions."
- **Check:** 2 panels; dendrogram tree SVG; heatmap 6×6 with diagonal=1; captions.
- **Continue:** "Reorder the heatmap to match the dendrogram leaf order and mention the two closest species in the summary."

### 9. CRISPR screen hits
- **Figures:** ranked bar of gene scores (`show_chart`), bubble of effect vs confidence (`render_bubble`)
- **Prompt:** "Visualise a CRISPR knockout screen as one report: a ranked bar chart of the top/bottom 8 gene depletion scores, and a bubble chart where x = log fold-change, y = -log10 p, bubble size = number of guides, for 10 genes. Title, summary, captions."
- **Check:** 2 panels; bar 8; bubble 10 sized points; captions.
- **Continue:** "Colour depleted vs enriched genes differently in the bubble chart."

### 10. Sequencing QC dashboard
- **Figures:** per-base quality line (`show_chart`), GC histogram (`render_histogram`), read-status donut (`render_donut`)
- **Prompt:** "Build ONE FASTQ quality-control report with three figures: mean per-base Phred quality across 20 cycles (declining from ~38 to ~28), a histogram of per-read GC content (%), and a donut of read outcomes (passed 88%, adapter-trimmed 7%, too-short 3%, failed 2%). Title, summary, captions."
- **Check:** 3 panels; line 20 pts; histogram; donut 4 slices; captions.
- **Continue:** "Add a red reference band note where quality drops below 30 and call it out in the summary."

## Batch 2 — Clinical trials & survival (11–20)

### 11. Two-arm survival trial
- **Figures:** Kaplan–Meier (`render_kaplan_meier`), forest of subgroup HRs (`render_forest`)
- **Prompt:** "Report a two-arm oncology trial as one artifact: Kaplan–Meier survival curves for treatment vs control (treatment median ~24 mo, control ~15 mo, with censoring), and a forest plot of hazard ratios across 5 subgroups (age, sex, stage, biomarker+, biomarker-). Title, summary, captions."
- **Check:** 2 panels; KM two step curves; forest 5 rows with CI and HR=1 reference; captions.
- **Continue:** "Add a median-survival annotation to each KM arm and note the overall HR in the summary."

### 12. Adverse-event profile
- **Figures:** AE bar by grade (`show_chart`), organ-system treemap (`render_treemap`)
- **Prompt:** "Summarise adverse events for a trial as one report: a stacked/grouped bar of the 6 most common AEs by CTCAE grade, and a treemap of AE counts grouped by organ system. Title, summary, captions."
- **Check:** 2 panels; bar 6 AEs; treemap hierarchical boxes; captions.
- **Continue:** "Highlight grade 3+ events and add their total to the summary."

### 13. Biomarker vs response
- **Figures:** scatter with trend (`show_chart` scatter), boxplot by responder status (`render_boxplot`)
- **Prompt:** "Show a biomarker-response story as one report: a scatter of baseline biomarker vs tumour shrinkage (%), and a boxplot of the biomarker split by responders vs non-responders. Title, summary, captions."
- **Check:** 2 panels; scatter points; boxplot 2 groups; captions.
- **Continue:** "Add a fitted trend note to the scatter caption and flag the responder median gap."

### 14. Dose-escalation
- **Figures:** dose-response line (`show_chart`), gauge of MTD utilisation (`render_gauge`)
- **Prompt:** "Report a phase-1 dose escalation as one artifact: a line chart of DLT rate vs dose level (5 doses, rising), and a gauge showing the recommended phase-2 dose as a fraction of the max tested. Title, summary, captions."
- **Check:** 2 panels; line 5 pts; gauge single value in range; captions.
- **Continue:** "Mark the MTD dose on the line chart and set the gauge zones (safe/caution/toxic)."

### 15. Enrollment over time
- **Figures:** cumulative area (`render_area`), site bar (`show_chart`)
- **Prompt:** "Visualise trial enrollment as one report: a cumulative enrollment area chart over 12 months and a bar chart of participants recruited per site (6 sites). Title, summary, captions."
- **Check:** 2 panels; area monotonic; bar 6; captions.
- **Continue:** "Add the target enrollment as a reference and note whether the trial is on track in the summary."

### 16. Vital-signs monitoring
- **Figures:** multi-line vitals (`show_chart`), calendar heatmap of visits (`render_calendar_heatmap`)
- **Prompt:** "Build ONE patient-monitoring report: a line chart of heart rate, systolic BP and temperature over 10 visits, and a calendar heatmap of visit adherence over ~8 weeks. Title, summary, captions."
- **Check:** 2 panels; line 3 series; calendar grid coloured by day; captions.
- **Continue:** "Add a shaded normal range note for heart rate and flag any out-of-range visit."

### 17. Diagnostic test performance
- **Figures:** ROC-style line (`show_chart`), confusion heatmap (`render_heatmap`)
- **Prompt:** "Report a diagnostic test as one artifact: an ROC-style curve (FPR vs TPR, AUC ≈ 0.86) and a 2×2 confusion-matrix heatmap (TP 82, FN 18, FP 12, TN 88). Title, summary, captions."
- **Check:** 2 panels; line rising to top-left; heatmap 2×2; captions.
- **Continue:** "Add the AUC value to the curve caption and compute sensitivity/specificity in the summary."

### 18. Comparative effectiveness forest
- **Figures:** forest (`render_forest`), donut of study weights (`render_donut`)
- **Prompt:** "Meta-analysis as one report: a forest plot of 7 studies' effect sizes with a pooled estimate, and a donut of each study's weight in the pooled estimate. Title, summary, captions."
- **Check:** 2 panels; forest 7 rows + diamond/pooled; donut 7 slices; captions.
- **Continue:** "Note the heterogeneity (I²) in the summary and highlight the largest-weight study."

### 19. Length-of-stay & time-to-discharge
- **Figures:** time-to-discharge Kaplan–Meier (`render_kaplan_meier`), LOS histogram (`render_histogram`)
- **Prompt:** "Hospital length-of-stay report as one artifact: Kaplan–Meier time-to-discharge curves for two wards (ward A median ~4 days, ward B median ~7 days, some still-admitted censored), and a histogram of LOS in days across ~200 admissions (right-skewed). Title, summary, captions."
- **Check:** 2 panels; KM two step curves with censor ticks; skewed histogram; captions.
- **Continue:** "Add a median-stay annotation to each KM arm and a log-scale note for the histogram tail; flag the slower-discharging ward."
- **Plan swap (mid-run diversity rebalance):** was histogram+boxplot; swapped the boxplot for KM so `render_kaplan_meier` is exercised ≥2× (with #11).

### 20. Readmission risk factors
- **Figures:** risk-factor bar (`show_chart`), radar of a patient profile (`render_radar`)
- **Prompt:** "Readmission story as one report: a bar chart of odds ratios for 6 readmission risk factors, and a radar chart profiling a high-risk patient across 5 risk dimensions. Title, summary, captions."
- **Check:** 2 panels; bar 6; radar 5 axes; captions.
- **Continue:** "Overlay an average-patient profile on the radar as a second series."

## Batch 3 — Epidemiology, public health & geography (21–30)

### 21. Outbreak epidemic curve
- **Figures:** epi-curve bar (`show_chart`), transmission flow (`render_sankey`)
- **Prompt:** "Model an outbreak as one report: an epidemic-curve bar chart of daily new cases over 30 days (rise and fall), and a Sankey of transmission flow from source → clusters → cases. Title, summary, captions."
- **Check:** 2 panels; bar 30; sankey nodes+links; captions.
- **Continue:** "Add a 7-day moving-average line note to the epi curve and identify the peak day in the summary."

### 22. Disease prevalence choropleth
- **Figures:** choropleth (`render_choropleth`), ranked bar (`show_chart`)
- **Prompt:** "Show disease prevalence by US region as one report: a choropleth map shading a handful of states by prevalence, and a ranked bar of the top 8 states. Title, summary, captions."
- **Check:** 2 panels; choropleth regions shaded with legend; bar 8; captions.
- **Continue:** "Switch the choropleth colour scale to sequential blues and note the highest-prevalence state."

### 23. Vaccination coverage map
- **Figures:** marker map (`render_map`), coverage gauge (`render_gauge`)
- **Prompt:** "Vaccination story as one report: a map with markers for 6 clinics (lat/lng around a city) sized by doses given, and a gauge of overall population coverage vs a 70% target. Title, summary, captions."
- **Check:** 2 panels; leaflet map with markers; gauge with target; captions.
- **Continue:** "Cluster the markers and set the gauge target line at 80%."

### 24. Age–incidence pyramid
- **Figures:** population pyramid via bars (`show_chart`), area of trend (`render_area`)
- **Prompt:** "Build ONE report: a back-to-back age–sex incidence chart (bars for male/female across 8 age bands) and an area chart of incidence trend over 10 years. Title, summary, captions."
- **Check:** 2 panels; grouped bars 8 bands; area; captions.
- **Continue:** "Annotate the age band with peak incidence and the trend inflection year."

### 25. Contact-tracing network
- **Figures:** network (`render_network`), status donut (`render_donut`)
- **Prompt:** "Contact-tracing report as one artifact: a network of ~12 individuals with infection links, and a donut of case statuses (confirmed, recovered, quarantined, negative). Title, summary, captions."
- **Check:** 2 panels; network with hubs; donut 4 slices; captions.
- **Continue:** "Highlight the super-spreader node and add its contact count to the summary."

### 26. Seasonality of flu
- **Figures:** calendar heatmap (`render_calendar_heatmap`), line by season (`show_chart`)
- **Prompt:** "Flu seasonality as one report: a calendar heatmap of daily cases across a year, and a line chart overlaying 3 seasons' weekly case counts. Title, summary, captions."
- **Check:** 2 panels; calendar full-year grid; line 3 series; captions.
- **Continue:** "Point out the peak week each season in the summary and highlight winter months on the calendar."

### 27. Global mortality choropleth
- **Figures:** choropleth (`render_choropleth`), forest of relative risks (`render_forest`)
- **Prompt:** "Global mortality report as one artifact: a choropleth of mortality rate for several countries, and a forest of relative risk for 5 risk factors. Title, summary, captions."
- **Check:** 2 panels; choropleth; forest 5; captions.
- **Continue:** "Add a legend title to the choropleth and rank the countries in the summary."

### 28. Water-quality monitoring
- **Figures:** multi-line contaminants (`show_chart`), boxplot by site (`render_boxplot`)
- **Prompt:** "Environmental-health report as one artifact: a line chart of 3 contaminant levels over 12 months and a boxplot of one contaminant across 5 monitoring sites. Title, summary, captions."
- **Check:** 2 panels; line 3 series; boxplot 5; captions.
- **Continue:** "Add the regulatory limit as a reference on the line chart and flag exceedances."

### 29. Hospital capacity dashboard
- **Figures:** bed-occupancy gauge (`render_gauge`), ICU line (`show_chart`), department treemap (`render_treemap`)
- **Prompt:** "Hospital capacity report with three figures: a gauge of current bed occupancy (e.g. 82%), a line of ICU census over 14 days, and a treemap of beds by department. Title, summary, captions."
- **Check:** 3 panels; gauge; line 14; treemap; captions.
- **Continue:** "Set gauge zones (green<70, amber 70–90, red>90) and note if ICU is trending up."

### 30. Screening-program funnel
- **Figures:** funnel-style bar (`show_chart`), state diagram of the pathway (`render_state_diagram`)
- **Prompt:** "Cancer-screening program as one report: a funnel-shaped bar chart (invited → screened → recalled → diagnosed → treated) and a state diagram of the patient pathway. Title, summary, captions."
- **Check:** 2 panels; descending bars; state diagram SVG; captions.
- **Continue:** "Add conversion percentages between funnel stages to the summary."

## Batch 4 — Single-cell, proteomics & multi-omics (31–40)

### 31. Single-cell cluster overview
- **Figures:** UMAP-style scatter (`show_chart` scatter), cluster-size bar (`show_chart`)
- **Prompt:** "Single-cell report as one artifact: a UMAP-style scatter of ~200 cells coloured by 4 clusters, and a bar chart of cells per cluster. Title, summary, captions."
- **Check:** 2 panels; scatter multi-colour; bar 4; captions.
- **Continue:** "Add marker-gene labels to each cluster in the summary and recolour cluster 3."

### 32. Marker-gene dotplot heatmap
- **Figures:** expression heatmap (`render_heatmap`), radar of a cluster signature (`render_radar`)
- **Prompt:** "Show marker genes as one report: a heatmap of 6 marker genes × 4 clusters (mean expression), and a radar of cluster-1's signature across 6 genes. Title, summary, captions."
- **Check:** 2 panels; heatmap 6×4; radar 6 axes; captions.
- **Continue:** "Z-score the heatmap rows and note the top marker per cluster."

### 33. Cell-type composition across samples
- **Figures:** stacked area (`render_area`), sunburst of hierarchy (`render_sunburst`)
- **Prompt:** "Composition report as one artifact: a stacked area chart of 4 cell-type proportions across 6 samples, and a sunburst of the cell-type hierarchy (lineage → subtype). Title, summary, captions."
- **Check:** 2 panels; area stacked; sunburst radial; captions.
- **Continue:** "Order samples by treatment and highlight the sample with the biggest composition shift."

### 34. Proteogenomics: volcano + pQTL scan
- **Figures:** volcano (`render_volcano`), pQTL Manhattan (`render_manhattan`)
- **Prompt:** "Proteogenomics report as one artifact: a volcano of protein log2FC vs significance (10 proteins), and a Manhattan plot of protein-QTL associations across chromosomes 1–10 (a couple of peaks crossing genome-wide significance, e.g. chr4 -log10p 11, chr9 8.5). Title, summary, captions."
- **Check:** 2 panels; volcano 10 points with threshold lines; Manhattan chromosome-coloured points crossing a significance line; captions.
- **Continue:** "Label the 3 most significant proteins on the volcano and name the top pQTL locus in the summary."
- **Plan swap (mid-run diversity rebalance):** was volcano+boxplot; swapped the boxplot for a Manhattan so `render_manhattan` is exercised ≥2× (with #2).

### 35. Metabolite pathway flow
- **Figures:** sankey (`render_sankey`), bar of fold-changes (`show_chart`)
- **Prompt:** "Metabolomics report as one artifact: a Sankey of metabolite flux through a pathway (substrate → intermediates → product), and a bar of metabolite fold-changes (case vs control) for 7 metabolites. Title, summary, captions."
- **Check:** 2 panels; sankey; bar 7 (pos/neg); captions.
- **Continue:** "Diverging colour the bar around zero and call out the most up/down metabolite."

### 36. Multi-omics correlation
- **Figures:** correlation heatmap (`render_heatmap`), chord of cross-omic links (`render_chord`)
- **Prompt:** "Multi-omics integration report as one artifact: a correlation heatmap across 6 features (genes/proteins/metabolites), and a chord diagram of the strongest cross-omic correlations. Title, summary, captions."
- **Check:** 2 panels; heatmap 6×6; chord ribbons; captions.
- **Continue:** "Threshold the chord at |r|>0.5 and note the strongest cross-omic pair."

### 37. Protein–protein interaction module
- **Figures:** network (`render_network`), degree histogram (`render_histogram`)
- **Prompt:** "PPI report as one artifact: a network of ~15 proteins with interaction edges, and a histogram of node degrees. Title, summary, captions."
- **Check:** 2 panels; network; degree histogram; captions.
- **Continue:** "Highlight the top-degree hub and mention the network's density in the summary."

### 38. Flow-cytometry gating
- **Figures:** 2D scatter (`show_chart` scatter), population donut (`render_donut`)
- **Prompt:** "Flow cytometry report as one artifact: a 2D scatter (CD4 vs CD8) with ~150 cells in 3 populations, and a donut of population percentages. Title, summary, captions."
- **Check:** 2 panels; scatter 3 clusters; donut 3; captions.
- **Continue:** "Add gate labels to the scatter caption and the double-positive fraction to the summary."

### 39. Copy-number profile
- **Figures:** CN segment line (`show_chart`), chromosome heatmap (`render_heatmap`)
- **Prompt:** "Copy-number report as one artifact: a step-like line of log2 copy-ratio along a chromosome, and a heatmap of CN state across 5 samples × 8 regions. Title, summary, captions."
- **Check:** 2 panels; line; heatmap 5×8; captions.
- **Continue:** "Mark amplification/deletion thresholds on the line and count altered regions in the summary."

### 40. Drug-response dose curves
- **Figures:** dose-response line (`show_chart`), IC50 bar (`show_chart`)
- **Prompt:** "Pharmacology report as one artifact: dose-response curves (viability vs log dose) for 3 drugs, and a bar of their IC50 values. Title, summary, captions."
- **Check:** 2 panels; line 3 sigmoid-ish series; bar 3; captions.
- **Continue:** "Add IC50 markers on the curves and rank drug potency in the summary."

## Batch 5 — Neuroscience, imaging & physiology (41–50)

### 41. EEG band power
- **Figures:** band-power bar (`show_chart`), time–frequency heatmap (`render_heatmap`)
- **Prompt:** "EEG report as one artifact: a bar of average power in 5 frequency bands (delta–gamma), and a time–frequency heatmap (bands × 10 epochs). Title, summary, captions."
- **Check:** 2 panels; bar 5; heatmap 5×10; captions.
- **Continue:** "Highlight the dominant band and note any alpha suppression in the summary."

### 42. fMRI region connectivity
- **Figures:** connectivity chord (`render_chord`), network (`render_network`)
- **Prompt:** "fMRI connectivity report as one artifact: a chord diagram of connectivity among 6 brain regions, and a network of the same with edge weights. Title, summary, captions."
- **Check:** 2 panels; chord 6; network; captions.
- **Continue:** "Keep only the top 8 connections in the network and name the most connected region."

### 43. Reaction-time experiment
- **Figures:** RT histogram (`render_histogram`), condition boxplot (`render_boxplot`)
- **Prompt:** "Cognitive-task report as one artifact: a histogram of reaction times (ms, right-skewed) and a boxplot of RT across 3 task conditions. Title, summary, captions."
- **Check:** 2 panels; skewed histogram; boxplot 3; captions.
- **Continue:** "Add mean/median lines note and flag the slowest condition."

### 44. Sleep-stage hypnogram
- **Figures:** stage line/step (`show_chart`), stage donut (`render_donut`)
- **Prompt:** "Sleep study report as one artifact: a hypnogram (sleep stage over the night as a step line) and a donut of time spent per stage. Title, summary, captions."
- **Check:** 2 panels; step line; donut 4–5; captions.
- **Continue:** "Count REM cycles and add sleep efficiency to the summary."

### 45. Neuron spike raster
- **Figures:** raster-style scatter (`show_chart` scatter), firing-rate line (`show_chart`)
- **Prompt:** "Electrophysiology report as one artifact: a spike raster (trials × time as a scatter) and a peristimulus firing-rate line. Title, summary, captions."
- **Check:** 2 panels; scatter grid; line; captions.
- **Continue:** "Add a stimulus-onset marker and describe the response latency in the summary."

### 46. Heart-rate variability
- **Figures:** RR-interval line (`show_chart`), HRV histogram (`render_histogram`)
- **Prompt:** "HRV report as one artifact: a line of RR intervals over time and a histogram of their distribution. Title, summary, captions."
- **Check:** 2 panels; line; histogram; captions.
- **Continue:** "Add SDNN/RMSSD notes to the summary and flag any arrhythmic stretch."

### 47. Gait analysis
- **Figures:** joint-angle line (`show_chart`), symmetry radar (`render_radar`)
- **Prompt:** "Gait report as one artifact: joint-angle trajectories (hip/knee/ankle) over a gait cycle, and a radar of left–right symmetry across 5 metrics. Title, summary, captions."
- **Check:** 2 panels; line 3 series; radar 5; captions.
- **Continue:** "Overlay a healthy reference band note and quantify asymmetry in the summary."

### 48. Brain-region volume atlas
- **Figures:** region bar (`show_chart`), region-hierarchy dendrogram (`render_dendrogram`)
- **Prompt:** "Neuroanatomy report as one artifact: a bar of volumes for 8 brain regions and a dendrogram clustering those regions into their lobe hierarchy (lobe → region). Title, summary, captions."
- **Check:** 2 panels; bar 8; dendrogram tree SVG grouping regions under lobes; captions.
- **Continue:** "Normalise volumes to total brain volume and highlight the largest lobe."
- **Plan swap (mid-run diversity rebalance):** was bar+treemap; swapped the treemap for a dendrogram so `render_dendrogram` is exercised ≥2× (with #8); treemap is still covered 5×.

### 49. Pupillometry response
- **Figures:** pupil-size line (`show_chart`), condition area (`render_area`)
- **Prompt:** "Pupillometry report as one artifact: a line of pupil diameter over time for 2 stimuli, and an area chart of the difference wave. Title, summary, captions."
- **Check:** 2 panels; line 2 series; area; captions.
- **Continue:** "Mark the peak dilation time and add it to the summary."

### 50. Motor-learning curve
- **Figures:** learning-curve line (`show_chart`), gauge of final accuracy (`render_gauge`)
- **Prompt:** "Motor-learning report as one artifact: a learning curve (accuracy vs session, rising and plateauing) and a gauge of final accuracy vs a 90% goal. Title, summary, captions."
- **Check:** 2 panels; line rising; gauge; captions.
- **Continue:** "Fit a note for the plateau session and set the gauge goal to 95%."

## Batch 6 — Environment, climate & ecology (51–60)

### 51. Temperature anomaly trend
- **Figures:** anomaly line (`show_chart`), decade area (`render_area`)
- **Prompt:** "Climate report as one artifact: a line of global temperature anomaly over 40 years and an area chart of the anomaly by decade. Title, summary, captions."
- **Check:** 2 panels; line ~40 pts; area; captions.
- **Continue:** "Add a zero baseline and a trend note; call out the warmest decade."

### 52. CO₂ emissions by sector
- **Figures:** sector donut (`render_donut`), country choropleth (`render_choropleth`)
- **Prompt:** "Emissions report as one artifact: a donut of CO₂ by sector (energy, transport, industry, agriculture, buildings) and a choropleth of per-capita emissions for several countries. Title, summary, captions."
- **Check:** 2 panels; donut 5; choropleth; captions.
- **Continue:** "Rank the top 3 sectors in the summary and switch the choropleth to a red scale."

### 53. Species abundance survey
- **Figures:** rank-abundance line (`show_chart`), diversity bar (`show_chart`)
- **Prompt:** "Ecology report as one artifact: a rank-abundance curve (log y) for ~12 species and a bar of Shannon diversity across 4 habitats. Title, summary, captions."
- **Check:** 2 panels; line declining; bar 4; captions.
- **Continue:** "Add the dominant species note and highlight the most diverse habitat."

### 54. Rainfall & river flow
- **Figures:** dual-axis-style lines (`show_chart`), monthly boxplot (`render_boxplot`)
- **Prompt:** "Hydrology report as one artifact: lines of monthly rainfall and river discharge over a year, and a boxplot of daily discharge by season. Title, summary, captions."
- **Check:** 2 panels; line 2 series; boxplot 4; captions.
- **Continue:** "Mark the flood-risk threshold and flag the wettest month."

### 55. Air-quality index map
- **Figures:** marker map (`render_map`), AQI calendar (`render_calendar_heatmap`)
- **Prompt:** "Air-quality report as one artifact: a map of 6 monitoring stations coloured/sized by AQI, and a calendar heatmap of daily AQI over ~2 months. Title, summary, captions."
- **Check:** 2 panels; map markers; calendar; captions.
- **Continue:** "Add AQI category legend and note the worst-air day in the summary."

### 56. Deforestation over time
- **Figures:** forest-cover area (`render_area`), region treemap (`render_treemap`)
- **Prompt:** "Land-use report as one artifact: an area chart of forest cover loss over 20 years and a treemap of remaining forest by region. Title, summary, captions."
- **Check:** 2 panels; area; treemap; captions.
- **Continue:** "Add the total loss figure and highlight the fastest-declining region."

### 57. Renewable energy mix
- **Figures:** stacked area (`render_area`), source donut (`render_donut`)
- **Prompt:** "Energy report as one artifact: a stacked area of the electricity mix (coal, gas, nuclear, wind, solar) over 15 years, and a donut of the current-year mix. Title, summary, captions."
- **Check:** 2 panels; area stacked; donut 5; captions.
- **Continue:** "Call out the crossover year when renewables overtook coal."

### 58. Ocean temperature depth profile
- **Figures:** depth profile line (`show_chart`), heatmap over time (`render_heatmap`)
- **Prompt:** "Oceanography report as one artifact: a temperature-vs-depth profile line, and a heatmap of temperature (depth × month). Title, summary, captions."
- **Check:** 2 panels; line; heatmap; captions.
- **Continue:** "Mark the thermocline depth and note seasonal deepening in the summary."

### 59. Wildlife migration timeline
- **Figures:** timeline (`render_timeline`), route map (`render_map`)
- **Prompt:** "Migration report as one artifact: a timeline of migration events across a year, and a map of stopover sites. Title, summary, captions."
- **Check:** 2 panels; timeline; map markers; captions.
- **Continue:** "Add distances between stopovers to the summary and highlight the longest leg."

### 60. Recycling program metrics
- **Figures:** diversion-rate gauge (`render_gauge`), material bar (`show_chart`)
- **Prompt:** "Waste report as one artifact: a gauge of the recycling diversion rate vs a 50% target and a bar of tonnage by material type. Title, summary, captions."
- **Check:** 2 panels; gauge; bar; captions.
- **Continue:** "Set gauge zones and highlight the most-recycled material."

## Batch 7 — Business, finance & operations (61–70)

### 61. Revenue & margin story
- **Figures:** revenue line (`show_chart`), margin area (`render_area`)
- **Prompt:** "Business report as one artifact: a line of quarterly revenue over 3 years and an area chart of gross margin over the same period. Title, summary, captions."
- **Check:** 2 panels; line; area; captions.
- **Continue:** "Annotate the best quarter and note the margin trend."

### 62. Sales funnel
- **Figures:** funnel bar (`show_chart`), conversion state diagram (`render_state_diagram`)
- **Prompt:** "Sales report as one artifact: a funnel bar (leads → qualified → demo → proposal → closed) and a state diagram of the deal lifecycle. Title, summary, captions."
- **Check:** 2 panels; descending bar; state diagram; captions.
- **Continue:** "Add stage conversion rates to the summary."

### 63. Customer segmentation
- **Figures:** segment bubble (`render_bubble`), segment donut (`render_donut`)
- **Prompt:** "Marketing report as one artifact: a bubble chart of segments (x = frequency, y = monetary value, size = count) and a donut of revenue share by segment. Title, summary, captions."
- **Check:** 2 panels; bubble; donut; captions.
- **Continue:** "Label the highest-value segment and add its share to the summary."

### 64. Website analytics
- **Figures:** traffic line (`show_chart`), source treemap (`render_treemap`)
- **Prompt:** "Web-analytics report as one artifact: a line of daily sessions over a month and a treemap of traffic by source/medium. Title, summary, captions."
- **Check:** 2 panels; line ~30; treemap; captions.
- **Continue:** "Mark a campaign spike and note the top source."

### 65. Supply-chain flow
- **Figures:** sankey (`render_sankey`), lead-time boxplot (`render_boxplot`)
- **Prompt:** "Operations report as one artifact: a Sankey of goods flow (suppliers → warehouses → stores) and a boxplot of delivery lead times by supplier. Title, summary, captions."
- **Check:** 2 panels; sankey; boxplot; captions.
- **Continue:** "Highlight the highest-throughput path and flag the slowest supplier."

### 66. Project schedule
- **Figures:** gantt (`render_gantt`), milestone timeline (`render_timeline`)
- **Prompt:** "Project report as one artifact: a Gantt chart of 6 tasks with dependencies and a timeline of key milestones. Title, summary, captions."
- **Check:** 2 panels; gantt bars; timeline; captions.
- **Continue:** "Mark the critical path in the summary and add a milestone."

### 67. Financial portfolio
- **Figures:** allocation donut (`render_donut`), returns line (`show_chart`)
- **Prompt:** "Finance report as one artifact: a donut of portfolio allocation across 5 asset classes and a line of cumulative returns vs a benchmark over 24 months. Title, summary, captions."
- **Check:** 2 panels; donut 5; line 2 series; captions.
- **Continue:** "Add the alpha vs benchmark to the summary and highlight the best-performing month."

### 68. Churn analysis
- **Figures:** cohort heatmap (`render_heatmap`), churn-driver bar (`show_chart`)
- **Prompt:** "Retention report as one artifact: a cohort-retention heatmap (cohorts × months) and a bar of churn drivers by importance. Title, summary, captions."
- **Check:** 2 panels; heatmap; bar; captions.
- **Continue:** "Note the worst-retaining cohort and the top churn driver."

### 69. Manufacturing quality
- **Figures:** control-chart line (`show_chart`), defect Pareto bar (`show_chart`)
- **Prompt:** "Quality report as one artifact: a control chart of a measurement over 30 samples with control limits, and a Pareto bar of defect types. Title, summary, captions."
- **Check:** 2 panels; line with limits note; bar descending; captions.
- **Continue:** "Flag any out-of-control point and add cumulative % to the Pareto."

### 70. Org structure & headcount
- **Figures:** org class diagram (`render_class_diagram`), headcount bar (`show_chart`)
- **Prompt:** "HR report as one artifact: a class-diagram-style org chart of 5 departments and a bar of headcount per department. Title, summary, captions."
- **Check:** 2 panels; class diagram; bar 5; captions.
- **Continue:** "Add reporting lines note and highlight the largest team."

## Batch 8 — Social science, demographics & education (71–80)

### 71. Population demographics
- **Figures:** age pyramid bars (`show_chart`), median-age line (`show_chart`)
- **Prompt:** "Demography report as one artifact: an age–sex pyramid (bars) for a country and a line of median age over 30 years. Title, summary, captions."
- **Check:** 2 panels; grouped bars; line; captions.
- **Continue:** "Highlight the largest cohort and the ageing trend."

### 72. Election results
- **Figures:** vote-share donut (`render_donut`), region choropleth (`render_choropleth`)
- **Prompt:** "Election report as one artifact: a donut of national vote share (4 parties) and a choropleth of the winning party by region. Title, summary, captions."
- **Check:** 2 panels; donut 4; choropleth categorical; captions.
- **Continue:** "Add the margin in the closest region to the summary."

### 73. Survey Likert results
- **Figures:** diverging bar (`show_chart`), sentiment wordcloud (`render_wordcloud`)
- **Prompt:** "Survey report as one artifact: a diverging bar of Likert responses for 5 statements (strongly disagree → strongly agree) and a wordcloud of open-text themes. Title, summary, captions."
- **Check:** 2 panels; bar 5; wordcloud; captions.
- **Continue:** "Highlight the most-agreed statement and the biggest wordcloud term."

### 74. Education outcomes
- **Figures:** score histogram (`render_histogram`), subject radar (`render_radar`)
- **Prompt:** "Education report as one artifact: a histogram of test scores and a radar of a cohort's performance across 6 subjects. Title, summary, captions."
- **Check:** 2 panels; histogram; radar 6; captions.
- **Continue:** "Add the pass rate and the weakest subject to the summary."

### 75. Income distribution
- **Figures:** Lorenz-style line (`show_chart`), decile bar (`show_chart`)
- **Prompt:** "Inequality report as one artifact: a Lorenz curve (cumulative income vs population) and a bar of income share by decile. Title, summary, captions."
- **Check:** 2 panels; line with diagonal note; bar 10; captions.
- **Continue:** "Compute the Gini note and highlight the top-decile share."

### 76. Migration flows
- **Figures:** chord (`render_chord`), net-migration bar (`show_chart`)
- **Prompt:** "Migration report as one artifact: a chord of migration flows among 5 regions and a bar of net migration per region. Title, summary, captions."
- **Check:** 2 panels; chord 5; bar; captions.
- **Continue:** "Highlight the largest flow and the region with the biggest net gain."

### 77. Crime statistics
- **Figures:** type bar (`show_chart`), hotspot map (`render_map`)
- **Prompt:** "Public-safety report as one artifact: a bar of incidents by crime type and a map of hotspots. Title, summary, captions."
- **Check:** 2 panels; bar; map markers; captions.
- **Continue:** "Cluster the map markers and note the most common crime type."

### 78. Language usage
- **Figures:** speaker treemap (`render_treemap`), language wordcloud (`render_wordcloud`)
- **Prompt:** "Linguistics report as one artifact: a treemap of speakers by language family, and a wordcloud of ~15 languages sized by number of speakers (e.g. Mandarin 920, Spanish 475, English 373, Hindi 344, Arabic 290, Bengali 230, Portuguese 230, Russian 154, Japanese 125, ...). Title, summary, captions."
- **Check:** 2 panels; treemap; wordcloud with size-scaled terms; captions.
- **Continue:** "Highlight the largest family and call out the biggest wordcloud term in the summary."
- **Plan swap (mid-run diversity rebalance):** was treemap+line; swapped the line for a wordcloud so `render_wordcloud` is exercised ≥2× (with #73).

### 79. Social-media engagement
- **Figures:** engagement calendar (`render_calendar_heatmap`), platform donut (`render_donut`)
- **Prompt:** "Social report as one artifact: a calendar heatmap of daily posts/engagement and a donut of engagement by platform. Title, summary, captions."
- **Check:** 2 panels; calendar; donut; captions.
- **Continue:** "Note the most-active weekday and the top platform."

### 80. Research collaboration network
- **Figures:** co-authorship network (`render_network`), publication timeline (`render_timeline`)
- **Prompt:** "Bibliometrics report as one artifact: a co-authorship network of ~12 researchers and a timeline of key publications. Title, summary, captions."
- **Check:** 2 panels; network; timeline; captions.
- **Continue:** "Highlight the most-connected author and the most-cited year."

## Batch 9 — Workflows, systems & diagrams (81–90)

### 81. CI/CD pipeline
- **Figures:** flowchart (`render_flowchart`), build-time line (`show_chart`)
- **Prompt:** "DevOps report as one artifact: a flowchart of a CI/CD pipeline (commit → build → test → deploy) and a line of build durations over 20 runs. Title, summary, captions."
- **Check:** 2 panels; flowchart; line; captions.
- **Continue:** "Mark the failing stage and note the slowest build."

### 82. Microservice architecture
- **Figures:** architecture flowchart (`render_flowchart`), request-latency boxplot (`render_boxplot`)
- **Prompt:** "Systems report as one artifact: a flowchart of a microservice architecture (gateway → 4 services → db) and a boxplot of latency per service. Title, summary, captions."
- **Check:** 2 panels; flowchart; boxplot; captions.
- **Continue:** "Highlight the slowest service and add a caching note."

### 83. State machine of an order
- **Figures:** state diagram (`render_state_diagram`), status donut (`render_donut`)
- **Prompt:** "E-commerce report as one artifact: a state diagram of an order lifecycle and a donut of current order statuses. Title, summary, captions."
- **Check:** 2 panels; state diagram; donut; captions.
- **Continue:** "Add a cancellation transition and note the most common status."

### 84. API sequence + data model
- **Figures:** sequence diagram (`render_sequence`), ER diagram (`render_er_diagram`)
- **Prompt:** "Integration report as one artifact: a sequence diagram of an auth+data API exchange (client, gateway, auth, service) and an ER diagram of the 3 tables the service reads/writes (users, sessions, events) with their relationships. Title, summary, captions."
- **Check:** 2 panels; sequence diagram with lifelines/messages; ER diagram with entities + relationships; captions.
- **Continue:** "Add a retry loop to the sequence and a foreign-key note to the ER diagram."
- **Plan swap (mid-run diversity rebalance):** was sequence+call-bar; swapped the bar for an ER diagram so `render_er_diagram` is exercised ≥2× (with #85).

### 85. Database schema
- **Figures:** ER diagram (`render_er_diagram`), row-count bar (`show_chart`)
- **Prompt:** "Data-model report as one artifact: an ER diagram of 4 related tables (users, orders, products, payments) and a bar of row counts per table. Title, summary, captions."
- **Check:** 2 panels; ER diagram; bar 4; captions.
- **Continue:** "Add a foreign-key note and highlight the largest table."

### 86. Product roadmap
- **Figures:** gantt (`render_gantt`), theme mindmap (`render_mindmap`)
- **Prompt:** "Product report as one artifact: a Gantt of a 4-quarter roadmap and a mindmap of feature themes. Title, summary, captions."
- **Check:** 2 panels; gantt; mindmap; captions.
- **Continue:** "Mark a slipped milestone and add a new theme branch."

### 87. Incident response timeline
- **Figures:** timeline (`render_timeline`), severity donut (`render_donut`)
- **Prompt:** "Reliability report as one artifact: a timeline of an incident (detect → triage → mitigate → resolve → postmortem) and a donut of incidents by severity this quarter. Title, summary, captions."
- **Check:** 2 panels; timeline; donut; captions.
- **Continue:** "Add MTTR to the summary and highlight the sev-1 count."

### 88. Knowledge taxonomy
- **Figures:** mindmap (`render_mindmap`), sunburst (`render_sunburst`)
- **Prompt:** "Knowledge-management report as one artifact: a mindmap of a topic taxonomy and a sunburst of the same hierarchy with page counts. Title, summary, captions."
- **Check:** 2 panels; mindmap; sunburst; captions.
- **Continue:** "Deepen one branch and note the largest category."

### 89. Class model
- **Figures:** class diagram (`render_class_diagram`), method-count bar (`show_chart`)
- **Prompt:** "Software-design report as one artifact: a class diagram of 4 classes with inheritance and a bar of method counts per class. Title, summary, captions."
- **Check:** 2 panels; class diagram; bar; captions.
- **Continue:** "Add an interface and highlight the most complex class."

### 90. Deployment topology
- **Figures:** failover sequence diagram (`render_sequence`), region latency map (`render_map`)
- **Prompt:** "Infrastructure report as one artifact: a sequence diagram of a cross-region failover request (client → primary region → health check → secondary region → response) and a map of region endpoints sized/coloured by latency. Title, summary, captions."
- **Check:** 2 panels; sequence diagram with lifelines/messages; map markers; captions.
- **Continue:** "Add a retry/timeout branch to the failover sequence and flag the highest-latency region."
- **Plan swap (mid-run diversity rebalance):** was flowchart+map; swapped the flowchart for a sequence diagram so `render_sequence` is exercised ≥2× (with #84); flowchart is still covered 3×.

## Batch 10 — Physics, chemistry, engineering & misc (91–100)

### 91. Projectile motion
- **Figures:** trajectory line (`show_chart`), range bar (`show_chart`)
- **Prompt:** "Physics report as one artifact: a line of projectile trajectories at 3 launch angles and a bar of the range achieved at each angle. Title, summary, captions."
- **Check:** 2 panels; line 3 arcs; bar 3; captions.
- **Continue:** "Mark the optimal angle and add the max range to the summary."

### 92. Spectroscopy
- **Figures:** spectrum line (`show_chart`), peak bar (`show_chart`)
- **Prompt:** "Chemistry report as one artifact: an absorption spectrum line (absorbance vs wavelength with peaks) and a bar of peak intensities. Title, summary, captions."
- **Check:** 2 panels; line with peaks; bar; captions.
- **Continue:** "Label the strongest peak's wavelength in the summary."

### 93. Reaction kinetics
- **Figures:** concentration line (`show_chart`), rate boxplot (`render_boxplot`)
- **Prompt:** "Kinetics report as one artifact: concentration-vs-time curves for reactant and product, and a boxplot of measured rate constants across 4 temperatures. Title, summary, captions."
- **Check:** 2 panels; line 2 series; boxplot 4; captions.
- **Continue:** "Add a half-life note and highlight the fastest temperature."

### 94. Material stress–strain
- **Figures:** stress–strain line (`show_chart`), property radar (`render_radar`)
- **Prompt:** "Materials report as one artifact: stress–strain curves for 2 materials and a radar of 5 mechanical properties. Title, summary, captions."
- **Check:** 2 panels; line 2 series; radar 5; captions.
- **Continue:** "Mark the yield points and note the stronger material."

### 95. Circuit signal
- **Figures:** waveform line (`show_chart`), FFT bar (`show_chart`)
- **Prompt:** "Electronics report as one artifact: a time-domain waveform line and a frequency-spectrum bar (FFT magnitudes). Title, summary, captions."
- **Check:** 2 panels; line oscillating; bar; captions.
- **Continue:** "Label the dominant frequency in the summary."

### 96. Thermal simulation
- **Figures:** temperature heatmap (`render_heatmap`), cooling line (`show_chart`)
- **Prompt:** "Thermal report as one artifact: a heatmap of a temperature field (grid) and a line of the hotspot cooling over time. Title, summary, captions."
- **Check:** 2 panels; heatmap grid; line decaying; captions.
- **Continue:** "Mark the hotspot cell and add the cooling time constant."

### 97. Fluid-flow field
- **Figures:** velocity heatmap (`render_heatmap`), profile line (`show_chart`)
- **Prompt:** "CFD report as one artifact: a heatmap of a velocity field and a line of the velocity profile across the channel. Title, summary, captions."
- **Check:** 2 panels; heatmap; line parabola-ish; captions.
- **Continue:** "Mark the max-velocity location and note the Reynolds regime."

### 98. Orbital mechanics
- **Figures:** orbit scatter (`show_chart` scatter), altitude line (`show_chart`)
- **Prompt:** "Astrodynamics report as one artifact: a scatter tracing an elliptical orbit and a line of altitude over one period. Title, summary, captions."
- **Check:** 2 panels; scatter ellipse; line periodic; captions.
- **Continue:** "Mark perigee/apogee and add the period to the summary."

### 99. Signal-to-noise sweep
- **Figures:** SNR line (`show_chart`), BER bar (`show_chart`)
- **Prompt:** "Comms report as one artifact: a line of SNR vs distance and a bar of bit-error-rate at 4 SNR levels (log scale). Title, summary, captions."
- **Check:** 2 panels; line; bar 4; captions.
- **Continue:** "Mark the usable-range threshold and note the BER floor."

### 100. Robotics trajectory & energy
- **Figures:** path scatter (`show_chart` scatter), energy area (`render_area`), joint radar (`render_radar`)
- **Prompt:** "Robotics report with three figures: a scatter of a robot's XY path with waypoints, an area chart of cumulative energy use over the path, and a radar of joint-effort across 5 joints. Title, summary, captions."
- **Check:** 3 panels; scatter path; area; radar 5; no error cards; captions.
- **Continue:** "Mark the highest-effort joint and add total path length to the summary."

## Related documentation

- [Tool-selection fixtures](../../../crates/biorouter-mcp/tests/fixtures/autovis-datasets/README.md) — the companion probe: ten datasets that check tool *selection* rather than tool *output*, with its own recorded baseline.

- [run-results.md](run-results.md) — what actually happened when these 100 scenarios were run, per visualization and per batch.
- [hardening-log.md](hardening-log.md) — the fixes made between batches while this corpus was being executed.
- [Auto Visualiser extension guide](../../extensions/built-in/auto-visualiser.md) — what the extension is, how to enable it, and the full tool catalogue these scenarios exercise.
- [Agent Drafter stress test](../agent-drafter-stress-test/README.md) — the sibling 100-prompt stress campaign, run the same way against the app-authoring extension.
- [Agent Browser debugging guide](../../desktop-ui/agent-browser-debugging.md) — how the dev GUI was driven and inspected to verify each report.
