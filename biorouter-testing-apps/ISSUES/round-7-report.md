# BioRouter QA — Round 7 Issues Report (apps 31–35)

Statistics batch, first half. Apps 31–35 = Bayesian MCMC, GLM-from-scratch (R),
ARIMA, hypothesis-testing suite (R), bootstrap/resampling.

## Outcome

| # | App | Lang | Tests | Note |
|---|-----|------|-------|------|
| 31 | bayesian-mcmc | Python | 108 pass | clean 1-shot (MH/Gibbs/HMC/slice + R-hat/ESS/HPD) |
| 32 | glm-from-scratch | **R** | all pass on `R CMD INSTALL` | premature stop → resume → NAMESPACE fix |
| 33 | timeseries-arima | Python | 70 pass | clean 1-shot (AR/MA/ARIMA/SARIMA/HW/auto-order) |
| 34 | hypothesis-testing | **R** | 111 pass | clean 1-shot (param/nonparam/categorical + corrections) |
| 35 | bootstrap-resampling | Python | 90 pass | undeclared scipy dep |

4/5 clean or one-fix; **36 apps total, ~3,600 passing tests** across Rust /
Python / C++ / R.

## Findings

**"Works in my session" reproducibility issues persist, now across languages:**
- **R (app 32):** NAMESPACE imports a nonexistent `stats::nulldev` — passes under
  `devtools::load_all()` (lenient) but fails `R CMD INSTALL`. → tightened R
  verification to use real install, not in-session loading.
- **Python (app 35):** uses `scipy` but never declares it (no pyproject dep / no
  requirements) — clean install fails `ModuleNotFound`. → the dependency-declaration
  gap is the Python analog of app 32's NAMESPACE gap.
These join the earlier src-layout / CLI-needs-install / skipped-test cases as one
coherent meta-finding: **MiMo optimizes for its transient environment and
under-specifies the reproducible-distribution contract** (manifests, namespaces,
declared deps). A "verify from a clean, dependency-isolated checkout" guard (the
Plan-B Stop hook does exactly the build half) is the highest-leverage product fix.

**Premature stops broadened (app 32):** occurred at metadata→source (not only
code→tests), confirming continue-on-truncation as the proper fix over the
prompt-only mitigation (which still helped the code→tests case).

**R is the strongest analytics toolchain (now ~4/5 clean; the one miss was a
fixable NAMESPACE typo).** Validates the R support added to `analyze`.

## Improvement
No new source change (the running loop stays stable). The round-7 emphasis is
*verification rigor*: clean-room install checks for both R (`R CMD INSTALL`) and
Python (fresh venv) now reliably catch the reproducibility class — which the
shipped Plan-B verify-and-checkpoint Stop hook would enforce in-product.
