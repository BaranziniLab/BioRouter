#' Power Curves and ASCII Plotting
#'
#' Generate data frames of power vs. a varying parameter and display
#' them as ASCII plots in the terminal.
#'
#' @name power_curves
NULL

# ---------------------------------------------------------------------------
# Exported functions
# ---------------------------------------------------------------------------

#' Generate power curve data
#'
#' Evaluates a power function over a range of values for a chosen parameter.
#'
#' @param power_func A power function from this package.
#' @param varying One of \code{"n"}, \code{"d"}, \code{"alpha"}, or
#'   \code{"power"} (the parameter to vary on the x-axis).
#' @param n_range If \code{varying = "n"}, the range of sample sizes.
#' @param d_range If \code{varying = "d"}, the range of effect sizes.
#' @param alpha_range If \code{varying = "alpha"}, the range of alpha values.
#' @param ... Additional fixed arguments to \code{power_func}.
#' @return A data frame with columns \code{x} and \code{power}.
#' @export
#' @examples
#' curves <- power_curves(power_t_test, varying = "n", d = 0.5,
#'                        n_range = c(10, 100), type = "two.sample")
power_curves <- function(power_func, varying = c("n", "d", "alpha"),
                         n_range = c(5, 200),
                         d_range = c(0.1, 1.0),
                         alpha_range = c(0.001, 0.10),
                         ...) {
  varying <- match.arg(varying)

  switch(varying,
    n = {
      x_vals <- seq(n_range[1], n_range[2], length.out = 50)
      pw <- sapply(x_vals, function(x) {
        tryCatch(power_func(n = x, ...), error = function(e) NA_real_)
      })
    },
    d = {
      x_vals <- seq(d_range[1], d_range[2], length.out = 50)
      pw <- sapply(x_vals, function(x) {
        tryCatch(power_func(d = x, ...), error = function(e) NA_real_)
      })
    },
    alpha = {
      x_vals <- seq(alpha_range[1], alpha_range[2], length.out = 50)
      pw <- sapply(x_vals, function(x) {
        tryCatch(power_func(alpha = x, ...), error = function(e) NA_real_)
      })
    }
  )

  data.frame(x = x_vals, power = pw)
}

#' Print an ASCII plot to the terminal
#'
#' Renders a simple character-art line plot.
#'
#' @param x Numeric vector (x-axis).
#' @param y Numeric vector (y-axis).
#' @param width Character width of the plot (default 60).
#' @param height Character height of the plot (default 20).
#' @param xlab X-axis label.
#' @param ylab Y-axis label.
#' @param title Optional title.
#' @return Invisibly returns the character matrix of the plot.
#' @export
#' @examples
#' x <- 1:50
#' y <- 1 - (1 - 0.05)^x
#' print_ascii_plot(x, y, xlab = "n", ylab = "Power",
#'                  title = "Power vs n")
print_ascii_plot <- function(x, y, width = 60L, height = 20L,
                             xlab = "x", ylab = "y", title = NULL) {
  # Remove NAs
  ok <- !is.na(x) & !is.na(y)
  x <- x[ok]
  y <- y[ok]

  x_min <- min(x); x_max <- max(x)
  y_min <- min(y); y_max <- max(y)
  if (y_max == y_min) y_max <- y_min + 1

  # Create blank canvas
  canvas <- matrix(" ", nrow = height, ncol = width)

  # Map data to canvas coordinates
  col_idx <- round((x - x_min) / (x_max - x_min) * (width - 1)) + 1
  row_idx <- round((y - y_min) / (y_max - y_min) * (height - 1)) + 1
  row_idx <- height - row_idx + 1  # invert for top-down

  col_idx <- pmax(1L, pmin(width, col_idx))
  row_idx <- pmax(1L, pmin(height, row_idx))

  for (i in seq_along(col_idx)) {
    canvas[row_idx[i], col_idx[i]] <- "*"
  }

  # Add axis labels
  y_labels <- sprintf("%.2f", seq(y_min, y_max, length.out = 5))
  x_labels <- sprintf("%.1f", seq(x_min, x_max, length.out = min(6, width)))

  # Print
  cat("\n")
  if (!is.null(title)) {
    cat(sprintf("  %s\n", title))
  }
  cat(sprintf("  %s | %s\n", ylab, paste(rep("-", width), collapse = "")))

  for (r in 1:height) {
    label <- if (r %% max(1, height %/% 5) == 1) {
      idx <- round((height - r) / (height - 1) * 4) + 1
      idx <- min(idx, 5)
      sprintf("%6s |", y_labels[idx])
    } else {
      "       |"
    }
    cat(label, paste(canvas[r, ], collapse = ""), "\n", sep = "")
  }

  cat("       +", paste(rep("-", width), collapse = ""), "\n", sep = "")
  cat("        ", paste(x_labels, collapse = " "), "\n", sep = "")
  cat("        ", xlab, "\n\n", sep = "")

  invisible(list(canvas = canvas, x = x, y = y))
}
