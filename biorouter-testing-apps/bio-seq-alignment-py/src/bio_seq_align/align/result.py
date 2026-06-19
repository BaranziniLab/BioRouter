"""Alignment result container."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional


@dataclass
class AlignmentResult:
    """Holds the output of any pairwise alignment algorithm."""

    aligned_seq1: str
    aligned_seq2: str
    score: float
    identity: float          # 0.0–1.0
    matches: int = 0
    mismatches: int = 0
    gaps: int = 0
    algorithm: str = ""
    start1: Optional[int] = None   # alignment start in seq1 (0-based)
    end1: Optional[int] = None     # alignment end in seq1 (exclusive)
    start2: Optional[int] = None
    end2: Optional[int] = None

    def __post_init__(self) -> None:
        """Recompute match/mismatch/gap counts from the aligned strings.

        Always recomputes so that counts stay consistent with the
        alignment even when a caller explicitly passes matches but
        leaves mismatches/gaps at their defaults.
        """
        self.matches = 0
        self.mismatches = 0
        self.gaps = 0
        for a, b in zip(self.aligned_seq1, self.aligned_seq2):
            if a == "-" or b == "-":
                self.gaps += 1
            elif a == b:
                self.matches += 1
            else:
                self.mismatches += 1
        # Recompute identity from the authoritative counts.
        length = len(self.aligned_seq1)
        self.identity = self.matches / length if length else 0.0

    # ── helpers ──────────────────────────────────────────────

    @property
    def length(self) -> int:
        return len(self.aligned_seq1)

    def alignment_lines(self, block: int = 60) -> list[str]:
        """Return pretty-printed alignment lines in blocks.

        Returns a list of strings, each block showing seq1, match line, seq2.
        """
        mid_chars: list[str] = []
        for a, b in zip(self.aligned_seq1, self.aligned_seq2):
            if a == b:
                mid_chars.append("|")
            elif a == "-" or b == "-":
                mid_chars.append(" ")
            else:
                mid_chars.append(".")
        mid = "".join(mid_chars)

        lines: list[str] = []
        for i in range(0, len(self.aligned_seq1), block):
            s1 = self.aligned_seq1[i : i + block]
            m  = mid[i : i + block]
            s2 = self.aligned_seq2[i : i + block]
            pos1 = i
            lines.append(f"Seq1  {pos1:>5}  {s1}  {min(pos1 + len(s1.replace('-','')), len(self.aligned_seq1.replace('-','')))}")
            lines.append(f"             {m}")
            lines.append(f"Seq2  {pos1:>5}  {s2}  {min(pos1 + len(s2.replace('-','')), len(self.aligned_seq2.replace('-','')))}")
            lines.append("")
        return lines

    def summary(self) -> str:
        return (
            f"Algorithm : {self.algorithm}\n"
            f"Score     : {self.score}\n"
            f"Length    : {self.length}\n"
            f"Identity  : {self.identity*100:.1f}% ({self.matches}/{self.length})\n"
            f"Matches   : {self.matches}\n"
            f"Mismatches: {self.mismatches}\n"
            f"Gaps      : {self.gaps}"
        )
