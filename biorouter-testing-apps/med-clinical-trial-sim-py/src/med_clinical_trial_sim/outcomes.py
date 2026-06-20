"""
Outcome models for clinical trials.

Supports three endpoint types:
- Binary (e.g., response vs. no-response)
- Continuous (e.g., change from baseline in biomarker)
- Time-to-event (e.g., progression-free survival)

Each model can generate random observations for treatment and control arms
given effect-size parameters, and compute a two-sample test statistic (Z or log-rank).
"""

from __future__ import annotations

import math
import random
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum
from typing import List, Optional, Sequence, Tuple


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _erf(x: float) -> float:
    """Error function via Abramowitz & Stegun 7.1.26 approximation.

    Max absolute error ~ 1.5e-7.
    """
    sign = 1.0 if x >= 0 else -1.0
    ax = abs(x)
    t = 1.0 / (1.0 + 0.325909 * ax)
    poly = t * (0.254829592 + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))))
    result = 1.0 - poly * math.exp(-ax * ax)
    return sign * result


def _normal_cdf(x: float) -> float:
    """Standard-normal CDF via error-function."""
    return 0.5 * (1.0 + _erf(x / math.sqrt(2.0)))


def _normal_ppf(p: float) -> float:
    """Rational approximation to the standard-normal inverse CDF (Abramowitz & Stegun 26.2.23).

    Accurate to ~4.5e-4 for 1e-7 < p < 1-1e-7.
    """
    if p <= 0.0 or p >= 1.0:
        raise ValueError("p must be in (0, 1)")
    if p < 0.5:
        return -_normal_ppf(1.0 - p)
    t = math.sqrt(-2.0 * math.log(1.0 - p))
    c0, c1, c2 = 2.515517, 0.802853, 0.010328
    d1, d2, d3 = 1.432788, 0.189269, 0.001308
    return t - (c0 + c1 * t + c2 * t * t) / (1.0 + d1 * t + d2 * t * t + d3 * t * t * t)


def _chi2_cdf_1df(x: float) -> float:
    """CDF of chi-squared with 1 df via the standard-normal CDF."""
    if x <= 0.0:
        return 0.0
    return 2.0 * _normal_cdf(math.sqrt(x)) - 1.0


def _chi2_ppf_1df(p: float) -> float:
    """PPF (inverse CDF) of chi-squared with 1 df."""
    return _normal_ppf((1.0 + p) / 2.0) ** 2


def _chi2_sf_1df(x: float) -> float:
    """Survival function (1-CDF) of chi-squared with 1 df."""
    return 1.0 - _chi2_cdf_1df(x)


# ---------------------------------------------------------------------------
# NumPy shim — use numpy when available, else fall back to random module
# ---------------------------------------------------------------------------

try:
    import numpy as np
    from numpy.random import Generator as _RNG

    def _make_rng(seed):
        return np.random.default_rng(seed)

    def _ensure_rng(rng):
        """Wrap a raw seed or non-RNG object into a proper RNG."""
        if isinstance(rng, _RNG):
            return rng
        return _make_rng(rng)

    def _rand_normal(rng, size: int) -> list:
        return _ensure_rng(rng).standard_normal(size).tolist()

    def _rand_uniform(rng, size: int) -> list:
        return _ensure_rng(rng).random(size).tolist()

    def _rand_exponential(rng, size: int) -> list:
        return _ensure_rng(rng).exponential(1.0, size).tolist()

    def _sum(xs: Sequence[float]) -> float:
        return float(np.sum(xs))

    def _mean(xs: Sequence[float]) -> float:
        return float(np.mean(xs))

    def _var(xs: Sequence[float], ddof: int = 1) -> float:
        return float(np.var(xs, ddof=ddof))

    def _sqrt(x: float) -> float:
        return float(np.sqrt(x))

    HAS_NUMPY = True

except ImportError:
    HAS_NUMPY = False

    class _FakeRNG:
        def __init__(self, seed=None):
            if isinstance(seed, (int, float, str, bytes, bytearray)):
                self._state = random.Random(seed)
            elif seed is None:
                self._state = random.Random()
            else:
                # For non-seedable objects (e.g. object()), use a random seed
                self._state = random.Random()

        def standard_normal(self, size):
            return [self._state.gauss(0.0, 1.0) for _ in range(size)]

        def random(self, size):
            return [self._state.random() for _ in range(size)]

        def exponential(self, _scale=1.0, size=1):
            return [self._state.expovariate(1.0 / _scale) for _ in range(size)]

    def _make_rng(seed):
        return _FakeRNG(seed)

    def _ensure_rng(rng):
        """Wrap a raw seed or non-RNG object into a proper RNG."""
        if isinstance(rng, _FakeRNG):
            return rng
        return _make_rng(rng)

    def _rand_normal(rng, size):
        return _ensure_rng(rng).standard_normal(size)

    def _rand_uniform(rng, size):
        return _ensure_rng(rng).random(size)

    def _rand_exponential(rng, size):
        return _ensure_rng(rng).exponential(1.0, size)

    def _sum(xs):
        return sum(xs)

    def _mean(xs):
        return sum(xs) / len(xs)

    def _var(xs, ddof=1):
        m = _mean(xs)
        return sum((x - m) ** 2 for x in xs) / (len(xs) - ddof)

    def _sqrt(x):
        return math.sqrt(x)


# ---------------------------------------------------------------------------
# Outcome types
# ---------------------------------------------------------------------------

class OutcomeType(Enum):
    BINARY = "binary"
    CONTINUOUS = "continuous"
    TIME_TO_EVENT = "tte"


# ---------------------------------------------------------------------------
# Abstract outcome model
# ---------------------------------------------------------------------------

class OutcomeModel(ABC):
    """Base class for outcome models."""

    outcome_type: OutcomeType

    @abstractmethod
    def generate_arm(self, n: int, rng: object) -> List[float]:
        """Generate *n* observations for one arm."""
        ...

    @abstractmethod
    def test_statistic(self, obs_ctrl: Sequence[float], obs_treat: Sequence[float]) -> float:
        """Compute a Z-like test statistic (two-sided).  Positive favours treatment."""
        ...

    def p_value(self, z: float) -> float:
        """Two-sided p-value from a Z statistic."""
        return 2.0 * (1.0 - _normal_cdf(abs(z)))


# ---------------------------------------------------------------------------
# Binary endpoint
# ---------------------------------------------------------------------------

@dataclass
class BinaryOutcome(OutcomeModel):
    """Binomial endpoint: response rate p_ctrl vs. p_treat.

    Parameters
    ----------
    p_control : float
        Response probability in the control arm (0–1).
    p_treatment : float
        Response probability in the treatment arm (0–1).
    """

    p_control: float = 0.30
    p_treatment: float = 0.50
    outcome_type: OutcomeType = field(default=OutcomeType.BINARY, init=False)

    def generate_arm(self, n: int, rng: object) -> List[float]:
        """Return *n* binary (0/1) observations."""
        return [1.0 if u < self.p_treatment else 0.0 for u in _rand_uniform(rng, n)]

    def generate_control(self, n: int, rng: object) -> List[float]:
        return [1.0 if u < self.p_control else 0.0 for u in _rand_uniform(rng, n)]

    def test_statistic(self, obs_ctrl: Sequence[float], obs_treat: Sequence[float]) -> float:
        """Two-proportion Z-test (pooled SE)."""
        n0, n1 = len(obs_ctrl), len(obs_treat)
        p0 = _mean(obs_ctrl)
        p1 = _mean(obs_treat)
        p_pool = (_sum(obs_ctrl) + _sum(obs_treat)) / (n0 + n1)
        se = _sqrt(p_pool * (1.0 - p_pool) * (1.0 / n0 + 1.0 / n1))
        if se < 1e-15:
            return 0.0
        return (p1 - p0) / se

    @property
    def effect_size(self) -> float:
        """Risk difference."""
        return self.p_treatment - self.p_control

    def __repr__(self) -> str:
        return f"BinaryOutcome(p_control={self.p_control}, p_treatment={self.p_treatment})"


# ---------------------------------------------------------------------------
# Continuous endpoint
# ---------------------------------------------------------------------------

@dataclass
class ContinuousOutcome(OutcomeModel):
    """Normal endpoint: Y ~ N(mu_ctrl + delta, sigma²) for treatment.

    Parameters
    ----------
    mean_control : float
        Mean outcome in the control arm.
    std_dev : float
        Common standard deviation.
    mean_treatment : float
        Mean outcome in the treatment arm.
    """

    mean_control: float = 0.0
    std_dev: float = 1.0
    mean_treatment: float = 0.5
    outcome_type: OutcomeType = field(default=OutcomeType.CONTINUOUS, init=False)

    def generate_arm(self, n: int, rng: object) -> List[float]:
        return [self.mean_treatment + s * self.std_dev for s in _rand_normal(rng, n)]

    def generate_control(self, n: int, rng: object) -> List[float]:
        return [self.mean_control + s * self.std_dev for s in _rand_normal(rng, n)]

    def test_statistic(self, obs_ctrl: Sequence[float], obs_treat: Sequence[float]) -> float:
        """Two-sample Z-test with pooled variance."""
        n0, n1 = len(obs_ctrl), len(obs_treat)
        m0, m1 = _mean(obs_ctrl), _mean(obs_treat)
        s0, s1 = _var(obs_ctrl), _var(obs_treat)
        sp = ((n0 - 1) * s0 + (n1 - 1) * s1) / (n0 + n1 - 2)
        se = _sqrt(sp * (1.0 / n0 + 1.0 / n1))
        if se < 1e-15:
            return 0.0
        return (m1 - m0) / se

    @property
    def effect_size(self) -> float:
        """Cohen's d."""
        return (self.mean_treatment - self.mean_control) / self.std_dev

    def __repr__(self) -> str:
        return (f"ContinuousOutcome(mean_control={self.mean_control}, "
                f"std_dev={self.std_dev}, mean_treatment={self.mean_treatment})")


# ---------------------------------------------------------------------------
# Time-to-event endpoint
# ---------------------------------------------------------------------------

@dataclass
class TimeToEventOutcome(OutcomeModel):
    """Exponential time-to-event endpoint with independent censoring.

    Treatment arm:  T ~ Exp(lambda_treat)  → median = ln(2)/lambda_treat
    Control arm:    T ~ Exp(lambda_control)
    Censoring:      C ~ Exp(lambda_censor)  (admin censoring horizon)

    Parameters
    ----------
    median_control : float
        Median survival in the control arm.
    hazard_ratio : float
        Hazard ratio (treatment / control).  HR < 1 = beneficial.
    median_censor : float
        Median administrative censoring time.
    """

    median_control: float = 12.0
    hazard_ratio: float = 0.65
    median_censor: float = 24.0
    outcome_type: OutcomeType = field(default=OutcomeType.TIME_TO_EVENT, init=False)

    def generate_arm(self, n: int, rng: object) -> List[float]:
        """Generate *n* observed (possibly censored) event times for the treatment arm."""
        lam_t = math.log(2.0) / (self.median_control * self.hazard_ratio)
        lam_c = math.log(2.0) / self.median_censor
        raw = _rand_exponential(rng, n)
        times = [r / lam_t for r in raw]
        censor_times = [c / lam_c for c in _rand_exponential(rng, n)]
        return [min(t, c) for t, c in zip(times, censor_times)]

    def generate_control(self, n: int, rng: object) -> List[float]:
        lam_ctrl = math.log(2.0) / self.median_control
        lam_c = math.log(2.0) / self.median_censor
        raw = _rand_exponential(rng, n)
        times = [r / lam_ctrl for r in raw]
        censor_times = [c / lam_c for c in _rand_exponential(rng, n)]
        return [min(t, c) for t, c in zip(times, censor_times)]

    def test_statistic(self, obs_ctrl: Sequence[float], obs_treat: Sequence[float]) -> float:
        """Log-rank Z-statistic (simplified: test based on observed events).

        Uses the standard log-rank formulation assuming equal allocation
        and proportional hazards.
        """
        # Combine all unique event times
        events_ctrl = [(t, 1) for t in obs_ctrl]
        events_treat = [(t, 1) for t in obs_treat]
        all_events = events_ctrl + events_treat
        all_events.sort(key=lambda x: x[0])

        n_at_risk = len(obs_ctrl) + len(obs_treat)
        o_minus_e = 0.0  # observed - expected in control
        var_sum = 0.0

        for t, arm in all_events:
            if n_at_risk <= 0:
                break
            # Number of events at this time (may have ties)
            d = sum(1 for tt, aa in all_events if abs(tt - t) < 1e-12)
            n_ctrl = sum(1 for tt, aa in events_ctrl if tt >= t - 1e-12)
            n_treat = sum(1 for tt, aa in events_treat if tt >= t - 1e-12)

            if n_ctrl + n_treat > 0:
                e_ctrl = d * n_ctrl / (n_ctrl + n_treat)
            else:
                e_ctrl = 0.0
            o_ctrl = sum(1 for tt, aa in events_ctrl if abs(tt - t) < 1e-12)

            o_minus_e += o_ctrl - e_ctrl
            if n_ctrl + n_treat > 1:
                var_sum += d * n_ctrl * n_treat / ((n_ctrl + n_treat) ** 2)

            # Remove events that occurred at this time
            events_ctrl = [(tt, aa) for tt, aa in events_ctrl if abs(tt - t) > 1e-12]
            events_treat = [(tt, aa) for tt, aa in events_treat if abs(tt - t) > 1e-12]
            n_at_risk -= d

        if var_sum < 1e-15:
            return 0.0
        return o_minus_e / _sqrt(var_sum)

    @property
    def effect_size(self) -> float:
        """Log hazard ratio."""
        return math.log(self.hazard_ratio)

    def __repr__(self) -> str:
        return (f"TimeToEventOutcome(median_control={self.median_control}, "
                f"hazard_ratio={self.hazard_ratio}, median_censor={self.median_censor})")


# ---------------------------------------------------------------------------
# Factory
# ---------------------------------------------------------------------------

def make_outcome(outcome_type: str, **kwargs) -> OutcomeModel:
    """Factory to create an OutcomeModel from a string type."""
    mapping = {
        "binary": BinaryOutcome,
        "continuous": ContinuousOutcome,
        "tte": TimeToEventOutcome,
        "time_to_event": TimeToEventOutcome,
    }
    cls = mapping.get(outcome_type.lower())
    if cls is None:
        raise ValueError(f"Unknown outcome type: {outcome_type!r}. Choose from {list(mapping)}")
    return cls(**kwargs)
