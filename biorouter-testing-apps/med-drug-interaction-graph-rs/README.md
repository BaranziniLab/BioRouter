# med-drug-interaction-graph-rs

A **drug-drug interaction (DDI) graph engine** in Rust that models drugs and their interactions as a weighted, typed graph. It can load drug databases from CSV/JSON, query interactions for a patient's medication regimen, rank them by severity, detect interaction chains/cascades, find hub drugs, and suggest safer alternatives.

## Features

- **Graph Model**: Drugs as nodes (name, class, targets), interactions as typed/weighted edges (PK/PD, severity, mechanism, evidence level)
- **Multi-format Loading**: Load from CSV or JSON databases
- **Interaction Query**: Given a medication list, find all pairwise interactions, ranked by severity
- **Chain Detection**: Find interaction "cascades" where drug A interacts with B, B with C, etc.
- **Hub Analysis**: Identify high-risk drugs that are hubs in the interaction network (degree centrality, weighted centrality)
- **Alternative Suggestions**: Find safer drugs in the same therapeutic class with fewer/lower-severity interactions
- **Severity Scoring**: Comprehensive risk scoring for entire medication regimens
- **CLI**: Full command-line interface for loading databases, querying, and analysis

## Architecture

```
src/
├── main.rs          # CLI entry point
├── model.rs         # Core data structures (Drug, Interaction, Severity, etc.)
├── io.rs            # CSV/JSON loading and database validation
├── graph.rs         # Graph engine (petgraph-based) and algorithms
├── query.rs         # Interaction query engine
├── severity.rs      # Regimen severity scoring and profiling
├── suggest.rs       # Alternative drug suggestion engine
└── cli.rs           # CLI argument parsing (clap)
```

## Quick Start

### Build

```bash
cargo build --release
```

### Run tests

```bash
cargo test
```

### Query interactions

```bash
# Load a database and check interactions for a medication list
cargo run -- query -d data/sample_database.json -m "warfarin,aspirin,fluoxetine"

# With detailed mechanism descriptions
cargo run -- query -d data/sample_database.json -m "warfarin,aspirin,fluoxetine,amiodarone" --detailed

# Detect chains up to depth 5
cargo run -- query -d data/sample_database.json -m "warfarin,fluoxetine,omeprazole" -c 5
```

### Explore a drug

```bash
# Show all interactions for a drug
cargo run -- drug -d data/sample_database.json -n warfarin

# List all drugs in database
cargo run -- drug -d data/sample_database.json -n "" --list-all
```

### Find alternatives

```bash
# Find same-class alternatives for aspirin given a regimen
cargo run -- alternatives -d data/sample_database.json --for-drug aspirin -r "warfarin,aspirin"

# Broad search across drug classes
cargo run -- alternatives -d data/sample_database.json --for-drug fluoxetine -r "warfarin,fluoxetine" --broad
```

### Graph analysis

```bash
# Show connected components and centrality
cargo run -- analyze -d data/sample_database.json --components --centrality

# Find hub drugs at the 90th percentile
cargo run -- analyze -d data/sample_database.json --hubs 0.9
```

### Compare regimens

```bash
# Compare two medication regimens for safety
cargo run -- compare -d data/sample_database.json \
  -a "warfarin,omeprazole,metformin" \
  -b "warfarin,ibuprofen,amiodarone"
```

## Database Format

### JSON

```json
{
  "drugs": [
    {"name": "warfarin", "class": "anticoagulant", "targets": ["VKORC1", "CYP2C9"], "brand_names": ["Coumadin"]}
  ],
  "interactions": [
    {"drug_a": "warfarin", "drug_b": "aspirin", "type": "pharmacodynamic", "severity": "major", "mechanism": "...", "evidence": "established", "recommendation": "..."}
  ]
}
```

### CSV (drugs)

```csv
name,class,targets,brand_names
warfarin,anticoagulant,VKORC1;CYP2C9,Coumadin;Jantoven
```

### CSV (interactions)

```csv
drug_a,drug_b,type,severity,mechanism,evidence,recommendation
warfarin,aspirin,pharmacodynamic,major,Additive anticoagulant effect,established,Monitor INR
```

### Severity Levels

| Level | Score | Description |
|-------|-------|-------------|
| Minor | 1 | Monitor patient, low clinical significance |
| Moderate | 2 | May require dose adjustment or monitoring |
| Major | 3 | Avoid combination if possible |
| Contraindicated | 4 | Never use together |

### Interaction Types

- **Pharmacokinetic (PK)**: One drug affects absorption/distribution/metabolism/excretion of another
- **Pharmacodynamic (PD)**: Drugs have additive/synergistic/adverse effects at target level
- **Both**: Combined PK and PD interactions

### Evidence Levels

- **Established**: Confirmed by multiple studies / clinical guidelines
- **Probable**: Supported by case series or strong pharmacological reasoning
- **Suspected**: Limited evidence, theoretical or case reports
- **Unknown**: Interaction is plausible but unverified

## Sample Data

The `data/sample_database.json` file contains 20 common drugs with 24 interactions covering:
- Warfarin interactions (aspirin, NSAIDs, SSRIs, amiodarone, carbamazepine)
- SSRI combinations (serotonin syndrome risk)
- RAAS blockade (ACE inhibitor + ARB)
- Statin interactions (amiodarone, cyclosporine)
- Digoxin interactions (amiodarone, verapamil)

## Dependencies

- `petgraph` — Graph data structures and algorithms
- `serde` / `serde_json` — JSON serialization
- `csv` — CSV parsing
- `clap` — Command-line argument parsing
- `thiserror` — Error handling

## License

MIT
