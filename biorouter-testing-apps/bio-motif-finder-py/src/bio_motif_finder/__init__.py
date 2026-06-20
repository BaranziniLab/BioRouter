"""
Bio-Motif-Finder-Py: DNA motif-discovery toolkit.

A Python toolkit implementing multiple algorithms for finding regulatory motifs
in DNA sequences, with PWM utilities and scoring functions.
"""

__version__ = "0.1.0"
__author__ = "BioRouter"

from bio_motif_finder.pwm import PWM
from bio_motif_finder.score import InformationContent, BackgroundModel
from bio_motif_finder.greedy import GreedyMotifFinder
from bio_motif_finder.gibbs import GibbsSampler
from bio_motif_finder.meme import MEMELite
from bio_motif_finder.simulate import MotifSimulator

__all__ = [
    "PWM",
    "InformationContent",
    "BackgroundModel",
    "GreedyMotifFinder",
    "GibbsSampler",
    "MEMELite",
    "MotifSimulator",
]
