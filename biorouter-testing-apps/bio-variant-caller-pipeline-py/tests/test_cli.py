"""Tests for the CLI module."""

from __future__ import annotations

import json
import os

import pytest

from bio_variant_caller.cli import (
    build_parser,
    load_reads_tsv,
    load_reference,
    main,
    save_reads_tsv,
    save_truth_tsv,
)
from bio_variant_caller.models import AlignedRead, Strand
from bio_variant_caller.simulate import ReadSimulator, SimConfig, TruthVariant


class TestCLIParsing:
    def test_parser_has_run(self):
        parser = build_parser()
        args = parser.parse_args(["run", "-r", "ref.fa", "-R", "reads.tsv"])
        assert args.command == "run"

    def test_parser_has_simulate(self):
        parser = build_parser()
        args = parser.parse_args(["simulate", "-r", "ref.fa"])
        assert args.command == "simulate"

    def test_parser_has_eval(self):
        parser = build_parser()
        args = parser.parse_args(["eval", "-v", "out.vcf", "-t", "truth.tsv"])
        assert args.command == "eval"

    def test_parser_defaults(self):
        parser = build_parser()
        args = parser.parse_args(["run", "-r", "ref.fa", "-R", "reads.tsv"])
        assert args.output == "output.vcf"
        assert args.min_depth == 8
        assert args.min_af == 0.2


class TestReferenceLoading:
    def test_load_plain_text(self, tmp_path):
        ref_file = tmp_path / "ref.txt"
        ref_file.write_text("ACGTACGT\nACGTACGT\n")
        result = load_reference(str(ref_file))
        assert result == "ACGTACGTACGTACGT"

    def test_load_fasta(self, tmp_path):
        ref_file = tmp_path / "ref.fa"
        ref_file.write_text(">chr1\nACGT\n>chr2\nTGCA\n")
        result = load_reference(str(ref_file))
        assert result == "ACGTTGCA"

    def test_load_lowercase(self, tmp_path):
        ref_file = tmp_path / "ref.fa"
        ref_file.write_text("acgtacgt")
        result = load_reference(str(ref_file))
        assert result == "ACGTACGT"


class TestReadsIO:
    def test_save_and_load_tsv(self, tmp_path):
        reads = [
            AlignedRead("r1", 10, "50M", "A" * 50, [30] * 50, Strand.FORWARD, 60),
            AlignedRead("r2", 20, "50M", "C" * 50, [25] * 50, Strand.REVERSE, 40),
        ]
        filepath = tmp_path / "reads.tsv"
        save_reads_tsv(reads, str(filepath))
        loaded = load_reads_tsv(str(filepath))
        assert len(loaded) == 2
        assert loaded[0].name == "r1"
        assert loaded[0].ref_start == 10
        assert loaded[0].strand == Strand.FORWARD
        assert loaded[1].strand == Strand.REVERSE
        assert loaded[1].map_quality == 40

    def test_load_with_defaults(self, tmp_path):
        """Reads file with minimal columns should load with defaults."""
        filepath = tmp_path / "minimal.tsv"
        filepath.write_text("r1\t0\t50M\tAAAAA\t30,30,30,30,30\n")
        loaded = load_reads_tsv(str(filepath))
        assert len(loaded) == 1
        assert loaded[0].strand == Strand.FORWARD
        assert loaded[0].map_quality == 60


class TestTruthIO:
    def test_save_and_load_truth(self, tmp_path):
        truth = [
            TruthVariant(pos=10, ref="A", alt="G"),
            TruthVariant(pos=30, ref="C", alt="T"),
        ]
        filepath = tmp_path / "truth.tsv"
        save_truth_tsv(truth, str(filepath))
        content = filepath.read_text()
        assert "#chrom" in content
        assert "10" in content
        assert "A" in content
        assert "G" in content


class TestCLIIntegration:
    def test_simulate_and_run(self, tmp_path):
        """End-to-end: simulate → run → VCF output."""
        # Create reference
        ref_file = tmp_path / "ref.fa"
        ref_file.write_text("ACGT" * 50)  # 200 bp

        # Simulate reads
        reads_file = tmp_path / "reads.tsv"
        truth_file = tmp_path / "truth.tsv"
        ret = main([
            "simulate", "-r", str(ref_file),
            "-o", str(reads_file),
            "-t", str(truth_file),
            "-c", "20",
            "--variants", "0:A:G", "4:T:A",
            "--seed", "42",
        ])
        assert ret == 0
        assert reads_file.exists()
        assert truth_file.exists()

        # Run pipeline
        vcf_file = tmp_path / "output.vcf"
        stats_file = tmp_path / "stats.json"
        ret = main([
            "run", "-r", str(ref_file),
            "-R", str(reads_file),
            "-o", str(vcf_file),
            "--stats", str(stats_file),
        ])
        assert ret == 0
        assert vcf_file.exists()
        assert stats_file.exists()

        # Check VCF content
        vcf_content = vcf_file.read_text()
        assert "VCFv4.2" in vcf_content

        # Check stats
        stats = json.loads(stats_file.read_text())
        assert stats["reference_length"] == 200
        assert stats["num_reads"] > 0
        assert stats["variants_called"] >= 0

    def test_simulate_only(self, tmp_path):
        """Test simulate sub-command standalone."""
        ref_file = tmp_path / "ref.fa"
        ref_file.write_text("ACGT" * 25)  # 100 bp

        reads_file = tmp_path / "reads.tsv"
        truth_file = tmp_path / "truth.tsv"
        ret = main([
            "simulate", "-r", str(ref_file),
            "-o", str(reads_file),
            "-t", str(truth_file),
            "-c", "10",
        ])
        assert ret == 0

    def test_no_command_shows_help(self, capsys):
        """No sub-command should show help and return 1."""
        ret = main([])
        assert ret == 1

    def test_eval_sub_command(self, tmp_path):
        """Test eval sub-command."""
        ref_file = tmp_path / "ref.fa"
        ref_file.write_text("ACGT" * 50)

        # Simulate
        reads_file = tmp_path / "reads.tsv"
        truth_file = tmp_path / "truth.tsv"
        main([
            "simulate", "-r", str(ref_file),
            "-o", str(reads_file),
            "-t", str(truth_file),
            "--variants", "100:A:G",
        ])

        # Run
        vcf_file = tmp_path / "output.vcf"
        main([
            "run", "-r", str(ref_file),
            "-R", str(reads_file),
            "-o", str(vcf_file),
        ])

        # Eval
        ret = main(["eval", "-v", str(vcf_file), "-t", str(truth_file)])
        assert ret == 0  # should find the truth variant
