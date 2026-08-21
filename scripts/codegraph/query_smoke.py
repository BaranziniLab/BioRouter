#!/usr/bin/env python3
"""Regression smoke test for CodeGraph queries against known BioRouter facts.

Every assertion below is ground truth independently verifiable with grep, so a
wrong index fails the suite rather than quietly returning plausible output.

Usage: python3 scripts/codegraph/query_smoke.py
"""
import json
import os
import subprocess
import sys

CG = os.environ.get("CODEGRAPH_BIN", "codegraph")
ROOT = subprocess.run(["git", "rev-parse", "--show-toplevel"],
                      capture_output=True, text=True).stdout.strip()


def run(*args, as_json=False):
    p = subprocess.run([CG, *args], cwd=ROOT, capture_output=True, text=True, timeout=180)
    if p.returncode != 0:
        command = " ".join([CG, *args])
        detail = p.stderr.strip() or p.stdout.strip() or "no diagnostic output"
        raise RuntimeError(f"{command} failed with exit {p.returncode}: {detail}")
    if as_json:
        try:
            return json.loads(p.stdout)
        except json.JSONDecodeError as exc:
            raise RuntimeError(f"{CG} returned invalid JSON for {' '.join(args)}") from exc
    return p.stdout


results = []


def check(name, ok, detail=""):
    results.append((name, ok, detail))
    print(f"  [{'PASS' if ok else 'FAIL'}] {name}" + (f"  — {detail}" if detail else ""))


print("== CodeGraph query smoke tests ==")

# 1. Both dispatch_tool_call definitions are found, in the right files.
out = run("query", "dispatch_tool_call", "--limit", "10")
check("query finds ExtensionManager::dispatch_tool_call",
      "extension_manager.rs" in out)
check("query finds Agent::dispatch_tool_call",
      "crates/biorouter/src/agents/agent.rs" in out)

# 2. node returns the real source of the five-condition delegation gate.
out = run("node", "subagents_enabled")
check("node returns subagents_enabled source",
      "has_non_injected_extensions" in out and "BioRouterMode::Auto" in out,
      "the 5-condition gate body")

# 3. callers attributes a call to its ENCLOSING function, not the raw line.
callers = run("callers", "dispatch_tool_call", "--limit", "200", "--json", as_json=True)
names = {c["name"] for c in callers.get("callers", [])}
check("callers finds the HTTP route handler call_tool", "call_tool" in names)
check("callers finds the coding-agent bridge dispatch_one", "dispatch_one" in names)
check("callers finds Agent::handle_approved_and_denied_tools",
      "handle_approved_and_denied_tools" in names)

# 4. impact reaches the route node, i.e. routes are first-class graph nodes.
out = run("impact", "dispatch_tool_call", "--depth", "2")
check("impact reaches the POST /agent/call_tool route node",
      "POST /agent/call_tool" in out)

# 5. A distinctively-named frontend symbol resolves in the TS half.
out = run("node", "shouldAutoRepairArtifact")
check("node resolves a TSX symbol (shouldAutoRepairArtifact)",
      "BaseChat" in out or "ARTIFACT_REPAIR_ACTIVE_GRACE_MS" in out)

# 6. Vendored bundles stay OUT of a complete, successful file listing. The
# positive checks keep an empty-but-successful response from satisfying the
# negative exclusion assertion.
files = run("files")
check("minified vendor bundles are excluded",
      "Project Structure (" in files and "crates" in files and ".min.js" not in files)

# 7. affected narrows, rather than returning the whole suite, at low depth.
n1 = len([l for l in run("affected", "ui/desktop/src/components/artifacts/ArtifactViewer.tsx",
                         "--quiet", "--depth", "1").splitlines() if l.strip()])
check("affected --depth 1 is selective", 0 < n1 < 20, f"{n1} test files")

print()
bad = [n for n, ok, _ in results if not ok]
print(f"query_smoke: {len(results)-len(bad)}/{len(results)} passed")
sys.exit(1 if bad else 0)
