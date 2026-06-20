#' Main Biomarker Discovery Pipeline
#'
#' Ties together preprocessing, univariate screening, LASSO, RFE,
#' stability selection, evaluation, and reporting into a single workflow.

#' Run the full biomarker discovery pipeline
#'
#' @param X Numeric matrix (samples x features) or data.frame.
#' @param y Numeric outcome vector (0/1 for binary).
#' @param var_threshold Numeric. Low-variance filter threshold (default 0.01).
#' @param missing_threshold Numeric. Max missing fraction per feature (default 0.3).
#' @param norm_method Character. Normalization method (default "zscore").
#' @param univariate_method Character. Screening method (default "auto").
#' @param alpha_cor Numeric. Significance level for univariate (default 0.05).
#' @param lasso_lambda Numeric. LASSO penalty (default 0.05).
#' @param lasso_alpha Numeric. Elastic-net mixing (default 1 = pure LASSO).
#' @param n_stability_boot Integer. Stability selection iterations (default 50).
#' @param stability_threshold Numeric. Stability selection frequency cutoff (default 0.6).
#' @param rfe_step_frac Numeric. RFE elimination fraction per step (default 0.2).
#' @param rfe_min_features Integer. Minimum features for RFE (default 5).
#' @param n_cv_folds Integer. CV folds for evaluation (default 5).
#' @param top_univariate Integer. Top N univariate features for panel (default 20).
#' @param report_file Optional file to save report.
#' @param seed Integer. Random seed (default 42).
#' @param verbose Logical. Print progress messages? (default TRUE).
#' @return List with all intermediate and final results.
#' @export
pipeline <- function(X, y,
                     var_threshold = 0.01,
                     missing_threshold = 0.3,
                     norm_method = "zscore",
                     univariate_method = "auto",
                     alpha_cor = 0.05,
                     lasso_lambda = 0.05,
                     lasso_alpha = 1,
                     n_stability_boot = 50,
                     stability_threshold = 0.6,
                     rfe_step_frac = 0.2,
                     rfe_min_features = 5,
                     n_cv_folds = 5,
                     top_univariate = 20,
                     report_file = NULL,
                     seed = 42,
                     verbose = TRUE) {
  msg <- function(...) if (verbose) message("[pipeline] ", ...)

  # --- Step 1: Preprocessing ---
  msg("Step 1: Preprocessing...")
  pre <- preprocess_data(X, y = y,
                         var_threshold = var_threshold,
                         missing_threshold = missing_threshold,
                         norm_method = norm_method)
  X_clean <- pre$X
  y_clean <- pre$y
  msg(sprintf("  Retained %d of %d features.", ncol(X_clean), ncol(as.matrix(X))))
  msg(sprintf("  Removed %d low-var, %d high-miss features.",
              length(pre$removed_var), length(pre$removed_miss)))

  # --- Step 2: Univariate Screening ---
  msg("Step 2: Univariate screening...")
  screen <- screen_univariate(X_clean, y_clean, method = univariate_method)
  n_sig <- sum(!is.na(screen$p_BH) & screen$p_BH <= alpha_cor)
  msg(sprintf("  %d features significant at BH-corrected alpha=%.2f", n_sig, alpha_cor))

  # --- Step 3: LASSO ---
  msg("Step 3: LASSO feature selection...")
  lasso_mod <- tryCatch(
    fit_lasso(X_clean, y_clean, lambda = lasso_lambda, alpha = lasso_alpha),
    error = function(e) { msg("  LASSO failed:", e$message); NULL }
  )
  if (!is.null(lasso_mod)) {
    msg(sprintf("  LASSO selected %d features.", length(lasso_selected(lasso_mod))))
  }

  # --- Step 4: RFE ---
  msg("Step 4: Recursive Feature Elimination...")
  rfe_res <- tryCatch(
    recursive_feature_elimination(X_clean, y_clean,
                                   step_frac = rfe_step_frac,
                                   min_features = rfe_min_features,
                                   lambda = lasso_lambda, seed = seed),
    error = function(e) { msg("  RFE failed:", e$message); NULL }
  )
  if (!is.null(rfe_res)) {
    msg(sprintf("  RFE best panel: %d features (step %d, AUC=%.3f).",
                length(rfe_res$best_features), rfe_res$best_step,
                rfe_res$history$auc[rfe_res$best_step]))
  }

  # --- Step 5: Stability Selection ---
  msg("Step 5: Stability selection...")
  stab_res <- tryCatch(
    select_features_stability(X_clean, y_clean,
                               n_boot = n_stability_boot,
                               threshold = stability_threshold,
                               lambda = lasso_lambda, seed = seed),
    error = function(e) { msg("  Stability failed:", e$message); NULL }
  )
  if (!is.null(stab_res)) {
    msg(sprintf("  Stability selected %d features (threshold=%.2f).",
                length(stab_res$selected), stab_res$threshold))
  }

  # --- Step 6: Rank Panels ---
  msg("Step 6: Ranking candidate panels...")
  rank_res <- rank_biomarker_panels(
    X_clean, y_clean,
    screen_df = screen,
    lasso_model = lasso_mod,
    rfe_result = rfe_res,
    stability_result = stab_res,
    n_folds = n_cv_folds,
    lambda = lasso_lambda,
    seed = seed,
    top_univariate = top_univariate
  )

  # --- Step 7: Report ---
  msg("Step 7: Generating report...")
  rpt <- report_results(rank_res, screen_df = screen,
                        stability_result = stab_res,
                        file = report_file)

  msg("Pipeline complete.")
  msg(sprintf("Best panel: %s (AUC=%.4f, Acc=%.4f)",
              rank_res$ranking$panel[1],
              rank_res$ranking$auc[1],
              rank_res$ranking$accuracy[1]))

  list(
    preprocessed = pre,
    screen = screen,
    lasso_model = lasso_mod,
    rfe_result = rfe_res,
    stability_result = stab_res,
    ranking = rank_res,
    report = rpt
  )
}
