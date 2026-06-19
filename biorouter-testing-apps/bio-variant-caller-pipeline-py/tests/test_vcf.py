"""Tests for VCF output writer."""

from __future__ import annotations

import io

import pytest

from bio_variant_caller.models import Genotype, Variant, VariantType
from bio_variant_caller.vcf import VCFWriter, variants_to_vcf_string, write_vcf


def _make_variant(
    pos: int = 100,
    ref: str = "A",
    alt: str = "G",
    depth: int = 30,
    alt_count: int = 15,
    quality: float = 50.0,
) -> Variant:
    af = alt_count / depth if depth else 0.0
    return Variant(
        chrom="chr1",
        pos=pos,
        ref=ref,
        alt=alt,
        variant_type=VariantType.SNP,
        quality=quality,
        depth=depth,
        alt_count=alt_count,
        allele_frequency=af,
        genotype=Genotype.HET,
        genotype_quality=50.0,
        ts_tv="ts",
        allele_balance=af,
        strand_balance=0.5,
    )


class TestVCFWriter:
    def test_header_lines(self):
        """Header should contain VCF version and column names."""
        writer = VCFWriter()
        buf = io.StringIO()
        writer.write_header(buf)
        content = buf.getvalue()
        assert "##fileformat=VCFv4.2" in content
        assert "#CHROM" in content
        assert "POS" in content
        assert "REF" in content
        assert "ALT" in content
        assert "QUAL" in content
        assert "FILTER" in content
        assert "INFO" in content

    def test_sample_column_in_header(self):
        """Sample name should appear in the header."""
        writer = VCFWriter(sample_name="MY_SAMPLE")
        buf = io.StringIO()
        writer.write_header(buf)
        assert "MY_SAMPLE" in buf.getvalue()

    def test_single_variant_record(self):
        """A single variant should produce a valid VCF line."""
        writer = VCFWriter()
        v = _make_variant(pos=99, ref="A", alt="G", depth=30, alt_count=15)
        buf = io.StringIO()
        writer.write_variant(v, buf)
        line = buf.getvalue().strip()
        parts = line.split("\t")
        assert parts[0] == "chr1"
        assert parts[1] == "100"  # 1-based
        assert parts[3] == "A"
        assert parts[4] == "G"
        assert "DP=30" in parts[7]
        assert "AF=" in parts[7]

    def test_multiple_variants(self):
        """Writing multiple variants should produce correct line count."""
        variants = [_make_variant(pos=i) for i in range(10)]
        content = variants_to_vcf_string(variants)
        lines = [l for l in content.split("\n") if l and not l.startswith("##")]
        # Header line + 10 variant lines
        assert len(lines) == 11

    def test_filter_low_depth(self):
        """Low depth variant should get LowDepth filter."""
        writer = VCFWriter()
        v = _make_variant(depth=3, alt_count=2)
        filt = writer._filter_field(v)
        assert "LowDepth" in filt

    def test_filter_strand_bias(self):
        """Extreme strand balance should get StrandBias filter."""
        writer = VCFWriter()
        v = _make_variant()
        v.strand_balance = 0.05
        filt = writer._filter_field(v)
        assert "StrandBias" in filt

    def test_filter_pass(self):
        """Good quality variant should be PASS."""
        writer = VCFWriter()
        v = _make_variant(depth=30, quality=50.0)
        filt = writer._filter_field(v)
        assert filt == "PASS"

    def test_write_to_file(self, tmp_path):
        """Test writing VCF to an actual file."""
        filepath = tmp_path / "test.vcf"
        variants = [_make_variant(pos=i) for i in range(5)]
        count = write_vcf(variants, str(filepath))
        assert count == 5
        assert filepath.exists()
        content = filepath.read_text()
        assert "VCFv4.2" in content

    def test_info_field_contents(self):
        """INFO field should contain DP, AF, TSTV, AB, SB."""
        v = _make_variant(depth=30, alt_count=15)
        v.ts_tv = "tv"
        writer = VCFWriter()
        buf = io.StringIO()
        writer.write_variant(v, buf)
        line = buf.getvalue().strip()
        parts = line.split("\t")
        info = parts[7]
        assert "DP=30" in info
        assert "AF=" in info
        assert "TSTV=tv" in info
        assert "AB=" in info
        assert "SB=" in info

    def test_genotype_field(self):
        """FORMAT and sample columns should encode GT:GQ:DP:AD."""
        v = _make_variant(depth=30, alt_count=15)
        writer = VCFWriter()
        buf = io.StringIO()
        writer.write_variant(v, buf)
        line = buf.getvalue().strip()
        parts = line.split("\t")
        assert parts[8] == "GT:GQ:DP:AD"
        sample = parts[9]
        assert "0/1" in sample  # het genotype
        assert "50" in sample   # GQ
        assert "30" in sample   # DP

    def test_empty_variant_list(self):
        """Writing empty list should produce header only."""
        content = variants_to_vcf_string([])
        lines = content.strip().split("\n")
        # Just header lines
        assert all(l.startswith("##") or l.startswith("#") for l in lines if l)

    def test_write_variants_returns_string(self):
        """write_variants with no file arg returns VCF string."""
        writer = VCFWriter()
        content = writer.write_variants([_make_variant()])
        assert "VCFv4.2" in content
        assert "chr1" in content
