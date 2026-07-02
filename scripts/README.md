# BioRouter Scripts

This directory contains scripts for building, testing, deploying, and analyzing
BioRouter.

## Headless Linux app build

Use this when a future coding agent is asked to build the headless Debian/Ubuntu
browser app artifact:

```bash
source bin/activate-hermit
scripts/package-headless-linux.sh
```

This runs `scripts/build-headless-linux.sh`, verifies the artifact with
`scripts/verify-headless-artifact.sh --tar`, and writes:

- `dist/headless-linux-x64/`
- `dist/biorouter-headless-linux-x64.tar.gz`

The release artifact must contain only the app: the three Linux binaries, the
browser bundle, and `manifest.txt`. Do not package local profiles, sessions,
provider credentials, AWS credentials, SSH keys, macOS Keychain exports, or
other user-specific configuration. Runtime secret migration, when needed for a
private deployment, is handled separately by `scripts/sync-headless-secrets-macos.sh`.

See `docs/headless-linux.md` for the full runbook.

## Benchmark scripts

## run-benchmarks.sh

This script runs BioRouter benchmarks across multiple provider:model pairs and analyzes the results.

### Prerequisites

- BioRouter CLI must be built or installed
- `jq` command-line tool for JSON processing (optional, but recommended for result analysis)

### Usage

```bash
./scripts/run-benchmarks.sh [options]
```

#### Options

- `-p, --provider-models`: Comma-separated list of provider:model pairs (e.g., 'openai:gpt-4o,anthropic:claude-sonnet-4')
- `-s, --suites`: Comma-separated list of benchmark suites to run (e.g., 'core,small_models')
- `-o, --output-dir`: Directory to store benchmark results (default: './benchmark-results')
- `-d, --debug`: Use debug build instead of release build
- `-h, --help`: Show help message

#### Examples

```bash
# Run with release build (default)
./scripts/run-benchmarks.sh --provider-models 'openai:gpt-4o,anthropic:claude-sonnet-4' --suites 'core,small_models'

# Run with debug build
./scripts/run-benchmarks.sh --provider-models 'openai:gpt-4o' --suites 'core' --debug
```

### How It Works

The script:
1. Parses the provider:model pairs and benchmark suites
2. Determines whether to use the debug or release binary
3. For each provider:model pair:
   - Sets the `BIOROUTER_PROVIDER` and `BIOROUTER_MODEL` environment variables
   - Runs the benchmark with the specified suites
   - Analyzes the results for failures
4. Generates a summary of all benchmark runs

### Output

The script creates the following files in the output directory:

- `summary.md`: A summary of all benchmark results
- `{provider}-{model}.json`: Raw JSON output from each benchmark run
- `{provider}-{model}-analysis.txt`: Analysis of each benchmark run

### Exit Codes

- `0`: All benchmarks completed successfully
- `1`: One or more benchmarks failed

## parse-benchmark-results.sh

This script analyzes a single benchmark JSON result file and identifies any failures.

### Usage

```bash
./scripts/parse-benchmark-results.sh path/to/benchmark-results.json
```

### Output

The script outputs an analysis of the benchmark results to stdout, including:

- Basic information about the benchmark run
- Results for each evaluation in each suite
- Summary of passed and failed metrics

### Exit Codes

- `0`: All metrics passed successfully
- `1`: One or more metrics failed
