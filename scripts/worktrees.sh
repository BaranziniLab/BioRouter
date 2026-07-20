#!/usr/bin/env bash
# Save/restore this clone's git-worktree layout, so the same set of worktrees
# can be recreated on another machine from the pushed branches.
#
#   scripts/worktrees.sh save      # snapshot the current layout into scripts/worktrees.manifest
#   scripts/worktrees.sh restore   # recreate every missing worktree listed in the manifest
#
# Manifest format: one "path<TAB>branch" line per secondary worktree. Paths
# inside the repo are stored relative to the repo root, paths under $HOME as
# ~/..., anything else verbatim (e.g. /private/tmp scratch checkouts, which
# macOS clears on reboot -- restore simply recreates them). The primary
# checkout is never listed; detached worktrees are skipped.
set -euo pipefail

# The main worktree is always listed first, regardless of where we run from.
top="$(git worktree list --porcelain | head -n 1 | sed 's/^worktree //')"
manifest="${WORKTREES_MANIFEST:-$top/scripts/worktrees.manifest}"

save() {
  local path="" branch="" tmp
  tmp="$(mktemp)"
  while IFS= read -r line; do
    case "$line" in
      "worktree "*) path="${line#worktree }" ;;
      "branch refs/heads/"*) branch="${line#branch refs/heads/}" ;;
      "")
        if [ -n "$path" ] && [ -n "$branch" ] && [ "$path" != "$top" ]; then
          case "$path" in
            "$top"/*) path="${path#"$top"/}" ;;
            "$HOME"/*) path="~/${path#"$HOME"/}" ;;
          esac
          printf '%s\t%s\n' "$path" "$branch" >>"$tmp"
        fi
        path="" branch=""
        ;;
    esac
  done < <(git worktree list --porcelain; echo)
  mv "$tmp" "$manifest"
  echo "saved $(wc -l <"$manifest" | tr -d ' ') worktree(s) to $manifest:"
  sed 's/^/  /' "$manifest"
}

restore() {
  [ -f "$manifest" ] || { echo "no manifest at $manifest" >&2; exit 1; }
  git fetch origin --prune
  local path branch dest failed=0
  while IFS=$'\t' read -r path branch; do
    [ -n "$path" ] && [ -n "$branch" ] || continue
    case "$path" in
      "~/"*) dest="$HOME/${path#\~/}" ;;
      /*) dest="$path" ;;
      *) dest="$top/$path" ;;
    esac
    if [ -e "$dest" ]; then
      echo "skip (exists): $dest"
      continue
    fi
    if ! git show-ref --verify -q "refs/heads/$branch"; then
      git branch --track "$branch" "origin/$branch"
    fi
    if git worktree add "$dest" "$branch"; then
      echo "restored: $dest -> $branch"
    else
      echo "FAILED: $dest -> $branch (branch may be checked out elsewhere)" >&2
      failed=$((failed + 1))
    fi
  done <"$manifest"
  [ "$failed" -eq 0 ] || exit 1
}

case "${1:-}" in
  save) save ;;
  restore) restore ;;
  *) echo "usage: scripts/worktrees.sh {save|restore}" >&2; exit 2 ;;
esac
