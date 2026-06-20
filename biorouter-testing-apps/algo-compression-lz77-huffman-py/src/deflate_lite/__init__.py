"""
deflate-lite — LZ77 + Huffman compression toolkit.

Modules
-------
bitio    : Bitstream I/O (BitWriter / BitReader)
lz77     : LZ77 sliding-window encoder / decoder
huffman  : Canonical Huffman coding
codec    : Combined LZ77 → Huffman pipeline with file container
analyze  : Entropy and compression-ratio analysis
cli      : Command-line interface
"""

from deflate_lite.codec import compress, decompress, compress_file, decompress_file

__all__ = ["compress", "decompress", "compress_file", "decompress_file"]
__version__ = "0.1.0"
