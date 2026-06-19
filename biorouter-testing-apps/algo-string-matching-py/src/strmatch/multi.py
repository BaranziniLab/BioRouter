"""Aho-Corasick multi-pattern matching automaton.

Builds a trie of all patterns, then computes failure links (à la KMP)
and output/dictionary links so that every text position is checked against
all patterns in a single left-to-right scan.

Preprocessing: O(Σ|pᵢ|)  — total pattern length.
Search:        O(n + z)   — text length + number of matches.
Space:         O(Σ|pᵢ|·|Σ|) in the worst case for the transition table,
               but typically O(Σ|pᵢ|) with failure-link fallback.
"""

from __future__ import annotations

from collections import deque


class _Node:
    """Trie node."""
    __slots__ = ("children", "fail", "output", "pat_idx")

    def __init__(self) -> None:
        self.children: dict[str, _Node] = {}
        self.fail: _Node | None = None   # failure link
        self.output: int = -1            # index of pattern ending here (-1 = none)
        self.pat_idx: int = -1           # alias kept for clarity


class AhoCorasick:
    """Aho-Corasick automaton for multi-pattern matching.

    >>> ac = AhoCorasick(["he", "she", "his", "hers"])
    >>> ac.search("ahishers")
    [(1, 'his'), (3, 'she'), (4, 'he'), (5, 'hers')]
    """

    def __init__(self, patterns: list[str]) -> None:
        self.patterns = list(patterns)
        self.root = _Node()
        self._build_trie()
        self._build_failure_links()

    # ---- construction --------------------------------------------------

    def _build_trie(self) -> None:
        for idx, pat in enumerate(self.patterns):
            if not pat:
                continue
            node = self.root
            for ch in pat:
                node = node.children.setdefault(ch, _Node())
            node.output = idx
            node.pat_idx = idx

    def _build_failure_links(self) -> None:
        queue: deque[_Node] = deque()
        # Depth-1 nodes fail to root.
        for child in self.root.children.values():
            child.fail = self.root
            queue.append(child)

        while queue:
            current = queue.popleft()
            for ch, child in current.children.items():
                queue.append(child)
                fail_node = current.fail
                while fail_node is not None and ch not in fail_node.children:
                    fail_node = fail_node.fail
                child.fail = fail_node.children[ch] if fail_node and ch in fail_node.children else self.root
                if child.fail is child:
                    child.fail = self.root  # avoid self-loop
                # Propagate output: if failure node is terminal, inherit.
                if child.fail.output >= 0 and child.output < 0:
                    child.output = child.fail.output

    # ---- search --------------------------------------------------------

    def search(self, text: str) -> list[tuple[int, str]]:
        """Return (start_index, matched_pattern) pairs for all matches in *text*.

        Results are ordered by start position.
        """
        results: list[tuple[int, str]] = []
        node = self.root
        for i, ch in enumerate(text):
            while node is not self.root and ch not in node.children:
                node = node.fail if node.fail else self.root
            node = node.children.get(ch, self.root) if ch in node.children else self.root
            # Follow output links (handles patterns that are suffixes of others).
            temp: _Node | None = node
            while temp is not None:
                if temp.output >= 0:
                    pat = self.patterns[temp.output]
                    results.append((i - len(pat) + 1, pat))
                temp = temp.fail if temp is not self.root else None
                if temp is self.root:
                    break
        results.sort()
        return results


def aho_corasick_search(text: str, patterns: list[str]) -> list[tuple[int, str]]:
    """Convenience wrapper: build and search in one call.

    >>> aho_corasick_search("ahishers", ["he", "she", "his", "hers"])
    [(1, 'his'), (3, 'she'), (4, 'he'), (5, 'hers')]
    """
    ac = AhoCorasick(patterns)
    return ac.search(text)
