# deflate-lite

A pure-Python compression toolkit implementing **LZ77** sliding-window
compression combined with **canonical Huffman** coding — a simplified
version of the DEFLATE algorithm used in gzip/zlib/PNG.

## Features

| Module | Purpose |
|---|---|
| `bitio.py` | Bitstream reader/writer (LSB-first byte packing) |
| `lz77.py` | LZ77 encoder/decoder with configurable window & lookahead |
| `huffman.py` | Canonical Huffman tree builder, encoder/decoder |
| `codec.py` | Combined LZ77 → Huffman pipeline with self-describing file container |
| `analyze.py` | Shannon entropy, compression ratio, bits-per-byte analysis |
| `cli.py` | Command-line interface (compress / decompress / analyze / info) |

## Quick start

```bash
# Create a virtualenv and install in dev mode
python3 -m venv .venv && source .venv/bin/activate
pip install -e ".[dev]"   # or: pip install pytest && pip install -e .

# Compress a file
deflate-lite compress input.txt output.dlz

# Decompress
deflate-lite decompress output.dlz restored.txt

# Analyse entropy & compression ratio
deflate-lite info input.txt
deflate-lite analyze input.txt output.dlz
```

## Run tests

```bash
python -m pytest tests/ -v
```

## File container format (DLZ2)

Every compressed blob is self-describing:

```
Offset  Size    Field
──────  ──────  ──────────────────────────────────────────
0       4       Magic bytes: b'DLZ2'
4       1       Flags (currently 0, reserved)
5       8       Original size (uint64, little-endian)
13      8       LZ77 serialised stream length (uint64, LE)
21      128     Canonical Huffman code-length table
                (256 entries × 4 bits each, LSB-first packing)
149     …       Huffman-coded payload (byte-aligned)
```

### Code-length table

Each of the 256 byte values has a 4-bit code-length (0–15).
Length 0 means the byte does not appear in the data.
Canonical Huffman codes are derived deterministically from these
lengths (ascending length, then ascending symbol).

### LZ77 token stream (inside the Huffman payload)

The Huffman payload decodes to a byte stream of serialised LZ77
tokens.  Each token is one of:

| Tag | Bytes | Meaning |
|-----|-------|---------|
| `0x00` | +1 | Literal: the following byte |
| `0x01` | +5 | Match: offset (2 bytes BE) + length (2 bytes BE) + following literal byte |
| `0x02` | +4 | Final match (reaches end of input): offset (2 bytes BE) + length (2 bytes BE), no trailing literal |

Default LZ77 parameters: window = 4096 bytes, lookahead = 258 bytes,
minimum match length = 3.

### V1 container (DLZ1, legacy)

Same layout but without the LZ stream length field (13 bytes shorter).
Decompression uses a best-effort estimation; DLZ2 is preferred.

## Programmatic usage

```python
from deflate_lite import compress, decompress

original = b"hello world " * 1000
compressed = compress(original, window_size=4096)
restored = decompress(compressed)
assert restored == original
```

## Architecture

```
compress(data)
    │
    ▼
 LZ77 encode ──► token stream ──► serialise to bytes
                                        │
                                        ▼
                              Huffman encode bytes
                                        │
                                        ▼
                              Wrap in DLZ2 container
                                        │
                                        ▼
                                  compressed blob

decompress(blob)
    │
    ▼
 Parse DLZ2 header (magic, sizes, code-length table)
    │
    ▼
 Huffman decode payload ──► LZ77 byte stream
    │
    ▼
 LZ77 decode ──► original data
```

## License

MIT
