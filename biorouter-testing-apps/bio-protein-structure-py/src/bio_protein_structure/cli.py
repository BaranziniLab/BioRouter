"""
Command-line interface for bio-protein-structure.

Usage::

    bio-protein-structure analyze structure.pdb
    bio-protein-structure ramachandran structure.pdb
    bio-protein-structure info structure.pdb
"""

from __future__ import annotations

import argparse
import sys
from typing import List, Optional, Sequence

from .pdb import PDBParser, Structure, Model, Chain, Residue
from .geometry import phi_angle, psi_angle, distance
from .dssp import assign_secondary_structure, ss_summary, ss_fraction
from .sequence import (
    chain_sequence_1letter,
    residue_composition,
    three_to_one,
    is_standard_amino_acid,
)
from .contacts import contact_map, clash_count


def _build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="bio-protein-structure",
        description="Protein structure analysis toolkit",
    )
    sub = p.add_subparsers(dest="command", help="Available commands")

    # --- analyze ---
    analyze_p = sub.add_parser("analyze", help="Full structural analysis of a PDB file")
    analyze_p.add_argument("pdb_file", help="Path to PDB file")
    analyze_p.add_argument("--chain", "-c", help="Restrict to a specific chain")

    # --- ramachandran ---
    rama_p = sub.add_parser("ramachandran", help="Report Ramachandran (phi/psi) angles")
    rama_p.add_argument("pdb_file", help="Path to PDB file")
    rama_p.add_argument("--chain", "-c", help="Restrict to a specific chain")

    # --- info ---
    info_p = sub.add_parser("info", help="Quick summary of a PDB file")
    info_p.add_argument("pdb_file", help="Path to PDB file")

    return p


# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------

def cmd_analyze(args: argparse.Namespace) -> int:
    """Full structural analysis."""
    parser = PDBParser()
    try:
        struct = parser.parse_file(args.pdb_file)
    except (FileNotFoundError, Exception) as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    model = struct.first_model
    if model is None:
        print("No models found in PDB file.", file=sys.stderr)
        return 1

    print(f"Title:   {struct.title or '(none)'}")
    print(f"Models:  {len(struct.models)}")
    print(f"Chains:  {model.chain_ids}")
    print()

    for chain in model:
        if args.chain and chain.chain_id != args.chain:
            continue

        print(f"--- Chain {chain.chain_id} ---")
        print(f"  Residues:      {len(chain)}")
        seq_1 = chain_sequence_1letter(chain)
        print(f"  Sequence:      {seq_1}")
        print(f"  Sequence len:  {len(seq_1)}")

        # Secondary structure
        labels = assign_secondary_structure(chain)
        n_atoms = sum(len(res) for res in chain)
        print(f"  Atoms:         {n_atoms}")

        ss = ss_summary(chain)
        frac = ss_fraction(chain)
        print(f"  SS helix:      {ss['H']} ({frac['H']:.1%})")
        print(f"  SS sheet:      {ss['E']} ({frac['E']:.1%})")
        print(f"  SS coil:       {ss['C']} ({frac['C']:.1%})")

        # Contacts & clashes
        cmap = contact_map(chain)
        n_clashes = clash_count(chain)
        print(f"  Contacts (8Å): {len(cmap)}")
        print(f"  Clash count:   {n_clashes}")

        # Residue composition
        comp = residue_composition(chain)
        print(f"  Composition:   {comp}")
        print()

    return 0


def cmd_ramachandran(args: argparse.Namespace) -> int:
    """Report Ramachandran phi/psi angles."""
    parser = PDBParser()
    try:
        struct = parser.parse_file(args.pdb_file)
    except (FileNotFoundError, Exception) as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    model = struct.first_model
    if model is None:
        print("No models found.")
        return 1

    print(f"{'Chain':>5} {'ResName':>7} {'ResSeq':>6} {'Phi':>8} {'Psi':>8}")
    print("-" * 40)

    for chain in model:
        if args.chain and chain.chain_id != args.chain:
            continue

        residues = list(chain)
        for i, res in enumerate(residues):
            phi_val: Optional[float] = None
            psi_val: Optional[float] = None

            if i > 0:
                c_prev = residues[i - 1].c
                if c_prev and res.n and res.ca and res.c:
                    phi_val = phi_angle(c_prev.coord, res.n.coord, res.ca.coord, res.c.coord)

            if i < len(residues) - 1:
                n_next = residues[i + 1].n
                if res.n and res.ca and res.c and n_next:
                    psi_val = psi_angle(res.n.coord, res.ca.coord, res.c.coord, n_next.coord)

            phi_str = f"{phi_val:8.2f}" if phi_val is not None else "      --"
            psi_str = f"{psi_val:8.2f}" if psi_val is not None else "      --"

            print(
                f"{chain.chain_id:>5} {res.name:>7} {res.res_seq:>6}"
                f" {phi_str} {psi_str}"
            )

    return 0


def cmd_info(args: argparse.Namespace) -> int:
    """Quick info summary."""
    parser = PDBParser()
    try:
        struct = parser.parse_file(args.pdb_file)
    except (FileNotFoundError, Exception) as exc:
        print(f"Error: {exc}", file=sys.stderr)
        return 1

    model = struct.first_model
    if model is None:
        print("No models found.")
        return 1

    total_atoms = sum(len(res) for chain in model for res in chain)
    total_residues = sum(len(chain) for chain in model)

    print(f"File:    {args.pdb_file}")
    print(f"Title:   {struct.title or '(none)'}")
    print(f"Models:  {len(struct.models)}")
    print(f"Chains:  {model.chain_ids}")
    print(f"Residues: {total_residues}")
    print(f"Atoms:   {total_atoms}")

    return 0


# ---------------------------------------------------------------------------
# Main entry point
# ---------------------------------------------------------------------------

COMMANDS = {
    "analyze": cmd_analyze,
    "ramachandran": cmd_ramachandran,
    "info": cmd_info,
}


def main(argv: Optional[Sequence[str]] = None) -> int:
    """CLI entry point."""
    p = _build_parser()
    args = p.parse_args(argv)

    if args.command is None:
        p.print_help()
        return 0

    handler = COMMANDS.get(args.command)
    if handler is None:
        print(f"Unknown command: {args.command}", file=sys.stderr)
        return 1

    return handler(args)


if __name__ == "__main__":
    sys.exit(main())
