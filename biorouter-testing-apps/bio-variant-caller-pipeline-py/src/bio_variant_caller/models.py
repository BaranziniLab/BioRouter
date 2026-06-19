"""Data models shared across the pipeline."""

from __future__ import annotations

import enum
from dataclasses import dataclass, field
from typing import List, Optional


# ---------------------------------------------------------------------------
# Enums
# ---------------------------------------------------------------------------

class Strand(enum.IntEnum):
    FORWARD = 0
    REVERSE = 1


class VariantType(enum.Enum):
    SNP = "SNP"
    INSERTION = "INS"
    DELETION = "DEL"
    MNP = "MNP"          # multi-nucleotide polymorphism


class Genotype(enum.Enum):
    HOM_REF = "0/0"
    HET = "0/1"
    HOM_ALT = "1/1"
    UNCALLED = "./."


# ---------------------------------------------------------------------------
# Read model
# ---------------------------------------------------------------------------

@dataclass
class AlignedRead:
    """A single aligned read (SAM-like simplified model).

    Attributes
    ----------
    name : str
        Read identifier.
    ref_start : int
        0-based leftmost position where this read aligns to the reference.
    cigar : str
        CIGAR string (e.g. ``"10M2I5M3D8M"``).
    sequence : str
        Read bases (query sequence).
    base_qualities : list[int]
        Phred+33 encoded base qualities, one per query base.
    strand : Strand
        Forward or reverse strand.
    map_quality : int
        Mapping quality (Phred-scaled).
    """
    name: str
    ref_start: int
    cigar: str
    sequence: str
    base_qualities: List[int]
    strand: Strand = Strand.FORWARD
    map_quality: int = 60


# ---------------------------------------------------------------------------
# Pileup model
# ---------------------------------------------------------------------------

@dataclass
class PileupBase:
    """A single base observed at a pileup position."""
    base: str                   # A/C/G/T
    base_quality: int           # Phred quality
    strand: Strand
    read_name: str = ""
    is_insertion: bool = False   # base is first base of an inserted segment
    is_deletion: bool = False    # position is covered by a deletion


@dataclass
class PileupPosition:
    """Aggregated pileup information at one reference coordinate."""
    ref_pos: int                 # 0-based reference position
    ref_base: str                # reference base at this position
    bases: List[PileupBase] = field(default_factory=list)

    @property
    def depth(self) -> int:
        return len(self.bases)

    def base_counts(self) -> dict[str, int]:
        """Return {base: count} ignoring indel flags."""
        counts: dict[str, int] = {}
        for b in self.bases:
            counts[b.base] = counts.get(b.base, 0) + 1
        return counts

    def strand_counts(self) -> dict[str, dict[str, int]]:
        """Return {base: {forward: N, reverse: N}}."""
        result: dict[str, dict[str, int]] = {}
        for b in self.bases:
            key = "forward" if b.strand == Strand.FORWARD else "reverse"
            result.setdefault(b.base, {"forward": 0, "reverse": 0})[key] += 1
        return result

    def quality_weighted_counts(self) -> dict[str, float]:
        """Return base counts weighted by base quality (probability of being correct)."""
        counts: dict[str, float] = {}
        for b in self.bases:
            # Convert Phred to probability that base is correct
            p_correct = 1.0 - 10 ** (-b.base_quality / 10.0)
            counts[b.base] = counts.get(b.base, 0.0) + p_correct
        return counts


# ---------------------------------------------------------------------------
# Variant model
# ---------------------------------------------------------------------------

@dataclass
class Variant:
    """A called variant."""
    chrom: str
    pos: int                          # 0-based
    ref: str                          # reference allele(s)
    alt: str                          # alternate allele(s)
    variant_type: VariantType
    quality: float = 0.0              # Phred-scaled variant quality
    depth: int = 0
    alt_count: int = 0
    allele_frequency: float = 0.0
    genotype: Genotype = Genotype.UNCALLED
    genotype_quality: float = 0.0
    # annotation fields
    ts_tv: Optional[str] = None       # ts or tv
    allele_balance: Optional[float] = None
    strand_balance: Optional[float] = None
    # ground truth (from simulator)
    truth_ref: Optional[str] = None
    truth_alt: Optional[str] = None
    is_true_positive: Optional[bool] = None
