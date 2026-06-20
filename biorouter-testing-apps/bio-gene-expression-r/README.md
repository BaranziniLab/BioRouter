# bio-gene-expression-r

RNA-Seq Differential Gene Expression Analysis Toolkit in R.

A self-contained toolkit for RNA-seq differential gene expression analysis, built with base R and standard CRAN packages. No Bioconductor dependencies.

## Features

- **I/O**: Read count matrices (CSV/TSV) and sample metadata with validation
- **Normalization**: CPM, TMM-like scaling factors, median-of-ratios (DESeq2-style)
- **Filtering**: Low-count gene removal based on CPM thresholds
- **DE Testing**: Quasi-likelihood F-test with Wilcoxon/t-test fallback
- **Visualization**: Volcano plot and MA plot data preparation
- **PCA**: Principal component analysis of samples
- **Results**: CSV export with significance annotations
- **CLI**: Command-line interface via `Rscript`

## Project Structure

```
bio-gene-expression-r/
├── DESCRIPTION          # R package manifest
├── NAMESPACE            # Exported functions
├── LICENSE              # MIT license
├── README.md            # This file
├── run_de_analysis.R    # CLI entry point
├── R/
│   ├── io.R             # Data I/O (read counts, metadata)
│   ├── normalization.R  # CPM, TMM, median-of-ratios
│   ├── filtering.R      # Low-count gene filtering
│   ├── statistics.R     # DE testing (quasi-likelihood, Wilcoxon, t-test)
│   ├── results.R        # Results table formatting, CSV export
│   ├── visualization.R  # Volcano & MA plot data prep
│   ├── pca.R            # PCA of samples
│   ├── pipeline.R       # End-to-end pipeline function
│   ├── synthetic.R      # Synthetic test data generation
│   └── utils.R          # Helper functions
├── tests/
│   ├── testthat.R       # Test runner
│   └── testthat/
│       ├── test-io.R
│       ├── test-normalization.R
│       ├── test-filtering.R
│       ├── test-statistics.R
│       ├── test-results.R
│       ├── test-visualization.R
│       ├── test-pca.R
│       ├── test-pipeline.R
│       └── test-synthetic.R
└── man/                 # Documentation (generated)
```

## Quick Start

### Using the CLI

```bash
Rscript run_de_analysis.R \
  --counts counts.csv \
  --metadata metadata.csv \
  --method quasi_likelihood \
  --norm median_of_ratios \
  --output de_results.csv
```

### Using in R

```r
# Source all modules
for (f in list.files("R", pattern = "\\.R$", full.names = TRUE)) source(f)

# Run the full pipeline
result = run_de_pipeline(
  counts_file = "counts.csv",
  metadata_file = "metadata.csv"
)

# Access results
head(result$results)
result$summary
result$pca_result$coordinates
```

## Input Format

### Count Matrix (CSV)
- Rows = genes, Columns = samples
- First column = gene IDs
- Values = raw integer counts

```
gene,S1,S2,S3,S4
Gene1,120,95,130,110
Gene2,5,3,8,2
```

### Metadata (CSV)
- Rows = samples
- Required columns: `sample`, `condition`

```
sample,condition
S1,control
S2,control
S3,treated
S4,treated
```

## Normalization Methods

| Method | Description |
|--------|-------------|
| `median_of_ratios` | DESeq2-style median-of-ratios (default) |
| `tmm` | Trimmed mean of M-values (simplified edgeR) |
| `cpm` | Counts per million |

## DE Testing Methods

| Method | Description |
|--------|-------------|
| `quasi_likelihood` | Quasi-likelihood F-test with dispersion estimation (default) |
| `wilcoxon` | Wilcoxon rank-sum test (non-parametric fallback) |
| `t_test` | Welch's t-test |

## Running Tests

```bash
cd tests
Rscript testthat.R
```

## License

MIT
