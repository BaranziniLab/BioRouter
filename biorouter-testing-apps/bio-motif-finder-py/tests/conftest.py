"""
Pytest configuration and shared fixtures.
"""

import pytest
import numpy as np

from bio_motif_finder.pwm import PWM
from bio_motif_finder.score import BackgroundModel, MotifScorer, InformationContent
from bio_motif_finder.simulate import MotifSimulator


@pytest.fixture
def sample_sequences():
    """Provide sample aligned sequences for testing."""
    return [
        "ATCGATCG",
        "ATCGATCG",
        "ATCGATCG",
        "ATCGATCG",
        "ATCGATCG",
    ]


@pytest.fixture
def varied_sequences():
    """Provide sequences with some variation."""
    return [
        "ATCGATCG",
        "ATCAATCG",
        "ATCGATCA",
        "ATCGATCG",
        "ATCAATCA",
    ]


@pytest.fixture
def background_uniform():
    """Provide uniform background model."""
    return BackgroundModel()


@pytest.fixture
def background_gc_rich():
    """Provide GC-rich background model."""
    return BackgroundModel({'A': 0.2, 'C': 0.3, 'G': 0.3, 'T': 0.2})


@pytest.fixture
def sample_pwm(sample_sequences):
    """Provide PWM built from sample sequences."""
    return PWM.from_sequences(sample_sequences)


@pytest.fixture
def scorer(background_uniform):
    """Provide motif scorer."""
    return MotifScorer(background_uniform)


@pytest.fixture
def ic_calculator(background_uniform):
    """Provide information content calculator."""
    return InformationContent(background_uniform)


@pytest.fixture
def simulator():
    """Provide motif simulator with fixed seed."""
    return MotifSimulator(seed=42)


@pytest.fixture
def planted_motif_data(simulator):
    """Provide dataset with planted motif."""
    return simulator.generate_dataset(
        num_sequences=20,
        sequence_length=100,
        motif_length=8,
        motif="ATCGATCG",
        mutations_per_instance=1
    )


@pytest.fixture
def small_planted_motif(simulator):
    """Provide small dataset for fast testing."""
    return simulator.generate_dataset(
        num_sequences=10,
        sequence_length=50,
        motif_length=6,
        motif="ATCGAT",
        mutations_per_instance=1
    )
