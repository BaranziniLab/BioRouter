"""Tests for the pileup engine."""

from __future__ import annotations

import pytest

from bio_variant_caller.models import AlignedRead, PileupPosition, Strand
from bio_variant_caller.pileup import (
    PileupEngine,
    cigar_consumed_bases,
    parse_cigar,
    quick_pileup,
)


# ---------------------------------------------------------------------------
# CIGAR parsing
# ---------------------------------------------------------------------------

class TestCigarParsing:
    def test_simple_match(self):
        assert parse_cigar("100M") == [(100, "M")]

    def test_mixed_ops(self):
        result = parse_cigar("10M2I5M3D8M")
        assert result == [(10, "M"), (2, "I"), (5, "M"), (3, "D"), (8, "M")]

    def test_clips(self):
        result = parse_cigar("5S90M5S")
        assert result == [(5, "S"), (90, "M"), (5, "S")]

    def test_empty(self):
        assert parse_cigar("") == []

    def test_consumed_bases_match_only(self):
        ops = parse_cigar("100M")
        q, r = cigar_consumed_bases(ops)
        assert q == 100
        assert r == 100

    def test_consumed_bases_with_indel(self):
        ops = parse_cigar("10M2I5M3D8M")
        q, r = cigar_consumed_bases(ops)
        assert q == 25  # 10 + 2 + 5 + 8 (M and I consume query)
        assert r == 26  # 10 + 5 + 3 + 8 (M and D consume ref)


# ---------------------------------------------------------------------------
# Pileup engine
# ---------------------------------------------------------------------------

class TestPileupEngine:
    def test_single_read_full_coverage(self, simple_reference):
        """A single read covering the entire reference."""
        reads = [
            AlignedRead(
                name="r1",
                ref_start=0,
                cigar=f"{len(simple_reference)}M",
                sequence=simple_reference,
                base_qualities=[30] * len(simple_reference),
                strand=Strand.FORWARD,
            )
        ]
        pileup = quick_pileup(simple_reference, reads)
        assert len(pileup) == len(simple_reference)
        for pos in range(len(simple_reference)):
            assert pileup[pos].depth == 1
            assert pileup[pos].ref_base == simple_reference[pos]

    def test_two_reads_same_position(self, simple_reference):
        """Two reads at the same position."""
        seq = simple_reference[0:50]
        reads = [
            AlignedRead("r1", 0, "50M", seq, [30] * 50, Strand.FORWARD),
            AlignedRead("r2", 0, "50M", seq, [35] * 50, Strand.REVERSE),
        ]
        pileup = quick_pileup(simple_reference, reads)
        assert pileup[0].depth == 2
        assert pileup[25].depth == 2

    def test_overlapping_reads(self, simple_reference):
        """Two reads that partially overlap."""
        reads = [
            AlignedRead("r1", 0, "50M", simple_reference[0:50], [30] * 50, Strand.FORWARD),
            AlignedRead("r2", 25, "50M", simple_reference[25:75], [35] * 50, Strand.REVERSE),
        ]
        pileup = quick_pileup(simple_reference, reads)
        # Positions 0-24: depth 1
        assert pileup[0].depth == 1
        # Positions 25-49: depth 2
        assert pileup[25].depth == 2
        assert pileup[49].depth == 2
        # Positions 50-74: depth 1
        assert pileup[50].depth == 1

    def test_empty_pileup(self, simple_reference):
        """No reads → empty pileup."""
        pileup = quick_pileup(simple_reference, [])
        assert len(pileup) == 0

    def test_base_counts(self, simple_reference):
        """Check base counts at a position with mixed bases."""
        ref_base = simple_reference[0]
        reads = [
            AlignedRead("r1", 0, "50M", simple_reference[:50], [30] * 50, Strand.FORWARD),
            AlignedRead("r2", 0, "50M", simple_reference[:50], [30] * 50, Strand.FORWARD),
            AlignedRead("r3", 0, "50M",
                        "X" + simple_reference[1:50],  # mutation at pos 0
                        [30] * 50, Strand.REVERSE),
        ]
        pileup = quick_pileup(simple_reference, reads)
        counts = pileup[0].base_counts()
        assert counts.get(ref_base, 0) == 2
        assert counts.get("X", 0) == 1

    def test_strand_counts(self, simple_reference):
        """Verify strand breakdown."""
        reads = [
            AlignedRead("r1", 0, "50M", simple_reference[:50], [30] * 50, Strand.FORWARD),
            AlignedRead("r2", 0, "50M", simple_reference[:50], [30] * 50, Strand.REVERSE),
        ]
        pileup = quick_pileup(simple_reference, reads)
        sc = pileup[0].strand_counts()
        ref = simple_reference[0]
        assert sc[ref]["forward"] == 1
        assert sc[ref]["reverse"] == 1

    def test_min_mapq_filter(self, simple_reference):
        """Reads below mapq threshold should be excluded."""
        reads = [
            AlignedRead("r1", 0, "50M", simple_reference[:50], [30] * 50,
                        Strand.FORWARD, map_quality=10),
            AlignedRead("r2", 0, "50M", simple_reference[:50], [30] * 50,
                        Strand.FORWARD, map_quality=60),
        ]
        engine = PileupEngine(simple_reference, reads, min_mapq=30)
        pileup = engine.build()
        assert pileup[0].depth == 1

    def test_quality_weighted_counts(self, simple_reference):
        """Quality-weighted counts should favor high-quality bases."""
        reads = [
            AlignedRead("r1", 0, "50M", simple_reference[:50], [40] * 50, Strand.FORWARD),
            AlignedRead("r2", 0, "50M",
                        "X" + simple_reference[1:50],
                        [5] * 50, Strand.FORWARD),  # low quality alt
        ]
        pileup = quick_pileup(simple_reference, reads)
        wqc = pileup[0].quality_weighted_counts()
        ref = simple_reference[0]
        # High-quality ref base should have much higher weight
        assert wqc[ref] > wqc.get("X", 0)

    def test_covered_positions(self, simple_reference):
        """Covered positions should be sorted."""
        reads = [
            AlignedRead("r1", 0, "10M", simple_reference[:10], [30] * 10, Strand.FORWARD),
            AlignedRead("r2", 50, "10M", simple_reference[50:60], [30] * 10, Strand.FORWARD),
        ]
        engine = PileupEngine(simple_reference, reads)
        covered = engine.covered_positions()
        assert covered == sorted(covered)
        assert 0 in covered
        assert 50 in covered
        assert 25 not in covered

    def test_depth_at(self, simple_reference):
        """depth_at returns 0 for uncovered positions."""
        reads = [
            AlignedRead("r1", 10, "10M", simple_reference[10:20], [30] * 10, Strand.FORWARD),
        ]
        engine = PileupEngine(simple_reference, reads)
        assert engine.depth_at(10) == 1
        assert engine.depth_at(15) == 1
        assert engine.depth_at(0) == 0

    def test_deletion_cigar(self, simple_reference):
        """A deletion CIGAR should create positions marked as deletion."""
        # Read with a 3bp deletion at ref positions 5-7
        # CIGAR: 5M3D45M — ref consumes 53, query consumes 50
        seq = simple_reference[:5] + simple_reference[8:50]  # skip 3 ref bases
        reads = [
            AlignedRead("r1", 0, "5M3D45M", seq, [30] * 50, Strand.FORWARD),
        ]
        pileup = quick_pileup(simple_reference, reads)
        # Deletion positions should have is_deletion bases
        assert len(pileup[5].bases) > 0
        del_bases = [b for b in pileup[5].bases if b.is_deletion]
        assert len(del_bases) > 0

    def test_insertion_cigar(self, simple_reference):
        """An insertion CIGAR should produce extra bases at the insertion point."""
        # Insertion of 2 bases after ref position 5
        # CIGAR: 6M2I44M — ref consumes 50, query consumes 52
        seq = simple_reference[:6] + "NN" + simple_reference[6:50]
        reads = [
            AlignedRead("r1", 0, "6M2I44M", seq, [30] * 52, Strand.FORWARD),
        ]
        pileup = quick_pileup(simple_reference, reads)
        # Position 5 (preceding the insertion) should have insertion-marked bases
        ins_bases = [b for b in pileup[5].bases if b.is_insertion]
        assert len(ins_bases) > 0
