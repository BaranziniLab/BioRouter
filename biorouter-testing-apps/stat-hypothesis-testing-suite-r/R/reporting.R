#' Comprehensive Reporting Function
#'
#' Given data and a test choice, runs assumption checks and the test,
#' printing a readable report.

#' Generate a comprehensive hypothesis test report
#'
#' @param ... Arguments passed to the test function
#' @param test Character: name of the test to run. One of:
#'   "one_sample_t", "two_sample_t", "paired_t", "welch_t",
#'   "one_way_anova", "two_way_anova", "f_test_variances",
#'   "pearson_r", "simple_regression", "multiple_regression",
#'   "wilcoxon_rank_sum", "wilcoxon_signed_rank", "kruskal_wallis",
#'   "mann_whitney", "spearman_rho", "sign_test",
#'   "chi_square_gof", "chi_square_independence", "fisher_exact", "mcnemar",
#'   "shapiro_wilk", "ks_test"
#' @param alpha Numeric: significance level (default 0.05)
#' @param check_assumptions Logical: whether to run assumption checks (default TRUE)
#' @param print_report Logical: whether to print the report (default TRUE)
#' @return A list containing the test result and assumption checks
#' @export
hyp_report <- function(..., test, alpha = 0.05,
                       check_assumptions = TRUE, print_report = TRUE) {

  # Capture the arguments
  args <- list(...)

  # Run assumption checks if requested
  assumptions <- NULL
  if (check_assumptions) {
    assumptions <- run_assumption_checks(test, args)
  }

  # Run the test
  result <- run_test(test, args)

  if (print_report) {
    print_report_text(result, assumptions, alpha, test)
  }

  return(list(
    result = result,
    assumptions = assumptions,
    alpha = alpha
  ))
}

# ---- Internal: Run the appropriate test ----

run_test <- function(test_name, args) {
  switch(test_name,
    "one_sample_t" = do.call(hyp_one_sample_t, args),
    "two_sample_t" = do.call(hyp_two_sample_t, args),
    "paired_t" = do.call(hyp_paired_t, args),
    "welch_t" = do.call(hyp_welch_t, args),
    "one_way_anova" = do.call(hyp_one_way_anova, args),
    "two_way_anova" = do.call(hyp_two_way_anova, args),
    "f_test_variances" = do.call(hyp_f_test_variances, args),
    "pearson_r" = do.call(hyp_pearson_r, args),
    "simple_regression" = do.call(hyp_simple_regression, args),
    "multiple_regression" = do.call(hyp_multiple_regression, args),
    "wilcoxon_rank_sum" = do.call(hyp_wilcoxon_rank_sum, args),
    "wilcoxon_signed_rank" = do.call(hyp_wilcoxon_signed_rank, args),
    "kruskal_wallis" = do.call(hyp_kruskal_wallis, args),
    "mann_whitney" = do.call(hyp_mann_whitney, args),
    "spearman_rho" = do.call(hyp_spearman_rho, args),
    "sign_test" = do.call(hyp_sign_test, args),
    "chi_square_gof" = do.call(hyp_chi_square_gof, args),
    "chi_square_independence" = do.call(hyp_chi_square_independence, args),
    "fisher_exact" = do.call(hyp_fisher_exact, args),
    "mcnemar" = do.call(hyp_mcnemar, args),
    "shapiro_wilk" = do.call(hyp_shapiro_wilk, args),
    "ks_test" = do.call(hyp_ks_test, args),
    stop(paste("Unknown test:", test_name))
  )
}

# ---- Internal: Run assumption checks ----

run_assumption_checks <- function(test_name, args) {
  checks <- list()

  # Check normality for parametric tests
  parametric_tests <- c("one_sample_t", "two_sample_t", "paired_t",
                         "welch_t", "one_way_anova", "pearson_r",
                         "simple_regression", "multiple_regression")

  if (test_name %in% parametric_tests) {
    if (test_name == "one_sample_t" || test_name == "ks_test") {
      x <- args$x
      if (!is.null(x)) {
        sw <- hyp_shapiro_wilk(x)
        ks <- hyp_ks_test(x)
        checks$normality_x <- list(shapiro_wilk = sw, ks_test = ks)
      }
    } else if (test_name %in% c("two_sample_t", "welch_t")) {
      if (!is.null(args$x) && !is.null(args$y)) {
        sw_x <- hyp_shapiro_wilk(args$x)
        sw_y <- hyp_shapiro_wilk(args$y)
        checks$normality_x <- list(shapiro_wilk = sw_x)
        checks$normality_y <- list(shapiro_wilk = sw_y)
        # Check equal variances (for two_sample_t)
        if (test_name == "two_sample_t") {
          ft <- hyp_f_test_variances(args$x, args$y)
          checks$equal_variances <- ft
        }
      }
    } else if (test_name == "paired_t") {
      if (!is.null(args$x) && !is.null(args$y)) {
        d <- args$x - args$y
        sw <- hyp_shapiro_wilk(d)
        checks$normality_differences <- list(shapiro_wilk = sw)
      }
    }
  }

  # Check expected frequencies for chi-square
  if (test_name == "chi_square_gof") {
    obs <- args$observed
    exp_val <- args$expected
    if (!is.null(obs) && !is.null(exp_val)) {
      checks$expected_frequencies <- all(exp_val >= 5)
    }
  }

  # Check cell counts for Fisher's exact
  if (test_name == "fisher_exact") {
    checks$cell_count_note <- "Fisher's exact is appropriate for small cell counts"
  }

  return(checks)
}

# ---- Internal: Print the report ----

print_report_text <- function(result, assumptions, alpha, test_name) {
  sep_line <- paste(rep("=", 60), collapse = "")
  dash_line <- paste(rep("-", 40), collapse = "")

  cat("\n")
  cat(sep_line, "\n")
  cat(sprintf("  HYPOTHESIS TEST REPORT: %s\n", result$test_name))
  cat(sep_line, "\n\n")

  # Assumption checks
  if (!is.null(assumptions) && length(assumptions) > 0) {
    cat("ASSUMPTION CHECKS:\n")
    cat(dash_line, "\n")
    for (name in names(assumptions)) {
      check <- assumptions[[name]]
      if (is.list(check) && !is.null(check$shapiro_wilk)) {
        sw <- check$shapiro_wilk
        status <- ifelse(sw$p_value > alpha, "PASS", "FAIL")
        cat(sprintf("  [%s] Normality (Shapiro-Wilk): W = %.4f, p = %.4f\n",
                    status, sw$statistic, sw$p_value))
      } else if (is.logical(check)) {
        status <- ifelse(check, "PASS", "FAIL")
        cat(sprintf("  [%s] Expected frequencies >= 5\n", status))
      } else if (is.character(check)) {
        cat(sprintf("  [NOTE] %s\n", check))
      } else if (is.list(check) && !is.null(check$statistic)) {
        cat(sprintf("  [INFO] %s: p = %.4f\n", check$test_name, check$p_value))
      }
    }
    cat("\n")
  }

  # Test results
  cat("TEST RESULTS:\n")
  cat(dash_line, "\n")
  cat(sprintf("  Test: %s\n", result$method))
  cat(sprintf("  Statistic: %.6f\n", result$statistic))
  cat(sprintf("  df: %s\n", paste(result$df, collapse = ", ")))
  cat(sprintf("  p-value: %.6f\n", result$p_value))

  if (!is.null(result$effect_size)) {
    cat(sprintf("  %s: %.6f\n", result$effect_name, result$effect_size))
  }

  if (!is.null(result$ci_lower) && !is.null(result$ci_upper)) {
    cat(sprintf("  95%% CI: [%.6f, %.6f]\n", result$ci_lower, result$ci_upper))
  }

  # Interpretation
  cat("\nINTERPRETATION:\n")
  cat(dash_line, "\n")

  if (result$p_value < alpha) {
    cat(sprintf("  REJECT the null hypothesis (p = %.4f < %.4f)\n",
                result$p_value, alpha))
    cat("  There is significant evidence against the null hypothesis.\n")
  } else {
    cat(sprintf("  FAIL TO REJECT the null hypothesis (p = %.4f >= %.4f)\n",
                result$p_value, alpha))
    cat("  There is insufficient evidence against the null hypothesis.\n")
  }

  # Additional info from extras
  if (length(result$extra) > 0) {
    cat("\nADDITIONAL DETAILS:\n")
    cat(dash_line, "\n")
    for (nm in names(result$extra)) {
      val <- result$extra[[nm]]
      if (is.numeric(val) && length(val) == 1) {
        cat(sprintf("  %s: %.6f\n", nm, val))
      } else if (!is.null(val) && !is.data.frame(val)) {
        cat(sprintf("  %s: %s\n", nm, paste(val, collapse = ", ")))
      }
    }
  }

  cat("\n")
  cat(sep_line, "\n")
  cat("\n")
}
