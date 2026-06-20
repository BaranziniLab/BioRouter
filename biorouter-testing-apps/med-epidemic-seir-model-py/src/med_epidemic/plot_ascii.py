"""ASCII plotting utility for epidemic trajectories.

Renders terminal-friendly plots of compartment curves using basic
ASCII characters.
"""

from __future__ import annotations

from typing import List, Optional

import numpy as np


# Characters used for each series
_PALETTE = ["#", "o", "x", "+", "*", "@", "%", "~"]


def ascii_plot(
    t: np.ndarray,
    series: List[np.ndarray],
    labels: List[str],
    width: int = 80,
    height: int = 24,
    title: str = "",
) -> str:
    """Render an ASCII plot of one or more time-series.

    Parameters
    ----------
    t : 1-D time axis
    series : list of 1-D y arrays (same length as *t*)
    labels : legend labels for each series
    width, height : character dimensions of the plot area
    title : plot title
    """
    if not series:
        return ""

    n_series = len(series)
    y_all = np.concatenate(series)
    y_min = float(np.nanmin(y_all))
    y_max = float(np.nanmax(y_all))

    # avoid division by zero
    y_range = y_max - y_min if y_max != y_min else 1.0

    t_min, t_max = float(t[0]), float(t[-1])
    t_range = t_max - t_min if t_max != t_min else 1.0

    # Build the canvas
    canvas: List[List[str]] = [[" "] * width for _ in range(height)]

    # Map each series to canvas
    for s_idx, y in enumerate(series):
        char = _PALETTE[s_idx % len(_PALETTE)]
        for col in range(width):
            t_val = t_min + col / (width - 1) * t_range
            # interpolate y at this t
            y_val = float(np.interp(t_val, t, y))
            row = height - 1 - int((y_val - y_min) / y_range * (height - 1))
            row = max(0, min(height - 1, row))
            canvas[row][col] = char

    # Render
    lines: List[str] = []

    if title:
        lines.append(title.center(width + 20))
        lines.append("")

    # y-axis labels: top and bottom
    y_top_label = f"{y_max:>10.1f}"
    y_bot_label = f"{y_min:>10.1f}"

    for r in range(height):
        if r == 0:
            prefix = y_top_label + " |"
        elif r == height - 1:
            prefix = y_bot_label + " |"
        elif r == height // 2:
            mid_val = (y_max + y_min) / 2
            prefix = f"{mid_val:>10.1f} |"
        else:
            prefix = " " * 11 + "|"
        lines.append(prefix + "".join(canvas[r]))

    # x-axis
    x_line = " " * 12 + "+" + "-" * (width - 1)
    lines.append(x_line)
    x_labels = f"  {t_min:.0f}" + " " * (width - len(f"{t_min:.0f}") - len(f"{t_max:.0f}") - 2) + f"{t_max:.0f}"
    lines.append(" " * 12 + x_labels)
    lines.append(f"  {'Time (days)':^{width + 8}}")

    # Legend
    legend_parts = []
    for i, lbl in enumerate(labels):
        char = _PALETTE[i % len(_PALETTE)]
        legend_parts.append(f"  {char} = {lbl}")
    lines.append("")
    lines.append("  Legend:" + "   ".join(legend_parts))

    return "\n".join(lines)
