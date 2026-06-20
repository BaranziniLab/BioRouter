"""Tests for the epidemic metrics module."""

import numpy as np
import pytest

from med_epidemic.metrics import (
    compute_R0,
    compute_Rt,
    peak_infections,
    attack_rate,
    final_size,
    epidemic_duration,
    compute_metrics,
    EpidemicMetrics,
)
from med_epidemic.solver import ODESolution


class TestComputeR0:
    def test_basic(self):
        assert compute_R0(0.5, 0.1) == 5.0

    def test_R0_equals_one(self):
        assert compute_R0(0.3, 0.3) == 1.0

    def test_gamma_zero(self):
        assert compute_R0(0.5, 0) == float("inf")

    def test_beta_zero(self):
        assert compute_R0(0, 0.5) == 0.0


class TestComputeRt:
    def _make_solution(self):
        t = np.linspace(0, 100, 1000)
        # S declining from 10000 to 2000, I peaking, R growing
        S = 10000 * np.exp(-0.02 * t)
        R = 10000 - S - 100 * np.sin(t / 10) ** 2 * np.exp(-0.01 * t)
        I = 10000 - S - R
        y = np.array([S, I, R])
        return ODESolution(t=t, y=y)

    def test_Rt_decreases_as_S_declines(self):
        sol = self._make_solution()
        Rt = compute_Rt(sol, beta=0.5, gamma=0.1, s_index=0, N=10000)
        # early Rt should be higher than late Rt
        assert Rt[50] > Rt[-50]

    def test_Rt_at_start_equals_R0(self):
        """When S ≈ N at t=0, Rt ≈ R0."""
        t = np.linspace(0, 10, 100)
        S = np.full_like(t, 10000.0)
        I = np.ones_like(t)
        R = np.zeros_like(t)
        sol = ODESolution(t=t, y=np.array([S, I, R]))
        Rt = compute_Rt(sol, beta=0.3, gamma=0.1, s_index=0, N=10000)
        # at t=0, Rt = 0.3/0.1 * 10000/10000 = 3.0
        assert abs(Rt[0] - 3.0) < 1e-8


class TestPeakInfections:
    def test_peak_detection(self):
        t = np.linspace(0, 100, 1000)
        I = 100 * np.exp(-((t - 30) ** 2) / 100)  # Gaussian peak at t=30
        S = 10000 - I
        R = np.zeros_like(t)
        sol = ODESolution(t=t, y=np.array([S, I, R]))
        peak_val, peak_t = peak_infections(sol, i_index=1)
        assert peak_val == pytest.approx(100, abs=0.1)
        assert peak_t == pytest.approx(30, abs=0.5)


class TestAttackRate:
    def test_full_epidemic(self):
        """If S goes from 10000 to 0, attack rate = 1.0."""
        t = np.array([0, 1, 2])
        S = np.array([10000, 5000, 0])
        I = np.array([0, 0, 0])
        R = np.array([0, 5000, 10000])
        sol = ODESolution(t=t, y=np.array([S, I, R]))
        ar = attack_rate(sol, N=10000, s_index=0)
        assert ar == pytest.approx(1.0)

    def test_no_epidemic(self):
        """If S stays at N, attack rate ≈ 0."""
        t = np.array([0, 1])
        S = np.array([10000, 10000])
        I = np.array([0, 0])
        R = np.array([0, 0])
        sol = ODESolution(t=t, y=np.array([S, I, R]))
        ar = attack_rate(sol, N=10000, s_index=0)
        assert ar == pytest.approx(0.0)


class TestFinalSize:
    def test_basic(self):
        t = np.array([0, 1])
        R = np.array([0, 5000])
        sol = ODESolution(t=t, y=np.array([R]))
        assert final_size(sol, r_index=0) == 5000.0


class TestEpidemicDuration:
    def test_basic(self):
        t = np.linspace(0, 100, 1000)
        I = np.where(t < 60, 100, 0.5)  # drops at t=60
        sol = ODESolution(t=t, y=np.array([I]))
        dur = epidemic_duration(sol, i_index=0, threshold=1.0)
        assert dur is not None
        assert dur >= 55 and dur <= 65

    def test_never_below_threshold(self):
        t = np.array([0, 1, 2])
        I = np.array([100, 600, 700])  # peak at end, never drops below 500
        sol = ODESolution(t=t, y=np.array([I]))
        # Peak is at t=2 with value 700; tail = [700]; never drops below 500
        dur = epidemic_duration(sol, i_index=0, threshold=500)
        assert dur is None


class TestComputeMetrics:
    def test_aggregate(self):
        t = np.linspace(0, 100, 1000)
        S = 10000 * np.exp(-0.02 * t)
        I = 500 * np.sin(np.pi * t / 60) * np.exp(-0.01 * t)
        I = np.maximum(I, 0)
        R = 10000 - S - I
        sol = ODESolution(t=t, y=np.array([S, I, R]))
        m = compute_metrics(sol, beta=0.5, gamma=0.1, N=10000,
                            s_index=0, i_index=1, r_index=2)
        assert isinstance(m, EpidemicMetrics)
        assert m.R0 == pytest.approx(5.0)
        assert m.peak_infected > 0
        assert m.attack_rate >= 0
        assert m.attack_rate <= 1

    def test_summary_dict(self):
        m = EpidemicMetrics(R0=3.0, peak_infected=500, peak_time=30,
                            attack_rate=0.7, final_size=7000,
                            total_pop=10000)
        d = m.summary_dict()
        assert isinstance(d, dict)
        assert d["R0"] == 3.0
