"""Reference-aware pileup engine.

Given a reference sequence and a collection of aligned reads, the pileup
engine computes per-position base counts, strand information, and quality
scores that downstream callers and annotators consume.
"""

from __future__ import annotations

import re
from typing import Dict, List, Optional, Tuple

from .models import AlignedRead, PileupBase, PileupPosition, Strand


# ---------------------------------------------------------------------------
# CIGAR parsing
# ---------------------------------------------------------------------------

_CIGAR_RE = re.compile(r"(\d+)([MIDNSHP=X])")


def parse_cigar(cigar: str) -> List[Tuple[int, str]]:
    """Parse a CIGAR string into (length, operation) tuples."""
    return [(int(m.group(1)), m.group(2)) for m in _CIGAR_RE.finditer(cigar)]


def cigar_consumed_bases(cigar_ops: List[Tuple[int, str]]) -> Tuple[int, int]:
    """Return (query_bases_consumed, ref_bases_consumed) for a CIGAR.

    Consumed operations:
      M/=/X  – query and ref
      I/S    – query only
      D/N    – ref only
      H/P    – neither
    """
    q_consumed = 0
    r_consumed = 0
    for length, op in cigar_ops:
        if op in ("M", "=", "X"):
            q_consumed += length
            r_consumed += length
        elif op in ("I", "S"):
            q_consumed += length
        elif op in ("D", "N"):
            r_consumed += length
    return q_consumed, r_consumed


# ---------------------------------------------------------------------------
# Pileup engine
# ---------------------------------------------------------------------------

class PileupEngine:
    """Build a pileup from a reference and a set of aligned reads.

    Parameters
    ----------
    reference : str
        The reference sequence (upper-case, no whitespace).
    reads : list[AlignedRead]
        Aligned reads with position, CIGAR, sequence, and base qualities.
    min_mapq : int
        Minimum mapping quality for a read to be included (default 0).
    """

    def __init__(
        self,
        reference: str,
        reads: List[AlignedRead],
        min_mapq: int = 0,
    ) -> None:
        self.reference = reference.upper()
        self.ref_length = len(reference)
        self.reads = reads
        self.min_mapq = min_mapq
        self._pileup: Optional[Dict[int, PileupPosition]] = None

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def build(self) -> Dict[int, PileupPosition]:
        """Build and cache the pileup. Returns {ref_pos: PileupPosition}."""
        if self._pileup is not None:
            return self._pileup

        pileup: Dict[int, PileupPosition] = {}

        for read in self.reads:
            if read.map_quality < self.min_mapq:
                continue
            self._pileup_read(read, pileup)

        self._pileup = pileup
        return pileup

    def get_position(self, ref_pos: int) -> Optional[PileupPosition]:
        """Get pileup at a single reference position."""
        pileup = self.build()
        return pileup.get(ref_pos)

    def get_positions(
        self, start: int = 0, end: Optional[int] = None
    ) -> List[PileupPosition]:
        """Return pileup positions in a range, sorted by position."""
        pileup = self.build()
        if end is None:
            end = self.ref_length
        return [
            pileup[pos]
            for pos in sorted(pileup.keys())
            if start <= pos < end
        ]

    def covered_positions(self) -> List[int]:
        """Return sorted list of positions with any coverage."""
        return sorted(self.build().keys())

    def depth_at(self, ref_pos: int) -> int:
        """Return depth at a given position (0 if no coverage)."""
        pp = self.get_position(ref_pos)
        return pp.depth if pp else 0

    # ------------------------------------------------------------------
    # Internal
    # ------------------------------------------------------------------

    def _ensure_position(
        self, ref_pos: int, pileup: Dict[int, PileupPosition]
    ) -> PileupPosition:
        if ref_pos not in pileup:
            ref_base = self.reference[ref_pos] if ref_pos < self.ref_length else "N"
            pileup[ref_pos] = PileupPosition(ref_pos=ref_pos, ref_base=ref_base)
        return pileup[ref_pos]

    def _pileup_read(
        self, read: AlignedRead, pileup: Dict[int, PileupPosition]
    ) -> None:
        """Walk the CIGAR and deposit bases into the pileup."""
        cigar_ops = parse_cigar(read.cigar)
        query_idx = 0      # index into read.sequence / base_qualities
        ref_pos = read.ref_start

        for length, op in cigar_ops:
            if op in ("M", "=", "X"):
                # Aligning operations: both query and ref advance
                for i in range(length):
                    if query_idx >= len(read.sequence):
                        break
                    if ref_pos < 0 or ref_pos >= self.ref_length:
                        query_idx += 1
                        ref_pos += 1
                        continue
                    pp = self._ensure_position(ref_pos, pileup)
                    bq = (
                        read.base_qualities[query_idx]
                        if query_idx < len(read.base_qualities)
                        else 0
                    )
                    pp.bases.append(
                        PileupBase(
                            base=read.sequence[query_idx],
                            base_quality=bq,
                            strand=read.strand,
                            read_name=read.name,
                        )
                    )
                    query_idx += 1
                    ref_pos += 1

            elif op == "I":
                # Insertion: query bases not aligned to reference
                # Mark the preceding reference position's last base as having
                # an insertion after it
                insert_ref_pos = ref_pos - 1
                if 0 <= insert_ref_pos < self.ref_length:
                    pp = self._ensure_position(insert_ref_pos, pileup)
                    for i in range(length):
                        if query_idx >= len(read.sequence):
                            break
                        bq = (
                            read.base_qualities[query_idx]
                            if query_idx < len(read.base_qualities)
                            else 0
                        )
                        pp.bases.append(
                            PileupBase(
                                base=read.sequence[query_idx],
                                base_quality=bq,
                                strand=read.strand,
                                read_name=read.name,
                                is_insertion=(i == 0),  # only first base marked
                            )
                        )
                        query_idx += 1

            elif op == "D":
                # Deletion: reference bases not covered by query
                for i in range(length):
                    if ref_pos < 0 or ref_pos >= self.ref_length:
                        ref_pos += 1
                        continue
                    pp = self._ensure_position(ref_pos, pileup)
                    pp.bases.append(
                        PileupBase(
                            base=read.sequence[query_idx - 1]
                            if query_idx > 0
                            else "N",
                            base_quality=0,
                            strand=read.strand,
                            read_name=read.name,
                            is_deletion=True,
                        )
                    )
                    ref_pos += 1

            elif op in ("S", "H"):
                # Soft/hard clip: skip query bases
                if op == "S":
                    query_idx += length

            elif op == "N":
                # Skipped region (intron): skip ref bases
                ref_pos += length

            elif op == "P":
                # Padding: skip both (shouldn't normally appear)
                pass


def quick_pileup(
    reference: str,
    reads: List[AlignedRead],
    min_mapq: int = 0,
) -> Dict[int, PileupPosition]:
    """Convenience function: build a pileup in one call."""
    engine = PileupEngine(reference, reads, min_mapq=min_mapq)
    return engine.build()
