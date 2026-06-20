"""
bio_assembly - A mini de-novo genome assembler in pure Python.

Provides two assembly strategies:
  1. Overlap-Layout-Consensus (OLC) - suitable for long reads
  2. De Bruijn Graph - suitable for short reads

Both produce contig assemblies from FASTA/FASTQ input.
"""

__version__ = "0.1.0"
