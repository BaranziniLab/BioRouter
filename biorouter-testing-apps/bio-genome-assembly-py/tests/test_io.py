"""
Tests for the I/O module.
"""

import os
import tempfile

import pytest

from bio_assembly.io import (
    SequenceRecord,
    read_fasta,
    read_fastq,
    read_sequences,
    write_fasta,
    write_fastq,
)


class TestSequenceRecord:
    """Tests for SequenceRecord dataclass."""
    
    def test_basic_creation(self):
        """Test creating a SequenceRecord."""
        record = SequenceRecord(
            id="test_read",
            description="test sequence",
            sequence="ACGTACGT",
        )
        assert record.id == "test_read"
        assert record.description == "test sequence"
        assert record.sequence == "ACGTACGT"
        assert len(record) == 8
    
    def test_reverse_complement(self):
        """Test reverse complement calculation."""
        record = SequenceRecord(
            id="test",
            description="",
            sequence="ACGT",
        )
        rc = record.reverse_complement()
        assert rc.sequence == "ACGT"  # Reverse complement of ACGT is ACGT
        
        record2 = SequenceRecord(
            id="test2",
            description="",
            sequence="ATCG",
        )
        rc2 = record2.reverse_complement()
        assert rc2.sequence == "CGAT"
    
    def test_repr(self):
        """Test string representation."""
        record = SequenceRecord(
            id="read1",
            description="test",
            sequence="ACGT",
        )
        assert "read1" in repr(record)
        assert "len=4" in repr(record)


class TestFastaIO:
    """Tests for FASTA file I/O."""
    
    def test_write_and_read_fasta(self):
        """Test writing and reading FASTA files."""
        records = [
            SequenceRecord("seq1", "first sequence", "ACGTACGT"),
            SequenceRecord("seq2", "second sequence", "TTTTCCCC"),
            SequenceRecord("seq3", "third sequence", "GGGGAAAA"),
        ]
        
        with tempfile.NamedTemporaryFile(mode='w', suffix='.fasta', delete=False) as f:
            tmpfile = f.name
        
        try:
            write_fasta(records, tmpfile)
            read_records = list(read_fasta(tmpfile))
            
            assert len(read_records) == 3
            assert read_records[0].id == "seq1"
            assert read_records[0].sequence == "ACGTACGT"
            assert read_records[1].id == "seq2"
            assert read_records[2].id == "seq3"
        finally:
            os.unlink(tmpfile)
    
    def test_fasta_with_long_sequence(self):
        """Test FASTA with sequences longer than line width."""
        seq = "A" * 200
        record = SequenceRecord("long_seq", "long sequence", seq)
        
        with tempfile.NamedTemporaryFile(mode='w', suffix='.fasta', delete=False) as f:
            tmpfile = f.name
        
        try:
            write_fasta([record], tmpfile, line_width=80)
            read_records = list(read_fasta(tmpfile))
            
            assert len(read_records) == 1
            assert len(read_records[0].sequence) == 200
        finally:
            os.unlink(tmpfile)
    
    def test_read_fasta_file(self):
        """Test reading a FASTA file."""
        with tempfile.NamedTemporaryFile(mode='w', suffix='.fasta', delete=False) as f:
            f.write(">seq1\n")
            f.write("ACGT\n")
            f.write(">seq2\n")
            f.write("TTTT\n")
            f.write("CCCC\n")
            tmpfile = f.name
        
        try:
            records = list(read_fasta(tmpfile))
            assert len(records) == 2
            assert records[0].sequence == "ACGT"
            assert records[1].sequence == "TTTTCCCC"
        finally:
            os.unlink(tmpfile)


class TestFastqIO:
    """Tests for FASTQ file I/O."""
    
    def test_write_and_read_fastq(self):
        """Test writing and reading FASTQ files."""
        records = [
            SequenceRecord("read1", "first read", "ACGTACGT", "IIIIIIII"),
            SequenceRecord("read2", "second read", "TTTTCCCC", "88888888"),
        ]
        
        with tempfile.NamedTemporaryFile(mode='w', suffix='.fastq', delete=False) as f:
            tmpfile = f.name
        
        try:
            write_fastq(records, tmpfile)
            read_records = list(read_fastq(tmpfile))
            
            assert len(read_records) == 2
            assert read_records[0].id == "read1"
            assert read_records[0].sequence == "ACGTACGT"
            assert read_records[0].quality == "IIIIIIII"
            assert read_records[1].id == "read2"
        finally:
            os.unlink(tmpfile)


class TestAutoDetect:
    """Tests for auto-detection of file format."""
    
    def test_auto_detect_fasta(self):
        """Test auto-detection of FASTA format."""
        with tempfile.NamedTemporaryFile(mode='w', suffix='.seq', delete=False) as f:
            f.write(">seq1\nACGT\n")
            tmpfile = f.name
        
        try:
            records = read_sequences(tmpfile)
            assert len(records) == 1
            assert records[0].id == "seq1"
        finally:
            os.unlink(tmpfile)
    
    def test_auto_detect_fastq(self):
        """Test auto-detection of FASTQ format."""
        with tempfile.NamedTemporaryFile(mode='w', suffix='.seq', delete=False) as f:
            f.write("@read1\nACGT\n+\nIIII\n")
            tmpfile = f.name
        
        try:
            records = read_sequences(tmpfile)
            assert len(records) == 1
            assert records[0].id == "read1"
        finally:
            os.unlink(tmpfile)
