#!/usr/bin/env bash
# Deterministically seed a Biorouter app into the store so biorouterd can serve
# it. The SDK is copied from the worktree templates so the served bundle is the
# real runtime. Model defaults to xiaomi_mimo / mimo-v2.5.
#
# Usage: seed-app.sh <id> <title> <description> <index.html> <main.ts> [exts] [system_prompt] [model]
set -euo pipefail

ID="$1"; TITLE="$2"; DESC="$3"; INDEX="$4"; MAIN="$5"
EXTS="${6:-}"            # comma-separated extensions, optional
SYS="${7:-}"            # system prompt, optional
MODEL="${8:-mimo-v2.5}" # model name, optional

ROOT="$HOME/.config/biorouter/agent_drafter/$ID"
TEMPLATES="$(cd "$(dirname "$0")/../../crates/biorouter-mcp/src/agent_drafter/templates" && pwd)"
NOW=$(date +%s)

mkdir -p "$ROOT/src"
cp "$TEMPLATES/sdk.ts" "$ROOT/src/sdk.ts"
cp "$MAIN" "$ROOT/src/main.ts"
# Substitute title/description placeholders in the index.
sed -e "s/{{TITLE}}/$TITLE/g" -e "s/{{DESCRIPTION}}/$DESC/g" "$INDEX" > "$ROOT/index.html"

# extensions JSON array
ext_json="[]"
if [ -n "$EXTS" ]; then
  ext_json=$(printf '%s' "$EXTS" | awk -F, '{printf "["; for(i=1;i<=NF;i++){printf "%s\"%s\"", (i>1?",":""), $i} printf "]"}')
fi

# JSON-escape the system prompt minimally (quotes + backslashes).
sys_esc=$(printf '%s' "$SYS" | sed 's/\\/\\\\/g; s/"/\\"/g')

cat > "$ROOT/manifest.json" <<JSON
{
  "id": "$ID",
  "title": "$TITLE",
  "description": "$DESC",
  "kind": "agentic",
  "entry": "index.html",
  "created_at": $NOW,
  "updated_at": $NOW,
  "agent": {
    "system_prompt": "$sys_esc",
    "greeting": "Hi! How can I help?",
    "model": { "provider": "xiaomi_mimo", "model": "$MODEL" },
    "extensions": $ext_json
  }
}
JSON

echo "Seeded app '$ID' -> $ROOT"
