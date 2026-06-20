
# ---------------------------------------------------------------------------
# formula.R — build model matrix from a formula + data frame
# Handles factors (dummy coding), intercept, numeric predictors
# ---------------------------------------------------------------------------

build_model_matrix = function(formula, data) {
  # Use stats::model.frame and stats::model.matrix for robust formula parsing
  # This handles factors, interactions, intercept automatically
  mf = stats::model.frame(formula, data = data, na.action = na.pass)
  y  = stats::model.response(mf)
  X  = stats::model.matrix(formula, data = mf)

  # Ensure no NAs remain in X
  complete = complete.cases(X)
  if (!all(complete)) {
    X = X[complete, , drop = FALSE]
    y = y[complete]
  }

  list(y = y, X = X, terms = stats::terms(mf))
}
