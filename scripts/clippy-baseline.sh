#!/bin/bash

# Baseline clippy rules - only fail on NEW violations
#
# Format: "rule_name|violation_parser"
#
# Violation parsers (run clippy on your rule to see which fits):
#   function_name - When spans show: "fn my_function(..."
#   type_name     - When spans show: "struct MyStruct" or "enum MyEnum"
#   file_only     - When spans show file-level issues
#
# Note: If your rule doesn't fit these parsers, you may need to add a new parser
# to the parse_violation() function below
#
# To add new rules:
# 1. Add rule below: "clippy::your_rule|violation_parser"
# 2. Generate baseline: ./scripts/clippy-baseline.sh generate clippy::your_rule

set -o pipefail

BASELINE_RULES=(
    "clippy::too_many_lines|function_name"
)

parse_violation() {
    local rule_code="$1"
    local violation_parser="$2"

    case "$violation_parser" in
        "function_name")
            jq -r --arg rule "$rule_code" '
                select(.reason == "compiler-message" and .message.code.code == $rule) |
                (.message.spans | map(select(.is_primary))[0] // .message.spans[0]) as $span |
                ([$span.text[]?.text] | join(" ")) as $source |
                ((try ($source | capture("\\bfn\\s+(?<name>[A-Za-z_][A-Za-z0-9_]*)").name)
                  catch null) // ("line-" + ($span.line_start | tostring))) as $name |
                "\($span.file_name)::\($name)"
            '
            ;;
        "type_name")
            jq -r --arg rule "$rule_code" '
                select(.reason == "compiler-message" and .message.code.code == $rule) |
                (.message.spans | map(select(.is_primary))[0] // .message.spans[0]) as $span |
                ([$span.text[]?.text] | join(" ")) as $source |
                ((try ($source | capture("\\b(?:struct|enum)\\s+(?<name>[A-Za-z_][A-Za-z0-9_]*)").name)
                  catch null) // ("line-" + ($span.line_start | tostring))) as $name |
                "\($span.file_name)::\($name)"
            '
            ;;
        "file_only")
            jq -r --arg rule "$rule_code" '
                select(.reason == "compiler-message" and .message.code.code == $rule) |
                (.message.spans | map(select(.is_primary))[0] // .message.spans[0]) |
                .file_name
            '
            ;;
        *)
            echo "Unknown violation parser: $violation_parser" >&2
            exit 1
            ;;
    esac
}

get_baseline_file() {
    local rule_name="$1"
    local safe_name=$(echo "$rule_name" | sed 's/clippy:://' | sed 's/:/-/g')
    echo "clippy-baselines/${safe_name}.txt"
}


generate_baseline() {
    local rule_name="$1"

    [[ -z "$rule_name" ]] && { echo "Missing rule name"; return 1; }

    local violation_parser=""
    for rule in "${BASELINE_RULES[@]}"; do
        [[ "${rule%|*}" == "$rule_name" ]] && { violation_parser="${rule#*|}"; break; }
    done

    [[ -z "$violation_parser" ]] && { echo "Unknown rule: $rule_name"; return 1; }

    local baseline_file=$(get_baseline_file "$rule_name")

    cargo clippy --jobs 2 --message-format=json -- -W "$rule_name" | \
        parse_violation "$rule_name" "$violation_parser" | \
        sort -u > "$baseline_file"

    echo "✅ Generated baseline for $rule_name ($(wc -l < "$baseline_file") violations)"
}


# Check a single rule from pre-generated JSON (optimized version)
check_rule_from_json() {
    local temp_json="$1"
    local rule_name="$2"
    local violation_parser="$3"
    local baseline_file="$4"

    echo "  → Checking $rule_name"

    if [[ ! -f "$baseline_file" ]]; then
        echo "  ❌ $rule_name: baseline file not found"
        return 1
    fi

    local temp_parsed=$(mktemp)
    parse_violation "$rule_name" "$violation_parser" < "$temp_json" | sort -u > "$temp_parsed"

    local new_violations_file=$(mktemp)
    comm -13 <(sort -u "$baseline_file") <(sort -u "$temp_parsed") > "$new_violations_file"

    if [[ -s "$new_violations_file" ]]; then
        echo "  ❌ $rule_name: NEW violations found:"

        while IFS= read -r violation; do
            # Extract all violations for this rule and find the matching one
            jq -c 'select(.message.code.code == "'"$rule_name"'")' "$temp_json" 2>/dev/null | while read -r json_line; do
                parsed_id=$(echo "$json_line" | parse_violation "$rule_name" "$violation_parser")
                if [[ "$parsed_id" == "$violation" ]]; then
                    echo "$json_line" | jq -r '.message.rendered' | sed 's/^/    /'
                    break
                fi
            done
        done < "$new_violations_file"

        rm "$temp_parsed" "$new_violations_file"
        return 1
    fi

    rm "$new_violations_file"

    echo "  ✅ $rule_name: ok"
    rm "$temp_parsed"
    return 0
}

check_all_baseline_rules() {
    echo "🔍 Checking baseline clippy rules..."

    local clippy_flags=""
    for rule in "${BASELINE_RULES[@]}"; do
        local rule_name="${rule%|*}"
        clippy_flags="$clippy_flags -W $rule_name"
    done

    local temp_json=$(mktemp)
    cargo clippy --jobs 2 --message-format=json -- $clippy_flags > "$temp_json"

    local failed_rules=()

    # Check each rule against its baseline
    for rule in "${BASELINE_RULES[@]}"; do
        local rule_name="${rule%|*}"
        local violation_parser="${rule#*|}"
        local baseline_file=$(get_baseline_file "$rule_name")

        if ! check_rule_from_json "$temp_json" "$rule_name" "$violation_parser" "$baseline_file"; then
            failed_rules+=("$rule_name")
        fi
    done

    rm "$temp_json"

    if [[ ${#failed_rules[@]} -gt 0 ]]; then
        echo ""
        echo "❌ Failed baseline checks for: ${failed_rules[*]}"
        exit 1
    else
        echo ""
        echo "✅ All baseline clippy checks passed!"
    fi
}

if [[ "${1:-}" == "generate" ]]; then
    generate_baseline "${2:-}"
fi
