"""Bayesian variant caller.

Calls SNPs and simple indels from a pileup using a likelihood-based
genotype model.  The caller evaluates three diploid genotypes (AA, AB, BB)
and picks the most probable, reporting Phred-scaled quality scores.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Dict, List, Optional

from .models import (
    AlignedRead,
    Genotype,
    PileupPosition,
    Strand,
    Variant,
    VariantType,
)
from .phred import phred_to_prob, prob_to_phred


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

@dataclass
class CallerConfig:
    """Tuning knobs for the variant caller.

    Attributes
    ----------
    min_depth : int
        Minimum number of bases to consider a position callable.
    min_alt_allele_frequency : float
        Minimum alt-allele frequency to call a variant.
    min_base_quality : int
        Minimum base quality to include a base in the count.
    min_genotype_quality : int
        Minimum genotype quality (Phred) to emit a call.
    strand_bias_threshold : float
        Maximum fraction of alt-supporting reads on one strand
        (if exceeded, flag strand bias).
    """
    min_depth: int = 8
    min_alt_allele_frequency: float = 0.2
    min_base_quality: int = 20
    min_genotype_quality: int = 20
    strand_bias_threshold: float = 0.9


# ---------------------------------------------------------------------------
# Prior probabilities (uniform over genotypes)
# ---------------------------------------------------------------------------

# Genotype priors: log10 P(G) for AA, AB, BB
_PRIORS = {
    "AA": math.log10(0.25),
    "AB": math.log10(0.50),
    "BB": math.log10(0.25),
}


# ---------------------------------------------------------------------------
# Bayesian genotype caller
# ---------------------------------------------------------------------------

class VariantCaller:
    """Bayesian genotype caller operating on pileup positions.

    Parameters
    ----------
    config : CallerConfig
        Caller tuning parameters.
    ref_name : str
        Reference/chromosome name for VCF output.
    """

    def __init__(self, config: Optional[CallerConfig] = None, ref_name: str = "ref") -> None:
        self.config = config or CallerConfig()
        self.ref_name = ref_name

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def call(self, pileup: Dict[int, PileupPosition]) -> List[Variant]:
        """Call variants across all pileup positions.

        Returns a list of Variant objects (one per called variant site).
        """
        variants: List[Variant] = []
        for ref_pos in sorted(pileup.keys()):
            pp = pileup[ref_pos]
            v = self.call_position(pp)
            if v is not None:
                variants.append(v)
        return variants

    def call_position(self, pp: PileupPosition) -> Optional[Variant]:
        """Call a variant at a single pileup position.

        Returns None if no variant is called.
        """
        cfg = self.config

        # Filter bases by quality
        good_bases = [
            b for b in pp.bases
            if b.base_quality >= cfg.min_base_quality and not b.is_deletion
        ]

        depth = len(good_bases)
        if depth < cfg.min_depth:
            return None

        # Count bases
        counts: Dict[str, int] = {}
        for b in good_bases:
            counts[b.base] = counts.get(b.base, 0) + 1

        # Find alt allele (most frequent non-reference base)
        ref_base = pp.ref_base
        alt_candidates = {
            base: cnt for base, cnt in counts.items() if base != ref_base
        }
        if not alt_candidates:
            return None

        alt_base = max(alt_candidates, key=alt_candidates.get)  # type: ignore[arg-type]
        alt_count = alt_candidates[alt_base]
        allele_freq = alt_count / depth

        if allele_freq < cfg.min_alt_allele_frequency:
            return None

        # Determine variant type
        variant_type = VariantType.SNP
        ref_allele = ref_base
        alt_allele = alt_base

        # Bayesian genotype call
        genotype, gt_qual = self._bayesian_genotype(
            ref_base, alt_base, good_bases, allele_freq
        )

        if gt_qual < cfg.min_genotype_quality:
            return None

        # Strand balance
        strand_counts = self._strand_split(good_bases, alt_base)
        sb = self._strand_balance(strand_counts)

        # Allele balance
        ab = alt_count / depth if depth > 0 else 0.0

        return Variant(
            chrom=self.ref_name,
            pos=pp.ref_pos,
            ref=ref_allele,
            alt=alt_allele,
            variant_type=variant_type,
            quality=gt_qual,
            depth=depth,
            alt_count=alt_count,
            allele_frequency=allele_freq,
            genotype=genotype,
            genotype_quality=gt_qual,
            allele_balance=ab,
            strand_balance=sb,
        )

    def call_from_reads(
        self, reference: str, reads: List[AlignedRead]
    ) -> List[Variant]:
        """Convenience: pileup + call in one step."""
        from .pileup import PileupEngine

        engine = PileupEngine(reference, reads)
        pileup = engine.build()
        return self.call(pileup)

    # ------------------------------------------------------------------
    # Bayesian model
    # ------------------------------------------------------------------

    def _bayesian_genotype(
        self,
        ref_base: str,
        alt_base: str,
        bases: list,
        observed_freq: float,
    ) -> tuple[Genotype, float]:
        """Compute P(G|D) for genotypes AA, AB, BB using Bayes' rule.

        Genotypes:
          AA = hom-ref    (both chromosomes carry ref)
          AB = het        (one ref, one alt)
          BB = hom-alt    (both chromosomes carry alt)

        Likelihood model:
          P(base | AA) = 1 - eps  if base == ref, else eps
          P(base | AB) = 0.5      (either allele equally likely)
          P(base | BB) = eps      if base == ref, else 1 - eps
          where eps = per-base error probability from base quality
        """
        if not bases:
            return Genotype.UNCALLED, 0.0

        base_probs = [phred_to_prob(b.base_quality) for b in bases]

        log_likelihoods: Dict[str, float] = {}

        for gt_name, gt_ratio in [("AA", (1.0, 0.0)), ("AB", (0.5, 0.5)), ("BB", (0.1, 1.0))]:
            p_ref_emit, p_alt_emit = gt_ratio
            ll = 0.0
            for b, eps in zip(bases, base_probs):
                if b.base == ref_base:
                    ll += math.log10(p_ref_emit * (1 - eps) + (1 - p_ref_emit) * eps)
                elif b.base == alt_base:
                    ll += math.log10(p_alt_emit * (1 - eps) + (1 - p_alt_emit) * eps)
                else:
                    ll += math.log10(eps / 3.0)
            log_likelihoods[gt_name] = ll

        # Add priors
        for gt_name in log_likelihoods:
            log_likelihoods[gt_name] += _PRIORS[gt_name]

        # Find MAP genotype
        best_gt = max(log_likelihoods, key=log_likelihoods.get)  # type: ignore[arg-type]

        # Convert to Phred-scaled quality
        sorted_gts = sorted(log_likelihoods.items(), key=lambda x: x[1], reverse=True)
        if len(sorted_gts) >= 2:
            max_ll = sorted_gts[0][1]
            log_sum = math.log10(
                sum(10 ** (val - max_ll) for _, val in sorted_gts)
            ) + max_ll
            log_p_best = sorted_gts[0][1] - log_sum
            p_not_best = 1.0 - 10 ** log_p_best
            if p_not_best <= 0:
                gt_qual = 99.0
            else:
                gt_qual = prob_to_phred(p_not_best)
        else:
            gt_qual = 0.0

        gt_map = {
            "AA": Genotype.HOM_REF,
            "AB": Genotype.HET,
            "BB": Genotype.HOM_ALT,
        }
        return gt_map[best_gt], min(gt_qual, 99.0)

    # ------------------------------------------------------------------
    # Strand helpers
    # ------------------------------------------------------------------

    def _strand_split(
        self, bases: list, alt_base: str
    ) -> Dict[str, int]:
        """Count alt-supporting reads per strand."""
        result = {"forward": 0, "reverse": 0}
        for b in bases:
            if b.base == alt_base:
                key = "forward" if b.strand == Strand.FORWARD else "reverse"
                result[key] += 1
        return result

    def _strand_balance(self, strand_counts: Dict[str, int]) -> float:
        """Fraction of alt-supporting reads on forward strand."""
        total = strand_counts.get("forward", 0) + strand_counts.get("reverse", 0)
        if total == 0:
            return 0.5
        return strand_counts["forward"] / total
