"""
ASCII tree renderer.

Provides pretty-printing of phylogenetic trees in the terminal with
branch length annotations and support values.
"""

from __future__ import annotations

from typing import Optional

from bio_phylo.tree import Node


def ascii_tree(
    tree: Node,
    show_branch_lengths: bool = True,
    show_support: bool = False,
    precision: int = 3,
    char_width: float = 1.0,
    branch_char: str = "─",
    corner_char: str = "╮",
    tee_char: str = "├",
    corner_bottom_char: str = "╯",
    vertical_char: str = "│",
) -> str:
    """Render a phylogenetic tree as an ASCII string.

    Parameters
    ----------
    tree : Node
        Root of the tree.
    show_branch_lengths : bool
        If True, annotate branches with their lengths.
    show_support : bool
        If True, show node names as support values at internal nodes.
    precision : int
        Decimal places for branch lengths.
    char_width : float
        Number of character positions per unit branch length.
    branch_char, corner_char, tee_char, corner_bottom_char, vertical_char : str
        Characters used for drawing.

    Returns
    -------
    str
        Multi-line string with the tree drawing.
    """
    renderer = _AsciiRenderer(
        show_branch_lengths=show_branch_lengths,
        show_support=show_support,
        precision=precision,
        char_width=char_width,
        branch_char=branch_char,
        corner_char=corner_char,
        tee_char=tee_char,
        corner_bottom_char=corner_bottom_char,
        vertical_char=vertical_char,
    )
    renderer._render(tree, 0, "")
    return "\n".join(renderer.lines)


class _AsciiRenderer:
    """Internal renderer that builds the ASCII tree line by line."""

    def __init__(
        self,
        show_branch_lengths: bool,
        show_support: bool,
        precision: int,
        char_width: float,
        branch_char: str,
        corner_char: str,
        tee_char: str,
        corner_bottom_char: str,
        vertical_char: str,
    ) -> None:
        self.show_bl = show_branch_lengths
        self.show_support = show_support
        self.precision = precision
        self.char_width = char_width
        self.bc = branch_char
        self.cc = corner_char
        self.tc = tee_char
        self.cbc = corner_bottom_char
        self.vc = vertical_char
        self.lines: list[str] = []

    def _render(self, node: Node, depth: int, prefix: str) -> None:
        """Recursively render the tree."""
        if node.is_leaf:
            label = node.name
            if self.show_bl and node.branch_length is not None:
                bl_str = f"[{node.branch_length:.{self.precision}f}]"
                label = f"{bl_str} {label}"
            self.lines.append(f"{prefix}{self.bc} {label}")
            return

        # Internal node
        children = node.children
        n_children = len(children)
        bl_label = ""
        if self.show_support and node.name:
            bl_label = node.name
        elif self.show_bl and node.branch_length is not None:
            bl_label = f"[{node.branch_length:.{self.precision}f}]"

        for i, child in enumerate(children):
            is_last = i == n_children - 1
            if is_last:
                new_prefix = prefix + "    "
                connector = self.cbc + self.bc * 2
            else:
                new_prefix = prefix + self.vc + "   "
                connector = self.tc + self.bc * 2

            if i == 0 and bl_label:
                # Add the internal node label on the first branch
                self.lines.append(f"{prefix}{self.cc}{self.bc} {bl_label}")

            self._render(child, depth + 1, new_prefix)

    def _render_compact(self, node: Node, depth: int) -> list[str]:
        """Alternative compact rendering that aligns labels vertically."""
        if node.is_leaf:
            label = node.name
            if self.show_bl and node.branch_length is not None:
                label = f"{label} ({node.branch_length:.{self.precision}f})"
            return [f"{label}"]

        child_lines = []
        for i, child in enumerate(node.children):
            cl = self._render_compact(child, depth + 1)
            child_lines.append(cl)

        # This is more complex — fall back to simple rendering
        return self._render_compact_simple(node, depth)

    def _render_compact_simple(self, node: Node, depth: int) -> list[str]:
        """Render in a compact aligned style."""
        if node.is_leaf:
            label = node.name
            if self.show_bl and node.branch_length is not None:
                label += f" ({node.branch_length:.{self.precision}f})"
            return [label]

        result = []
        children = node.children
        n = len(children)

        for i, child in enumerate(children):
            is_last = i == n - 1
            prefix = "└── " if is_last else "├── "
            connector = "    " if is_last else "│   "

            child_lines = self._render_compact_simple(child, depth + 1)

            if child_lines:
                result.append(prefix + child_lines[0])
                for line in child_lines[1:]:
                    result.append(connector + line)

        return result


def render_tree_compact(
    tree: Node,
    show_branch_lengths: bool = True,
    precision: int = 3,
) -> str:
    """Render a tree in a compact style with aligned branches.

    This produces a cleaner output than the default renderer.
    """
    lines = _compact_render(tree, show_branch_lengths, precision)
    return "\n".join(lines)


def _compact_render(
    node: Node,
    show_bl: bool,
    precision: int,
) -> list[str]:
    """Recursively render in compact style."""
    if node.is_leaf:
        label = node.name
        if show_bl and node.branch_length is not None:
            label += f": {node.branch_length:.{precision}f}"
        return [label]

    children = node.children
    n = len(children)
    lines: list[str] = []

    for i, child in enumerate(children):
        is_last = i == n - 1
        branch_prefix = "└── " if is_last else "├── "
        continue_prefix = "    " if is_last else "│   "

        child_lines = _compact_render(child, show_bl, precision)

        if child_lines:
            lines.append(f"{branch_prefix}{child_lines[0]}")
            for cl in child_lines[1:]:
                lines.append(f"{continue_prefix}{cl}")

    return lines


def draw_tree_ascii(
    tree: Node,
    width: int = 80,
    show_branch_lengths: bool = True,
    show_names: bool = True,
) -> str:
    """Draw a tree using proportional branch lengths in a fixed-width format.

    This is a more sophisticated renderer that scales branch lengths
    proportionally to fit within the given width.
    """
    if tree.is_leaf:
        return tree.name

    # Calculate the total tree height
    max_height = tree.height()
    if max_height == 0:
        max_height = 1.0

    # Scale factor
    available_width = width - 30  # Reserve space for labels
    scale = available_width / max_height

    lines: list[str] = []
    _draw_subtree(tree, 0, scale, show_branch_lengths, show_names, lines, "")
    return "\n".join(lines)


def _draw_subtree(
    node: Node,
    depth: float,
    scale: float,
    show_bl: bool,
    show_names: bool,
    lines: list[str],
    prefix: str,
) -> None:
    """Draw a subtree recursively."""
    if node.is_leaf:
        bl_str = ""
        if show_bl and node.branch_length is not None:
            bl_str = f" {node.branch_length:.3f}"
        label = node.name if show_names else ""
        x_pos = int(depth * scale)
        branch_line = "─" * max(0, x_pos - len(prefix))
        lines.append(f"{prefix}{branch_line}──{label}{bl_str}")
        return

    bl = node.branch_length or 0.0
    new_depth = depth + bl

    children = node.children
    n = len(children)

    # Draw each child
    for i, child in enumerate(children):
        is_last = i == n - 1
        if is_last:
            child_prefix = prefix + "│" + " " * int(bl * scale)
        else:
            child_prefix = prefix + " " * int(bl * scale)

        _draw_subtree(child, new_depth, scale, show_bl, show_names, lines, child_prefix)
