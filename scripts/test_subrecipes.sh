#!/bin/bash
set -e

if [ -f .env ]; then
  export $(grep -v '^#' .env | xargs)
fi

if [ -z "$SKIP_BUILD" ]; then
  echo "Building biorouter..."
  cargo build --release --bin biorouter
  echo ""
else
  echo "Skipping build (SKIP_BUILD is set)..."
  echo ""
fi

SCRIPT_DIR=$(pwd)

# Add biorouter binary to PATH so subagents can find it when spawning
export PATH="$SCRIPT_DIR/target/release:$PATH"

# Set default provider and model if not already set
# Use fast model for CI to speed up tests
export BIOROUTER_PROVIDER="${BIOROUTER_PROVIDER:-anthropic}"
export BIOROUTER_MODEL="${BIOROUTER_MODEL:-claude-3-5-haiku-20241022}"

echo "Using provider: $BIOROUTER_PROVIDER"
echo "Using model: $BIOROUTER_MODEL"
echo ""

TESTDIR=$(mktemp -d)
echo "Created test directory: $TESTDIR"

cp -r "$SCRIPT_DIR/scripts/test-subrecipes-examples/"* "$TESTDIR/"
echo "Copied test workflows from scripts/test-subrecipes-examples"

echo ""
echo "=== Testing Subworkflow Execution ==="
echo "Workflow: $TESTDIR/project_analyzer.yaml"
echo ""

# Create sample code files for analysis
echo "Creating sample code files for testing..."
cat > "$TESTDIR/sample.rs" << 'EOF'
// TODO: Add error handling
fn calculate(x: i32, y: i32) -> i32 {
    x + y
}

#[test]
fn test_calculate() {
    assert_eq!(calculate(2, 2), 4);
}
EOF

cat > "$TESTDIR/sample.py" << 'EOF'
# FIXME: Optimize this function
def process_data(items):
    """Process a list of items"""
    return [item * 2 for item in items]

def test_process_data():
    assert process_data([1, 2, 3]) == [2, 4, 6]
EOF

cat > "$TESTDIR/README.md" << 'EOF'
# Sample Project
This is a test project for analyzing code patterns.
## TODO
- Add more tests
EOF
echo ""

RESULTS=()

check_workflow_output() {
  local tmpfile=$1
  local mode=$2
  
  # Check for unified subagent tool invocation (new format: "─── subagent |")
  if grep -q "─── subagent" "$tmpfile"; then
    echo "✓ SUCCESS: Subagent tool invoked"
    RESULTS+=("✓ Subagent tool invocation ($mode)")
  else
    echo "✗ FAILED: No evidence of subagent tool invocation"
    RESULTS+=("✗ Subagent tool invocation ($mode)")
  fi
  
  # Check that both subworkflows were called. The CLI renders the tool argument as
  # "subworkflow <name>" (crates/biorouter-cli/src/session/output.rs); the pre-rename
  # "subrecipe" spelling is still accepted here so an older binary can be tested too.
  if grep -qiE "sub(workflow|recipe).*file_stats|file_stats.*sub(workflow|recipe)" "$tmpfile" \
    && grep -qiE "sub(workflow|recipe).*code_patterns|code_patterns.*sub(workflow|recipe)" "$tmpfile"; then
    echo "✓ SUCCESS: Both subworkflows (file_stats, code_patterns) found in output"
    RESULTS+=("✓ Both subworkflows present ($mode)")
  else
    echo "✗ FAILED: Not all subworkflows found in output"
    RESULTS+=("✗ Subworkflow names ($mode)")
  fi
}

echo "Running workflow with parallel subworkflows..."
TMPFILE=$(mktemp)
if (cd "$TESTDIR" && "$SCRIPT_DIR/target/release/biorouter" run --workflow project_analyzer_parallel.yaml --no-session 2>&1) | tee "$TMPFILE"; then
  echo "✓ SUCCESS: Workflow completed successfully"
  RESULTS+=("✓ Workflow exit code")
  check_workflow_output "$TMPFILE" "parallel"
else
  echo "✗ FAILED: Workflow execution failed"
  RESULTS+=("✗ Workflow exit code")
fi
rm "$TMPFILE"
echo ""

rm -rf "$TESTDIR"

echo "=== Test Summary ==="
for result in "${RESULTS[@]}"; do
  echo "$result"
done

if echo "${RESULTS[@]}" | grep -q "✗"; then
  echo ""
  echo "Some tests failed!"
  exit 1
else
  echo ""
  echo "All tests passed!"
fi
