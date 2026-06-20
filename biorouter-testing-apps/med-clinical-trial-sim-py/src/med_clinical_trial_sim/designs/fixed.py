"""
Fixed-sample-size trial design.

The simplest design: recruit a pre-determined total sample size, then
perform a single analysis.  No interim looks, no adaptive modifications.
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
)
from ..spending import SpendingFunction


# ---------------------------------------------------------------------------
# Sample-size formulas
# ---------------------------------------------------------------------------

def _ss_binary(p0: float, p1: float, alpha: float, power: float,
               allocation_ratio: float = 1.0) -> int:
    """Two-proportion sample size (per arm) for a two-sided Z-test.

    Uses the normal approximation.  Returns *per-arm* n.
    """
    z_alpha = _normal_ppf(1.0 - alpha / 2.0)
    z_beta = _normal_ppf(power)
    p_bar = (p0 + allocation_ratio * p1) / (1.0 + allocation_ratio)
    n1 = ((z_alpha * _sqrt(p_bar * (1.0 - p_bar) * (1.0 + 1.0 / allocation_ratio))
           + z_beta * _sqrt(p0 * (1.0 - p0) / allocation_ratio + p1 * (1.0 - p1)))
          / (p1 - p0)) ** 2
    return max(int(math.ceil(n1)), 1)


def _ss_continuous(mu0: float, mu1: float, sigma: float, alpha: float,
                   power: float, allocation_ratio: float = 1.0) -> int:
    """Two-sample sample size for a continuous endpoint."""
    z_alpha = _normal_ppf(1.0 - alpha / 2.0)
    z_beta = _normal_ppf(power)
    n = ((z_alpha + z_beta) ** 2 * sigma ** 2 * (1.0 + 1.0 / allocation_ratio)
         / (mu1 - mu0) ** 2)
    return max(int(math.ceil(n)), 1)


def _ss_tte(median_ctrl: float, hr: float, alpha: float, power: float,
            dropout_rate: float = 0.0, events_frac: float = 0.8,
            allocation_ratio: float = 1.0) -> int:
    """Schoenfeld formula for two-arm time-to-event sample size.

    Parameters
    ----------
    events_frac : float
        Fraction of recruited subjects expected to have an event.
    dropout_rate : float
        Overall dropout / loss-to-follow-up probability.
    """
    log_hr = math.log(hr)
    if abs(log_hr) < 1e-15:
        # No effect: infinite sample size needed
        return 999_999
    z_alpha = _normal_ppf(1.0 - alpha / 2.0)
    z_beta = _normal_ppf(power)
    # Number of events needed
    d = ((z_alpha + z_beta) ** 2 * (1.0 + allocation_ratio) ** 2
         / (allocation_ratio * log_hr ** 2))
    # Account for events fraction and dropouts
    n_per_arm = math.ceil(d / (2.0 * events_frac * (1.0 - dropout_rate)))
    return max(int(n_per_arm), 1)


# ---------------------------------------------------------------------------
# FixedDesign
# ---------------------------------------------------------------------------

@dataclass
class FixedDesign:
    """Fixed-sample-size clinical trial design.

    Parameters
    ----------
    outcome : OutcomeModel
        Outcome model describing the endpoint and effect sizes.
    n_per_arm : int, optional
        Fixed sample size per arm.  If None, it is computed from the
        desired power.
    alpha : float
        Two-sided significance level (default 0.05).
    power : float
        Desired power (only used if n_per_arm is None).
    dropout_rate : float
        Anticipated dropout rate — increases required sample size.
    """

    outcome: OutcomeModel
    n_per_arm: Optional[int] = None
    alpha: float = 0.05
    power: float = 0.80
    dropout_rate: float = 0.0

    def __post_init__(self):
        if self.n_per_arm is None:
            self.n_per_arm = self._compute_sample_size()

    def _compute_sample_size(self) -> int:
        """Compute per-arm sample size from the outcome model parameters."""
        adj_alpha = self.alpha  # already two-sided
        if isinstance(self.outcome, BinaryOutcome):
            n = _ss_binary(self.outcome.p_control, self.outcome.p_treatment,
                           adj_alpha, self.power)
        elif isinstance(self.outcome, ContinuousOutcome):
            n = _ss_continuous(self.outcome.mean_control, self.outcome.mean_treatment,
                               self.outcome.std_dev, adj_alpha, self.power)
        elif isinstance(self.outcome, TimeToEventOutcome):
            n = _ss_tte(self.outcome.median_control, self.outcome.hazard_ratio,
                        adj_alpha, self.power, dropout_rate=self.dropout_rate)
        else:
            raise ValueError(f"Unsupported outcome type: {type(self.outcome)}")
        # Adjust for dropout
        if self.dropout_rate > 0:
            n = int(math.ceil(n / (1.0 - self.dropout_rate)))
        return n

    # ------------------------------------------------------------------
    # Simulation interface
    # ------------------------------------------------------------------

    def generate_data(self, rng: object) -> Dict[str, object]:
        """Generate data for one trial replicate.

        Returns
        -------
        dict with keys 'ctrl', 'treat' (lists of observations),
        'n_ctrl', 'n_treat', 'z', 'p_value', 'reject'.
        """
        from ..outcomes import _ensure_rng
        rng = _ensure_rng(rng)
        n = self.n_per_arm
        ctrl = self.outcome.generate_control(n, rng)
        treat = self.outcome.generate_arm(n, rng)

        z = self.outcome.test_statistic(ctrl, treat)
        p_val = self.outcome.p_value(z)
        return {
            "ctrl": ctrl,
            "treat": treat,
            "n_ctrl": n,
            "n_treat": n,
            "z": z,
            "p_value": p_val,
            "reject": p_val < self.alpha,
            "n_analyses": 1,
            "stopped_early": False,
        }

    @property
    def total_sample_size(self) -> int:
        return self.n_per_arm * 2

    def __repr__(self) -> str:
        return (f"FixedDesign(outcome={self.outcome}, n_per_arm={self.n_per_arm}, "
                f"alpha={self.alpha}, power={self.power})")
