"""Tests for the SIR, SEIR, SEIRD, and SEIR-intervention models.

Covers:
- Conservation: compartments always sum to N
- R0 analytic: R0 = beta/gamma for SIR
- Final-size relation: attack rate matches analytic transcendental equation
- Solver accuracy against known solutions
- Intervention reduces peak
"""

import numpy as np
import pytest

from med_epidemic.models.sir import SIRModel, SIRParams, sir_analytic_final_size
from med_epidemic.models.seir import SEIRModel, SEIRParams
from med_epidemic.models.seird import SEIRDModel, SEIRDParams
from med_epidemic.models.seir_intervention import (
    SEIRInterventionModel,
    SEIRInterventionParams,
    Intervention,
)


# ============================================================================
# SIR tests
# ============================================================================

class TestSIR:
    N = 10000
    beta = 0.5
    gamma = 0.1

    def _model(self, **overrides):
        params = SIRParams(
            beta=overrides.get("beta", self.beta),
            gamma=overrides.get("gamma", self.gamma),
            N=overrides.get("N", self.N),
            I0=overrides.get("I0", 10),
        )
        return SIRModel(params)

    def test_conservation(self):
        """S + I + R == N at every time step."""
        m = self._model()
        sol = m.run(t_span=(0, 100), dt=0.1)
        totals = sol.y.sum(axis=0)
        assert np.allclose(totals, self.N, atol=1e-6)

    def test_R0_analytic(self):
        m = self._model()
        assert abs(m.R0_value - self.beta / self.gamma) < 1e-10

    def test_R0_infinite_when_gamma_zero(self):
        m = self._model(gamma=0.0)
        assert m.R0_value == float("inf")

    def test_final_size_matches_analytic(self):
        """Numerical attack rate should be close to the analytic final-size relation."""
        m = self._model()
        sol = m.run(t_span=(0, 300), dt=0.1)
        R0 = m.R0_value
        # analytic
        ar_analytic = sir_analytic_final_size(R0)
        # numeric
        S_final = sol.y[0, -1]
        ar_numeric = 1.0 - S_final / self.N
        assert abs(ar_numeric - ar_analytic) < 0.05

    def test_final_size_equation(self):
        """Verify the analytic solver itself: r = 1 - exp(-R0 * r)."""
        for R0_val in [0.5, 1.0, 1.5, 2.0, 5.0]:
            r = sir_analytic_final_size(R0_val)
            assert abs(r - (1 - np.exp(-R0_val * r))) < 1e-10

    def test_peak_occurs(self):
        """With R0 > 1, infections must peak and then decline."""
        m = self._model()
        sol = m.run(t_span=(0, 200), dt=0.1)
        I = sol.y[1]
        peak_idx = int(np.argmax(I))
        assert I[peak_idx] > 10  # peak > initial
        assert I[-1] < I[peak_idx]  # declining after peak

    def test_infection_never_exceeds_population(self):
        m = self._model()
        sol = m.run(t_span=(0, 200), dt=0.1)
        assert np.all(sol.y >= 0)
        assert np.all(sol.y.sum(axis=0) <= self.N + 1e-6)

    def test_state_names(self):
        assert SIRModel.state_names() == ("S", "I", "R")

    def test_validation_negative_beta(self):
        with pytest.raises(ValueError):
            SIRModel(SIRParams(beta=-1, gamma=0.1, N=1000))

    def test_validation_I0_exceeds_N(self):
        with pytest.raises(ValueError):
            SIRModel(SIRParams(beta=0.5, gamma=0.1, N=100, I0=200))


# ============================================================================
# SEIR tests
# ============================================================================

class TestSEIR:
    N = 10000
    beta = 0.4
    sigma = 0.2
    gamma = 0.1

    def _model(self):
        return SEIRModel(SEIRParams(
            beta=self.beta, sigma=self.sigma, gamma=self.gamma,
            N=self.N, I0=10, E0=5,
        ))

    def test_conservation(self):
        m = self._model()
        sol = m.run(t_span=(0, 200), dt=0.1)
        totals = sol.y.sum(axis=0)
        assert np.allclose(totals, self.N, atol=1e-6)

    def test_R0_analytic(self):
        m = self._model()
        assert abs(m.R0_value - self.beta / self.gamma) < 1e-10

    def test_all_compartments_nonneg(self):
        m = self._model()
        sol = m.run(t_span=(0, 200), dt=0.1)
        assert np.all(sol.y >= -1e-10)

    def test_peak_occurs(self):
        m = self._model()
        sol = m.run(t_span=(0, 200), dt=0.1)
        I = sol.y[2]  # I is index 2 in SEIR (S=0, E=1, I=2, R=3)
        peak_idx = int(np.argmax(I))
        assert I[peak_idx] > 10
        assert I[-1] < I[peak_idx]

    def test_state_names(self):
        assert SEIRModel.state_names() == ("S", "E", "I", "R")

    def test_larger_latent_period_delays_peak(self):
        """Higher sigma (shorter latent period) should peak earlier."""
        fast = SEIRModel(SEIRParams(
            beta=self.beta, sigma=0.5, gamma=self.gamma, N=self.N, I0=10,
        ))
        slow = SEIRModel(SEIRParams(
            beta=self.beta, sigma=0.1, gamma=self.gamma, N=self.N, I0=10,
        ))
        sol_fast = fast.run(t_span=(0, 200), dt=0.1)
        sol_slow = slow.run(t_span=(0, 200), dt=0.1)
        t_peak_fast = sol_fast.t[int(np.argmax(sol_fast.y[2]))]
        t_peak_slow = sol_slow.t[int(np.argmax(sol_slow.y[2]))]
        # shorter latent period → earlier peak
        assert t_peak_fast < t_peak_slow


# ============================================================================
# SEIRD tests
# ============================================================================

class TestSEIRD:
    N = 10000
    beta = 0.4
    sigma = 0.2
    gamma = 0.1
    mu = 0.02

    def _model(self):
        return SEIRDModel(SEIRDParams(
            beta=self.beta, sigma=self.sigma, gamma=self.gamma,
            mu=self.mu, N=self.N, I0=10,
        ))

    def test_conservation(self):
        m = self._model()
        sol = m.run(t_span=(0, 200), dt=0.1)
        totals = sol.y.sum(axis=0)
        assert np.allclose(totals, self.N, atol=1e-6)

    def test_R0_uses_gamma_plus_mu(self):
        m = self._model()
        expected = self.beta / (self.gamma + self.mu)
        assert abs(m.R0_value - expected) < 1e-10

    def test_deaths_accumulate(self):
        m = self._model()
        sol = m.run(t_span=(0, 200), dt=0.1)
        D = sol.y[4]  # D is index 4
        # deaths should be monotonically non-decreasing
        assert all(D[i] <= D[i + 1] for i in range(len(D) - 1))
        assert D[-1] > 0  # some deaths occurred

    def test_state_names(self):
        assert SEIRDModel.state_names() == ("S", "E", "I", "R", "D")


# ============================================================================
# SEIR + Intervention tests
# ============================================================================

class TestSEIRIntervention:
    N = 10000
    beta = 0.4
    sigma = 0.2
    gamma = 0.1

    def test_intervention_reduces_peak(self):
        """An intervention should reduce the peak infection count."""
        base = SEIRInterventionModel(SEIRInterventionParams(
            beta_base=self.beta, sigma=self.sigma, gamma=self.gamma,
            N=self.N, I0=10,
        ))
        # 50% reduction starting at t=20
        intervention = SEIRInterventionModel(SEIRInterventionParams(
            beta_base=self.beta, sigma=self.sigma, gamma=self.gamma,
            N=self.N, I0=10,
            interventions=[Intervention(start=20, end=60, reduction=0.5)],
        ))
        sol_base = base.run(t_span=(0, 200), dt=0.1)
        sol_iv = intervention.run(t_span=(0, 200), dt=0.1)

        peak_base = sol_base.y[2].max()
        peak_iv = sol_iv.y[2].max()
        assert peak_iv < peak_base

    def test_intervention_conservation(self):
        m = SEIRInterventionModel(SEIRInterventionParams(
            beta_base=self.beta, sigma=self.sigma, gamma=self.gamma,
            N=self.N, I0=10,
            interventions=[Intervention(start=20, reduction=0.8)],
        ))
        sol = m.run(t_span=(0, 200), dt=0.1)
        totals = sol.y.sum(axis=0)
        assert np.allclose(totals, self.N, atol=1e-6)

    def test_full_lockdown_stops_spread(self):
        """100% reduction from the start should prevent any epidemic."""
        m = SEIRInterventionModel(SEIRInterventionParams(
            beta_base=self.beta, sigma=self.sigma, gamma=self.gamma,
            N=self.N, I0=1,
            interventions=[Intervention(start=0, reduction=1.0)],
        ))
        sol = m.run(t_span=(0, 100), dt=0.1)
        I = sol.y[2]
        # with full lockdown, I should decline monotonically
        assert I[-1] <= I[0] + 1e-6

    def test_R0_value_reflects_base_beta(self):
        m = SEIRInterventionModel(SEIRInterventionParams(
            beta_base=self.beta, sigma=self.sigma, gamma=self.gamma,
            N=self.N, I0=10,
        ))
        assert abs(m.R0_value - self.beta / self.gamma) < 1e-10

    def test_multiple_interventions_compound(self):
        """Two overlapping 50% reductions should compound to 75% reduction."""
        m = SEIRInterventionModel(SEIRInterventionParams(
            beta_base=0.4, sigma=0.2, gamma=0.1,
            N=10000, I0=10,
            interventions=[
                Intervention(start=0, end=100, reduction=0.5),
                Intervention(start=0, end=100, reduction=0.5),
            ],
        ))
        # effective beta should be 0.4 * 0.5 * 0.5 = 0.1
        assert abs(m.beta_fn(50) - 0.1) < 1e-10
