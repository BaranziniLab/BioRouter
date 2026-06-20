"""
LZ77 sliding-window compression.

Encoder emits a stream of tokens:
    (offset, length, next_byte)
where offset/length encode a back-reference into the already-seen
window and next_byte is the literal that follows the match.

Special case: when a match reaches the exact end of input and there is
no following literal, next_byte is None.

The decoder replays those tokens to reconstruct the original data.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import List, Optional, Tuple


# -------------------------------------------------------------------
# Token representation
# -------------------------------------------------------------------

@dataclass(frozen=True, slots=True)
class Token:
    """One LZ77 token: back-reference + optional literal."""
    offset: int              # distance into window (0 = literal-only)
    length: int              # match length (0 = no match)
    byte: Optional[int]      # literal byte after match; None = end-of-input


# -------------------------------------------------------------------
# Encoder
# -------------------------------------------------------------------

_SENTINEL_TAG = 0x02  # used in serialisation for "match, no literal"


def _find_longest_match(
    data: bytes,
    pos: int,
    window_size: int,
    lookahead_size: int,
) -> Tuple[int, int]:
    """
    Find the longest match of data[pos:pos+lookahead_size] within
    data[max(0, pos-window_size):pos].

    Returns (offset, length).  offset is the distance *back* from pos.
    If no match is found returns (0, 0).
    """
    best_offset = 0
    best_length = 0

    if pos == 0:
        return (0, 0)

    search_start = max(0, pos - window_size)
    limit = min(pos + lookahead_size, len(data))

    for start in range(search_start, pos):
        length = 0
        while pos + length < limit and data[start + length] == data[pos + length]:
            length += 1
            if start + length >= pos:
                # Overlapping match: the source pointer has reached
                # the current write position.  In LZ77 this is legal
                # (it effectively repeats the first part of the match)
                # but we must stop when length reaches the distance
                # because further bytes would be undefined.
                if length >= pos - start:
                    # Can extend by repeating from the match start
                    # but only up to lookahead_size
                    break

        if length >= 3 and length > best_length:
            best_length = length
            best_offset = pos - start
            if best_length >= lookahead_size:
                break

    return (best_offset, best_length)


def encode(
    data: bytes,
    window_size: int = 4096,
    lookahead_size: int = 258,
    min_match: int = 3,
) -> List[Token]:
    """
    Compress *data* with LZ77 and return a list of Tokens.
    """
    tokens: List[Token] = []
    pos = 0
    n = len(data)

    while pos < n:
        offset, length = _find_longest_match(data, pos, window_size, lookahead_size)

        if length >= min_match:
            next_pos = pos + length
            if next_pos < n:
                # Match followed by a literal
                tokens.append(Token(offset, length, data[next_pos]))
                pos = next_pos + 1
            else:
                # Match reaches end of input — no trailing literal
                tokens.append(Token(offset, length, None))
                pos = next_pos
        else:
            # No match — emit literal
            tokens.append(Token(0, 0, data[pos]))
            pos += 1

    return tokens


# -------------------------------------------------------------------
# Decoder
# -------------------------------------------------------------------

def decode(tokens: List[Token]) -> bytes:
    """Reconstruct original bytes from a list of LZ77 Tokens."""
    buf = bytearray()
    for tok in tokens:
        if tok.offset == 0 and tok.length == 0:
            # Pure literal
            buf.append(tok.byte)
        else:
            # Back-reference
            start = len(buf) - tok.offset
            for i in range(tok.length):
                buf.append(buf[start + i])
            if tok.byte is not None:
                buf.append(tok.byte)
    return bytes(buf)


# -------------------------------------------------------------------
# Serialise / deserialise token stream to bytes
# -------------------------------------------------------------------

def encode_to_bytes(data: bytes, window_size: int = 4096, lookahead_size: int = 258) -> bytes:
    """
    Encode *data* and serialise the token stream into a compact byte
    format for storage or piping into the Huffman stage.

    Format (per token):
        0x00 <byte>                          — literal
        0x01 <offset:2be> <length:2be> <byte> — match + literal
        0x02 <offset:2be> <length:2be>        — match, no literal (end-of-input)
    """
    tokens = encode(data, window_size, lookahead_size)
    out = bytearray()
    for tok in tokens:
        if tok.offset == 0 and tok.length == 0:
            out.append(0x00)
            out.append(tok.byte)
        elif tok.byte is not None:
            out.append(0x01)
            out.extend(tok.offset.to_bytes(2, "big"))
            out.extend(tok.length.to_bytes(2, "big"))
            out.append(tok.byte)
        else:
            out.append(0x02)
            out.extend(tok.offset.to_bytes(2, "big"))
            out.extend(tok.length.to_bytes(2, "big"))
    return bytes(out)


def decode_from_bytes(data: bytes) -> bytes:
    """Inverse of `encode_to_bytes`."""
    tokens: List[Token] = []
    i = 0
    while i < len(data):
        tag = data[i]
        i += 1
        if tag == 0x00:
            tokens.append(Token(0, 0, data[i]))
            i += 1
        elif tag == 0x01:
            offset = int.from_bytes(data[i : i + 2], "big")
            i += 2
            length = int.from_bytes(data[i : i + 2], "big")
            i += 2
            byte = data[i]
            i += 1
            tokens.append(Token(offset, length, byte))
        elif tag == 0x02:
            offset = int.from_bytes(data[i : i + 2], "big")
            i += 2
            length = int.from_bytes(data[i : i + 2], "big")
            i += 2
            tokens.append(Token(offset, length, None))
        else:
            raise ValueError(f"Unknown token tag 0x{tag:02x} at position {i - 1}")
    return decode(tokens)
