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
Exit status is non-zero for parse errors, exact-path coverage drift, or a
resolver-risk metric above its checked-in regression ceiling.
"""
import collections
import fnmatch
import json
import os
import re
import sqlite3
import subprocess
import sys

STDLIB_NAMES = """map filter len push get set insert remove next into from clone
to_string iter collect contains join split trim parse unwrap expect is_empty
keys values entries find reduce sort slice replace then catch""".split()

LANG_EXTENSIONS = {
    "rust": (".rs",),
    "typescript": (".ts", ".mts", ".cts"),
    "tsx": (".tsx",),
    "javascript": (".js", ".mjs", ".cjs"),
    "python": (".py",),
    "yaml": (".yaml", ".yml"),
    "swift": (".swift",),
    "xml": (".xml",),
}

# These baselines record CodeGraph 1.5.0 on this commit. The ceilings allow
# ordinary source growth while still turning material resolver drift into a
# failing check. Lower both when CodeGraph improves; raise a ceiling only after
# reviewing the newly introduced false-positive or unresolved edges.
BASELINE_IMPOSSIBLE_RUST_CALL_RATIO = 0.1115
BASELINE_RESOLVABLE_MISS_RATIO = 0.0267
BASELINE_STDLIB_TARGET_RATIO = 0.1620
MAX_IMPOSSIBLE_RUST_CALL_RATIO = 0.120
MAX_RESOLVABLE_MISS_RATIO = 0.030
MAX_STDLIB_TARGET_RATIO = 0.170


def pattern_matches(path: str, pattern: str) -> bool:
    """Match the gitignore-style forms used by codegraph.json."""
    pattern = pattern.lstrip("/")
    if pattern.endswith("/"):
        directory = pattern.rstrip("/")
        return path == directory or path.startswith(directory + "/")
    if pattern.startswith("**/"):
        return fnmatch.fnmatchcase(path, pattern) or fnmatch.fnmatchcase(path, pattern[3:])
    if "/" not in pattern:
        return any(fnmatch.fnmatchcase(part, pattern) for part in path.split("/"))
    return fnmatch.fnmatchcase(path, pattern)


def load_excludes(root: str) -> list[str]:
    config_path = os.path.join(root, "codegraph.json")
    with open(config_path, encoding="utf-8") as fh:
        config = json.load(fh)
    excludes = config.get("exclude", [])
    if not isinstance(excludes, list) or not all(isinstance(item, str) for item in excludes):
        raise ValueError("codegraph.json 'exclude' must be a list of strings")
    return excludes


def is_excluded(path: str, patterns: list[str]) -> bool:
    excluded = False
    for raw_pattern in patterns:
        negated = raw_pattern.startswith("!")
        pattern = raw_pattern[1:] if negated else raw_pattern
        if pattern_matches(path, pattern):
            excluded = not negated
    return excluded


def tracked_source_paths(root: str, excludes: list[str]) -> dict[str, set[str]]:
    tracked = subprocess.run(
        ["git", "ls-files", "-z"], cwd=root, capture_output=True, check=True
    ).stdout.decode("utf-8").split("\0")
    ignored = set(
        subprocess.run(
            ["git", "ls-files", "-ci", "--exclude-standard", "-z"],
            cwd=root,
            capture_output=True,
            check=True,
        ).stdout.decode("utf-8").split("\0")
    )
    expected = {language: set() for language in LANG_EXTENSIONS}
    for path in tracked:
        if not path or path in ignored or is_excluded(path, excludes):
            continue
        lowered = path.lower()
        for language, extensions in LANG_EXTENSIONS.items():
            if lowered.endswith(extensions):
                expected[language].add(path)
                break
    return expected


def check_ratio(note, label: str, numerator: int, denominator: int,
                baseline: float, ceiling: float) -> bool:
    if denominator <= 0:
        note(label, "FAIL: denominator is zero")
        return False
    ratio = numerator / denominator
    status = "ok" if ratio <= ceiling else "REGRESSION"
    note(label, f"{numerator} of {denominator} ({100.0 * ratio:.2f}%; "
         f"baseline {100.0 * baseline:.2f}%; "
         f"ceiling {100.0 * ceiling:.2f}%) {status}")
    return ratio <= ceiling


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
    expected = tracked_source_paths(root, load_excludes(root))
    indexed_languages = {row[0] for row in q("SELECT DISTINCT language FROM files")}
    unknown_languages = sorted(indexed_languages - LANG_EXTENSIONS.keys())
    if unknown_languages:
        note("indexed languages without a path rule", ", ".join(unknown_languages))
        hard_fail = True
    for lang in LANG_EXTENSIONS:
        indexed = {row[0] for row in q("SELECT path FROM files WHERE language=?", lang)}
        missing = sorted(expected[lang] - indexed)
        unexpected = sorted(indexed - expected[lang])
        note(f"{lang} expected={len(expected[lang])} indexed={len(indexed)}",
             "ok" if not missing and not unexpected else
             f"MISSING {len(missing)} UNEXPECTED {len(unexpected)}")
        for path in missing[:10]:
            print(f"    MISSING     {path}")
        for path in unexpected[:10]:
            print(f"    UNEXPECTED  {path}")
        if missing or unexpected:
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
    if not check_ratio(note, "impossible cross-crate call edges", bad, total_rust,
                       BASELINE_IMPOSSIBLE_RUST_CALL_RATIO,
                       MAX_IMPOSSIBLE_RUST_CALL_RATIO):
        hard_fail = True

    print("\n== 3. COMPLETENESS: calls naming an in-repo fn that failed to resolve ==")
    misses = one("""SELECT COUNT(*) FROM unresolved_refs u WHERE u.reference_kind='calls'
        AND EXISTS (SELECT 1 FROM nodes n WHERE n.name=u.reference_name
                    AND n.kind IN ('function','method'))""")
    allun = one("SELECT COUNT(*) FROM unresolved_refs WHERE reference_kind='calls'")
    resolved = one("SELECT COUNT(*) FROM edges WHERE kind='calls'")
    note("unresolved calls naming an in-repo fn", f"{misses} of {allun} unresolved")
    if not check_ratio(note, "resolvable calls left unresolved", misses, resolved + misses,
                       BASELINE_RESOLVABLE_MISS_RATIO,
                       MAX_RESOLVABLE_MISS_RATIO):
        hard_fail = True

    print("\n== 4. FP EXPOSURE: resolved calls onto stdlib-shaped names ==")
    marks = ",".join("?" * len(STDLIB_NAMES))
    risk = one(f"""SELECT COUNT(*) FROM edges e JOIN nodes t ON t.id=e.target
                   WHERE e.kind='calls' AND t.name IN ({marks})""", *STDLIB_NAMES)
    if not check_ratio(note, "calls resolved onto a stdlib-shaped name", risk, resolved,
                       BASELINE_STDLIB_TARGET_RATIO,
                       MAX_STDLIB_TARGET_RATIO):
        hard_fail = True
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
