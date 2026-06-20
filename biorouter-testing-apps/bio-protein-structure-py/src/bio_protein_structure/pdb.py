"""
PDB file parser.

Parses PDB-format files with support for:
- ATOM and HETATM records
- Multi-model (MODEL/ENDMDL) structures
- Multi-chain structures
- Residue and atom hierarchies
- Coordinates, B-factors, occupancy, element symbols

Hierarchy:  Structure > Model > Chain > Residue > Atom
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, Iterator, List, Optional, Sequence, Tuple


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------

@dataclass
class Atom:
    """A single atom parsed from an ATOM or HETATM record."""
    serial: int
    name: str
    alt_loc: str
    res_name: str
    chain_id: str
    res_seq: int
    icode: str
    x: float
    y: float
    z: float
    occupancy: float
    temp_factor: float
    element: str
    record_type: str  # "ATOM" or "HETATM"

    @property
    def coord(self) -> Tuple[float, float, float]:
        return (self.x, self.y, self.z)

    def __repr__(self) -> str:
        return (
            f"Atom({self.serial} {self.name} "
            f"{self.res_name} {self.chain_id}:{self.res_seq} "
            f"({self.x:.3f}, {self.y:.3f}, {self.z:.3f}))"
        )


@dataclass
class Residue:
    """A residue (or HETATM group) containing one or more atoms."""
    name: str
    res_seq: int
    chain_id: str
    icode: str = ""
    atoms: List[Atom] = field(default_factory=list)

    def __len__(self) -> int:
        return len(self.atoms)

    def __iter__(self) -> Iterator[Atom]:
        return iter(self.atoms)

    def get_atom(self, name: str) -> Optional[Atom]:
        """Return the first atom matching *name*, or None."""
        for a in self.atoms:
            if a.name == name:
                return a
        return None

    @property
    def ca(self) -> Optional[Atom]:
        return self.get_atom("CA")

    @property
    def c(self) -> Optional[Atom]:
        return self.get_atom("C")

    @property
    def n(self) -> Optional[Atom]:
        return self.get_atom("N")

    @property
    def o(self) -> Optional[Atom]:
        return self.get_atom("O")

    def __repr__(self) -> str:
        return (
            f"Residue({self.name} {self.chain_id}:{self.res_seq} "
            f"atoms={len(self.atoms)})"
        )


@dataclass
class Chain:
    """A polymer chain containing an ordered list of residues."""
    chain_id: str
    residues: List[Residue] = field(default_factory=list)

    def __len__(self) -> int:
        return len(self.residues)

    def __iter__(self) -> Iterator[Residue]:
        return iter(self.residues)

    def __getitem__(self, idx: int) -> Residue:
        return self.residues[idx]

    def __repr__(self) -> str:
        return f"Chain({self.chain_id} residues={len(self.residues)})"


@dataclass
class Model:
    """A single model containing chains."""
    model_id: int
    chains: Dict[str, Chain] = field(default_factory=dict)

    @property
    def chain_ids(self) -> List[str]:
        return sorted(self.chains.keys())

    def __iter__(self) -> Iterator[Chain]:
        for cid in self.chain_ids:
            yield self.chains[cid]

    def __len__(self) -> int:
        return len(self.chains)

    def get_chain(self, chain_id: str) -> Optional[Chain]:
        return self.chains.get(chain_id)

    @property
    def atoms(self) -> List[Atom]:
        """Flat list of all atoms across all chains/residues."""
        result: List[Atom] = []
        for chain in self:
            for residue in chain:
                result.extend(residue.atoms)
        return result

    def __repr__(self) -> str:
        return f"Model({self.model_id} chains={self.chain_ids})"


@dataclass
class Structure:
    """Top-level container: one or more models from a PDB file."""
    title: str = ""
    models: List[Model] = field(default_factory=list)

    @property
    def first_model(self) -> Optional[Model]:
        return self.models[0] if self.models else None

    def __iter__(self) -> Iterator[Model]:
        return iter(self.models)

    def __len__(self) -> int:
        return len(self.models)

    def __repr__(self) -> str:
        return f"Structure(title={self.title!r} models={len(self.models)})"


# ---------------------------------------------------------------------------
# Parser
# ---------------------------------------------------------------------------

class PDBParseError(Exception):
    """Raised when a PDB file cannot be parsed."""


def _parse_atom_line(line: str) -> Optional[Atom]:
    """Parse a single ATOM or HETATM line.

    Returns None if the line is not an ATOM/HETATM record.
    """
    record = line[:6].strip()
    if record not in ("ATOM", "HETATM"):
        return None

    try:
        serial = int(line[6:11].strip())
        name = line[12:16].strip()
        alt_loc = line[16].strip()
        res_name = line[17:20].strip()
        chain_id = line[21].strip() or "A"
        res_seq = int(line[22:26].strip())
        icode = line[26].strip()
        x = float(line[30:38].strip())
        y = float(line[38:46].strip())
        z = float(line[46:54].strip())
        occupancy = float(line[54:60].strip()) if line[54:60].strip() else 1.0
        temp_factor = float(line[60:66].strip()) if line[60:66].strip() else 0.0
        element = line[76:78].strip() if len(line) > 76 else name[:1]
    except (ValueError, IndexError) as exc:
        raise PDBParseError(f"Malformed ATOM/HETATM line: {line.rstrip()!r}") from exc

    return Atom(
        serial=serial,
        name=name,
        alt_loc=alt_loc,
        res_name=res_name,
        chain_id=chain_id,
        res_seq=res_seq,
        icode=icode,
        x=x,
        y=y,
        z=z,
        occupancy=occupancy,
        temp_factor=temp_factor,
        element=element,
        record_type=record,
    )


class PDBParser:
    """Parse a PDB file or string into a Structure object.

    Usage::

        parser = PDBParser()
        struct = parser.parse_file("1crn.pdb")
        # or
        struct = parser.parse_string(pdb_text)
    """

    def __init__(self) -> None:
        self.warnings: List[str] = []

    # -- public API -----------------------------------------------------------

    def parse_file(self, path: str | Path) -> Structure:
        """Parse a PDB file from disk."""
        p = Path(path)
        if not p.exists():
            raise FileNotFoundError(f"PDB file not found: {p}")
        text = p.read_text(encoding="utf-8", errors="replace")
        return self.parse_string(text)

    def parse_string(self, text: str) -> Structure:
        """Parse PDB text into a Structure."""
        structure = Structure()
        current_model: Optional[Model] = None
        current_chain: Optional[Chain] = None
        current_residue: Optional[Residue] = None

        for line in text.splitlines():
            record = line[:6].strip() if len(line) >= 6 else ""

            # --- Title -------------------------------------------------------
            if record == "TITLE":
                title_part = line[10:].strip()
                structure.title = (
                    (structure.title + " " + title_part).strip()
                    if structure.title
                    else title_part
                )
                continue

            # --- MODEL -------------------------------------------------------
            if record == "MODEL":
                model_id = int(line[10:14].strip()) if len(line) > 10 else 1
                current_model = Model(model_id=model_id)
                structure.models.append(current_model)
                current_chain = None
                current_residue = None
                continue

            # --- ENDMDL ------------------------------------------------------
            if record == "ENDMDL":
                current_model = None
                current_chain = None
                current_residue = None
                continue

            # --- ATOM / HETATM -----------------------------------------------
            atom = _parse_atom_line(line)
            if atom is not None:
                # Ensure we have a model
                if current_model is None:
                    current_model = Model(model_id=1)
                    structure.models.append(current_model)

                # Ensure we have a chain
                if current_chain is None or current_chain.chain_id != atom.chain_id:
                    current_chain = Chain(chain_id=atom.chain_id)
                    current_model.chains[atom.chain_id] = current_chain
                    current_residue = None

                # Ensure we have a residue
                res_key = (atom.res_name, atom.res_seq, atom.chain_id, atom.icode)
                if current_residue is None or (
                    current_residue.res_seq != atom.res_seq
                    or current_residue.chain_id != atom.chain_id
                    or current_residue.name != atom.res_name
                ):
                    current_residue = Residue(
                        name=atom.res_name,
                        res_seq=atom.res_seq,
                        chain_id=atom.chain_id,
                        icode=atom.icode,
                    )
                    current_chain.residues.append(current_residue)

                current_residue.atoms.append(atom)

        # If no MODEL records were present, we already created model 1.
        if not structure.models:
            self.warnings.append("No MODEL/ENDMDL records found; treating as single model.")

        return structure


# ---------------------------------------------------------------------------
# Convenience helpers
# ---------------------------------------------------------------------------

def residue_key(res: Residue) -> Tuple[str, int, str]:
    """Unique key for a residue: (chain_id, res_seq, res_name)."""
    return (res.chain_id, res.res_seq, res.name)


def chain_sequence(chain: Chain) -> List[str]:
    """Return the 3-letter residue names in order for a chain."""
    return [res.name for res in chain]
