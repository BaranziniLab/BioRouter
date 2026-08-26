#!/usr/bin/env bash
set -e

# Check if OpenAPI schema is up-to-date
# Invoke this through `just check-openapi-schema`, which regenerates the schema
# and frontend client before this script compares them with the committed version.

echo "🔍 Checking OpenAPI schema is up-to-date..."

# Check if the generated schema differs from the committed version. Compare the
# exact output: whitespace-only drift still means the checked-in generator
# result is stale.
echo "🔍 Comparing generated schema with committed version..."
if ! git diff --exit-code -- ui/desktop/openapi.json ui/desktop/src/api/; then
  echo ""
  echo "❌ OpenAPI schema is out of date!"
  echo ""
  echo "The generated OpenAPI schema differs from the committed version."
  echo "This usually means that API types were added or modified without updating the schema."
  echo ""
  echo "To fix this issue:"
  echo "1. Run 'just generate-openapi' locally"
  echo "2. Commit the changes to ui/desktop/openapi.json and ui/desktop/src/api/"
  echo "3. Push your changes"
  echo ""
  echo "Changes detected:"
  git diff ui/desktop/openapi.json ui/desktop/src/api/
  exit 1
fi

# `git diff` does not report an untracked file. A generator that adds a new API
# module would otherwise let CI pass while leaving that module out of the
# commit, which is exactly the drift this gate exists to prevent.
untracked_generated="$(
  git ls-files --others --exclude-standard -- ui/desktop/openapi.json ui/desktop/src/api/
)"
if [ -n "$untracked_generated" ]; then
  echo ""
  echo "❌ OpenAPI generation created untracked client files:"
  printf '%s\n' "$untracked_generated"
  echo ""
  echo "Run 'just generate-openapi' and commit every generated file."
  exit 1
fi

echo "✅ OpenAPI schema is up-to-date"
