"""Tests for the read simulator."""

from __future__ import annotations

import pytest

from bio_variant_caller.models import AlignedRead, Strand, VariantType
from bio_variant_caller.simulate import (
    ReadSimulator,
    SimConfig,
    TruthVariant,
    create_truth_variants,
    simulate_reads,
)


class TestReadSimulator:
    def test_simulate_no_variants(self, simple_reference):
        """Simulating without variants should produce reads matching the reference."""
        config = SimConfig(seed=42, coverage=5, read_length=50)
        sim = ReadSimulator(simple_reference, config)
        reads, truth = sim.simulate()
        assert len(truth) == 0
        assert len(reads) > 0
        # All reads should be valid
        for r in reads:
            assert r.ref_start >= 0
            assert r.ref_start + len(r.sequence) <= len(simple_reference)
            assert len(r.sequence) == len(r.base_qualities)

    def test_simulate_with_snp(self, simple_reference):
        """Simulating with an injected SNP should carry it in the reads."""
        config = SimConfig(seed=42, coverage=20, read_length=50)
        sim = ReadSimulator(simple_reference, config)
        snp_pos = 25
        ref_base = simple_reference[snp_pos]
        alt_base = "G" if ref_base != "G" else "C"
        sim.inject_snp(snp_pos, alt=alt_base)
        reads, truth = sim.simulate()

        assert len(truth) == 1
        assert truth[0].pos == snp_pos
        assert truth[0].alt == alt_base

        # Reads covering snp_pos should carry the alt base
        reads_at_pos = [
            r for r in reads
            if r.ref_start <= snp_pos < r.ref_start + len(r.sequence)
        ]
        assert len(reads_at_pos) > 0
        for r in reads_at_pos:
            offset = snp_pos - r.ref_start
            assert r.sequence[offset] == alt_base

    def test_coverage_approximation(self, simple_reference):
        """Simulated coverage should be approximately as requested."""
        config = SimConfig(seed=42, coverage=10, read_length=50)
        sim = ReadSimulator(simple_reference, config)
        reads, _ = sim.simulate()
        # Expected reads ≈ (ref_len * coverage) / read_len
        expected = int(len(simple_reference) * 10 / 50)
        assert abs(len(reads) - expected) <= 2

    def test_read_lengths_match(self, simple_reference):
        """All reads should have the configured read length."""
        config = SimConfig(seed=42, coverage=5, read_length=75)
        sim = ReadSimulator(simple_reference, config)
        reads, _ = sim.simulate()
        for r in reads:
            assert len(r.sequence) == 75

    def test_base_qualities_present(self, simple_reference):
        """All base qualities should be within configured range."""
        config = SimConfig(seed=42, coverage=5, read_length=50,
                           min_base_quality=20, max_base_quality=40)
        sim = ReadSimulator(simple_reference, config)
        reads, _ = sim.simulate()
        for r in reads:
            for q in r.base_qualities:
                assert 1 <= q <= 40

    def test_reproducibility(self, simple_reference):
        """Same seed should produce identical results."""
        config1 = SimConfig(seed=42, coverage=10, read_length=50)
        config2 = SimConfig(seed=42, coverage=10, read_length=50)
        reads1, _ = ReadSimulator(simple_reference, config1).simulate()
        reads2, _ = ReadSimulator(simple_reference, config2).simulate()
        assert len(reads1) == len(reads2)
        for r1, r2 in zip(reads1, reads2):
            assert r1.name == r2.name
            assert r1.ref_start == r2.ref_start
            assert r1.sequence == r2.sequence

    def test_different_seeds(self, simple_reference):
        """Different seeds should produce different reads."""
        config1 = SimConfig(seed=1, coverage=10, read_length=30)
        config2 = SimConfig(seed=99, coverage=10, read_length=30)
        reads1, _ = ReadSimulator(simple_reference, config1).simulate()
        reads2, _ = ReadSimulator(simple_reference, config2).simulate()
        # At least one read should differ in position or sequence
        sigs1 = [(r.ref_start, r.sequence[:5]) for r in reads1]
        sigs2 = [(r.ref_start, r.sequence[:5]) for r in reads2]
        assert sigs1 != sigs2

    def test_add_variant(self, simple_reference):
        """add_variant should register and return a TruthVariant."""
        sim = ReadSimulator(simple_reference, SimConfig(seed=1))
        tv = sim.add_variant(10, ref="A", alt="G")
        assert tv.pos == 10
        assert tv.ref == "A"
        assert tv.alt == "G"
        assert tv.variant_type == VariantType.SNP

    def test_inject_snp_convenience(self, simple_reference):
        """inject_snp should auto-detect ref base."""
        sim = ReadSimulator(simple_reference, SimConfig(seed=1))
        expected_ref = simple_reference[20]
        tv = sim.inject_snp(20)
        assert tv.ref == expected_ref

    def test_truth_vcf_generation(self, simple_reference):
        """generate_truth_vcf should return Variant objects."""
        sim = ReadSimulator(simple_reference, SimConfig(seed=1))
        sim.inject_snp(10, alt="G")
        sim.inject_snp(30, alt="T")
        truth_vcf = sim.generate_truth_vcf()
        assert len(truth_vcf) == 2
        assert truth_vcf[0].pos == 10
        assert truth_vcf[1].pos == 30

    def test_reads_cover_injected_positions(self, simple_reference):
        """Reads should cover the positions where variants are injected."""
        config = SimConfig(seed=42, coverage=20, read_length=100)
        sim = ReadSimulator(simple_reference, config)
        sim.inject_snp(50, alt="G")
        reads, _ = sim.simulate()

        covering = [
            r for r in reads
            if r.ref_start <= 50 < r.ref_start + len(r.sequence)
        ]
        assert len(covering) > 0


class TestConvenienceFunctions:
    def test_simulate_reads_function(self, simple_reference):
        """The simulate_reads function should work end-to-end."""
        config = SimConfig(seed=42, coverage=5, read_length=50)
        reads, truth = simulate_reads(simple_reference, config=config)
        assert len(reads) > 0
        assert len(truth) == 0

    def test_create_truth_variants(self, simple_reference):
        """create_truth_variants should create a list of TruthVariant."""
        truth = create_truth_variants(
            simple_reference,
            positions=[10, 20, 30],
            alts=["G", "T", "A"],
        )
        assert len(truth) == 3
        assert truth[0].pos == 10
        assert truth[0].alt == "G"
        assert truth[1].pos == 20
        assert truth[1].alt == "T"
        assert truth[2].pos == 30
        assert truth[2].alt == "A"
