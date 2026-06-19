"""
Tests for cli.py — Command-line interface.
"""

import os
import tempfile
import pytest
from bio_phylo.cli import _cmd_build, _cmd_distance, _cmd_info, _build_tree
from bio_phylo.distance import compute_distance_matrix, DistanceMatrix
from bio_phylo.tree import Node, from_newick


# ======================================================================
# Sample data
# ======================================================================

SAMPLE_FASTA = """>Human
ATGCGTACGT
Chimp
ATGCGTACCT
Gorilla
ATGCGTACTT
Mouse
ATGCGTACAT
"""


# ======================================================================
# Helper to create temp FASTA files
# ======================================================================


@pytest.fixture
def fasta_file(tmp_path):
    """Create a temporary FASTA file."""
    path = tmp_path / "alignment.fasta"
    path.write_text(SAMPLE_FASTA)
    return str(path)


@pytest.fixture
def simple_fasta(tmp_path):
    """Create a simpler FASTA file for testing."""
    content = """>A
ACGT
>B
TGCA
>C
AACC
"""
    path = tmp_path / "simple.fasta"
    path.write_text(content)
    return str(path)


# ======================================================================
# _build_tree function tests
# ======================================================================


class TestBuildTree:
    def test_upgma(self):
        """Build UPGMA tree from alignment."""
        seqs = {"A": "ACGT", "B": "TGCA", "C": "AACC"}
        tree = _build_tree("upgma", alignment=seqs)
        assert tree.num_leaves == 3

    def test_nj(self):
        """Build NJ tree from alignment."""
        seqs = {"A": "ACGT", "B": "TGCA", "C": "AACC"}
        tree = _build_tree("nj", alignment=seqs)
        assert tree.num_leaves == 3

    def test_parsimony(self):
        """Build parsimony tree from alignment."""
        seqs = {"A": "ACGT", "B": "TGCA", "C": "AACC"}
        tree = _build_tree("parsimony", alignment=seqs)
        assert tree.num_leaves == 3

    def test_from_distance_matrix(self):
        """Build tree from distance matrix."""
        dm = DistanceMatrix.from_square(
            ["A", "B", "C"],
            [[0, 1, 2], [1, 0, 2], [2, 2, 0]],
        )
        tree = _build_tree("nj", dm=dm)
        assert tree.num_leaves == 3

    def test_unknown_method_raises(self):
        with pytest.raises(ValueError, match="Unknown method"):
            _build_tree("unknown_method")

    def test_parsimony_needs_alignment(self):
        with pytest.raises(ValueError, match="Need alignment"):
            _build_tree("parsimony")


# ======================================================================
# CLI commands
# ======================================================================


class TestCmdBuild:
    def test_build_nj(self, simple_fasta):
        """Build NJ tree from a file."""
        ret = _cmd_build(["--input", simple_fasta, "--method", "nj"])
        assert ret == 0

    def test_build_upgma(self, simple_fasta):
        """Build UPGMA tree from a file."""
        ret = _cmd_build(["--input", simple_fasta, "--method", "upgma"])
        assert ret == 0

    def test_build_parsimony(self, simple_fasta):
        """Build parsimony tree from a file."""
        ret = _cmd_build(["--input", simple_fasta, "--method", "parsimony"])
        assert ret == 0

    def test_build_with_output(self, simple_fasta, tmp_path):
        """Build and write Newick to file."""
        out = str(tmp_path / "tree.nwk")
        ret = _cmd_build(["--input", simple_fasta, "--output", out])
        assert ret == 0
        assert os.path.exists(out)
        content = open(out).read().strip()
        assert content.endswith(";")

    def test_build_no_input(self):
        """Error when no input provided."""
        ret = _cmd_build([])
        assert ret == 1

    def test_build_with_model(self, simple_fasta):
        """Build with different models."""
        for model in ["p-distance", "jukes-cantor", "kimura-2param"]:
            ret = _cmd_build(["--input", simple_fasta, "--model", model])
            assert ret == 0


class TestCmdDistance:
    def test_distance(self, simple_fasta):
        ret = _cmd_distance(["--input", simple_fasta])
        assert ret == 0

    def test_distance_no_input(self):
        ret = _cmd_distance([])
        assert ret == 1


class TestCmdInfo:
    def test_info(self):
        ret = _cmd_info(["((A:0.1,B:0.2):0.3,C:0.4);"])
        assert ret == 0

    def test_info_no_input(self):
        ret = _cmd_info([])
        assert ret == 1
