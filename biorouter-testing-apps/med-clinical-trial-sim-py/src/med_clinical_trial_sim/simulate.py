"""
Monte Carlo simulation engine for clinical trial designs.

Runs many replicate trials and collects operating characteristics
(type-I error, power, expected sample size, stopping probabilities,
etc.).
"""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional

from .designs.fixed import FixedDesign
from .designs.group_sequential import GroupSequentialDesign
from .designs.response_adaptive import ResponseAdaptiveDesign

# Union type for any design
TrialDesign = FixedDesign | GroupSequentialDesign | ResponseAdaptiveDesign


# ---------------------------------------------------------------------------
# Single-replicate result container
# ---------------------------------------------------------------------------

@dataclass
class SimResult:
    """Outcome of a single simulated trial replicate."""

    reject: bool
    n_ctrl: int
    n_treat: int
    n_analyses: int
    stopped_early: bool
    stop_reason: Optional[str]
    z: float
    p_value: float
    total_n: int = 0
    looks: Optional[List[float]] = None
    alloc_probs: Optional[List[List[float]]] = None

    def __post_init__(self):
        if self.total_n == 0:
            self.total_n = self.n_ctrl + self.n_treat


# ---------------------------------------------------------------------------
# Simulation runner
# ---------------------------------------------------------------------------

@dataclass
class SimulationOutput:
    """Aggregated output from a full Monte Carlo simulation."""

    design: Any
    n_reps: int
    seed: Optional[int]
    results: List[SimResult] = field(repr=False)
    elapsed_sec: float = 0.0

    # Aggregated OCs (computed lazily)
    _type_i_error: Optional[float] = field(default=None, repr=False)
    _power: Optional[float] = field(default=None, repr=False)
    _mean_sample_size: Optional[float] = field(default=None, repr=False)
    _mean_analyses: Optional[float] = field(default=None, repr=False)
    _stop_efficacy: Optional[float] = field(default=None, repr=False)
    _stop_futility: Optional[float] = field(default=None, repr=False)

    @property
    def rejections(self) -> int:
        return sum(1 for r in self.results if r.reject)

    @property
    def rejections_rate(self) -> float:
        return self.rejections / self.n_reps if self.n_reps > 0 else 0.0

    @property
    def mean_sample_size(self) -> float:
        if self._mean_sample_size is None:
            self._mean_sample_size = sum(r.total_n for r in self.results) / self.n_reps
        return self._mean_sample_size

    @property
    def mean_analyses(self) -> float:
        if self._mean_analyses is None:
            self._mean_analyses = sum(r.n_analyses for r in self.results) / self.n_reps
        return self._mean_analyses

    @property
    def frac_efficacy_stop(self) -> float:
        return sum(1 for r in self.results if r.stop_reason == "efficacy") / self.n_reps

    @property
    def frac_futility_stop(self) -> float:
        return sum(1 for r in self.results if r.stop_reason == "futiltiy") / self.n_reps

    def summary(self) -> Dict[str, Any]:
        """Return a summary dictionary of operating characteristics."""
        return {
            "design": repr(self.design),
            "n_reps": self.n_reps,
            "rejection_rate": round(self.rejections_rate, 4),
            "mean_sample_size": round(self.mean_sample_size, 1),
            "mean_analyses": round(self.mean_analyses, 2),
            "frac_efficacy_stop": round(self.frac_efficacy_stop, 4),
            "frac_futility_stop": round(self.frac_futility_stop, 4),
            "elapsed_sec": round(self.elapsed_sec, 2),
        }


# ---------------------------------------------------------------------------
# Main simulation function
# ---------------------------------------------------------------------------

def run_simulation(
    design: TrialDesign,
    n_reps: int = 1000,
    seed: Optional[int] = None,
    verbose: bool = False,
) -> SimulationOutput:
    """Run a Monte Carlo simulation of a clinical trial design.

    Parameters
    ----------
    design : TrialDesign
        The trial design to simulate.
    n_reps : int
        Number of Monte Carlo replicates.
    seed : int, optional
        Random seed for reproducibility.
    verbose : bool
        If True, print progress every 10% of reps.

    Returns
    -------
    SimulationOutput
        Aggregated simulation results.
    """
    from .outcomes import _make_rng

    rng = _make_rng(seed)
    results: List[SimResult] = []

    t0 = time.time()
    report_interval = max(1, n_reps // 10)

    for i in range(n_reps):
        data = design.generate_data(rng)

        sr = SimResult(
            reject=data["reject"],
            n_ctrl=data["n_ctrl"],
            n_treat=data["n_treat"],
            n_analyses=data["n_analyses"],
            stopped_early=data.get("stopped_early", False),
            stop_reason=data.get("stop_reason"),
            z=data["z"],
            p_value=data["p_value"],
            total_n=data["n_ctrl"] + data["n_treat"],
            looks=data.get("looks"),
            alloc_probs=data.get("alloc_probs"),
        )
        results.append(sr)

        if verbose and (i + 1) % report_interval == 0:
            pct = 100.0 * (i + 1) / n_reps
            print(f"  [{pct:5.1f}%] rep {i+1}/{n_reps} — running rejection rate: "
                  f"{sum(1 for r in results if r.reject)/(i+1):.3f}")

    elapsed = time.time() - t0

    return SimulationOutput(
        design=design,
        n_reps=n_reps,
        seed=seed,
        results=results,
        elapsed_sec=elapsed,
    )
