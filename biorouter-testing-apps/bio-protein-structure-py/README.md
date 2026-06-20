# bio-protein-structure-py

A pure-Python protein structure analysis toolkit for PDB-format files.

## Features

- **PDB Parser**: Parse ATOM/HETATM records with full support for multi-model, multi-chain structures, coordinates, B-factors, and occupancy.
- **Geometry Utilities**: Compute inter-atomic distances, bond angles, dihedral (torsion) angles, backbone phi/psi torsions, radius of gyration, and center of mass.
- **Secondary Structure Assignment**: Simplified DSSP-like heuristic using backbone hydrogen-bond geometry and torsion angles to assign helix, sheet, or coil.
- **Contact Maps & Clash Detection**: Residue-residue contact maps based on Cα distances, and atomic clash detection using van der Waals radii.
- **Sequence Analysis**: Residue composition, sequence extraction from structure, and 3-letter to 1-letter amino acid code conversion.
- **Structure Superposition**: Kabsch algorithm for optimal superposition and RMSD calculation between two structures.

## Installation

```bash
pip install -e ".[dev]"
```

## Usage

### CLI

```bash
# Analyze a PDB file
bio-protein-structure analyze structure.pdb

# Get Ramachandran angles
bio-protein-structure ramachandran structure.pdb
```

### Python API

```python
from bio_protein_structure.pdb import PDBParser
from bio_protein_structure.geometry import distance, bond_angle, dihedral_angle
from bio_protein_structure.superpose import kabsch_superpose, rmsd

parser = PDBParser()
structure = parser.parse_file("structure.pdb")

for model in structure:
    for chain in model:
        for residue in chain:
            print(residue.name, residue.resseq)
```

## Project Layout

```
src/bio_protein_structure/
    __init__.py     - Package root, version
    pdb.py          - PDB file parser
    geometry.py     - Geometric calculations
    sequence.py     - Residue composition & sequence extraction
    dssp.py         - Secondary structure assignment
    contacts.py     - Contact maps & clash detection
    superpose.py    - Kabsch superposition & RMSD
    cli.py          - Command-line interface
tests/
    conftest.py     - Shared fixtures and PDB test data
    test_pdb.py     - Parser tests
    test_geometry.py- Geometry tests
    test_sequence.py- Sequence tests
    test_dssp.py    - DSSP tests
    test_contacts.py- Contact/clash tests
    test_superpose.py-Superposition tests
    test_cli.py     - CLI tests
```

## Running Tests

```bash
pytest -v
```

## License

MIT
