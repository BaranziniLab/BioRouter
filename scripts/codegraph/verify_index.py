#!/usr/bin/env python3
"""Verify a CodeGraph index of this repo against ground truth.

Four independent checks, each able to fail a plausible-but-wrong index:

  1. COVERAGE     every tracked source file of a supported language is indexed,
                  and nothing failed to parse.
  2. SOUNDNESS    no `calls` edge crosses a Rust crate boundary that has no
                  Cargo dependency. Such an edge cannot exist in a compiling
                  workspace, so every hit is a resolver false positive.
  3. COMPLETENESS calls that name an in-repo function but failed to resolve.
  4. FP EXPOSURE  resolved calls whose target name is a stdlib/builtin method,
                  where a single unlucky in-repo definition absorbs them.

Usage: python3 scripts/codegraph/verify_index.py [repo-root]
Exit status is non-zero if a hard check (parse errors, coverage gap) fails.
"""
import collections
import os
import re
import sqlite3
import subprocess
import sys

STDLIB_NAMES = """map filter len push get set insert remove next into from clone
to_string iter collect contains join split trim parse unwrap expect is_empty
keys values entries find reduce sort slice replace then catch""".split()

LANG_BY_EXT = {"rs": "rust", "ts": "typescript", "tsx": "tsx", "py": "python"}


def main() -> int:
    root = sys.argv[1] if len(sys.argv) > 1 else subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True
    ).stdout.strip()
    db_path = os.path.join(root, ".codegraph", "codegraph.db")
    if not os.path.exists(db_path):
        print(f"no index at {db_path} — run 'codegraph init' first", file=sys.stderr)
        return 1
    db = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    q = lambda s, *a: db.execute(s, a).fetchall()
    one = lambda s, *a: db.execute(s, a).fetchone()[0]
    note = lambda k, v: print(f"  {k:<48} {v}")
    hard_fail = False

    print("== 1. COVERAGE ==")
    for ext, lang in LANG_BY_EXT.items():
        tracked = len(subprocess.run(
            ["git", "ls-files", f"*.{ext}"], cwd=root, capture_output=True, text=True
        ).stdout.split())
        indexed = one("SELECT COUNT(*) FROM files WHERE language=?", lang)
        gap = tracked - indexed
        note(f".{ext}  tracked={tracked} indexed={indexed}", "ok" if gap <= 0 else f"GAP {gap}")
        if gap > 0:
            hard_fail = True
    errs = one("SELECT COUNT(*) FROM files WHERE errors IS NOT NULL AND errors NOT IN ('','[]')")
    note("files with parse errors", errs)
    if errs:
        hard_fail = True

    print("\n== 2. SOUNDNESS: impossible cross-crate call edges ==")
    deps = {}
    crates_dir = os.path.join(root, "crates")
    for crate in sorted(os.listdir(crates_dir)):
        toml = os.path.join(crates_dir, crate, "Cargo.toml")
        if not os.path.isfile(toml):
            continue
        with open(toml) as fh:
            body = fh.read()
        deps[crate] = set(re.findall(r"^(biorouter[a-z-]*)\s*(?:=|\.)", body, re.M))

    pairs = collections.Counter()
    for sfile, tfile, n in q("""
        SELECT src.file_path, tgt.file_path, COUNT(*)
        FROM edges e JOIN nodes src ON src.id=e.source JOIN nodes tgt ON tgt.id=e.target
        WHERE e.kind='calls' AND src.language='rust' AND tgt.language='rust'
          AND src.file_path LIKE 'crates/%' AND tgt.file_path LIKE 'crates/%'
        GROUP BY 1,2"""):
        sc, tc = sfile.split("/")[1], tfile.split("/")[1]
        if sc != tc and tc not in deps.get(sc, set()):
            pairs[(sc, tc)] += n

    total_rust = one("""SELECT COUNT(*) FROM edges e JOIN nodes s ON s.id=e.source
                        WHERE e.kind='calls' AND s.language='rust'""")
    for (sc, tc), n in pairs.most_common(10):
        print(f"  IMPOSSIBLE  {sc} -> {tc}   {n} edges")
    bad = sum(pairs.values())
    note("impossible cross-crate call edges", f"{bad} of {total_rust} rust calls "
         f"({100.0*bad/total_rust:.1f}%)")

    print("\n== 3. COMPLETENESS: calls naming an in-repo fn that failed to resolve ==")
    misses = one("""SELECT COUNT(*) FROM unresolved_refs u WHERE u.reference_kind='calls'
        AND EXISTS (SELECT 1 FROM nodes n WHERE n.name=u.reference_name
                    AND n.kind IN ('function','method'))""")
    allun = one("SELECT COUNT(*) FROM unresolved_refs WHERE reference_kind='calls'")
    resolved = one("SELECT COUNT(*) FROM edges WHERE kind='calls'")
    note("unresolved calls naming an in-repo fn", f"{misses} of {allun} unresolved")
    note("resolution rate over the resolvable universe",
         f"{100.0*resolved/(resolved+misses):.1f}%")

    print("\n== 4. FP EXPOSURE: resolved calls onto stdlib-shaped names ==")
    marks = ",".join("?" * len(STDLIB_NAMES))
    risk = one(f"""SELECT COUNT(*) FROM edges e JOIN nodes t ON t.id=e.target
                   WHERE e.kind='calls' AND t.name IN ({marks})""", *STDLIB_NAMES)
    note("calls resolved onto a stdlib-shaped name", f"{risk} of {resolved} "
         f"({100.0*risk/resolved:.1f}%)")
    print("  worst absorbers (one in-repo def soaking up many calls):")
    for name, n, defs in q(f"""
        SELECT * FROM (
          SELECT t.name AS nm, COUNT(*) AS c,
                 (SELECT COUNT(*) FROM nodes n2 WHERE n2.name=t.name
                  AND n2.kind IN ('function','method')) AS defs
          FROM edges e JOIN nodes t ON t.id=e.target
          WHERE e.kind='calls' AND t.name IN ({marks})
          GROUP BY t.name
        ) WHERE defs = 1 ORDER BY c DESC LIMIT 8""", *STDLIB_NAMES):
        print(f"    {name:<12} {n:>5} calls -> {defs} in-repo def")

    print()
    print("verify_index: " + ("FAIL" if hard_fail else "PASS"))
    return 1 if hard_fail else 0


if __name__ == "__main__":
    sys.exit(main())
