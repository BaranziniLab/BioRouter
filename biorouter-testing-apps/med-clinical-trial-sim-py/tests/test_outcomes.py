"""Tests for outcome models."""

import math
import pytest

from med_clinical_trial_sim.outcomes import (
    BinaryOutcome,
    ContinuousOutcome,
    TimeToEventOutcome,
    _normal_cdf,
    _normal_ppf,
    _chi2_cdf_1df,
    _chi2_ppf_1df,
    make_outcome,
    HAS_NUMPY,
)


# ---------------------------------------------------------------------------
# Utility function tests
# ---------------------------------------------------------------------------

class TestNormalCDF:
    def test_zero(self):
        assert abs(_normal_cdf(0.0) - 0.5) < 1e-6

    def test_large_positive(self):
        assert _normal_cdf(5.0) > 0.9999

    def test_large_negative(self):
        assert _normal_cdf(-5.0) < 0.0001

    def test_known_value(self):
        # Φ(1) ≈ 0.8413
        assert abs(_normal_cdf(1.0) - 0.8413) < 0.001

    def test_symmetry(self):
        for x in [0.5, 1.0, 1.5, 2.0, 3.0]:
            assert abs(_normal_cdf(x) + _normal_cdf(-x) - 1.0) < 1e-6


class TestNormalPPF:
    def test_05(self):
        assert abs(_normal_ppf(0.5)) < 1e-4

    def test_975(self):
        # z_{0.975} ≈ 1.96
        assert abs(_normal_ppf(0.975) - 1.96) < 0.01

    def test_round_trip(self):
        for p in [0.01, 0.05, 0.1, 0.25, 0.5, 0.75, 0.9, 0.95, 0.99]:
            z = _normal_ppf(p)
            assert abs(_normal_cdf(z) - p) < 0.01

    def test_bounds(self):
        with pytest.raises(ValueError):
            _normal_ppf(0.0)
        with pytest.raises(ValueError):
            _normal_ppf(1.0)


class TestChi2:
    def test_cdf_1df_known(self):
        # χ²(1) at 3.841 ≈ 0.95
        assert abs(_chi2_cdf_1df(3.841) - 0.95) < 0.01

    def test_ppf_roundtrip(self):
        for p in [0.10, 0.25, 0.50, 0.75, 0.90, 0.95]:
            x = _chi2_ppf_1df(p)
            assert abs(_chi2_cdf_1df(x) - p) < 0.05


# ---------------------------------------------------------------------------
# BinaryOutcome tests
# ---------------------------------------------------------------------------

class TestBinaryOutcome:
    def test_generate_arm_shape(self):
        model = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        obs = model.generate_arm(100, object())
        assert len(obs) == 100
        assert all(v in (0.0, 1.0) for v in obs)

    def test_generate_control_shape(self):
        model = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        obs = model.generate_control(50, object())
        assert len(obs) == 50
        assert all(v in (0.0, 1.0) for v in obs)

    def test_proportion_close_to_p(self):
        model = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        obs = model.generate_arm(10000, 42)
        mean = sum(obs) / len(obs)
        assert abs(mean - 0.5) < 0.05

    def test_effect_size(self):
        model = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        assert abs(model.effect_size - 0.2) < 1e-10

    def test_test_stat_null(self):
        """Under the null (p_ctrl == p_treat), Z should be ~N(0,1)."""
        model = BinaryOutcome(p_control=0.5, p_treatment=0.5)
        zs = []
        for seed in range(200):
            ctrl = model.generate_control(200, seed)
            treat = model.generate_arm(200, seed + 10000)
            z = model.test_statistic(ctrl, treat)
            zs.append(z)
        # Mean should be close to 0
        mean_z = sum(zs) / len(zs)
        assert abs(mean_z) < 0.15
        # Variance should be close to 1
        var_z = sum((z - mean_z) ** 2 for z in zs) / (len(zs) - 1)
        assert abs(var_z - 1.0) < 0.3

    def test_repr(self):
        model = BinaryOutcome(p_control=0.3, p_treatment=0.5)
        assert "p_control=0.3" in repr(model)


# ---------------------------------------------------------------------------
# ContinuousOutcome tests
# ---------------------------------------------------------------------------

class TestContinuousOutcome:
    def test_generate_arm_shape(self):
        model = ContinuousOutcome(mean_control=0, std_dev=1, mean_treatment=0.5)
        obs = model.generate_arm(50, 42)
        assert len(obs) == 50
        assert all(isinstance(v, float) for v in obs)

    def test_mean_close_to_mu(self):
        model = ContinuousOutcome(mean_control=2.0, std_dev=1.0, mean_treatment=2.5)
        obs = model.generate_arm(5000, 42)
        mean = sum(obs) / len(obs)
        assert abs(mean - 2.5) < 0.1

    def test_effect_size(self):
        model = ContinuousOutcome(mean_control=0, std_dev=2, mean_treatment=1)
        assert abs(model.effect_size - 0.5) < 1e-10

    def test_test_stat_null(self):
        model = ContinuousOutcome(mean_control=0, std_dev=1, mean_treatment=0)
        zs = []
        for seed in range(200):
            ctrl = model.generate_control(100, seed)
            treat = model.generate_arm(100, seed + 10000)
            z = model.test_statistic(ctrl, treat)
            zs.append(z)
        mean_z = sum(zs) / len(zs)
        assert abs(mean_z) < 0.15

    def test_test_stat_rejects_under_effect(self):
        model = ContinuousOutcome(mean_control=0, std_dev=1, mean_treatment=0.5)
        rejections = 0
        for seed in range(500):
            ctrl = model.generate_control(100, seed)
            treat = model.generate_arm(100, seed + 10000)
            z = model.test_statistic(ctrl, treat)
            if model.p_value(z) < 0.05:
                rejections += 1
        power = rejections / 500
        assert power > 0.80, f"Expected power > 0.80, got {power}"


# ---------------------------------------------------------------------------
# TimeToEventOutcome tests
# ---------------------------------------------------------------------------

class TestTimeToEventOutcome:
    def test_generate_arm_shape(self):
        model = TimeToEventOutcome(median_control=12, hazard_ratio=0.65, median_censor=24)
        obs = model.generate_arm(50, 42)
        assert len(obs) == 50
        assert all(v > 0 for v in obs)

    def test_median_approximately_correct(self):
        model = TimeToEventOutcome(median_control=12, hazard_ratio=1.0, median_censor=100)
        obs = model.generate_control(2000, 42)
        med = sorted(obs)[len(obs) // 2]
        # With heavy censoring at 100, median should be near 12
        assert 8 < med < 20

    def test_effect_size(self):
        model = TimeToEventOutcome(median_control=12, hazard_ratio=0.5, median_censor=24)
        assert abs(model.effect_size - math.log(0.5)) < 1e-10


# ---------------------------------------------------------------------------
# make_outcome factory
# ---------------------------------------------------------------------------

class TestMakeOutcome:
    def test_binary(self):
        m = make_outcome("binary", p_control=0.3, p_treatment=0.5)
        assert isinstance(m, BinaryOutcome)

    def test_continuous(self):
        m = make_outcome("continuous", mean_control=0, std_dev=1, mean_treatment=0.5)
        assert isinstance(m, ContinuousOutcome)

    def test_tte(self):
        m = make_outcome("tte", median_control=12, hazard_ratio=0.65)
        assert isinstance(m, TimeToEventOutcome)

    def test_invalid(self):
        with pytest.raises(ValueError):
            make_outcome("invalid")
