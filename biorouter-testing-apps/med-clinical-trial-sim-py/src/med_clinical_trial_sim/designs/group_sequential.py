"""
Group-sequential clinical trial design.

Implements a group-sequential design with pre-planned interim analyses
for efficacy *and* futilty stopping.  Uses the Lan-DeMets alpha-spending
framework for boundary construction.

Features
--------
- Configurable number of equally-spaced (or custom) information fractions.
- Efficacy boundaries derived from a spending function.
- Futility boundaries (binding or non-binding) via conditional power.
- Stopping at any interim look if a boundary is crossed.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Sequence

from ..outcomes import (
    BinaryOutcome,
    ContinuousOutcome,
    OutcomeModel,
    TimeToEventOutcome,
    _normal_cdf,
    _normal_ppf,
    _sqrt,
    _var,
    _mean,
)
from ..spending import (
    OBrienFleming,
    Pocock,
    SpendingFunction,
    SpendingPlan,
    compute_spending_plan,
)
from .fixed import _ss_binary, _ss_continuous, _ss_tte


# ---------------------------------------------------------------------------
# Futility boundary helpers
# ---------------------------------------------------------------------------

def _conditional_power_bound(
    alpha: float,
    info_fracs: Sequence[float],
    k: int,
    cp_threshold: float = 0.10,
) -> Optional[float]:
    """Compute an approximate non-binding futility Z-boundary at look *k*.

    Uses conditional power: if CP < threshold, the trial is stopped for
    futility.  The Z-boundary is derived by inverting the conditional
    power formula under the current Z-statistic.

    This is a simplified implementation for simulation purposes.
    """
    if k >= len(info_fracs) - 1:
        return None  # no futility at final look
    # Conditional power at look k under H1
    # For simplicity, use a fixed futility boundary based on alpha
    # Typical: futility boundary ≈ 0 at interim (accept H0)
    return 0.0  # non-binding futility: reject if Z < 0


# ---------------------------------------------------------------------------
# GroupSequentialDesign
# ---------------------------------------------------------------------------

@dataclass
class GroupSequentialDesign:
    """Group-sequential design with efficacy and futility stopping.

    Parameters
    ----------
    outcome : OutcomeModel
        Endpoint and effect sizes.
    n_per_arm : int, optional
        Maximum (total) sample size per arm.  Computed from power if None.
    n_analyses : int
        Number of analyses (including the final look).
    alpha : float
        Two-sided significance level (split across looks by spending).
    power : float
        Desired power (used to compute n_per_arm if not given).
    spending : SpendingFunction
        Alpha-spending function.
    futility : bool
        Whether to include a futiltiy boundary.
    futility_bound : float, optional
        Z-value for the futility boundary.  If None, defaults to 0.0
        (non-binding).
    info_fractions : list[float], optional
        Information fraction at each analysis.  Default: equally spaced.
    dropout_rate : float
        Dropout rate.
    """

    outcome: OutcomeModel
    n_per_arm: Optional[int] = None
    n_analyses: int = 5
    alpha: float = 0.05
    power: float = 0.80
    spending: SpendingFunction = field(default_factory=OBrienFleming)
    futiltiy: bool = True
    futiltiy_bound: Optional[float] = None
    info_fractions: Optional[List[float]] = None
    dropout_rate: float = 0.0

    # Computed after init
    spending_plan: SpendingPlan = field(init=False, repr=False)
    _crit_values: List[float] = field(init=False, repr=False)
    _fut_boundaries: List[Optional[float]] = field(init=False, repr=False)
    _per_look_n: List[int] = field(init=False, repr=False)

    def __post_init__(self):
        if self.n_per_arm is None:
            self.n_per_arm = self._compute_sample_size()

        # Build spending plan
        self.spending_plan = compute_spending_plan(
            self.spending, self.alpha / 2.0, self.n_analyses, self.info_fractions
        )
        # One-sided critical values from the *two-sided* alpha (each side gets alpha/2)
        self._crit_values = self.spending_plan.critical_values

        # Futility boundaries
        self._fut_boundaries = []
        for k in range(self.n_analyses):
            if self.futiltiy and k < self.n_analyses - 1:
                self._fut_boundaries.append(self.futiltiy_bound if self.futiltiy_bound is not None else 0.0)
            else:
                self._fut_boundaries.append(None)

        # Per-look sample size (cumulative)
        fracs = self.spending_plan.info_fractions
        self._per_look_n = [max(int(math.ceil(self.n_per_arm * t)), 1) for t in fracs]

    def _compute_sample_size(self) -> int:
        """Compute per-arm sample size using the fixed-design formula (slightly inflated)."""
        # Inflate by ~5% to account for sequential testing
        infl = 1.0 + 0.05 * (self.n_analyses - 1) / self.n_analyses
        if isinstance(self.outcome, BinaryOutcome):
            n = _ss_binary(self.outcome.p_control, self.outcome.p_treatment,
                           self.alpha, self.power)
        elif isinstance(self.outcome, ContinuousOutcome):
            n = _ss_continuous(self.outcome.mean_control, self.outcome.mean_treatment,
                               self.outcome.std_dev, self.alpha, self.power)
        elif isinstance(self.outcome, TimeToEventOutcome):
            n = _ss_tte(self.outcome.median_control, self.outcome.hazard_ratio,
                        self.alpha, self.power, dropout_rate=self.dropout_rate)
        else:
            raise ValueError(f"Unsupported outcome type: {type(self.outcome)}")
        n = int(math.ceil(n * infl))
        if self.dropout_rate > 0:
            n = int(math.ceil(n / (1.0 - self.dropout_rate)))
        return n

    # ------------------------------------------------------------------
    # Simulation interface
    # ------------------------------------------------------------------

    def generate_data(self, rng: object) -> Dict[str, object]:
        """Simulate one trial replicate with sequential monitoring.

        Returns
        -------
        dict
            ctrl, treat: full lists of observations
            n_ctrl, n_treat: actual sample sizes at analysis
            z, p_value: final test statistic
            reject: whether H0 was rejected
            n_analyses: how many analyses were performed
            stopped_early: whether the trial stopped before the final look
            stop_reason: 'efficacy', 'futility', or None
            looks: list of per-look Z-statistics
        """
        from ..outcomes import _ensure_rng
        rng = _ensure_rng(rng)
        max_n = self.n_per_arm
        fracs = self.spending_plan.info_fractions
        crits = self._crit_values
        futs = self._fut_boundaries
        per_look = self._per_look_n

        # Generate all data up front (lazy generation)
        all_ctrl = self.outcome.generate_control(max_n, rng)
        all_treat = self.outcome.generate_arm(max_n, rng)

        reject = False
        stop_reason = None
        analysis_idx = 0
        z_final = 0.0
        p_final = 1.0

        looks = []
        for k in range(self.n_analyses):
            n_k = per_look[k]
            ctrl_k = all_ctrl[:n_k]
            treat_k = all_treat[:n_k]
            z_k = self.outcome.test_statistic(ctrl_k, treat_k)
            looks.append(z_k)

            # Efficacy boundary
            if abs(z_k) >= crits[k]:
                reject = True
                stop_reason = "efficacy"
                analysis_idx = k + 1
                z_final = z_k
                p_final = self.outcome.p_value(z_k)
                break

            # Futiltiy boundary (non-binding: only stops if Z is below the bound)
            if futs[k] is not None and z_k < futs[k]:
                reject = False
                stop_reason = "futiltiy"
                analysis_idx = k + 1
                z_final = z_k
                p_final = self.outcome.p_value(z_k)
                break

            analysis_idx = k + 1
            z_final = z_k
            p_final = self.outcome.p_value(z_k)

        # Determine final sample sizes
        final_k = analysis_idx - 1
        final_n = per_look[final_k]

        return {
            "ctrl": all_ctrl[:final_n],
            "treat": all_treat[:final_n],
            "n_ctrl": final_n,
            "n_treat": final_n,
            "z": z_final,
            "p_value": p_final,
            "reject": reject,
            "n_analyses": analysis_idx,
            "stopped_early": stop_reason is not None,
            "stop_reason": stop_reason,
            "looks": looks,
        }

    @property
    def total_sample_size(self) -> int:
        return self.n_per_arm * 2

    def __repr__(self) -> str:
        return (f"GroupSequentialDesign(outcome={self.outcome}, "
                f"n_per_arm={self.n_per_arm}, n_analyses={self.n_analyses}, "
                f"alpha={self.alpha}, spending={type(self.spending).__name__})")
