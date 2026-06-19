"""Shared test fixtures for the variant-calling pipeline tests."""

from __future__ import annotations

import random

import pytest

from bio_variant_caller.models import AlignedRead, PileupPosition, Strand, Variant
from bio_variant_caller.pileup import PileupEngine, quick_pileup
from bio_variant_caller.caller import CallerConfig, VariantCaller
from bio_variant_caller.simulate import ReadSimulator, SimConfig, TruthVariant


# ---------------------------------------------------------------------------
# Reference sequences
# ---------------------------------------------------------------------------

@pytest.fixture
def simple_reference() -> str:
    """A short, simple reference sequence (100 bp)."""
    return "ACGTACGTACGTACGTACGT" * 5  # 100 bp repeating pattern


@pytest.fixture
def long_reference() -> str:
    """A longer reference (1000 bp) with some complexity."""
    rng = random.Random(99)
    return "".join(rng.choice("ACGT") for _ in range(1000))


@pytest.fixture
def homopolymer_reference() -> str:
    """Reference containing homopolymer runs (A-run, G-run)."""
    return "ACGT" * 5 + "AAAAA" + "CGTG" * 5 + "GGGGG" + "ACGT" * 5


# ---------------------------------------------------------------------------
# Read sets
# ---------------------------------------------------------------------------

@pytest.fixture
def clean_reads_no_variants(simple_reference) -> list[AlignedRead]:
    """20 reads covering the reference with no variants (30x equivalent)."""
    rng = random.Random(123)
    ref_len = len(simple_reference)
    read_len = 50
    n_reads = 20
    reads = []
    for i in range(n_reads):
        start = rng.randint(0, ref_len - read_len)
        seq = simple_reference[start:start + read_len]
        quals = [rng.randint(30, 40) for _ in range(read_len)]
        strand = Strand.FORWARD if i % 2 == 0 else Strand.REVERSE
        reads.append(AlignedRead(
            name=f"clean_{i:03d}",
            ref_start=start,
            cigar=f"{read_len}M",
            sequence=seq,
            base_qualities=quals,
            strand=strand,
        ))
    return reads


@pytest.fixture
def reads_with_het_snp(simple_reference) -> tuple[list[AlignedRead], TruthVariant]:
    """20 reads, half carrying a SNP at position 25 (A→G, heterozygous)."""
    rng = random.Random(456)
    ref_len = len(simple_reference)
    read_len = 50
    snp_pos = 25
    ref_base = simple_reference[snp_pos]
    alt_base = "G" if ref_base != "G" else "C"
    n_reads = 20

    reads = []
    for i in range(n_reads):
        start = rng.randint(max(0, snp_pos - read_len + 1), min(ref_len - read_len, snp_pos))
        seq = list(simple_reference[start:start + read_len])

        # Inject alt into half the reads if they cover the snp_pos
        offset_in_read = snp_pos - start
        has_alt = (i < n_reads // 2) and 0 <= offset_in_read < read_len
        if has_alt:
            seq[offset_in_read] = alt_base

        quals = [rng.randint(30, 40) for _ in range(read_len)]
        strand = Strand.FORWARD if i % 2 == 0 else Strand.REVERSE
        reads.append(AlignedRead(
            name=f"het_{i:03d}",
            ref_start=start,
            cigar=f"{read_len}M",
            sequence="".join(seq),
            base_qualities=quals,
            strand=strand,
        ))

    truth = TruthVariant(pos=snp_pos, ref=ref_base, alt=alt_base)
    return reads, truth


@pytest.fixture
def reads_with_hom_snp(simple_reference) -> tuple[list[AlignedRead], TruthVariant]:
    """20 reads, all carrying a SNP at position 10 (homozygous alt)."""
    rng = random.Random(789)
    ref_len = len(simple_reference)
    read_len = 50
    snp_pos = 10
    ref_base = simple_reference[snp_pos]
    alt_base = "T" if ref_base != "T" else "A"
    n_reads = 20

    reads = []
    for i in range(n_reads):
        start = rng.randint(max(0, snp_pos - read_len + 1), min(ref_len - read_len, snp_pos))
        seq = list(simple_reference[start:start + read_len])

        offset_in_read = snp_pos - start
        if 0 <= offset_in_read < read_len:
            seq[offset_in_read] = alt_base

        quals = [rng.randint(30, 40) for _ in range(read_len)]
        strand = Strand.FORWARD if i % 2 == 0 else Strand.REVERSE
        reads.append(AlignedRead(
            name=f"hom_{i:03d}",
            ref_start=start,
            cigar=f"{read_len}M",
            sequence="".join(seq),
            base_qualities=quals,
            strand=strand,
        ))

    truth = TruthVariant(pos=snp_pos, ref=ref_base, alt=alt_base)
    return reads, truth


@pytest.fixture
def low_depth_reads(simple_reference) -> tuple[list[AlignedRead], TruthVariant]:
    """Only 5 reads at a position — below typical calling thresholds."""
    rng = random.Random(101)
    read_len = 50
    snp_pos = 30
    ref_base = simple_reference[snp_pos]
    alt_base = "G" if ref_base != "G" else "C"

    reads = []
    for i in range(5):
        start = snp_pos - read_len // 2
        seq = list(simple_reference[start:start + read_len])
        offset = snp_pos - start
        # All 5 reads carry alt (homozygous) to make calling feasible at low depth
        seq[offset] = alt_base
        quals = [rng.randint(30, 40) for _ in range(read_len)]
        reads.append(AlignedRead(
            name=f"low_{i}",
            ref_start=start,
            cigar=f"{read_len}M",
            sequence="".join(seq),
            base_qualities=quals,
            strand=Strand.FORWARD,
        ))

    truth = TruthVariant(pos=snp_pos, ref=ref_base, alt=alt_base)
    return reads, truth


@pytest.fixture
def strand_biased_reads(simple_reference) -> tuple[list[AlignedRead], TruthVariant]:
    """Reads where all alt-supporting reads are on one strand (strand bias)."""
    rng = random.Random(202)
    read_len = 50
    snp_pos = 40
    ref_base = simple_reference[snp_pos]
    alt_base = "C" if ref_base != "C" else "G"
    n_reads = 20

    reads = []
    for i in range(n_reads):
        start = snp_pos - read_len // 2
        seq = list(simple_reference[start:start + read_len])
        offset = snp_pos - start

        # Alt only on forward strand reads
        is_alt = (i < n_reads // 2)
        is_forward = is_alt  # all alt reads are forward, all ref reads are reverse
        if is_alt and 0 <= offset < read_len:
            seq[offset] = alt_base

        quals = [rng.randint(30, 40) for _ in range(read_len)]
        reads.append(AlignedRead(
            name=f"sb_{i:03d}",
            ref_start=start,
            cigar=f"{read_len}M",
            sequence="".join(seq),
            base_qualities=quals,
            strand=Strand.FORWARD if is_forward else Strand.REVERSE,
        ))

    truth = TruthVariant(pos=snp_pos, ref=ref_base, alt=alt_base)
    return reads, truth


# ---------------------------------------------------------------------------
# Caller config fixtures
# ---------------------------------------------------------------------------

@pytest.fixture
def default_config() -> CallerConfig:
    return CallerConfig()


@pytest.fixture
def sensitive_config() -> CallerConfig:
    """Low thresholds for sensitivity testing."""
    return CallerConfig(
        min_depth=3,
        min_alt_allele_frequency=0.15,
        min_base_quality=10,
        min_genotype_quality=10,
    )


@pytest.fixture
def strict_config() -> CallerConfig:
    """High thresholds for high-precision calling."""
    return CallerConfig(
        min_depth=15,
        min_alt_allele_frequency=0.35,
        min_base_quality=30,
        min_genotype_quality=40,
    )
