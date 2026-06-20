# med-icd-snomed-mapper-py

Clinical terminology crosswalk service for **ICD-10** and **SNOMED CT**, implemented in pure Python.

## Features

- **In-memory terminology store** — codes, descriptions, active flags, parent/child hierarchy
- **Crosswalk engine** — bidirectional ICD-10 ↔ SNOMED mapping with one-to-one and one-to-many support (map groups, rules, priority)
- **Hierarchy operations** — ancestors, descendants, is-a checks, lowest common ancestor (LCA), depth
- **Value-set expansion** — expand any root code to all descendants
- **Fuzzy search** — rapidfuzz-powered description search (token_sort, partial, token_set scoring)
- **Validation** — check if a code is valid and active
- **CSV / JSON loaders** — bootstrap terminologies and maps from standard formats
- **CLI** — full command-line interface (Click) with argparse fallback

## Quick Start

```bash
pip install -e ".[dev]"
```

### CLI usage

```bash
# Look up a code
medmapper --icd10-csv data/icd10_sample.csv lookup ICD-10-CM E11.9

# Map ICD-10 → SNOMED
medmapper --icd10-csv data/icd10_sample.csv --snomed-csv data/snomed_sample.csv \
          --map-csv data/crossmap.csv map ICD-10-CM E11.9 -t SNOMED-CT

# Expand a value set
medmapper --snomed-csv data/snomed_sample.csv expand SNOMED-CT 44054006

# Fuzzy search
medmapper --snomed-csv data/snomed_sample.csv search "diabetes"

# Validate
medmapper --icd10-csv data/icd10_sample.csv validate ICD-10-CM E11.9
```

### Python API

```python
from medmapper.terminology import TerminologyStore, load_concepts_csv, load_map_csv
from medmapper.hierarchy import Hierarchy
from medmapper.mapping import CrosswalkEngine
from medmapper.search import ConceptSearch
from medmapper.valueset import ValueSetExpander

store = TerminologyStore()
store.add_many(load_concepts_csv("data/icd10_sample.csv", "ICD-10-CM"))
store.add_many(load_concepts_csv("data/snomed_sample.csv", "SNOMED-CT"))

hierarchy = Hierarchy(store)
engine = CrosswalkEngine(store, load_map_csv("data/crossmap.csv"))
searcher = ConceptSearch(store)
expander = ValueSetExpander(store, hierarchy)

# Map
result = engine.map_code("ICD-10-CM", "E11.9", target_terminology="SNOMED-CT")
print(result.best.target_code)  # 111552007

# Hierarchy
print(hierarchy.is_a(("SNOMED-CT", "44054006"), ("SNOMED-CT", "138871004")))  # True

# Expand
vs = expander.expand("SNOMED-CT", "44054006")
print(vs.size)  # number of descendants + root

# Search
hits = searcher.search("diabetes mellitus", terminology="SNOMED-CT")
print(hits[0].description)
```

## Sample Data

Small embedded sample hierarchies in `data/`:

| File | Description |
|------|-------------|
| `data/icd10_sample.csv` | ~120 ICD-10-CM codes across 15 chapters |
| `data/snomed_sample.csv` | ~80 SNOMED CT concepts |
| `data/crossmap.csv` | ~70 cross-map entries (ICD-10 → SNOMED and reverse) |

## Testing

```bash
pip install -e ".[dev]"
pytest
```

## Project Structure

```
med-icd-snomed-mapper-py/
├── src/medmapper/
│   ├── __init__.py
│   ├── __main__.py
│   ├── terminology.py   # Concept, TerminologyStore, CSV/JSON loaders
│   ├── hierarchy.py      # DAG traversal: ancestors, descendants, LCA
│   ├── mapping.py        # CrosswalkEngine with 1:1 and 1:N support
│   ├── search.py         # Fuzzy text search
│   ├── valueset.py       # Value-set expansion
│   └── cli.py            # Click CLI + argparse fallback
├── data/                 # Sample data files
├── tests/                # pytest suite
├── pyproject.toml
└── README.md
```

## License

MIT
