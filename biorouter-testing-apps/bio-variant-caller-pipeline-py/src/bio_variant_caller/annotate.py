"""Variant annotation module.

Adds ts/tv classification, depth annotations, allele balance,
and other computed fields to called variants.
"""

from __future__ import annotations

from typing import List

from .models import Variant


# ---------------------------------------------------------------------------
# Transition / transversion classification
# ---------------------------------------------------------------------------

# Transitions: purine<->purine (A<->G) or pyrimidine<->pyrimidine (C<->T)
_TRANSITIONS = {
    ("A", "G"), ("G", "A"),
    ("C", "T"), ("T", "C"),
}


def classify_ts_tv(ref: str, alt: str) -> str:
    """Classify a SNP as transition (ts) or transversion (tv).

    For multi-nucleotide variants, classify based on first mismatch.

    >>> classify_ts_tv("A", "G")
    'ts'
    >>> classify_ts_tv("A", "C")
    'tv'
    """
    if not ref or not alt:
        return "unknown"

    # For MNP/multi-base, compare first differing position
    for r, a in zip(ref, alt):
        if r != a:
            return "ts" if (r, a) in _TRANSITIONS else "tv"

    # Same bases — shouldn't happen for a variant
    return "unknown"


def ts_tv_ratio(variants: List[Variant]) -> float:
    """Compute the ts/tv ratio across a set of SNPs.

    Returns 0.0 if there are no transversions.
    """
    ts = sum(1 for v in variants if v.ts_tv == "ts")
    tv = sum(1 for v in variants if v.ts_tv == "tv")
    if tv == 0:
        return float("inf") if ts > 0 else 0.0
    return ts / tv


# ---------------------------------------------------------------------------
# Annotator
# ---------------------------------------------------------------------------

class VariantAnnotator:
    """Annotate a list of variants with computed fields.

    This annotates in-place and returns the same list for convenience.
    """

    def annotate(self, variants: List[Variant]) -> List[Variant]:
        """Run all annotations on the variant list."""
        for v in variants:
            self._annotate_single(v)
        return variants

    def _annotate_single(self, v: Variant) -> None:
        """Annotate a single variant."""
        # ts/tv
        if v.variant_type.value == "SNP":
            v.ts_tv = classify_ts_tv(v.ref, v.alt)

        # allele balance (may already be set by caller)
        if v.allele_balance is None:
            v.allele_balance = v.alt_count / v.depth if v.depth > 0 else 0.0

        # depth is already set by caller, but ensure it exists
        # (no-op if already annotated)

    @staticmethod
    def annotate_file(filepath: str) -> List[Variant]:
        """Read variants from a simple TSV and annotate.

        This is a helper for testing; not the main pipeline path.
        """
        variants: List[Variant] = []
        with open(filepath) as fh:
            for line in fh:
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                parts = line.split("\t")
                if len(parts) < 5:
                    continue
                v = Variant(
                    chrom=parts[0],
                    pos=int(parts[1]),
                    ref=parts[2],
                    alt=parts[3],
                    variant_type=_guess_type(parts[2], parts[3]),
                    depth=int(parts[4]) if len(parts) > 4 else 0,
                )
                variants.append(v)
        annotator = VariantAnnotator()
        return annotator.annotate(variants)


def _guess_type(ref: str, alt: str) -> "VariantType":  # noqa: F821
    from .models import VariantType
    if len(ref) == 1 and len(alt) == 1:
        return VariantType.SNP
    elif len(ref) < len(alt):
        return VariantType.INSERTION
    elif len(ref) > len(alt):
        return VariantType.DELETION
    else:
        return VariantType.MNP
