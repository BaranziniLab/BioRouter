#!/usr/bin/env bash
#
# Issue #56 Task 30, Step 5 — the structural half of the master toggle's gate.
#
# The BEHAVIOURAL gate is the matrix in `crates/biorouter/tests/privacy_toggle.rs`
# (plus its three siblings, listed in that file's header). This catches the two
# failures a matrix test cannot see: an enforcement point added later with no row,
# and a gate nobody wired at all.
#
#   ./scripts/privacy-toggle-inventory.sh          # verify against the pinned lists
#   ./scripts/privacy-toggle-inventory.sh --print  # print the observed lists
#
# ⚠ COMMITTED, and that is the point. Task 30 specified this gate as a block of
# shell in a plan document. It was run once, by hand, and two reviewers correctly
# observed that a mechanism which exists only in someone's terminal pins nothing.
# Running it from source also surfaced two defects in the plan's own text, both
# recorded below where they were fixed.

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-verify}"
rc=0
fail() { echo "FAIL  $*"; rc=1; }
ok() { echo "ok    $*"; }

# ─────────────────────────────────────────────────────────────────────────────
# (1) The toggle is never read through `Config::get_param`, whose middle branch
#     resolves an environment variable that an agent holding `developer__shell`
#     can set. The reader is the config LOADER, so this is asserted on both: the
#     loader uses `all_values()`, and neither it nor the predicate touches
#     `get_param`.
# ─────────────────────────────────────────────────────────────────────────────
# (0) The panel is MOUNTED. A settings screen that is written and never reached
#     is the state `settings/security/SecurityToggle.tsx` is in today — declared,
#     plausible, zero consumers repo-wide.
tabs=$(grep -c 'data-testid="settings-.*-tab"' "$ROOT/ui/desktop/src/components/settings/SettingsView.tsx")
[ "$tabs" -eq 4 ] || fail "expected 4 settings tabs (3 before Privacy), found $tabs"
grep -q 'settings-privacy-tab' "$ROOT/ui/desktop/src/components/settings/SettingsView.tsx" \
  || fail "SettingsView has no Privacy tab"

loader() { awk '/fn load_privacy_tiers_from_config/,/^}/' "$ROOT/crates/biorouter/src/privacy/mod.rs"; }
[ "$(loader | grep -c 'get_param')" -eq 0 ] \
  || fail "the loader reads the toggle through Config::get_param"
[ "$(loader | grep -c 'all_values()')" -eq 1 ] \
  || fail "the loader must read all_values() exactly once"
[ "$(awk '/fn privacy_tiers_enabled/,/^}/' "$ROOT/crates/biorouter-mcp/src/privacy_toggle.rs" \
     | grep -c 'get_param\|env')" -eq 0 ] \
  || fail "the storage crate must know nothing about config"

# ─────────────────────────────────────────────────────────────────────────────
# The scan. Enumerates `(file, enclosing fn)` pairs for a pattern, which is what
# makes this stronger than a per-file count: three calls in one file is
# satisfiable by three calls in ONE function.
#
# ⚠ Three details, every one found by RUNNING this and not by reading it.
#
# (a) The patterns escape `(` as `[(]`, never `\(`: a dynamic regex reaches awk
#     as a STRING, and BSD awk rejects `\(` with "illegal primary in regular
#     expression" — on every file, so the whole scan produces nothing and every
#     check passes vacuously.
#
# (b) `#[cfg(test)] mod` blocks are skipped, or Task 12's own tests fail this:
#     they construct `PrivacyRefusal` and quite rightly do not read the toggle.
#     The skip triggers on `#[cfg(test)]` FOLLOWED BY a `mod` line, not on the
#     attribute alone — `update_provider` carries a `#[cfg(test)]` on a
#     statement, and stopping there would blind the rest of `agent.rs`.
#
# (c) **That skip has to END.** The plan's version set the flag and never
#     cleared it, so a file was invisible from its FIRST `#[cfg(test)] mod`
#     onward. `agents/agent.rs` has a nested one at line 422 and its four toggle
#     reads at 3323, 3545, 4753 and 7377 — Gate B's turn barrier, Gate B'`s
#     assertion in `provider()`, DR-4's ratchet and the cached-classification
#     skip. All four were invisible to the gate that exists to find them, which
#     is precisely the "wired to some of the gates" failure Step 5 was written
#     to reject, one level up. A column-0 `}` closes the enclosing top-level
#     item, so it is where the skip ends: rustfmt indents everything inside a
#     `mod`, so the only unindented `}` in a test module is its own last line.
#
# (d) **A `mod foo;` DECLARATION has no body to skip**, and treating it as one is
#     the same defect as (c) wearing a different hat. `crates/biorouter/src/
#     privacy/mod.rs` declares `#[cfg(test)] mod disclosure_tests;` in its module
#     list, thirty lines above `load_privacy_tiers_from_config` — so the skip
#     swallowed the loader and this script reported ONE writer of the toggle
#     where the tree has two, i.e. it lost sight of the startup load, which is
#     the more important of the two. `providers/mod.rs` has had the identical
#     shape (`#[cfg(test)] mod tier_tests;`) since Task 5 and was blinding
#     everything below it silently, because that file happens to hold nothing
#     this scan looks for. The external file is scanned in its own right, so
#     nothing is lost by not skipping the declaration; what the skip is FOR is an
#     inline `mod tests { … }`, and those do not end in a semicolon.
#
# (e) **The skip has to DISARM as well as end**, which is (c) once more at one
#     line's remove. The declaration branch consumed the `mod …;` line and left
#     `pending` set, so the NEXT `mod` line — an ordinary, inline, non-test one —
#     was skipped as though the attribute had been on it, taking everything
#     inside it with it. `privacy/mod.rs` and `providers/mod.rs` both happen to
#     follow their declaration lists with something other than a `mod` line, so
#     the tree does not reach it today; that is luck, not a property. The same
#     armed-forever shape is why a `#[cfg(test)] mod x;` written on ONE line no
#     longer arms the skip at all: it has no body, and the `mod` after it is not
#     its business.
# ─────────────────────────────────────────────────────────────────────────────
scan() {
  grep -rlE "$1" --include='*.rs' "$ROOT"/crates/*/src/ 2>/dev/null | while read -r f; do
    awk -v F="${f#"$ROOT"/}" -v RE="$1" '
      intest && /^\}/ { intest = 0; next }
      intest { next }
      pending && /^[[:space:]]*(pub[^ ]*[[:space:]]+)?mod[[:space:]]/ {
        if ($0 !~ /;[[:space:]]*$/) intest = 1
        pending = 0
        next
      }
      { pending = ($0 ~ /^[[:space:]]*#\[cfg\(test\)\]/ && $0 !~ /;[[:space:]]*$/) }
      /^[[:space:]]*(pub([(][a-z()]+[)])?[[:space:]]+)?(async[[:space:]]+)?fn[[:space:]]+[A-Za-z_]/ {
        match($0, /fn[[:space:]]+[A-Za-z_][A-Za-z0-9_]*/)
        cur = substr($0, RSTART, RLENGTH); sub(/^fn[[:space:]]+/, "", cur)
      }
      $0 ~ RE && cur != "" { print F "\t" cur }
    ' "$f"
  done | sort -u
}

# ─────────────────────────────────────────────────────────────────────────────
# (2) There is exactly ONE predicate and exactly ONE writer. A second, narrower
#     flag is how a scoped opt-out comes back through the side door, and it
#     would pass the behavioural matrix as long as the master one is wired
#     everywhere the matrix looks.
#
#     ⚠ Through `scan`, not a bare grep. The plan's text greps `crates/*/src/`
#     and expects 2; the storage module's own `the_setter_round_trips` test
#     names the setter three more times from inside a `#[cfg(test)] mod`, so a
#     bare grep reports 5 and "expect: exactly 2" reads as a failure that is
#     not one. The plan's own note concedes the scan "excludes `#[cfg(test)]`
#     -gated helpers only if this lives in a test module" — it does; the counter
#     just has to know that too.
# ─────────────────────────────────────────────────────────────────────────────
second_predicate=$(grep -rn "fn privacy_.*_enabled\|PRIVACY_.*ENFORCEMENT\|privacy_opt_out" \
  --include='*.rs' "$ROOT/crates/" | grep -v "fn privacy_tiers_enabled" | grep -v "privacy_tiers_enabled()")
[ -z "$second_predicate" ] || fail "a second privacy predicate exists:
$second_predicate"

writers=$(scan 'set_privacy_tiers_enabled[(]' | grep -v 'privacy_toggle\.rs')
writer_n=$(printf '%s\n' "$writers" | grep -c . )
if [ "$writer_n" -eq 2 ]; then
  ok "exactly two writers — the startup load and /config/upsert's gated arm"
else
  fail "expected 2 writers of the toggle, found $writer_n:
$writers"
fi

# ─────────────────────────────────────────────────────────────────────────────
# (3a) THE INVENTORY, split across the TWO axes the toggle can travel on.
#
# ⚠ WHY TWO LISTS. `CallCapability::sample` reads the provider tier and the
#   master toggle together, at one instant, and threads the pair. So an
#   enforcement point on a TOOL-CALL path must NOT read the toggle again — a
#   second read is the defect that lets a call pass Gate C with tiers on and
#   then relax a later barrier with tiers off. Those ask `cap.enforced()` /
#   `cap.restricts_private_data()` instead, and are list B.
#
#   The gates that are NOT on a tool-call path have no `CallCapability` to
#   consult — a provider bind, a turn, a spawn, a search, an HTTP route — and
#   each reads the toggle itself. Those are list A.
#
#   A gate that demands a direct toggle read everywhere would FAIL the design
#   this feature actually has, and the pressure it applies is toward re-adding
#   the second read. That is worse than no gate.
#
# ⚠ `privacy/visibility.rs::visible_to` is on NEITHER list. It is a PURE
#   predicate over (caller tier, target classification); the toggle is folded in
#   at each caller through `cap.enforced()`. Left reading the toggle it would be
#   a second read on `chatrecall`'s path. Its row in the behavioural matrix
#   stays — asserted through a caller, which is row 7.
# ─────────────────────────────────────────────────────────────────────────────
want_a() { cat <<'ROWS'
crates/biorouter-mcp/src/agent_drafter/catalog.rs	discover_kbs
crates/biorouter-mcp/src/knowledge/server.rs	kb_export
crates/biorouter-mcp/src/knowledge/tier.rs	assert_reachable
crates/biorouter-mcp/src/knowledge/tier.rs	raise_unlocked
crates/biorouter-server/src/routes/agent.rs	agent_add_extension
crates/biorouter-server/src/routes/agent.rs	update_agent_provider
crates/biorouter-server/src/routes/config_management.rs	remove_config
crates/biorouter-server/src/routes/config_management.rs	set_config_provider
crates/biorouter-server/src/routes/config_management.rs	upsert_config
crates/biorouter/src/agents/subagent_tool.rs	apply_settings_overrides
crates/biorouter/src/knowledge/conversation_ingest.rs	readable
crates/biorouter/src/privacy/alt_provider.rs	assert_alt_provider_allowed
crates/biorouter/src/privacy/capability.rs	sample
crates/biorouter/src/session/chat_history_search.rs	filters_private_sessions
crates/biorouter/src/session/session_manager.rs	bind_provider_if_allowed
ROWS
}
# `agent.rs`'s four, in their own block because they are the ones the plan's
# scan could not see at all (see the scan's note (c)): Gate B's turn barrier and
# DR-4's ratchet in `reply`, Gate B''s assertion in `provider`, the
# cached-classification skip in `update_provider`, and the eager-compaction skip.
want_a_extra() { cat <<'ROWS'
crates/biorouter/src/agents/agent.rs	maybe_spawn_eager_compaction
crates/biorouter/src/agents/agent.rs	provider
crates/biorouter/src/agents/agent.rs	reply
crates/biorouter/src/agents/agent.rs	update_provider
ROWS
}
# ⚠ `extension_manager.rs::get_extensions_info` is NOT a row, and the plan's
#   list B had it. It filters by `self.allowed_extension_keys(None)` — the row
#   directly above — so it holds no capability of its own and asks for none.
#   Adding it would demand a second read on a path that already has one answer.
#   Gates E and F2 are asserted behaviourally through it all the same (matrix
#   row 4/5), which is the difference between what this inventory pins and what
#   the matrix pins.
want_b() { cat <<'ROWS'
crates/biorouter/src/agents/chatrecall_extension.rs	handle_chatrecall
crates/biorouter/src/agents/extension_manager.rs	allowed_extension_keys
crates/biorouter/src/agents/extension_manager.rs	assert_extension_reachable
crates/biorouter/src/agents/extension_manager.rs	dispatch_tool_call
crates/biorouter/src/agents/extension_manager_extension.rs	check_enable_allowed
ROWS
}

have_a=$(scan 'privacy_tiers_enabled[(][)]' | grep -v '/privacy/mod\.rs' | grep -v '/privacy_toggle\.rs')
have_b=$(scan '[.](enforced|restricts_private_data)[(][)]' | grep -v '/privacy/capability\.rs')

if [ "$MODE" = "--print" ]; then
  echo "== list A (direct toggle reads, off the tool-call path) =="; echo "$have_a"
  echo "== list B (capability consumers, on the tool-call path) =="; echo "$have_b"
  exit 0
fi

want_a_all=$( { want_a; want_a_extra; } | sort -u )
if diff -u <(echo "$want_a_all") <(echo "$have_a"); then
  ok "exactly the off-the-tool-path gates read the toggle, and nothing else does"
else
  fail "the toggle's call sites and list A disagree.
      A '-' line is a list-A row whose gate does NOT read the toggle: that gate
      stays armed when the user turns the feature off. A '+' line is EITHER a new
      off-path enforcement point with no row, OR — far more likely, and the reason
      this split exists — a SECOND read of the toggle on a tool-call path, which
      reintroduces the sample-twice race. Add a row, or delete the read. Do not
      delete the '+' line from this list to make it pass."
fi
if diff -u <(want_b | sort -u) <(echo "$have_b"); then
  ok "every on-path gate decides from the capability it was handed"
else
  fail "the capability consumers and list B disagree. A '-' row is an on-path gate
      that decides some other way — check it is not re-reading a tier."
fi

# ─────────────────────────────────────────────────────────────────────────────
# (3b) THE CLOSURE THAT CATCHES A GATE NOBODY WIRED AT ALL. (3a) can only see a
#      gate that already reads the toggle. This starts from the other end: every
#      place that REFUSES on privacy grounds must sit inside a function that
#      reads the toggle or holds a capability. A new gate is, by definition, a
#      new refusal.
#
# ⚠ `tier::assert_reachable[(]` is NOT in this pattern, and the plan's text had
#   it. `assert_reachable` is itself a list-A row — it reads the toggle — so its
#   six CALL SITES (the three knowledge macros, `assert_kb_reachable`,
#   `stage_full_payload`, `assert_macro_target_reachable`, `handle_kb_frame`) are
#   governed transitively, and requiring them to read the toggle AGAIN is exactly
#   the double-read this feature's design forbids. With the pattern in, the gate
#   failed a correct tree with six false positives — the "gate that rejects the
#   correct implementation" failure class Task 30's own preamble is about.
# ─────────────────────────────────────────────────────────────────────────────
REFUSE='PrivacyRefusal::|privacy::refusal::|refusal::privacy_refusal[(]|path_policy::refusal[(]|PrivatePathPolicy::|privacy::visibility::'

# Sites that NAME a refusal without being one. Each is checked to still match,
# so a stale exemption is itself a failure rather than a silent widening.
exempt() { cat <<'ROWS'
crates/biorouter-server/src/routes/apps.rs	restore_route_provider
crates/biorouter/src/agents/subagent_tool.rs	dropped_extension_note
ROWS
}
# restore_route_provider  — `downcast_ref::<PrivacyRefusal>()` on an error
#                           `update_provider` already produced under the toggle.
#                           It CLASSIFIES a refusal; it does not make one.
# dropped_extension_note  — interpolates the shared `ASK_THE_USER_TO_SWITCH`
#                           wording into a note. The decision was made by the
#                           `partition` in `apply_settings_overrides`, which is a
#                           list-A row; this only says what happened.

refuse_sites=$(scan "$REFUSE" | grep -vE '/privacy/(refusal|path_policy|visibility)\.rs|/knowledge/tier\.rs')
governed=$( { echo "$have_a"; echo "$have_b"; } | sort -u )
unguarded=$(comm -23 <(echo "$refuse_sites") <(echo "$governed") | comm -23 - <(exempt | sort -u))
if [ -n "$unguarded" ]; then
  fail "these refuse on privacy grounds while consulting NEITHER the master toggle
      nor a CallCapability:
$unguarded
      Each is an enforcement point the user cannot turn off. Wire it to one axis
      or the other, then add its row to list A or list B AND to the behavioural
      matrix."
else
  ok "every privacy refusal is governed by the toggle or by a capability"
fi
stale=$(comm -13 <(echo "$refuse_sites") <(exempt | sort -u))
[ -z "$stale" ] || fail "these exemptions no longer match anything — delete them:
$stale"

# ─────────────────────────────────────────────────────────────────────────────
# (4) The badge is not conditionally unmounted. The wrong implementation is
#     `{enforcementOn && <PrivacyBadge …/>}` at a call site — which no badge
#     unit test can see, because the badge itself is fine.
#
# ⚠ A bare `&&\s*<PrivacyBadge` grep, as the plan's text has it, reports six
#   hits on a CORRECT tree: every one of them guards on the TIER (`{privacyTier
#   && <PrivacyBadge tier={privacyTier} …/>}`), i.e. "the daemon has not told us
#   a tier, so there is nothing to show" — not "enforcement is off, so hide it".
#   The check therefore requires the guard expression to be the same expression
#   passed as `tier`; anything else is the failure it is looking for.
# ─────────────────────────────────────────────────────────────────────────────
bad_guards=$(grep -rnE "(&&|\?)[[:space:]]*<PrivacyBadge" "$ROOT/ui/desktop/src/" --include='*.tsx' \
  | grep -vE '([A-Za-z_][A-Za-z0-9_.]*)[[:space:]]*(&&|\?)[[:space:]]*<PrivacyBadge[[:space:]]+tier=\{\1[[:space:]}]')
if [ -n "$bad_guards" ]; then
  fail "a call site decides whether the badge appears on something other than the
      tier itself — the badge must render unconditionally and vary its own
      presentation:
$bad_guards"
else
  ok "no call site hides the badge on anything but the absence of a tier"
fi

[ "$rc" -eq 0 ] && echo "Task 30 toggle inventory: PASS" || echo "Task 30 toggle inventory: FAIL"
exit "$rc"
