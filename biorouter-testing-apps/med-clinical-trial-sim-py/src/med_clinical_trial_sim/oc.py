"""
Operating characteristics (OC) table and reporting.

Aggregates simulation results across multiple scenarios (effect sizes,
sample sizes, etc.) and formats them for human-readable output.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Sequence, Tuple

from .simulate import SimulationOutput


# ---------------------------------------------------------------------------
# CI helpers (Wilson score for proportions)
# ---------------------------------------------------------------------------

def _wilson_ci(count: int, n: int, confidence: float = 0.95) -> Tuple[float, float]:
    """Wilson score interval for a binomial proportion."""
    if n == 0:
        return (0.0, 0.0)
    p_hat = count / n
    from .outcomes import _normal_ppf
    z = _normal_ppf(1.0 - (1.0 - confidence) / 2.0)
    denom = 1.0 + z ** 2 / n
    centre = (p_hat + z ** 2 / (2.0 * n)) / denom
    margin = z * math.sqrt((p_hat * (1.0 - p_hat) + z ** 2 / (4.0 * n)) / n) / denom
    return (max(centre - margin, 0.0), min(centre + margin, 1.0))


# ---------------------------------------------------------------------------
# Single-scenario row
# ---------------------------------------------------------------------------

@dataclass
class OCRow:
    """One row of an operating-characteristics table."""

    scenario: str
    n_reps: int
    rejection_rate: float
    ci_lower: float
    ci_upper: float
    mean_n: float
    mean_analyses: float
    frac_efficacy: float
    frac_futility: float

    def to_dict(self) -> Dict[str, Any]:
        return {
            "scenario": self.scenario,
            "n_reps": self.n_reps,
            "rejection_rate": round(self.rejection_rate, 4),
            "ci_95": f"({self.ci_lower:.3f}, {self.ci_upper:.3f})",
            "mean_n": round(self.mean_n, 1),
            "mean_analyses": round(self.mean_analyses, 2),
            "frac_efficacy_stop": round(self.frac_efficacy, 4),
            "frac_futility_stop": round(self.frac_futility, 4),
        }


# ---------------------------------------------------------------------------
# OC Table
# ---------------------------------------------------------------------------

@dataclass
class OCTable:
    """Operating characteristics table across multiple scenarios."""

    rows: List[OCRow] = field(default_factory=list)

    @classmethod
    def from_simulation(cls, sim: SimulationOutput, scenario: str = "") -> "OCTable":
        """Build an OC table from a single SimulationOutput."""
        oc = sim.rejections_rate
        n_rej = sim.rejections
        n_total = sim.n_reps
        ci_lo, ci_hi = _wilson_ci(n_rej, n_total)

        row = OCRow(
            scenario=scenario or repr(sim.design),
            n_reps=n_total,
            rejection_rate=oc,
            ci_lower=ci_lo,
            ci_upper=ci_hi,
            mean_n=sim.mean_sample_size,
            mean_analyses=sim.mean_analyses,
            frac_efficacy=sim.frac_efficacy_stop,
            frac_futility=sim.frac_futility_stop,
        )
        return cls(rows=[row])

    @classmethod
    def from_simulations(cls, sims: Sequence[Tuple[str, SimulationOutput]]) -> "OCTable":
        """Build a multi-row OC table from (label, SimulationOutput) pairs."""
        rows = []
        for label, sim in sims:
            oc = sim.rejections_rate
            n_rej = sim.rejections
            ci_lo, ci_hi = _wilson_ci(n_rej, sim.n_reps)
            rows.append(OCRow(
                scenario=label,
                n_reps=sim.n_reps,
                rejection_rate=oc,
                ci_lower=ci_lo,
                ci_upper=ci_hi,
                mean_n=sim.mean_sample_size,
                mean_analyses=sim.mean_analyses,
                frac_efficacy=sim.frac_efficacy_stop,
                frac_futility=sim.frac_futility_stop,
            ))
        return cls(rows=rows)

    def format_table(self, width: int = 100) -> str:
        """Return a formatted text table."""
        headers = [
            "Scenario", "N_reps", "Rej. Rate", "95% CI",
            "Mean N", "Mean Analyses", "Efficacy %", "Futility %",
        ]
        col_widths = [max(len(h) for h in headers)]
        # Compute column widths from data
        data_rows = []
        for row in self.rows:
            d = row.to_dict()
            data_rows.append(d)

        col_widths = []
        for i, h in enumerate(headers):
            vals = [h] + [str(list(d.values())[i]) for d in data_rows]
            col_widths.append(max(len(v) for v in vals))

        def fmt_row(vals):
            parts = [str(v).ljust(w) for v, w in zip(vals, col_widths)]
            return " | ".join(parts)

        sep = "-+-".join("-" * w for w in col_widths)
        lines = [
            fmt_row(headers),
            sep,
        ]
        for d in data_rows:
            lines.append(fmt_row(list(d.values())))

        return "\n".join(lines)

    def __str__(self) -> str:
        return self.format_table()


# ---------------------------------------------------------------------------
# Convenience: run scenarios and build table
# ---------------------------------------------------------------------------

def build_oc_table(
    simulations: Sequence[Tuple[str, Any]],
) -> OCTable:
    """Build an OC table from pre-run simulations.

    Parameters
    ----------
    simulations : list of (label, SimulationOutput)
    """
    return OCTable.from_simulations(simulations)
