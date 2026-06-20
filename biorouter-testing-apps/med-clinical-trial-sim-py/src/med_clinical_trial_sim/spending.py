"""
Alpha-spending functions for group-sequential clinical trials.

Implements the Lan-DeMets framework for pre-specified Type-I error
spending across interim analyses.  Given an overall alpha, the number
of looks K, and an information fraction at each look, the spending
function determines the local significance level α_k at each analysis.

References
----------
Lan, K. K. G. & DeMets, D. L. (1983). Discrete sequential boundaries
for clinical trials. *Biometrika*, 70(3), 597–603.

O'Brien, P. C. & Fleming, T. R. (1979). A multiple testing procedure
for clinical trials. *Biometrics*, 35(3), 549–556.

Pocock, S. J. (1977). Group sequential methods in the design and
analysis of clinical trials. *Biometrika*, 64(2), 191–199.
"""

from __future__ import annotations

import math
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import List, Optional, Tuple


# ---------------------------------------------------------------------------
# Alpha-spending function base
# ---------------------------------------------------------------------------

class SpendingFunction(ABC):
    """Abstract base class for alpha-spending functions."""

    @abstractmethod
    def spend(self, alpha: float, t: float) -> float:
        """Cumulative alpha spent by information fraction *t* (0 ≤ t ≤ 1).

        Parameters
        ----------
        alpha : float
            Total one-sided Type-I error budget.
        t : float
            Information fraction (proportion of total information observed).

        Returns
        -------
        float
            Cumulative alpha spent up to *t*.
        """
        ...


# ---------------------------------------------------------------------------
# O'Brien-Fleming type (Lan-DeMets approximation)
# ---------------------------------------------------------------------------

class OBrienFleming(SpendingFunction):
    """O'Brien-Fleming-type alpha-spending function (Lan-DeMets).

    α*(t) = 2 − 2·Φ( z_{α/2} / √t )

    This yields very small early spends, preserving most alpha for the
    final analysis — similar in spirit to the original O'Brien-Fleming
    boundaries.
    """

    def spend(self, alpha: float, t: float) -> float:
        if t <= 0.0:
            return 0.0
        if t >= 1.0:
            return alpha
        # z_{α/2} from the normal inverse CDF
        from .outcomes import _normal_ppf, _normal_cdf
        z_alpha2 = _normal_ppf(1.0 - alpha / 2.0)
        z = z_alpha2 / math.sqrt(t)
        return 2.0 * (1.0 - _normal_cdf(z))


# ---------------------------------------------------------------------------
# Pocock type (Lan-DeMets approximation)
# ---------------------------------------------------------------------------

class Pocock(SpendingFunction):
    """Pocock-type alpha-spending function (Lan-DeMets).

    α*(t) = α · ln(1 + (e − 1)·t)

    This spends alpha more evenly across analyses, yielding earlier
    stopping boundaries that are wider (closer to each other) than
    O'Brien-Fleming.
    """

    def spend(self, alpha: float, t: float) -> float:
        if t <= 0.0:
            return 0.0
        if t >= 1.0:
            return alpha
        return alpha * math.log(1.0 + (math.e - 1.0) * t)


# ---------------------------------------------------------------------------
# Linear spending (for comparison / flexibility)
# ---------------------------------------------------------------------------

class LinearSpending(SpendingFunction):
    """Linear alpha-spending: α*(t) = α·t.

    The simplest possible allocation — equal information-fraction
    proportional spending.
    """

    def spend(self: "LinearSpending", alpha: float, t: float) -> float:
        if t <= 0.0:
            return 0.0
        if t >= 1.0:
            return alpha
        return alpha * t


# ---------------------------------------------------------------------------
# Compute local (incremental) significance levels
# ---------------------------------------------------------------------------

@dataclass
class SpendingPlan:
    """Pre-computed spending plan for a group-sequential trial.

    Attributes
    ----------
    alpha : float
        Total one-sided Type-I error.
    n_analyses : int
        Number of analyses (including the final look).
    info_fractions : list[float]
        Information fraction at each analysis (must be strictly increasing,
        ending at 1.0).
    cumulative_spends : list[float]
        Cumulative alpha spent up to each analysis.
    local_alphas : list[float]
        Incremental (local) one-sided alpha at each analysis.
    """

    alpha: float
    n_analyses: int
    info_fractions: List[float]
    cumulative_spends: List[float]
    local_alphas: List[float]

    @property
    def critical_values(self) -> List[float]:
        """One-sided Z critical values for each local alpha."""
        from .outcomes import _normal_ppf
        return [_normal_ppf(1.0 - a) for a in self.local_alphas]


def compute_spending_plan(
    spending_fn: SpendingFunction,
    alpha: float,
    n_analyses: int,
    info_fractions: Optional[List[float]] = None,
) -> SpendingPlan:
    """Compute a spending plan.

    Parameters
    ----------
    spending_fn : SpendingFunction
        The Lan-DeMets spending function to use.
    alpha : float
        Total one-sided Type-I error (e.g. 0.025).
    n_analyses : int
        Number of analyses.
    info_fractions : list[float], optional
        Information fraction at each look. If None, uses equally spaced
        fractions: [1/K, 2/K, …, K/K].
    """
    if info_fractions is None:
        info_fractions = [(k + 1) / n_analyses for k in range(n_analyses)]
    else:
        info_fractions = list(info_fractions)

    if len(info_fractions) != n_analyses:
        raise ValueError(
            f"info_fractions length ({len(info_fractions)}) != n_analyses ({n_analyses})"
        )

    cumulative = []
    for t in info_fractions:
        cumulative.append(spending_fn.spend(alpha, t))

    local = []
    prev = 0.0
    for c in cumulative:
        local.append(max(c - prev, 0.0))
        prev = c

    return SpendingPlan(
        alpha=alpha,
        n_analyses=n_analyses,
        info_fractions=info_fractions,
        cumulative_spends=cumulative,
        local_alphas=local,
    )


# ---------------------------------------------------------------------------
# Convenience: pre-built spending plans
# ---------------------------------------------------------------------------

def obrien_fleming_plan(alpha: float, n_analyses: int,
                        info_fractions: Optional[List[float]] = None) -> SpendingPlan:
    """Shorthand for an O'Brien-Fleming spending plan."""
    return compute_spending_plan(OBrienFleming(), alpha, n_analyses, info_fractions)


def pocock_plan(alpha: float, n_analyses: int,
                info_fractions: Optional[List[float]] = None) -> SpendingPlan:
    """Shorthand for a Pocock spending plan."""
    return compute_spending_plan(Pocock(), alpha, n_analyses, info_fractions)
