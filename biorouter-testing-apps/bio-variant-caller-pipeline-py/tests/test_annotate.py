"""Tests for variant annotation module."""

from __future__ import annotations

import pytest

from bio_variant_caller.annotate import VariantAnnotator, classify_ts_tv, ts_tv_ratio
from bio_variant_caller.models import Variant, VariantType


# ---------------------------------------------------------------------------
# ts/tv classification
# ---------------------------------------------------------------------------

class TestTsTvClassification:
    def test_transition_ag(self):
        assert classify_ts_tv("A", "G") == "ts"

    def test_transition_ga(self):
        assert classify_ts_tv("G", "A") == "ts"

    def test_transition_ct(self):
        assert classify_ts_tv("C", "T") == "ts"

    def test_transition_tc(self):
        assert classify_ts_tv("T", "C") == "ts"

    def test_transversion_ac(self):
        assert classify_ts_tv("A", "C") == "tv"

    def test_transversion_at(self):
        assert classify_ts_tv("A", "T") == "tv"

    def test_transversion_gc(self):
        assert classify_ts_tv("G", "C") == "tv"

    def test_transversion_gt(self):
        assert classify_ts_tv("G", "T") == "tv"

    def test_transversion_ca(self):
        assert classify_ts_tv("C", "A") == "tv"

    def test_transversion_cg(self):
        assert classify_ts_tv("C", "G") == "tv"

    def test_transversion_ta(self):
        assert classify_ts_tv("T", "A") == "tv"

    def test_transversion_tg(self):
        assert classify_ts_tv("T", "G") == "tv"

    def test_mnp_first_mismatch(self):
        """For MNP, classify based on first differing position."""
        # AC vs AG: first diff at pos 1 is C→G (transversion)
        assert classify_ts_tv("AC", "AG") == "tv"
        # AC vs AT: first diff at pos 1 is C→T (transition)
        assert classify_ts_tv("AC", "AT") == "ts"

    def test_same_bases(self):
        assert classify_ts_tv("A", "A") == "unknown"

    def test_empty(self):
        assert classify_ts_tv("", "") == "unknown"


# ---------------------------------------------------------------------------
# ts/tv ratio
# ---------------------------------------------------------------------------

class TestTsTvRatio:
    def test_basic_ratio(self):
        variants = [
            Variant(chrom="1", pos=0, ref="A", alt="G", variant_type=VariantType.SNP, ts_tv="ts"),
            Variant(chrom="1", pos=1, ref="A", alt="G", variant_type=VariantType.SNP, ts_tv="ts"),
            Variant(chrom="1", pos=2, ref="A", alt="C", variant_type=VariantType.SNP, ts_tv="tv"),
            Variant(chrom="1", pos=3, ref="A", alt="T", variant_type=VariantType.SNP, ts_tv="tv"),
        ]
        assert ts_tv_ratio(variants) == 1.0

    def test_all_transitions(self):
        variants = [
            Variant(chrom="1", pos=0, ref="A", alt="G", variant_type=VariantType.SNP, ts_tv="ts"),
            Variant(chrom="1", pos=1, ref="C", alt="T", variant_type=VariantType.SNP, ts_tv="ts"),
        ]
        assert ts_tv_ratio(variants) == float("inf")

    def test_all_transversions(self):
        variants = [
            Variant(chrom="1", pos=0, ref="A", alt="C", variant_type=VariantType.SNP, ts_tv="tv"),
            Variant(chrom="1", pos=1, ref="G", alt="T", variant_type=VariantType.SNP, ts_tv="tv"),
        ]
        assert ts_tv_ratio(variants) == 0.0

    def test_empty_list(self):
        assert ts_tv_ratio([]) == 0.0


# ---------------------------------------------------------------------------
# VariantAnnotator
# ---------------------------------------------------------------------------

class TestVariantAnnotator:
    def test_annotate_snp_gets_ts_tv(self):
        annotator = VariantAnnotator()
        v = Variant(
            chrom="1", pos=10, ref="A", alt="G",
            variant_type=VariantType.SNP, depth=30, alt_count=15,
        )
        annotator.annotate([v])
        assert v.ts_tv == "ts"

    def test_annotate_snp_gets_tv(self):
        annotator = VariantAnnotator()
        v = Variant(
            chrom="1", pos=10, ref="A", alt="C",
            variant_type=VariantType.SNP, depth=30, alt_count=15,
        )
        annotator.annotate([v])
        assert v.ts_tv == "tv"

    def test_annotate_allele_balance(self):
        annotator = VariantAnnotator()
        v = Variant(
            chrom="1", pos=10, ref="A", alt="G",
            variant_type=VariantType.SNP, depth=30, alt_count=10,
        )
        annotator.annotate([v])
        assert v.allele_balance is not None
        assert abs(v.allele_balance - 10 / 30) < 1e-10

    def test_annotate_preserves_existing(self):
        """Annotation should not overwrite existing values."""
        annotator = VariantAnnotator()
        v = Variant(
            chrom="1", pos=10, ref="A", alt="G",
            variant_type=VariantType.SNP, depth=30, alt_count=15,
            allele_balance=0.8,  # pre-set
        )
        annotator.annotate([v])
        assert v.allele_balance == 0.8

    def test_annotate_multiple(self):
        annotator = VariantAnnotator()
        variants = [
            Variant(chrom="1", pos=i, ref="A", alt="G",
                    variant_type=VariantType.SNP, depth=30, alt_count=15)
            for i in range(5)
        ]
        result = annotator.annotate(variants)
        assert len(result) == 5
        for v in result:
            assert v.ts_tv == "ts"
            assert v.allele_balance is not None

    def test_indel_no_ts_tv(self):
        """Indels should not get ts/tv annotation."""
        annotator = VariantAnnotator()
        v = Variant(
            chrom="1", pos=10, ref="A", alt="AG",
            variant_type=VariantType.INSERTION, depth=30, alt_count=15,
        )
        annotator.annotate([v])
        assert v.ts_tv is None  # only SNPs get ts/tv

    def test_zero_depth(self):
        """Zero depth should not cause division by zero."""
        annotator = VariantAnnotator()
        v = Variant(
            chrom="1", pos=10, ref="A", alt="G",
            variant_type=VariantType.SNP, depth=0, alt_count=0,
        )
        annotator.annotate([v])
        assert v.allele_balance is not None
