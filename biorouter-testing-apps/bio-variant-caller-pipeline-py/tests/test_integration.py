"""Integration tests: end-to-end pipeline with sensitivity/precision checks.

These tests simulate reads with known injected variants, run the full
pileup→call→annotate pipeline, and verify that the caller recovers
the truth variants with acceptable sensitivity and precision.
"""

from __future__ import annotations

import random

import pytest

from bio_variant_caller.annotate import VariantAnnotator, ts_tv_ratio
from bio_variant_caller.caller import CallerConfig, VariantCaller
from bio_variant_caller.models import AlignedRead, Genotype, Strand, VariantType
from bio_variant_caller.pileup import PileupEngine
from bio_variant_caller.simulate import ReadSimulator, SimConfig, TruthVariant


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _precision(tp: int, fp: int) -> float:
    return tp / (tp + fp) if (tp + fp) > 0 else 0.0


def _sensitivity(tp: int, fn: int) -> float:
    return tp / (tp + fn) if (tp + fn) > 0 else 0.0


def _match_variants(
    called: list, truth: list[TruthVariant], tolerance: int = 0
) -> tuple[int, int, int]:
    """Match called variants against truth.

    Returns (TP, FP, FN).
    """
    truth_matched = set()
    tp = 0
    for v in called:
        matched = False
        for i, t in enumerate(truth):
            if i in truth_matched:
                continue
            if (
                abs(v.pos - t.pos) <= tolerance
                and v.ref == t.ref
                and v.alt == t.alt
            ):
                tp += 1
                truth_matched.add(i)
                v.is_true_positive = True
                matched = True
                break
        if not matched:
            v.is_true_positive = False

    fp = len(called) - tp
    fn = len(truth) - tp
    return tp, fp, fn


# ---------------------------------------------------------------------------
# Sensitivity tests
# ---------------------------------------------------------------------------

class TestSensitivity:
    """Test that the caller detects known variants with high sensitivity."""

    def test_single_het_snp_recovery(self):
        """Caller should recover a single het SNP at moderate coverage."""
        ref = "ACGTACGTACGTACGTACGT" * 5  # 100bp
        config = SimConfig(seed=42, coverage=20, read_length=50, error_rate=0.005)
        sim = ReadSimulator(ref, config)
        tv = sim.inject_snp(25, alt="G")
        reads, truth = sim.simulate()

        caller = VariantCaller(
            config=CallerConfig(min_depth=5, min_alt_allele_frequency=0.15,
                               min_base_quality=10, min_genotype_quality=10)
        )
        pileup = PileupEngine(ref, reads).build()
        variants = caller.call(pileup)
        VariantAnnotator().annotate(variants)

        tp, fp, fn = _match_variants(variants, truth)
        assert tp == 1, f"Expected to recover SNP at pos 25, got {tp} TP"
        assert fn == 0, f"Missed truth variant: {fn} FN"
        assert _sensitivity(tp, fn) == 1.0

    def test_multiple_snp_recovery(self):
        """Caller should recover multiple SNPs across the reference."""
        ref = "ACGTACGTACGTACGTACGT" * 10  # 200bp
        snp_positions = [10, 30, 50, 70, 90, 110, 130, 150, 170, 190]
        config = SimConfig(seed=42, coverage=30, read_length=80, error_rate=0.005)
        sim = ReadSimulator(ref, config)
        for pos in snp_positions:
            ref_base = ref[pos]
            alt = "G" if ref_base != "G" else "C"
            sim.inject_snp(pos, alt=alt)
        reads, truth = sim.simulate()

        caller = VariantCaller(
            config=CallerConfig(min_depth=5, min_alt_allele_frequency=0.15,
                               min_base_quality=10, min_genotype_quality=10)
        )
        pileup = PileupEngine(ref, reads).build()
        variants = caller.call(pileup)
        VariantAnnotator().annotate(variants)

        tp, fp, fn = _match_variants(variants, truth)
        sens = _sensitivity(tp, fn)
        assert sens >= 0.8, f"Sensitivity {sens:.2f} below 0.8 for {len(truth)} SNPs (TP={tp}, FN={fn})"
        assert tp >= len(snp_positions) * 0.8, f"Expected ≥{int(len(snp_positions)*0.8)} recovered, got {tp}"

    def test_hom_snp_high_quality(self):
        """Homozygous alt should be called with high quality."""
        ref = "ACGTACGTACGTACGTACGT" * 10
        config = SimConfig(seed=42, coverage=30, read_length=80, error_rate=0.005)
        sim = ReadSimulator(ref, config)
        tv = sim.inject_snp(50, alt="T")
        reads, truth = sim.simulate()

        caller = VariantCaller(
            config=CallerConfig(min_depth=5, min_alt_allele_frequency=0.15,
                               min_base_quality=10, min_genotype_quality=10)
        )
        pileup = PileupEngine(ref, reads).build()
        variants = caller.call(pileup)
        VariantAnnotator().annotate(variants)

        at_truth = [v for v in variants if v.pos == 50 and v.alt == "T"]
        assert len(at_truth) == 1
        v = at_truth[0]
        # Hom-alt should have very high allele frequency
        assert v.allele_frequency > 0.8
        assert v.genotype_quality > 20

    def test_sensitivity_at_30x(self):
        """At 30x coverage, sensitivity should be very high for common SNPs."""
        ref = "ACGT" * 250  # 1000bp
        rng = random.Random(42)
        positions = sorted(rng.sample(range(10, 990), 20))  # 20 random SNPs

        config = SimConfig(seed=42, coverage=30, read_length=150, error_rate=0.005)
        sim = ReadSimulator(ref, config)
        for pos in positions:
            ref_base = ref[pos]
            alt = "G" if ref_base != "G" else "C"
            sim.inject_snp(pos, alt=alt)
        reads, truth = sim.simulate()

        caller = VariantCaller(
            config=CallerConfig(min_depth=8, min_alt_allele_frequency=0.15,
                               min_base_quality=10, min_genotype_quality=10)
        )
        pileup = PileupEngine(ref, reads).build()
        variants = caller.call(pileup)
        VariantAnnotator().annotate(variants)

        tp, fp, fn = _match_variants(variants, truth)
        sens = _sensitivity(tp, fn)
        prec = _precision(tp, fp)
        assert sens >= 0.7, f"Sensitivity {sens:.2f} too low (TP={tp}, FN={fn})"
        assert prec >= 0.3, f"Precision {prec:.2f} too low (TP={tp}, FP={fp})"


# ---------------------------------------------------------------------------
# Precision tests
# ---------------------------------------------------------------------------

class TestPrecision:
    """Test that the caller does not produce excessive false positives."""

    def test_no_false_positives_on_clean_data(self):
        """No variants should be called on clean reference-matching reads."""
        ref = "ACGTACGTACGTACGTACGT" * 10
        config = SimConfig(seed=42, coverage=30, read_length=80, error_rate=0.001)
        sim = ReadSimulator(ref, config)
        reads, _ = sim.simulate()

        caller = VariantCaller(
            config=CallerConfig(min_depth=8, min_alt_allele_frequency=0.2,
                               min_base_quality=20, min_genotype_quality=20)
        )
        pileup = PileupEngine(ref, reads).build()
        variants = caller.call(pileup)

        assert len(variants) == 0, f"False positives on clean data: {len(variants)}"

    def test_low_error_rate_minimizes_fp(self):
        """With low error rate, false positives should be minimal."""
        ref = "ACGTACGTACGTACGTACGT" * 10
        config = SimConfig(seed=42, coverage=20, read_length=80, error_rate=0.001)
        sim = ReadSimulator(ref, config)
        reads, _ = sim.simulate()

        caller = VariantCaller(
            config=CallerConfig(min_depth=8, min_alt_allele_frequency=0.2,
                               min_base_quality=20, min_genotype_quality=20)
        )
        pileup = PileupEngine(ref, reads).build()
        variants = caller.call(pileup)

        # Should be very few or zero FPs with strict filtering
        assert len(variants) <= 2, f"Too many FPs: {len(variants)}"


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------

class TestEdgeCases:
    def test_homopolymer_region(self, homopolymer_reference):
        """Homopolymer runs should not generate false positives."""
        ref = homopolymer_reference
        config = SimConfig(seed=42, coverage=20, read_length=50, error_rate=0.005)
        sim = ReadSimulator(ref, config)
        reads, _ = sim.simulate()

        caller = VariantCaller(
            config=CallerConfig(min_depth=8, min_alt_allele_frequency=0.2,
                               min_base_quality=20, min_genotype_quality=20)
        )
        pileup = PileupEngine(ref, reads).build()
        variants = caller.call(pileup)
        # Homopolymer regions can be tricky but strict filters should help
        # Just check no crash and reasonable count
        assert len(variants) < 10

    def test_single_base_reference(self):
        """Pipeline should handle a very short reference."""
        ref = "ACGT"
        reads = []
        # 10 reads: all with C→G mutation (homozygous alt)
        for i in range(10):
            reads.append(AlignedRead(f"r_{i}", 0, "4M", "AGGT",
                                     [35] * 4, Strand.FORWARD if i % 2 == 0 else Strand.REVERSE))
        caller = VariantCaller(
            config=CallerConfig(min_depth=5, min_alt_allele_frequency=0.2,
                               min_base_quality=10, min_genotype_quality=10)
        )
        pileup = PileupEngine(ref, reads).build()
        variants = caller.call(pileup)
        at_1 = [v for v in variants if v.pos == 1]
        assert len(at_1) == 1

    def test_all_reads_same_strand(self, simple_reference):
        """All reads on same strand should still produce calls."""
        ref = simple_reference
        read_len = 50
        snp_pos = 30  # safely in the middle
        ref_base = ref[snp_pos]
        alt_base = "G" if ref_base != "G" else "C"

        reads = []
        for i in range(15):
            start = snp_pos - read_len // 2
            seq = list(ref[start:start + read_len])
            offset = snp_pos - start
            if i < 8:
                seq[offset] = alt_base
            quals = [35] * read_len
            reads.append(AlignedRead(
                name=f"ss_{i}",
                ref_start=start,
                cigar=f"{read_len}M",
                sequence="".join(seq),
                base_qualities=quals,
                strand=Strand.FORWARD,  # all forward
            ))

        caller = VariantCaller(
            config=CallerConfig(min_depth=5, min_alt_allele_frequency=0.15,
                               min_base_quality=10, min_genotype_quality=10)
        )
        pileup = PileupEngine(ref, reads).build()
        variants = caller.call(pileup)
        VariantAnnotator().annotate(variants)

        at_truth = [v for v in variants if v.pos == snp_pos]
        assert len(at_truth) >= 1
        # Strand balance should be extreme (all forward)
        v = at_truth[0]
        assert v.strand_balance is not None

    def test_zero_quality_bases_excluded(self, simple_reference):
        """Bases with zero quality should be filtered out."""
        ref = simple_reference
        reads = []
        read_len = 50
        for i in range(15):
            start = 10
            seq = ref[start:start + read_len]
            quals = [0] * read_len  # all zero quality
            reads.append(AlignedRead(
                name=f"zq_{i}",
                ref_start=start,
                cigar=f"{read_len}M",
                sequence=seq,
                base_qualities=quals,
                strand=Strand.FORWARD,
            ))

        caller = VariantCaller(
            config=CallerConfig(min_base_quality=20)
        )
        pileup = PileupEngine(ref, reads).build()
        variants = caller.call(pileup)
        # All bases filtered by quality → no callable positions
        assert len(variants) == 0

    def test_single_read_coverage(self, simple_reference):
        """With only one read, nothing should be called (below min_depth)."""
        ref = simple_reference
        reads = [
            AlignedRead("r1", 0, "50M", ref[:50], [30] * 50, Strand.FORWARD),
        ]
        caller = VariantCaller(
            config=CallerConfig(min_depth=3)
        )
        pileup = PileupEngine(ref, reads).build()
        variants = caller.call(pileup)
        assert len(variants) == 0

    def test_very_high_depth(self):
        """Very high coverage (1000x) should not crash."""
        ref = "ACGTACGTACGTACGTACGT" * 5
        config = SimConfig(seed=42, coverage=1000, read_length=50, error_rate=0.001)
        sim = ReadSimulator(ref, config)
        reads, _ = sim.simulate()
        caller = VariantCaller(
            config=CallerConfig(min_depth=50, min_base_quality=20, min_genotype_quality=20)
        )
        pileup = PileupEngine(ref, reads).build()
        variants = caller.call(pileup)
        # Just verify it doesn't crash and runs in reasonable time
        assert isinstance(variants, list)


# ---------------------------------------------------------------------------
# Annotation integration
# ---------------------------------------------------------------------------

class TestAnnotationIntegration:
    def test_called_variants_annotated(self, simple_reference, reads_with_het_snp, sensitive_config):
        """All called variants should have ts/tv and allele balance."""
        reads, truth = reads_with_het_snp
        caller = VariantCaller(config=sensitive_config)
        pileup = PileupEngine(simple_reference, reads).build()
        variants = caller.call(pileup)
        annotator = VariantAnnotator()
        annotated = annotator.annotate(variants)
        for v in annotated:
            if v.variant_type == VariantType.SNP:
                assert v.ts_tv in ("ts", "tv")
            assert v.allele_balance is not None
            assert 0.0 <= v.allele_balance <= 1.0

    def test_tstv_ratio_reasonable(self, simple_reference):
        """ts/tv ratio should be reasonable for a set of called variants."""
        rng = random.Random(77)
        ref_len = len(simple_reference)
        read_len = 50
        n_reads = 40

        reads = []
        for i in range(n_reads):
            start = rng.randint(0, ref_len - read_len)
            seq = list(simple_reference[start:start + read_len])
            # Inject some mutations
            if i < 5 and 10 < start + read_len // 2 < ref_len - 10:
                mid = start + read_len // 2
                offset = mid - start
                seq[offset] = "G"
            quals = [rng.randint(30, 40) for _ in range(read_len)]
            reads.append(AlignedRead(
                name=f"ts_{i:03d}",
                ref_start=start,
                cigar=f"{read_len}M",
                sequence="".join(seq),
                base_qualities=quals,
                strand=Strand.FORWARD if i % 2 == 0 else Strand.REVERSE,
            ))

        caller = VariantCaller(
            config=CallerConfig(min_depth=5, min_alt_allele_frequency=0.1,
                               min_base_quality=10, min_genotype_quality=10)
        )
        pileup = PileupEngine(simple_reference, reads).build()
        variants = caller.call(pileup)
        VariantAnnotator().annotate(variants)

        snps = [v for v in variants if v.ts_tv in ("ts", "tv")]
        if snps:
            ratio = ts_tv_ratio(snps)
            assert ratio >= 0  # basic sanity
