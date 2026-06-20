"""Tests for Phred quality score utilities."""

from __future__ import annotations

import pytest

from bio_variant_caller.phred import (
    average_phred,
    base_quality_to_weight,
    cap_quality,
    min_phred,
    phred_to_prob,
    prob_to_phred,
)


class TestPhredConversion:
    def test_phred_0(self):
        assert phred_to_prob(0) == 1.0

    def test_phred_10(self):
        assert abs(phred_to_prob(10) - 0.1) < 1e-10

    def test_phred_20(self):
        assert abs(phred_to_prob(20) - 0.01) < 1e-10

    def test_phred_30(self):
        assert abs(phred_to_prob(30) - 0.001) < 1e-10

    def test_prob_to_phred_roundtrip(self):
        for q in [0, 10, 20, 30, 40]:
            p = phred_to_prob(q)
            q_back = prob_to_phred(p)
            assert abs(q_back - q) < 0.01

    def test_prob_to_phred_zero(self):
        """Zero probability should cap at 100."""
        assert prob_to_phred(0.0) == 100.0

    def test_prob_to_phred_very_small(self):
        """Very small probability should give high Phred."""
        q = prob_to_phred(1e-10)
        assert q == 100.0  # capped


class TestWeightsAndAverages:
    def test_quality_weight_high(self):
        w = base_quality_to_weight(40)
        assert w > 0.99

    def test_quality_weight_low(self):
        w = base_quality_to_weight(0)
        assert 0.0 <= w <= 0.1

    def test_average_phred(self):
        assert average_phred([20, 30, 40]) == 30.0

    def test_average_phred_empty(self):
        assert average_phred([]) == 0.0

    def test_min_phred(self):
        assert min_phred([20, 10, 30]) == 10

    def test_min_phred_empty(self):
        assert min_phred([]) == 0

    def test_cap_quality(self):
        assert cap_quality(50) == 50
        assert cap_quality(150) == 99
        assert cap_quality(-5) == -5
