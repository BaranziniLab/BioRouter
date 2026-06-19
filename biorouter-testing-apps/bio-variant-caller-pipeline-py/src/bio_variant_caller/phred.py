"""Phred-quality arithmetic utilities."""

from __future__ import annotations

import math


def phred_to_prob(q: int) -> float:
    """Convert Phred quality score to error probability.

    >>> phred_to_prob(30)
    0.001
    """
    return 10 ** (-q / 10.0)


def prob_to_phred(p: float) -> float:
    """Convert error probability to Phred score.

    >>> prob_to_phred(0.001)
    30.0
    """
    if p <= 0:
        return 100.0  # cap at max practical quality
    return -10.0 * math.log10(p)


def qual_sum(log_probs: list[float]) -> float:
    """Sum Phred-scaled log-probabilities in a numerically stable way.

    Each element is a *negative* log-probability (Phred).  We return the
    combined Phred score.
    """
    if not log_probs:
        return 0.0
    # Convert to probabilities, multiply, convert back
    log_p = sum(-q / 10.0 * math.log(10) for q in log_probs)
    return prob_to_phred(1.0 - math.exp(log_p)) if log_p < 0 else 0.0


def base_quality_to_weight(q: int) -> float:
    """Return the weight of a base quality score (higher = more trusted).

    Weights are 1 - error_probability, clamped to [0.01, 1.0].
    """
    p_err = phred_to_prob(q)
    return max(0.01, 1.0 - p_err)


def average_phred(quals: list[int]) -> float:
    """Compute average Phred quality of a set of bases."""
    if not quals:
        return 0.0
    return sum(quals) / len(quals)


def min_phred(quals: list[int]) -> int:
    """Return minimum Phred quality in a set."""
    return min(quals) if quals else 0


def cap_quality(q: float, max_q: int = 99) -> int:
    """Cap a quality score at a maximum value."""
    return min(int(round(q)), max_q)
