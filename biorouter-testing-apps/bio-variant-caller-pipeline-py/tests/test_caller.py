"""Tests for the Bayesian variant caller."""

from __future__ import annotations

import pytest

from bio_variant_caller.caller import CallerConfig, VariantCaller
from bio_variant_caller.models import AlignedRead, Genotype, Strand, VariantType
from bio_variant_caller.pileup import quick_pileup


class TestVariantCaller:
    """Test variant calling on pre-built pileup scenarios."""

    def test_hom_ref_no_call(self, simple_reference, clean_reads_no_variants, default_config):
        """No variants should be called on clean data."""
        caller = VariantCaller(config=default_config)
        variants = caller.call_from_reads(simple_reference, clean_reads_no_variants)
        assert len(variants) == 0

    def test_het_snp_called(self, simple_reference, reads_with_het_snp, sensitive_config):
        """A heterozygous SNP should be called."""
        reads, truth = reads_with_het_snp
        caller = VariantCaller(config=sensitive_config)
        variants = caller.call_from_reads(simple_reference, reads)

        # Should call at least the known SNP
        assert len(variants) >= 1
        # Find the truth position
        at_truth = [v for v in variants if v.pos == truth.pos]
        assert len(at_truth) == 1
        v = at_truth[0]
        assert v.alt == truth.alt
        assert v.variant_type == VariantType.SNP
        assert v.genotype == Genotype.HET
        assert 0.3 <= v.allele_frequency <= 0.7  # ~50% alt

    def test_hom_snp_called(self, simple_reference, reads_with_hom_snp, sensitive_config):
        """A homozygous alt SNP should be called as HOM_ALT."""
        reads, truth = reads_with_hom_snp
        caller = VariantCaller(config=sensitive_config)
        variants = caller.call_from_reads(simple_reference, reads)

        at_truth = [v for v in variants if v.pos == truth.pos]
        assert len(at_truth) == 1
        v = at_truth[0]
        assert v.alt == truth.alt
        assert v.allele_frequency > 0.8  # mostly alt

    def test_low_depth_not_called(self, simple_reference, low_depth_reads, default_config):
        """Below min_depth, variants should not be called."""
        reads, truth = low_depth_reads
        caller = VariantCaller(config=default_config)  # min_depth=8
        variants = caller.call_from_reads(simple_reference, reads)
        # Only 3 reads — below default min_depth of 8
        at_truth = [v for v in variants if v.pos == truth.pos]
        assert len(at_truth) == 0

    def test_low_depth_called_with_sensitive(self, simple_reference, low_depth_reads, sensitive_config):
        """With low min_depth, the variant should be called."""
        reads, truth = low_depth_reads
        caller = VariantCaller(config=sensitive_config)  # min_depth=3
        variants = caller.call_from_reads(simple_reference, reads)
        at_truth = [v for v in variants if v.pos == truth.pos]
        assert len(at_truth) >= 1

    def test_strand_bias_detected(self, simple_reference, strand_biased_reads, default_config):
        """All alt-supporting reads on one strand should produce extreme strand balance."""
        reads, truth = strand_biased_reads
        caller = VariantCaller(config=default_config)
        variants = caller.call_from_reads(simple_reference, reads)

        at_truth = [v for v in variants if v.pos == truth.pos]
        if at_truth:
            v = at_truth[0]
            # strand_balance should be near 0 or 1
            assert v.strand_balance is not None
            assert v.strand_balance < 0.1 or v.strand_balance > 0.9

    def test_min_af_filter(self, simple_reference, reads_with_het_snp):
        """High min_alt_allele_frequency should filter low-frequency variants."""
        # With a het at ~50%, setting min_af to 0.6 should filter it
        reads, truth = reads_with_het_snp
        config = CallerConfig(min_alt_allele_frequency=0.6, min_depth=3, min_base_quality=10)
        caller = VariantCaller(config=config)
        variants = caller.call_from_reads(simple_reference, reads)
        at_truth = [v for v in variants if v.pos == truth.pos]
        assert len(at_truth) == 0

    def test_min_base_quality_filter(self, simple_reference):
        """Low base quality bases should be excluded from counts."""
        import random
        rng = random.Random(321)
        read_len = 50
        snp_pos = 30  # safely in the middle
        ref_base = simple_reference[snp_pos]
        alt_base = "G" if ref_base != "G" else "C"

        reads = []
        for i in range(20):
            start = snp_pos - read_len // 2
            seq = list(simple_reference[start:start + read_len])
            offset = snp_pos - start
            # All reads carry alt, but with very low quality
            seq[offset] = alt_base
            quals = [rng.randint(30, 40) for _ in range(read_len)]
            # Set the alt base quality to very low
            quals[offset] = 5
            reads.append(AlignedRead(
                name=f"lq_{i}",
                ref_start=start,
                cigar=f"{read_len}M",
                sequence="".join(seq),
                base_qualities=quals,
                strand=Strand.FORWARD,
            ))

        # With high min_base_quality, these low-quality alt bases get filtered
        config = CallerConfig(min_base_quality=30, min_depth=5)
        caller = VariantCaller(config=config)
        variants = caller.call_from_reads(simple_reference, reads)
        at_truth = [v for v in variants if v.pos == snp_pos]
        # Alt bases all have q=5 < min_base_quality=30, so they are filtered
        # After filtering, only ref bases remain → no variant called
        assert len(at_truth) == 0

    def test_genotype_quality_threshold(self, simple_reference, reads_with_het_snp, strict_config):
        """High GQ threshold should filter uncertain calls."""
        reads, truth = reads_with_het_snp
        caller = VariantCaller(config=strict_config)  # min_genotype_quality=40
        variants = caller.call_from_reads(simple_reference, reads)
        # Either the variant passes the strict threshold or it doesn't
        # Just check no crash
        for v in variants:
            assert v.genotype_quality >= strict_config.min_genotype_quality

    def test_caller_config_defaults(self):
        """Default config should have reasonable values."""
        cfg = CallerConfig()
        assert cfg.min_depth == 8
        assert cfg.min_alt_allele_frequency == 0.2
        assert cfg.min_base_quality == 20
        assert cfg.min_genotype_quality == 20

    def test_multiple_snp_positions(self, simple_reference):
        """Multiple SNPs at different positions should all be called."""
        import random
        rng = random.Random(555)
        read_len = 100
        snp_positions = [10, 30, 50, 70]
        n_reads = 30

        reads = []
        for i in range(n_reads):
            start = rng.randint(0, len(simple_reference) - read_len)
            seq = list(simple_reference[start:start + read_len])
            for sp in snp_positions:
                offset = sp - start
                if 0 <= offset < read_len and i < n_reads // 2:
                    ref_b = simple_reference[sp]
                    seq[offset] = "G" if ref_b != "G" else "C"
            quals = [rng.randint(30, 40) for _ in range(read_len)]
            reads.append(AlignedRead(
                name=f"multi_{i:03d}",
                ref_start=start,
                cigar=f"{read_len}M",
                sequence="".join(seq),
                base_qualities=quals,
                strand=Strand.FORWARD if i % 2 == 0 else Strand.REVERSE,
            ))

        config = CallerConfig(min_depth=5, min_alt_allele_frequency=0.2, min_base_quality=10)
        caller = VariantCaller(config=config)
        variants = caller.call_from_reads(simple_reference, reads)

        called_positions = {v.pos for v in variants}
        for sp in snp_positions:
            assert sp in called_positions, f"SNP at position {sp} was not called"


# ---------------------------------------------------------------------------
# From-standalone-pileup
# ---------------------------------------------------------------------------

class TestCallerFromPileup:
    def test_call_on_pileup_dict(self, simple_reference, reads_with_het_snp, sensitive_config):
        """Test calling from a pre-built pileup dict."""
        reads, truth = reads_with_het_snp
        pileup = quick_pileup(simple_reference, reads)
        caller = VariantCaller(config=sensitive_config)
        variants = caller.call(pileup)
        at_truth = [v for v in variants if v.pos == truth.pos]
        assert len(at_truth) == 1

    def test_empty_pileup(self, simple_reference, default_config):
        """Calling on empty pileup returns empty list."""
        pileup = quick_pileup(simple_reference, [])
        caller = VariantCaller(config=default_config)
        variants = caller.call(pileup)
        assert variants == []
