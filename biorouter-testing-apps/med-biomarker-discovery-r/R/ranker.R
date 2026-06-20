#' Biomarker Panel Ranking
#'
#' Combine univariate screening, LASSO, RFE, and stability selection
#' into candidate panels and rank them by CV performance.

#' Rank biomarker panels
#'
#' @param X Numeric matrix (samples x features).
#' @param y Integer 0/1 outcome.
#' @param screen_df Data frame from screen_univariate.
#' @param lasso_model List from fit_lasso.
#' @param rfe_result List from recursive_feature_elimination.
#' @param stability_result List from select_features_stability.
#' @param n_folds Integer. CV folds for evaluation (default 5).
#' @param lambda Numeric. LASSO penalty (default 0.05).
#' @param seed Integer. Random seed (default 42).
#' @param top_univariate Integer. How many top univariate features to include as a panel (default 20).
#' @param include_all Logical. Include "All Features" as a baseline panel? (default FALSE).
#' @return List with:
#'   \item{ranking}{Data frame: panel, n_features, auc, auc_se, accuracy, accuracy_se, features.}
#'   \item{panels}{Named list of feature vectors.}
#' @export
rank_biomarker_panels <- function(X, y, screen_df = NULL,
                                   lasso_model = NULL,
                                   rfe_result = NULL,
                                   stability_result = NULL,
                                   n_folds = 5,
                                   lambda = 0.05,
                                   seed = 42,
                                   top_univariate = 20,
                                   include_all = FALSE) {
  panels <- list()

  # Panel 1: Top univariate features
  if (!is.null(screen_df)) {
    top_feats <- head(screen_df$feature, min(top_univariate, nrow(screen_df)))
    if (length(top_feats) > 0) {
      panels[["Top_Univariate"]] <- top_feats
    }
    # BH-significant only
    bh_col <- "p_BH"
    if (bh_col %in% names(screen_df)) {
      bh_feats <- screen_df$feature[!is.na(screen_df[[bh_col]]) & screen_df[[bh_col]] <= 0.05]
      if (length(bh_feats) > 0) {
        panels[["BH_Significant"]] <- bh_feats
      }
    }
  }

  # Panel 2: LASSO-selected
  if (!is.null(lasso_model)) {
    lf <- lasso_selected(lasso_model)
    if (length(lf) > 0) panels[["LASSO"]] <- lf
  }

  # Panel 3: RFE best
  if (!is.null(rfe_result)) {
    rf <- rfe_result$best_features
    if (length(rf) > 0) panels[["RFE"]] <- rf
  }

  # Panel 4: Stability selection
  if (!is.null(stability_result)) {
    sf <- stability_result$selected
    if (length(sf) > 0) panels[["Stability"]] <- sf
  }

  # Panel 5: Union of all
  all_feats <- unique(unlist(panels))
  if (length(all_feats) > 0) {
    panels[["Union_All"]] <- all_feats
  }

  # Panel 6: Intersection of LASSO + Stability (high-confidence)
  if (!is.null(lasso_model) && !is.null(stability_result)) {
    inter <- intersect(lasso_selected(lasso_model), stability_result$selected)
    if (length(inter) > 0) panels[["LASSO_Stability_Intersect"]] <- inter
  }

  # Baseline: all features
  if (include_all) {
    panels[["All_Features"]] <- colnames(X)
  }

  if (length(panels) == 0) {
    stop("No panels could be constructed from the provided results.")
  }

  # Evaluate and rank
  ranking <- cross_validate_panel(X, y, panels,
                                   n_folds = n_folds,
                                   lambda = lambda, seed = seed)

  # Attach feature lists
  ranking$features <- I(lapply(ranking$panel, function(p) panels[[p]]))

  list(ranking = ranking, panels = panels)
}
