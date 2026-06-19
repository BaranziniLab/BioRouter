"""
Command-line interface for bio-phylo.

Usage examples::

    # Build from FASTA alignment
    bio-phylo build --input alignment.fasta --method nj --model k2p

    # Build from distance matrix
    bio-phylo build --matrix distances.txt --method upgma

    # With bootstrap support
    bio-phylo build --input alignment.fasta --method nj --bootstrap 100

    # Compute distances
    bio-phylo distance --input alignment.fasta --model jc

    # Show tree info
    bio-phylo info --newick "((A:0.1,B:0.2):0.3,C:0.4);"
"""

from __future__ import annotations

import sys
from typing import Optional

try:
    import click
except ImportError:
    click = None  # type: ignore[assignment]

from bio_phylo.ascii_tree import render_tree_compact
from bio_phylo.bootstrap import annotate_tree_with_support, bootstrap_support
from bio_phylo.distance import compute_distance_matrix
from bio_phylo.nj import neighbor_joining
from bio_phylo.parsimony import parsimony_greedy
from bio_phylo.tree import Node, from_newick
from bio_phylo.upgma import upgma
from bio_phylo.utils import (
    alignment_summary,
    read_fasta,
    read_distance_matrix,
    validate_alignment,
)
from bio_phylo.distance import DistanceMatrix


def _build_tree(
    method: str,
    alignment: Optional[dict[str, str]] = None,
    dm: Optional[DistanceMatrix] = None,
    model: str = "p-distance",
) -> Node:
    """Build a tree using the specified method."""
    if method in ("upgma", "nj"):
        if dm is None and alignment is not None:
            dm = compute_distance_matrix(alignment, model=model)
        if dm is None:
            raise ValueError("Need either alignment or distance matrix for distance methods")
        if method == "upgma":
            return upgma(dm)
        else:
            return neighbor_joining(dm)
    elif method in ("parsimony", "fitch"):
        if alignment is None:
            raise ValueError("Need alignment for parsimony method")
        return parsimony_greedy(alignment)
    else:
        raise ValueError(f"Unknown method '{method}'. Choose from: upgma, nj, parsimony")


HELP_TEXT = """\
bio-phylo - Molecular Phylogenetics Toolkit

Usage:
  bio-phylo build    --input FILE [--method METHOD] [--model MODEL] [--bootstrap N]
  bio-phylo build    --matrix FILE [--method METHOD]
  bio-phylo distance --input FILE [--model MODEL]
  bio-phylo info     NEWICK_STRING

Methods: upgma, nj, parsimony
Models:  p-distance, jukes-cantor, kimura-2param
"""


def _main_cli(args: list[str] | None = None) -> int:
    """Pure-Python CLI fallback when click is not installed."""
    if args is None:
        args = sys.argv[1:]

    if not args or args[0] in ("-h", "--help"):
        print(HELP_TEXT)
        return 0

    command = args[0]

    if command == "build":
        return _cmd_build(args[1:])
    elif command == "distance":
        return _cmd_distance(args[1:])
    elif command == "info":
        return _cmd_info(args[1:])
    else:
        print(f"Unknown command: {command}", file=sys.stderr)
        print(HELP_TEXT)
        return 1


def _cmd_build(args: list[str]) -> int:
    """Handle the 'build' subcommand."""
    input_file: Optional[str] = None
    matrix_file: Optional[str] = None
    method = "nj"
    model = "p-distance"
    bootstrap_n = 0
    output_newick: Optional[str] = None

    i = 0
    while i < len(args):
        if args[i] == "--input" and i + 1 < len(args):
            input_file = args[i + 1]
            i += 2
        elif args[i] == "--matrix" and i + 1 < len(args):
            matrix_file = args[i + 1]
            i += 2
        elif args[i] == "--method" and i + 1 < len(args):
            method = args[i + 1]
            i += 2
        elif args[i] == "--model" and i + 1 < len(args):
            model = args[i + 1]
            i += 2
        elif args[i] == "--bootstrap" and i + 1 < len(args):
            bootstrap_n = int(args[i + 1])
            i += 2
        elif args[i] == "--output" and i + 1 < len(args):
            output_newick = args[i + 1]
            i += 2
        else:
            print(f"Unknown option: {args[i]}", file=sys.stderr)
            return 1

    alignment = None
    dm = None

    if input_file:
        alignment = read_fasta(input_file)
        issues = validate_alignment(alignment)
        if issues:
            print("Alignment warnings:", file=sys.stderr)
            for issue in issues:
                print(f"  - {issue}", file=sys.stderr)
        print(alignment_summary(alignment))
        print()

    if matrix_file:
        dm = read_distance_matrix(matrix_file)
        print(f"Distance matrix: {len(dm.names)} taxa")
        print(dm.formatted())
        print()

    if alignment is None and dm is None:
        print("Error: provide --input or --matrix", file=sys.stderr)
        return 1

    tree = _build_tree(method, alignment=alignment, dm=dm, model=model)

    if bootstrap_n > 0 and alignment is not None:
        print(f"Computing bootstrap support ({bootstrap_n} replicates)...")
        support = bootstrap_support(
            alignment,
            tree_builder=lambda aln: _build_tree(method, alignment=aln, model=model),
            n_replicates=bootstrap_n,
        )
        tree = annotate_tree_with_support(tree, support, bootstrap_n)
        print()

    newick = tree.to_newick(precision=6)
    print("Newick:")
    print(newick)
    print()
    print("Tree:")
    print(render_tree_compact(tree, show_branch_lengths=True))

    if output_newick:
        with open(output_newick, "w") as f:
            f.write(newick + "\n")
        print(f"\nNewick written to: {output_newick}")

    return 0


def _cmd_distance(args: list[str]) -> int:
    """Handle the 'distance' subcommand."""
    input_file: Optional[str] = None
    model = "p-distance"

    i = 0
    while i < len(args):
        if args[i] == "--input" and i + 1 < len(args):
            input_file = args[i + 1]
            i += 2
        elif args[i] == "--model" and i + 1 < len(args):
            model = args[i + 1]
            i += 2
        else:
            print(f"Unknown option: {args[i]}", file=sys.stderr)
            return 1

    if input_file is None:
        print("Error: provide --input", file=sys.stderr)
        return 1

    alignment = read_fasta(input_file)
    dm = compute_distance_matrix(alignment, model=model)
    print(f"Distance matrix ({model}):")
    print(dm.formatted())
    return 0


def _cmd_info(args: list[str]) -> int:
    """Handle the 'info' subcommand."""
    newick_str: Optional[str] = None

    if args:
        newick_str = args[0]

    if newick_str is None:
        print("Error: provide a Newick string", file=sys.stderr)
        return 1

    tree = from_newick(newick_str)
    print(f"Leaves: {tree.num_leaves}")
    print(f"Internal nodes: {tree.num_internal_nodes()}")
    print(f"Binary: {tree.is_binary()}")
    print(f"Total branch length: {tree.total_branch_length:.6f}")
    print(f"Height: {tree.height():.6f}")
    print(f"Leaf names: {tree.leaf_names}")
    print()
    print("Newick:", tree.to_newick())
    print()
    print("ASCII tree:")
    print(render_tree_compact(tree, show_branch_lengths=True))
    return 0


# ======================================================================
# Click-based CLI (preferred)
# ======================================================================

if click is not None:

    @click.group()
    def cli():
        """bio-phylo: Molecular Phylogenetics Toolkit"""
        pass

    @cli.command()
    @click.option("--input", "input_file", type=click.Path(exists=True), help="FASTA alignment file")
    @click.option("--matrix", "matrix_file", type=click.Path(exists=True), help="Distance matrix file")
    @click.option("--method", type=click.Choice(["upgma", "nj", "parsimony"]), default="nj")
    @click.option(
        "--model",
        type=click.Choice(["p-distance", "jukes-cantor", "kimura-2param"]),
        default="p-distance",
    )
    @click.option("--bootstrap", "bootstrap_n", type=int, default=0, help="Number of bootstrap replicates")
    @click.option("--output", "output_file", type=click.Path(), default=None, help="Output Newick file")
    def build(input_file, matrix_file, method, model, bootstrap_n, output_file):
        """Build a phylogenetic tree."""
        alignment = None
        dm = None

        if input_file:
            alignment = read_fasta(input_file)
            issues = validate_alignment(alignment)
            if issues:
                click.echo("Alignment warnings:", err=True)
                for issue in issues:
                    click.echo(f"  - {issue}", err=True)
            click.echo(alignment_summary(alignment))
            click.echo()

        if matrix_file:
            dm = read_distance_matrix(matrix_file)
            click.echo(f"Distance matrix: {len(dm.names)} taxa")
            click.echo(dm.formatted())
            click.echo()

        if alignment is None and dm is None:
            click.echo("Error: provide --input or --matrix", err=True)
            return

        tree = _build_tree(method, alignment=alignment, dm=dm, model=model)

        if bootstrap_n > 0 and alignment is not None:
            click.echo(f"Computing bootstrap support ({bootstrap_n} replicates)...")
            support = bootstrap_support(
                alignment,
                tree_builder=lambda aln: _build_tree(method, alignment=aln, model=model),
                n_replicates=bootstrap_n,
            )
            tree = annotate_tree_with_support(tree, support, bootstrap_n)
            click.echo()

        newick = tree.to_newick(precision=6)
        click.echo("Newick:")
        click.echo(newick)
        click.echo()
        click.echo("Tree:")
        click.echo(render_tree_compact(tree, show_branch_lengths=True))

        if output_file:
            with open(output_file, "w") as f:
                f.write(newick + "\n")
            click.echo(f"\nNewick written to: {output_file}")

    @cli.command()
    @click.option("--input", "input_file", type=click.Path(exists=True), required=True)
    @click.option(
        "--model",
        type=click.Choice(["p-distance", "jukes-cantor", "kimura-2param"]),
        default="p-distance",
    )
    def distance(input_file, model):
        """Compute pairwise distances from an alignment."""
        alignment = read_fasta(input_file)
        dm = compute_distance_matrix(alignment, model=model)
        click.echo(f"Distance matrix ({model}):")
        click.echo(dm.formatted())

    @cli.command()
    @click.argument("newick_str")
    def info(newick_str):
        """Display information about a Newick tree."""
        tree = from_newick(newick_str)
        click.echo(f"Leaves: {tree.num_leaves}")
        click.echo(f"Internal nodes: {tree.num_internal_nodes()}")
        click.echo(f"Binary: {tree.is_binary()}")
        click.echo(f"Total branch length: {tree.total_branch_length:.6f}")
        click.echo(f"Height: {tree.height():.6f}")
        click.echo(f"Leaf names: {tree.leaf_names}")
        click.echo()
        click.echo("ASCII tree:")
        click.echo(render_tree_compact(tree, show_branch_lengths=True))

    main = cli
else:
    # Fallback to pure Python
    def main():
        sys.exit(_main_cli())


if __name__ == "__main__":
    main()
