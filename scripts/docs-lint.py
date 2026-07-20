#!/usr/bin/env python3
"""Mechanical conformance checks for the docs/ tree.

Encodes the rules in docs/organization.md and docs/contributing/documentation-style.md
that can be checked without judgement. Judgement calls — whether a status sentence is
honest, whether a folder is the right topic — are deliberately out of scope.

Usage:
    python3 scripts/docs-lint.py            # human summary, exits non-zero on findings
    python3 scripts/docs-lint.py --json     # machine-readable, for an agent to consume
    python3 scripts/docs-lint.py --folder docs/cli   # restrict to one folder
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from collections import defaultdict

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DOCS = os.path.join(REPO, "docs")

# docs/organization.md §8: these two are the only files allowed loose at the root.
ROOT_ALLOWED = {"README.md", "organization.md"}

# Folders whose files are exempt from the context header, per the style guide §1.
# releases/notes/ is a dated series framed by its folder index.
HEADER_EXEMPT_DIRS = {"docs/releases/notes"}
HEADER_EXEMPT_FILES = {
    "docs/contributing/documentation-style.md",  # names itself in prose, per §1
    "docs/organization.md",                      # ditto
}

STATUS_RE = re.compile(r"^>\s*\*\*Status:\*\*\s*(.+)$", re.M)
WHAT_RE = re.compile(r"^>\s*\*\*What this is\.\*\*", re.M)
AUDIENCE_RE = re.compile(r"^>\s*\*\*Audience:\*\*", re.M)
H1_RE = re.compile(r"^#\s+(.+)$", re.M)
FENCE_RE = re.compile(r"^([ \t]*)```([A-Za-z0-9_+-]*)\s*$", re.M)
# [text](target) — skip images and external/anchor-only links
LINK_RE = re.compile(r"(?<!!)\[([^\]]*)\]\(([^)]+)\)")


def rel(p: str) -> str:
    return os.path.relpath(p, REPO)


def strip_fences(text: str) -> str:
    """Blank out fenced code blocks, preserving line numbering.

    Everything the style guide says about headings, links and prose applies to
    prose. A `#` shell comment or a `[x](y)` example inside a fence is content,
    not a heading or a link, and "fixing" it would edit a code sample — which
    the style guide explicitly forbids.
    """
    out, in_fence, fence = [], False, ""
    for line in text.split("\n"):
        m = re.match(r"^[ \t]*(`{3,}|~{3,})", line)
        if m:
            tok = m.group(1)[0] * 3
            if not in_fence:
                in_fence, fence = True, tok
                out.append(line)          # keep the opening fence for the language check
                continue
            if tok == fence:
                in_fence = False
                out.append("")
                continue
        out.append("" if in_fence else line)
    return "\n".join(out)


def all_md() -> list[str]:
    out = []
    for root, dirs, files in os.walk(DOCS):
        dirs[:] = [d for d in dirs if not d.startswith(".")]
        for fn in sorted(files):
            if fn.endswith(".md"):
                out.append(os.path.join(root, fn))
    return sorted(out)


def check_file(path: str, findings: list[dict]) -> None:
    r = rel(path)
    d = os.path.dirname(r)
    name = os.path.basename(r)
    try:
        text = open(path, encoding="utf-8").read()
    except OSError as e:
        findings.append({"file": r, "rule": "unreadable", "detail": str(e)})
        return

    is_readme = name == "README.md"
    header_exempt = d in HEADER_EXEMPT_DIRS or r in HEADER_EXEMPT_FILES
    # Heading, link and section checks read prose only; the fence check below
    # deliberately still runs against the raw text.
    prose = strip_fences(text)

    # --- naming (organization.md §4, style §3) ---
    # Release notes are a version-named dated series, called out as an exception
    # in the style guide §1: the version IS the name, and `v1.88.3.md` carries
    # more information than any kebab-case rewrite of it could.
    is_release_note = d == "docs/releases/notes" and re.fullmatch(r"v\d+\.\d+\.\d+\.md", name)
    if name != "README.md" and not is_release_note:
        if not re.fullmatch(r"[a-z0-9]+(-[a-z0-9]+)*\.md", name):
            findings.append({"file": r, "rule": "naming/kebab-case",
                             "detail": f"{name} is not kebab-case.md"})

    # --- H1 (style §2) ---
    h1s = H1_RE.findall(prose)
    if not h1s:
        findings.append({"file": r, "rule": "h1/missing", "detail": "no `# H1`"})
    elif len(h1s) > 1:
        findings.append({"file": r, "rule": "h1/multiple",
                         "detail": f"{len(h1s)} H1s: {h1s[:3]}"})

    # --- context header (style §1) ---
    if not is_readme and not header_exempt:
        if not WHAT_RE.search(prose):
            findings.append({"file": r, "rule": "header/what-this-is",
                             "detail": "missing `> **What this is.**`"})
        if not STATUS_RE.search(prose):
            findings.append({"file": r, "rule": "header/status",
                             "detail": "missing `> **Status:**`"})
        if not AUDIENCE_RE.search(prose):
            findings.append({"file": r, "rule": "header/audience",
                             "detail": "missing `> **Audience:**`"})

    # --- status vocabulary + folder agreement (organization.md §1) ---
    m = STATUS_RE.search(prose)
    if m:
        status = m.group(1).strip()
        low = status.lower()
        kind = ("current" if low.startswith("current")
                else "historical" if "historical record" in low
                else "superseded" if low.startswith("superseded")
                else None)
        if kind is None:
            findings.append({"file": r, "rule": "status/vocabulary",
                             "detail": f"not one of Current/Historical record/Superseded: {status[:70]}"})
        in_history = r.startswith("docs/history/")
        if kind == "historical" and not in_history and not is_readme:
            findings.append({"file": r, "rule": "status/folder-disagreement",
                             "detail": "marked `Historical record` but not under docs/history/"})
        if kind == "current" and in_history and not is_readme:
            findings.append({"file": r, "rule": "status/folder-disagreement",
                             "detail": "marked `Current` but lives under docs/history/"})

    # --- fenced code blocks carry a language (style §5) ---
    for mm in FENCE_RE.finditer(text):
        # only opening fences: count fences before this one
        before = text[: mm.start()].count("```")
        if before % 2 == 0 and not mm.group(2):
            line = text[: mm.start()].count("\n") + 1
            findings.append({"file": r, "rule": "code/no-language",
                             "detail": f"unlabelled ``` at line {line}"})

    # --- Related documentation closer (style §1) ---
    if not is_readme and not header_exempt:
        if "## Related documentation" not in prose:
            findings.append({"file": r, "rule": "closer/related-documentation",
                             "detail": "no `## Related documentation` section"})

    # --- links resolve (style §6) ---
    for text_, target in LINK_RE.findall(prose):
        t = target.strip().split()[0]
        if t.startswith(("http://", "https://", "mailto:", "#")):
            continue
        t = t.split("#", 1)[0]
        if not t:
            continue
        dest = os.path.normpath(os.path.join(os.path.dirname(path), t))
        if not os.path.exists(dest):
            findings.append({"file": r, "rule": "link/broken",
                             "detail": f"[{text_[:34]}]({t})"})


def check_tree(findings: list[dict]) -> None:
    # root cleanliness (organization.md §8)
    for fn in sorted(os.listdir(DOCS)):
        p = os.path.join(DOCS, fn)
        if os.path.isfile(p) and not fn.startswith(".") and fn not in ROOT_ALLOWED:
            findings.append({"file": rel(p), "rule": "tree/loose-at-root",
                             "detail": "only README.md and organization.md may sit at docs/ root"})

    # every folder holding .md has an index, and the index lists its files
    for root, dirs, files in os.walk(DOCS):
        dirs[:] = [d for d in dirs if not d.startswith(".")]
        mds = [f for f in files if f.endswith(".md")]
        if not mds:
            continue
        r = rel(root)
        if "README.md" not in mds:
            findings.append({"file": r, "rule": "tree/no-index",
                             "detail": "folder holds .md files but has no README.md"})
            continue
        idx = open(os.path.join(root, "README.md"), encoding="utf-8").read()
        for f in sorted(mds):
            if f == "README.md":
                continue
            if f not in idx:
                findings.append({"file": f"{r}/{f}", "rule": "tree/not-in-index",
                                 "detail": f"not listed in {r}/README.md"})


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--folder")
    args = ap.parse_args()

    findings: list[dict] = []
    files = all_md()
    if args.folder:
        want = os.path.normpath(args.folder)
        files = [f for f in files if rel(f).startswith(want)]
        for f in files:
            check_file(f, findings)
    else:
        for f in files:
            check_file(f, findings)
        check_tree(findings)

    if args.json:
        print(json.dumps(findings, indent=2))
        return 1 if findings else 0

    by_rule: dict[str, int] = defaultdict(int)
    for f in findings:
        by_rule[f["rule"]] += 1
    print(f"docs-lint: {len(files)} markdown files checked, {len(findings)} findings\n")
    for rule in sorted(by_rule, key=lambda k: -by_rule[k]):
        print(f"  {by_rule[rule]:5d}  {rule}")
    if findings:
        print("\nfirst 40:")
        for f in findings[:40]:
            print(f"  {f['file']}: {f['rule']} — {f['detail']}")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
