"""
bio_variant_caller — a pure-Python variant-calling pipeline.

Modules
-------
models   – data classes for reads, pileup positions, and variants
phred    – Phred-quality arithmetic
pileup   – reference-aware pileup engine
caller   – Bayesian genotype caller
vcf      – VCF 4.2 writer
annotate – ts/tv, allele-balance, depth annotation
simulate – read simulator with injected ground-truth variants
cli      – command-line entry point
"""

__version__ = "0.1.0"
