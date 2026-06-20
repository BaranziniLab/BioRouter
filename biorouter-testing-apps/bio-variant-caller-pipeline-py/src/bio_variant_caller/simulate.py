"""Read simulator with ground-truth variant injection.

Generates synthetic aligned reads from a reference sequence with
configurable coverage, error rates, and known variant positions.
The simulator produces both the read data and a ground-truth manifest
for evaluating the caller's sensitivity and precision.
"""

from __future__ import annotations

import random
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple

from .models import AlignedRead, Strand, Variant, VariantType


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

@dataclass
class SimConfig:
    """Parameters for the read simulator.

    Attributes
    ----------
    seed : int
        Random seed for reproducibility.
    read_length : int
        Length of each simulated read.
    coverage : float
        Average read depth (e.g. 30 means ~30x).
    error_rate : float
        Per-base error rate for sequencing errors.
    min_base_quality : int
        Minimum base quality (Phred) for high-quality bases.
    max_base_quality : int
        Maximum base quality for high-quality bases.
    mean_base_quality : int
        Mean base quality for sequencing errors.
    """
    seed: int = 42
    read_length: int = 150
    coverage: float = 30.0
    error_rate: float = 0.01
    min_base_quality: int = 20
    max_base_quality: int = 40
    mean_base_quality: int = 15


# ---------------------------------------------------------------------------
# Ground-truth variant
# ---------------------------------------------------------------------------

@dataclass
class TruthVariant:
    """A variant injected by the simulator."""
    pos: int             # 0-based reference position
    ref: str             # original base
    alt: str             # injected base
    variant_type: VariantType = VariantType.SNP
    fraction: float = 1.0  # fraction of reads carrying the variant (1.0 = all)

    def to_variant(self) -> Variant:
        """Convert to a Variant for comparison."""
        return Variant(
            chrom="sim",
            pos=self.pos,
            ref=self.ref,
            alt=self.alt,
            variant_type=self.variant_type,
            truth_ref=self.ref,
            truth_alt=self.alt,
        )


# ---------------------------------------------------------------------------
# Read simulator
# ---------------------------------------------------------------------------

class ReadSimulator:
    """Simulate aligned reads from a reference with injected variants.

    Parameters
    ----------
    reference : str
        Reference sequence (upper-case).
    config : SimConfig
        Simulation parameters.
    """

    def __init__(self, reference: str, config: Optional[SimConfig] = None) -> None:
        self.reference = reference.upper()
        self.ref_length = len(reference)
        self.config = config or SimConfig()
        self.rng = random.Random(self.config.seed)
        self.truth_variants: List[TruthVariant] = []

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def simulate(
        self,
        variants: Optional[List[TruthVariant]] = None,
    ) -> Tuple[List[AlignedRead], List[TruthVariant]]:
        """Simulate reads and return (reads, truth_variants).

        Parameters
        ----------
        variants : list[TruthVariant], optional
            Additional variants to inject (merged with any already registered).

        Returns
        -------
        reads : list[AlignedRead]
            Simulated aligned reads.
        truth : list[TruthVariant]
            The ground-truth variants.
        """
        cfg = self.config

        # Merge any explicit variants with previously registered ones
        if variants:
            self.truth_variants.extend(variants)

        # Build a mutated reference incorporating all variants
        mut_ref = self._build_mutated_reference(self.truth_variants)

        # Calculate number of reads
        total_bases = self.ref_length * cfg.coverage
        n_reads = max(1, int(total_bases / cfg.read_length))

        reads: List[AlignedRead] = []
        for i in range(n_reads):
            read = self._simulate_one_read(i, mut_ref)
            reads.append(read)

        return reads, self.truth_variants

    def add_variant(
        self,
        pos: int,
        ref: Optional[str] = None,
        alt: Optional[str] = None,
        fraction: float = 1.0,
    ) -> TruthVariant:
        """Register a variant for injection.

        Parameters
        ----------
        pos : int
            0-based reference position.
        ref : str, optional
            Expected reference base (validated against reference).
        alt : str, optional
            Alternate base.  If None, a random transversion is chosen.
        fraction : float
            Fraction of reads carrying the variant (0-1).  Default 1.0 (all reads).

        Returns
        -------
        TruthVariant
            The registered variant.
        """
        if ref is None:
            ref = self.reference[pos]
        if alt is None:
            # Pick a random transversion
            bases = [b for b in "ACGT" if b != ref]
            alt = self.rng.choice(bases)

        # Determine type
        if len(ref) == 1 and len(alt) == 1:
            vtype = VariantType.SNP
        elif len(ref) < len(alt):
            vtype = VariantType.INSERTION
        elif len(ref) > len(alt):
            vtype = VariantType.DELETION
        else:
            vtype = VariantType.MNP

        tv = TruthVariant(pos=pos, ref=ref, alt=alt, variant_type=vtype, fraction=fraction)
        self.truth_variants.append(tv)
        return tv

    def inject_snp(
        self, pos: int, alt: Optional[str] = None, fraction: float = 1.0
    ) -> TruthVariant:
        """Convenience: inject a SNP at a position."""
        ref = self.reference[pos]
        return self.add_variant(pos, ref=ref, alt=alt, fraction=fraction)

    # ------------------------------------------------------------------
    # Internal
    # ------------------------------------------------------------------

    def _build_mutated_reference(
        self, variants: List[TruthVariant]
    ) -> str:
        """Build a reference string with variants applied."""
        mut_ref = list(self.reference)
        for v in variants:
            if v.pos < self.ref_length:
                mut_ref[v.pos] = v.alt
        return "".join(mut_ref)

    def _simulate_one_read(
        self, idx: int, mut_ref: str
    ) -> AlignedRead:
        """Simulate a single read."""
        cfg = self.config

        # Random start position
        max_start = max(0, self.ref_length - cfg.read_length)
        start = self.rng.randint(0, max_start)

        # Extract sequence from mutated reference
        end = min(start + cfg.read_length, self.ref_length)
        seq = mut_ref[start:end]

        # Determine strand
        strand = self.rng.choice([Strand.FORWARD, Strand.REVERSE])

        # Generate base qualities
        quals: List[int] = []
        for _ in range(len(seq)):
            if self.rng.random() < cfg.error_rate:
                # Error position: lower quality
                q = self.rng.randint(
                    max(1, cfg.mean_base_quality - 5),
                    cfg.mean_base_quality + 5
                )
            else:
                q = self.rng.randint(cfg.min_base_quality, cfg.max_base_quality)
            quals.append(q)

        # Build CIGAR (simple: all matches for now)
        cigar = f"{len(seq)}M"

        read = AlignedRead(
            name=f"read_{idx:06d}",
            ref_start=start,
            cigar=cigar,
            sequence=seq,
            base_qualities=quals,
            strand=strand,
            map_quality=self.rng.randint(30, 60),
        )
        return read

    def generate_truth_vcf(self) -> List[Variant]:
        """Return truth variants as Variant objects for comparison."""
        return [tv.to_variant() for tv in self.truth_variants]


# ---------------------------------------------------------------------------
# Convenience functions
# ---------------------------------------------------------------------------

def simulate_reads(
    reference: str,
    variants: Optional[List[TruthVariant]] = None,
    config: Optional[SimConfig] = None,
) -> Tuple[List[AlignedRead], List[TruthVariant]]:
    """One-shot read simulation.

    Returns (reads, truth_variants).
    """
    sim = ReadSimulator(reference, config)
    return sim.simulate(variants)


def create_truth_variants(
    reference: str,
    positions: List[int],
    alts: Optional[List[str]] = None,
    fractions: Optional[List[float]] = None,
) -> List[TruthVariant]:
    """Create a list of TruthVariant objects from position/alt pairs."""
    if alts is None:
        alts = [None] * len(positions)  # type: ignore[list-item]
    if fractions is None:
        fractions = [1.0] * len(positions)

    sim = ReadSimulator(reference)
    truth: List[TruthVariant] = []
    for pos, alt, frac in zip(positions, alts, fractions):
        tv = sim.add_variant(pos, alt=alt, fraction=frac)
        truth.append(tv)
    return truth
