# CodeGraphAgent Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `codegraphagent.brxt v0.1.0-rc1` — a BioRouter extension that downloads a vendored (but in this plan, unmodified-from-upstream) CodeGraph engine on first use, redirects its `.codegraph/` state into `<project>/.biorouter/codegraph/` via a symlink, and proxies MCP traffic to it.

**Architecture:** Monorepo (`Broccolito/CodeGraphAgent`) containing a Python proxy shim (`.brxt` payload) and a vendored CodeGraph engine (`engine/`) we build ourselves. The `.brxt` is ~20 KB and downloads the ~50 MB engine tarball from our own GitHub Releases on first use. Engine releases tagged `engine-vX.Y.Z`; `.brxt` releases tagged `vX.Y.Z`. No language additions in this plan — that's Plan 2.

**Tech Stack:** Python 3.11+, fastmcp, httpx, pytest, hatchling. Engine: Node 22.5+, TypeScript, tree-sitter, vitest (all vendored from upstream unchanged). Build: bash scripts + GitHub Actions.

**Spec:** [docs/superpowers/specs/2026-05-29-codegraphagent-extension-design.md](../specs/2026-05-29-codegraphagent-extension-design.md)

**Working directory throughout:** `/Users/wgu/Desktop/CodeGraphAgent/` (separate from BioRouter)

---

## Phase A — Repo scaffold

### Task A1: Clone the empty repo and create the directory layout

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/` (git clone target)

- [ ] **Step 1: Clone the empty repo locally**

Run:
```bash
cd /Users/wgu/Desktop && \
gh repo clone Broccolito/CodeGraphAgent && \
cd CodeGraphAgent && \
git status
```

Expected output:
```
On branch main
No commits yet
nothing to commit (create/copy files and use "git add" to track)
```

- [ ] **Step 2: Create top-level directory skeleton**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
mkdir -p src/codegraphagent tests engine scripts .github/workflows && \
ls -la
```

Expected: directories `src/`, `tests/`, `engine/`, `scripts/`, `.github/workflows/` exist.

### Task A2: Add `.gitignore`, `LICENSE`, `README.md`

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/.gitignore`
- Create: `/Users/wgu/Desktop/CodeGraphAgent/LICENSE`
- Create: `/Users/wgu/Desktop/CodeGraphAgent/README.md`

- [ ] **Step 1: Write `.gitignore`**

Create `/Users/wgu/Desktop/CodeGraphAgent/.gitignore`:

```gitignore
# Python
__pycache__/
*.py[cod]
*$py.class
*.egg-info/
.venv/
.eggs/
dist/
build/
*.whl

# uv
.uv/
uv.lock.bak

# pytest
.pytest_cache/
.coverage
htmlcov/

# IDEs
.vscode/
.idea/
*.swp
.DS_Store

# CodeGraphAgent build outputs
codegraphagent.brxt
*.tar.gz
*.zip

# Engine build outputs (built locally for testing)
engine/release/
engine/node_modules/
engine/dist/
```

- [ ] **Step 2: Write `LICENSE` (MIT, attributing upstream)**

Create `/Users/wgu/Desktop/CodeGraphAgent/LICENSE`:

```
MIT License

Copyright (c) 2026 Wanjun Gu and contributors

Portions of this repository (under engine/) are derived from CodeGraph
(https://github.com/colbymchenry/codegraph), Copyright (c) Colby McHenry,
also under the MIT License. The upstream LICENSE file is preserved verbatim
at engine/LICENSE.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 3: Write minimal `README.md`**

Create `/Users/wgu/Desktop/CodeGraphAgent/README.md`:

```markdown
# CodeGraphAgent

A [BioRouter](https://github.com/BaranziniLab/biorouter) extension (`.brxt`)
that provides a pre-indexed code knowledge graph (callers, callees, impact,
trace) via a vendored fork of [CodeGraph](https://github.com/colbymchenry/codegraph).

## What it does

CodeGraphAgent installs into a BioRouter session and exposes 9 MCP tools
(`codegraph_search`, `codegraph_callers`, `codegraph_callees`,
`codegraph_trace`, `codegraph_impact`, `codegraph_node`, `codegraph_explore`,
`codegraph_context`, `codegraph_status`) that let the agent answer "where is X
used?", "what does Y call?", and "what breaks if I change Z?" against the
current project — without re-parsing the codebase on every query.

The index lives at `<project>/.biorouter/codegraph/codegraph.db`. On first
use the extension downloads a vendored CodeGraph engine bundle (~50 MB) from
this repo's GitHub Releases.

## Install

Once a `.brxt` release is published, BioRouter installs the extension via:
`Settings → Extensions → Install from file → codegraphagent.brxt`.

## Status

v0.1.0-rc1 — initial release wrapping upstream CodeGraph unchanged.
See [CHANGELOG](CHANGELOG.md).

## Credits

Engine vendored from [CodeGraph](https://github.com/colbymchenry/codegraph)
(MIT, © Colby McHenry). The full upstream license is at `engine/LICENSE`.
```

- [ ] **Step 4: Commit the scaffold**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
git add .gitignore LICENSE README.md && \
git commit -m "chore: initial scaffold"
```

Expected: one new commit on `main`, no errors.

### Task A3: Write `manifest.json`

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/manifest.json`

- [ ] **Step 1: Write the manifest**

Create `/Users/wgu/Desktop/CodeGraphAgent/manifest.json`:

```json
{
  "name": "codegraphagent",
  "display_name": "CodeGraphAgent",
  "description": "Pre-indexed code knowledge graph (callers, callees, impact, trace) via vendored CodeGraph engine. Per-project index stored at <project>/.biorouter/codegraph/.",
  "version": "0.1.0",
  "entry_point": "codegraphagent",
  "repository": "https://github.com/Broccolito/CodeGraphAgent",
  "tools_count": 9,
  "env_vars": [
    {
      "key": "CODEGRAPH_NO_WATCH",
      "required": false,
      "auto_propagate": true,
      "default": "",
      "description": "Set to 1 to disable the file watcher (slow filesystems like WSL2)",
      "secret": false
    },
    {
      "key": "CODEGRAPH_ENGINE_PATH",
      "required": false,
      "auto_propagate": false,
      "default": "",
      "description": "Path to an already-extracted engine bundle (skips first-use download for air-gapped/CI)",
      "secret": false
    },
    {
      "key": "CODEGRAPH_ENGINE_VERSION",
      "required": false,
      "auto_propagate": false,
      "default": "",
      "description": "Override the pinned CodeGraphAgent engine release",
      "secret": false
    }
  ]
}
```

- [ ] **Step 2: Validate it parses as JSON**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
python3 -c "import json;json.load(open('manifest.json'))" && echo OK
```

Expected: `OK`

- [ ] **Step 3: Commit**

Run:
```bash
git add manifest.json && git commit -m "feat(manifest): .brxt manifest"
```

### Task A4: Write `pyproject.toml` and the package skeleton

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/pyproject.toml`
- Create: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/__init__.py`
- Create: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/__main__.py`
- Create: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/cli.py`

- [ ] **Step 1: Write `pyproject.toml`**

Create `/Users/wgu/Desktop/CodeGraphAgent/pyproject.toml`:

```toml
[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[project]
name = "codegraphagent"
version = "0.1.0"
description = "BioRouter extension wrapping a vendored CodeGraph engine as an MCP server with per-project index in .biorouter/codegraph/"
readme = "README.md"
license = {text = "MIT"}
authors = [{name = "Wanjun Gu", email = "wanjun.gu@ucsf.edu"}]
requires-python = ">=3.11"
keywords = ["mcp", "biorouter", "codegraph", "code-intelligence", "knowledge-graph"]
dependencies = [
    "fastmcp>=2.11.2",
    "httpx>=0.27",
]

[project.optional-dependencies]
dev = [
    "pytest>=8.0",
    "pytest-asyncio>=0.23",
]

[project.scripts]
codegraphagent = "codegraphagent.cli:main"

[tool.hatch.build.targets.wheel]
packages = ["src/codegraphagent"]
```

- [ ] **Step 2: Write minimal `__init__.py`**

Create `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/__init__.py`:

```python
"""CodeGraphAgent — BioRouter extension wrapping a vendored CodeGraph engine."""

__version__ = "0.1.0"
```

- [ ] **Step 3: Write `cli.py` stub**

Create `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/cli.py`:

```python
"""CLI entry point for codegraphagent.

This is the [project.scripts] target. It defers the real work to a function
in this module so that `python -m codegraphagent` and the `codegraphagent`
command share the same entrypoint.
"""


def main() -> int:
    """Run the CodeGraphAgent MCP server.

    Returns the process exit code (0 on clean shutdown, non-zero otherwise).
    Real implementation lands in later tasks; for now this is a stub.
    """
    print("codegraphagent: stub — wiring lands in Task E2")
    return 0
```

- [ ] **Step 4: Write `__main__.py` glue**

Create `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/__main__.py`:

```python
"""Allow `python -m codegraphagent` to run the CLI."""

import sys

from codegraphagent.cli import main

sys.exit(main())
```

- [ ] **Step 5: Install the package locally and verify it imports**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
python3 -m venv .venv && \
source .venv/bin/activate && \
pip install --quiet -e ".[dev]" && \
python -m codegraphagent
```

Expected output ends with: `codegraphagent: stub — wiring lands in Task E2`

- [ ] **Step 6: Commit**

Run:
```bash
git add pyproject.toml src/codegraphagent/__init__.py src/codegraphagent/__main__.py src/codegraphagent/cli.py && \
git commit -m "feat(shim): package skeleton with cli stub"
```

### Task A5: Add pytest scaffold

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/tests/__init__.py`
- Create: `/Users/wgu/Desktop/CodeGraphAgent/tests/conftest.py`
- Create: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_smoke.py`

- [ ] **Step 1: Create `tests/__init__.py`**

Run:
```bash
touch /Users/wgu/Desktop/CodeGraphAgent/tests/__init__.py
```

- [ ] **Step 2: Create `tests/conftest.py`**

Create `/Users/wgu/Desktop/CodeGraphAgent/tests/conftest.py`:

```python
"""Shared pytest fixtures for the codegraphagent test suite."""

from __future__ import annotations

import os
from pathlib import Path
from typing import Iterator

import pytest


@pytest.fixture
def tmp_project(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """Create a temp directory that looks like a project root.

    Sets BIOROUTER_WORKING_DIR to it so paths.resolve_project_root() finds it
    without walking the real filesystem.
    """
    monkeypatch.setenv("BIOROUTER_WORKING_DIR", str(tmp_path))
    # Create a marker so `.git`/pyproject heuristics don't reach into our real repo.
    (tmp_path / ".git").mkdir()
    return tmp_path


@pytest.fixture
def clean_env(monkeypatch: pytest.MonkeyPatch) -> Iterator[None]:
    """Strip CODEGRAPH_* env vars so tests start from a known state."""
    for var in list(os.environ):
        if var.startswith("CODEGRAPH_") or var == "BIOROUTER_WORKING_DIR":
            monkeypatch.delenv(var, raising=False)
    yield
```

- [ ] **Step 3: Write a smoke test**

Create `/Users/wgu/Desktop/CodeGraphAgent/tests/test_smoke.py`:

```python
"""Smoke test: package imports and version is set."""

import codegraphagent


def test_version_is_set():
    assert codegraphagent.__version__ == "0.1.0"


def test_cli_entrypoint_runs():
    from codegraphagent.cli import main
    rc = main()
    assert rc == 0
```

- [ ] **Step 4: Run the smoke test**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
source .venv/bin/activate && \
pytest tests/test_smoke.py -v
```

Expected: 2 passed.

- [ ] **Step 5: Commit**

Run:
```bash
git add tests/ && git commit -m "test: pytest scaffold + smoke test"
```

---

## Phase B — Python shim: errors + paths

### Task B1: Add `errors.py` (custom exception hierarchy)

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/errors.py`
- Create: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_errors.py`

- [ ] **Step 1: Write the failing test**

Create `/Users/wgu/Desktop/CodeGraphAgent/tests/test_errors.py`:

```python
"""Custom exception hierarchy carries enough info to surface MCP-level errors."""

import pytest

from codegraphagent.errors import (
    CodeGraphAgentError,
    LayoutConflictError,
    BootstrapError,
)


def test_layout_conflict_is_codegraphagent_error():
    err = LayoutConflictError("conflict", path="/tmp/x")
    assert isinstance(err, CodeGraphAgentError)
    assert err.path == "/tmp/x"


def test_bootstrap_error_carries_url_and_hashes():
    err = BootstrapError(
        "sha mismatch",
        url="https://example.com/x.tar.gz",
        expected_sha="abc",
        observed_sha="def",
    )
    assert isinstance(err, CodeGraphAgentError)
    assert err.url == "https://example.com/x.tar.gz"
    assert err.expected_sha == "abc"
    assert err.observed_sha == "def"


def test_bootstrap_error_optional_fields_default_to_none():
    err = BootstrapError("plain")
    assert err.url is None
    assert err.expected_sha is None
    assert err.observed_sha is None
```

- [ ] **Step 2: Run and verify it fails**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
source .venv/bin/activate && \
pytest tests/test_errors.py -v
```

Expected: ImportError / collection failure — `codegraphagent.errors` doesn't exist yet.

- [ ] **Step 3: Implement `errors.py`**

Create `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/errors.py`:

```python
"""Custom exception hierarchy for codegraphagent.

These exceptions carry structured detail (URLs, hashes, conflicting paths) so
the degraded-mode error shim can render useful, actionable messages back to
the agent over MCP.
"""

from __future__ import annotations


class CodeGraphAgentError(Exception):
    """Base class for any error originating in the shim itself."""


class LayoutConflictError(CodeGraphAgentError):
    """Raised when the project's .codegraph path exists as something other
    than the symlink we own (e.g. a real directory the user created)."""

    def __init__(self, message: str, *, path: str) -> None:
        super().__init__(message)
        self.path = path


class BootstrapError(CodeGraphAgentError):
    """Raised when the engine bundle could not be downloaded, verified, or
    extracted.

    Optional fields let the error shim show the user exactly what URL was
    attempted and (for SHA mismatches) what was expected vs observed.
    """

    def __init__(
        self,
        message: str,
        *,
        url: str | None = None,
        expected_sha: str | None = None,
        observed_sha: str | None = None,
    ) -> None:
        super().__init__(message)
        self.url = url
        self.expected_sha = expected_sha
        self.observed_sha = observed_sha
```

- [ ] **Step 4: Run and verify pass**

Run:
```bash
pytest tests/test_errors.py -v
```

Expected: 3 passed.

- [ ] **Step 5: Commit**

Run:
```bash
git add src/codegraphagent/errors.py tests/test_errors.py && \
git commit -m "feat(shim): error hierarchy with structured fields"
```

### Task B2: `paths.resolve_project_root` — happy path

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/paths.py`
- Create: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_paths.py`

- [ ] **Step 1: Write failing tests**

Create `/Users/wgu/Desktop/CodeGraphAgent/tests/test_paths.py`:

```python
"""paths.py — project root resolution and layout setup."""

from __future__ import annotations

import os
from pathlib import Path

import pytest

from codegraphagent import paths


def test_resolve_project_root_uses_env_var(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv("BIOROUTER_WORKING_DIR", str(tmp_path))
    assert paths.resolve_project_root() == tmp_path.resolve()


def test_resolve_project_root_falls_back_to_cwd_when_env_unset(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    monkeypatch.delenv("BIOROUTER_WORKING_DIR", raising=False)
    monkeypatch.chdir(tmp_path)
    assert paths.resolve_project_root() == tmp_path.resolve()


def test_resolve_project_root_walks_up_to_git_marker(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    (tmp_path / ".git").mkdir()
    nested = tmp_path / "a" / "b" / "c"
    nested.mkdir(parents=True)
    monkeypatch.delenv("BIOROUTER_WORKING_DIR", raising=False)
    monkeypatch.chdir(nested)
    assert paths.resolve_project_root() == tmp_path.resolve()
```

- [ ] **Step 2: Run, verify fail**

Run:
```bash
pytest tests/test_paths.py -v
```

Expected: ImportError on `codegraphagent.paths`.

- [ ] **Step 3: Implement `resolve_project_root`**

Create `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/paths.py`:

```python
"""Project-root resolution and on-disk layout setup.

The CodeGraph engine hardcodes its state directory as `.codegraph/` at the
project root. We want our state under `.biorouter/codegraph/` instead, so we
create the directory in `.biorouter/` and a symlink at `.codegraph` pointing
to it. From the engine's perspective nothing changed.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

from codegraphagent.errors import LayoutConflictError

_PROJECT_MARKERS = (".biorouter", ".git", "pyproject.toml")


def resolve_project_root() -> Path:
    """Return the absolute path of the project root.

    Order of preference:
    1. $BIOROUTER_WORKING_DIR if set.
    2. The nearest ancestor of CWD containing any of: .biorouter/, .git/,
       pyproject.toml.
    3. CWD as a fallback.
    """
    env = os.environ.get("BIOROUTER_WORKING_DIR")
    if env:
        return Path(env).resolve()

    cwd = Path.cwd().resolve()
    for candidate in [cwd, *cwd.parents]:
        if any((candidate / marker).exists() for marker in _PROJECT_MARKERS):
            return candidate
    return cwd
```

- [ ] **Step 4: Run, verify pass**

Run:
```bash
pytest tests/test_paths.py -v
```

Expected: 3 passed.

- [ ] **Step 5: Commit**

Run:
```bash
git add src/codegraphagent/paths.py tests/test_paths.py && \
git commit -m "feat(paths): resolve_project_root with env-var + marker walk"
```

### Task B3: `paths.ensure_layout` — directory + symlink creation

**Files:**
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/paths.py`
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_paths.py`

- [ ] **Step 1: Add failing tests**

Append to `/Users/wgu/Desktop/CodeGraphAgent/tests/test_paths.py`:

```python


def test_ensure_layout_creates_biorouter_codegraph_dir(tmp_project: Path):
    paths.ensure_layout(tmp_project)
    assert (tmp_project / ".biorouter" / "codegraph").is_dir()


def test_ensure_layout_creates_symlink(tmp_project: Path):
    paths.ensure_layout(tmp_project)
    link = tmp_project / ".codegraph"
    assert link.is_symlink()
    assert link.resolve() == (tmp_project / ".biorouter" / "codegraph").resolve()


def test_ensure_layout_is_idempotent(tmp_project: Path):
    paths.ensure_layout(tmp_project)
    paths.ensure_layout(tmp_project)
    link = tmp_project / ".codegraph"
    assert link.is_symlink()
```

- [ ] **Step 2: Run, verify fail**

Run:
```bash
pytest tests/test_paths.py::test_ensure_layout_creates_biorouter_codegraph_dir -v
```

Expected: AttributeError — `paths.ensure_layout` doesn't exist.

- [ ] **Step 3: Implement `ensure_layout`**

Append to `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/paths.py`:

```python


def ensure_layout(root: Path) -> None:
    """Ensure `<root>/.biorouter/codegraph/` exists and `<root>/.codegraph` is
    a symlink pointing at it.

    Idempotent: safe to call repeatedly.

    Raises:
        LayoutConflictError: if `<root>/.codegraph` exists as a real
            directory rather than a symlink.
    """
    state_dir = root / ".biorouter" / "codegraph"
    state_dir.mkdir(parents=True, exist_ok=True)

    link = root / ".codegraph"
    target = Path(".biorouter") / "codegraph"  # relative for portability

    if link.is_symlink():
        return  # idempotent — assume it points where we want
    if link.exists():
        raise LayoutConflictError(
            f"{link} exists as a real directory; "
            "rename or remove it, then restart CodeGraphAgent",
            path=str(link),
        )

    if sys.platform == "win32":
        _create_windows_junction(link, root / target)
    else:
        link.symlink_to(target, target_is_directory=True)


def _create_windows_junction(link: Path, target: Path) -> None:
    """Create a directory junction on Windows (doesn't need admin)."""
    subprocess.run(
        ["cmd", "/c", "mklink", "/J", str(link), str(target)],
        check=True,
        capture_output=True,
    )
```

- [ ] **Step 4: Run, verify pass**

Run:
```bash
pytest tests/test_paths.py -v
```

Expected: 6 passed.

- [ ] **Step 5: Commit**

Run:
```bash
git add src/codegraphagent/paths.py tests/test_paths.py && \
git commit -m "feat(paths): ensure_layout creates .biorouter/codegraph + symlink"
```

### Task B4: `paths.ensure_layout` — conflict detection

**Files:**
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_paths.py`

- [ ] **Step 1: Add failing tests**

Append to `tests/test_paths.py`:

```python


def test_ensure_layout_raises_on_real_codegraph_dir(tmp_project: Path):
    (tmp_project / ".codegraph").mkdir()
    with pytest.raises(LayoutConflictError) as excinfo:
        paths.ensure_layout(tmp_project)
    assert excinfo.value.path == str(tmp_project / ".codegraph")


def test_ensure_layout_leaves_existing_symlink_alone(tmp_project: Path):
    paths.ensure_layout(tmp_project)
    link = tmp_project / ".codegraph"
    mtime_before = link.lstat().st_mtime
    paths.ensure_layout(tmp_project)
    mtime_after = link.lstat().st_mtime
    assert mtime_before == mtime_after
```

Also update the import line near the top of `tests/test_paths.py` to include the error type:

```python
from codegraphagent.errors import LayoutConflictError
```

- [ ] **Step 2: Run, verify pass**

`ensure_layout` already raises `LayoutConflictError`. Run:
```bash
pytest tests/test_paths.py -v
```

Expected: 8 passed.

- [ ] **Step 3: Commit**

Run:
```bash
git add tests/test_paths.py && \
git commit -m "test(paths): cover layout conflict + symlink idempotency"
```

### Task B5: `paths.ensure_layout` — gitignore writes

**Files:**
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/paths.py`
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_paths.py`

- [ ] **Step 1: Add failing test**

Append to `tests/test_paths.py`:

```python


def test_ensure_layout_writes_gitignore_for_codegraph_symlink(tmp_project: Path):
    paths.ensure_layout(tmp_project)
    content = (tmp_project / ".gitignore").read_text()
    assert ".codegraph" in content


def test_ensure_layout_does_not_duplicate_gitignore_entry(tmp_project: Path):
    paths.ensure_layout(tmp_project)
    paths.ensure_layout(tmp_project)
    content = (tmp_project / ".gitignore").read_text()
    assert content.count(".codegraph") == 1


def test_ensure_layout_writes_state_dir_gitignore(tmp_project: Path):
    paths.ensure_layout(tmp_project)
    inner = tmp_project / ".biorouter" / "codegraph" / ".gitignore"
    assert inner.exists()
    content = inner.read_text()
    for needle in ("*.db", "*.lock", ".dirty", "cache/"):
        assert needle in content, f"missing {needle!r} in {content}"
```

- [ ] **Step 2: Run, verify fail**

Run:
```bash
pytest tests/test_paths.py::test_ensure_layout_writes_gitignore_for_codegraph_symlink -v
```

Expected: FAIL — `.gitignore` not written.

- [ ] **Step 3: Add gitignore writes to `ensure_layout`**

In `src/codegraphagent/paths.py`, add a helper and call it from `ensure_layout`. Replace the existing `ensure_layout` body so it now ends with `_write_gitignores(root, state_dir)` after creating the symlink:

```python
def ensure_layout(root: Path) -> None:
    state_dir = root / ".biorouter" / "codegraph"
    state_dir.mkdir(parents=True, exist_ok=True)

    link = root / ".codegraph"
    target = Path(".biorouter") / "codegraph"

    if not link.is_symlink():
        if link.exists():
            raise LayoutConflictError(
                f"{link} exists as a real directory; "
                "rename or remove it, then restart CodeGraphAgent",
                path=str(link),
            )
        if sys.platform == "win32":
            _create_windows_junction(link, root / target)
        else:
            link.symlink_to(target, target_is_directory=True)

    _write_gitignores(root, state_dir)


def _write_gitignores(root: Path, state_dir: Path) -> None:
    """Append `.codegraph` to <root>/.gitignore (if absent) and write a
    state-dir-local .gitignore that ignores the engine's runtime files."""
    root_gitignore = root / ".gitignore"
    if root_gitignore.exists():
        existing = root_gitignore.read_text()
        if ".codegraph" not in existing.splitlines():
            with root_gitignore.open("a") as fh:
                if not existing.endswith("\n"):
                    fh.write("\n")
                fh.write(".codegraph\n")
    else:
        root_gitignore.write_text(".codegraph\n")

    state_gitignore = state_dir / ".gitignore"
    if not state_gitignore.exists():
        state_gitignore.write_text(
            "# CodeGraph runtime state — do not commit\n"
            "*.db\n"
            "*.db-wal\n"
            "*.db-shm\n"
            "*.lock\n"
            ".dirty\n"
            "cache/\n"
        )
```

- [ ] **Step 4: Run, verify pass**

Run:
```bash
pytest tests/test_paths.py -v
```

Expected: 11 passed.

- [ ] **Step 5: Commit**

Run:
```bash
git add src/codegraphagent/paths.py tests/test_paths.py && \
git commit -m "feat(paths): write .gitignore entries for symlink + state dir"
```

---

## Phase C — Python shim: bootstrap

### Task C1: `bootstrap.platform_tag` and `bootstrap.archive_suffix`

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/bootstrap.py`
- Create: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_bootstrap.py`

- [ ] **Step 1: Write failing tests**

Create `/Users/wgu/Desktop/CodeGraphAgent/tests/test_bootstrap.py`:

```python
"""bootstrap.py — engine tarball download + extract + verification."""

from __future__ import annotations

import hashlib
import tarfile
from pathlib import Path
from unittest.mock import MagicMock

import pytest

from codegraphagent import bootstrap
from codegraphagent.errors import BootstrapError


def test_platform_tag_known(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setattr(bootstrap.platform, "system", lambda: "Darwin")
    monkeypatch.setattr(bootstrap.platform, "machine", lambda: "arm64")
    assert bootstrap.platform_tag() == "darwin-arm64"


def test_platform_tag_linux_x64(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setattr(bootstrap.platform, "system", lambda: "Linux")
    monkeypatch.setattr(bootstrap.platform, "machine", lambda: "x86_64")
    assert bootstrap.platform_tag() == "linux-x64"


def test_platform_tag_windows_x64(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setattr(bootstrap.platform, "system", lambda: "Windows")
    monkeypatch.setattr(bootstrap.platform, "machine", lambda: "AMD64")
    assert bootstrap.platform_tag() == "win32-x64"


def test_platform_tag_unsupported(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setattr(bootstrap.platform, "system", lambda: "OpenBSD")
    monkeypatch.setattr(bootstrap.platform, "machine", lambda: "amd64")
    with pytest.raises(BootstrapError):
        bootstrap.platform_tag()


def test_archive_suffix_unix(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setattr(bootstrap, "platform_tag", lambda: "linux-x64")
    assert bootstrap.archive_suffix() == "tar.gz"


def test_archive_suffix_windows(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setattr(bootstrap, "platform_tag", lambda: "win32-x64")
    assert bootstrap.archive_suffix() == "zip"
```

- [ ] **Step 2: Run, verify fail**

Run:
```bash
pytest tests/test_bootstrap.py -v
```

Expected: ImportError — `codegraphagent.bootstrap` missing.

- [ ] **Step 3: Implement platform helpers**

Create `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/bootstrap.py`:

```python
"""Engine bootstrap — download + verify + extract on first use.

The engine bundle is a per-platform tarball (or .zip on Windows) hosted on our
own GitHub Releases. The release manifest pins both the version and per-platform
SHA256s so a tampered or partial download is detected before we exec it.
"""

from __future__ import annotations

import hashlib
import json
import os
import platform
import shutil
import tarfile
import tempfile
import zipfile
from pathlib import Path

import httpx

from codegraphagent.errors import BootstrapError


_PLATFORM_MAP = {
    ("Darwin", "arm64"): "darwin-arm64",
    ("Darwin", "x86_64"): "darwin-x64",
    ("Linux", "x86_64"): "linux-x64",
    ("Linux", "aarch64"): "linux-arm64",
    ("Windows", "AMD64"): "win32-x64",
    ("Windows", "ARM64"): "win32-arm64",
}


def platform_tag() -> str:
    """Return the platform tag matching upstream's release asset naming.

    Raises BootstrapError on an unsupported platform.
    """
    key = (platform.system(), platform.machine())
    if key not in _PLATFORM_MAP:
        raise BootstrapError(
            f"Unsupported platform: {key}. "
            f"Supported: {sorted(_PLATFORM_MAP.values())}"
        )
    return _PLATFORM_MAP[key]


def archive_suffix() -> str:
    """Return the archive extension for the current platform."""
    return "zip" if platform_tag().startswith("win32") else "tar.gz"
```

- [ ] **Step 4: Run, verify pass**

Run:
```bash
pytest tests/test_bootstrap.py -v
```

Expected: 6 passed.

- [ ] **Step 5: Commit**

Run:
```bash
git add src/codegraphagent/bootstrap.py tests/test_bootstrap.py && \
git commit -m "feat(bootstrap): platform_tag + archive_suffix"
```

### Task C2: `bootstrap._sha256_of` helper

**Files:**
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/bootstrap.py`
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_bootstrap.py`

- [ ] **Step 1: Add failing test**

Append to `tests/test_bootstrap.py`:

```python


def test_sha256_of_file(tmp_path: Path):
    payload = b"hello, codegraph"
    fp = tmp_path / "blob.bin"
    fp.write_bytes(payload)
    assert bootstrap._sha256_of(fp) == hashlib.sha256(payload).hexdigest()
```

- [ ] **Step 2: Run, verify fail**

Run:
```bash
pytest tests/test_bootstrap.py::test_sha256_of_file -v
```

Expected: AttributeError.

- [ ] **Step 3: Implement**

Append to `src/codegraphagent/bootstrap.py`:

```python


def _sha256_of(path: Path) -> str:
    """Stream a file through SHA-256 in 64KB chunks. Memory-safe for large
    tarballs (engine bundles are ~50 MB)."""
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(64 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()
```

- [ ] **Step 4: Run, verify pass**

Run:
```bash
pytest tests/test_bootstrap.py -v
```

Expected: 7 passed.

- [ ] **Step 5: Commit**

Run:
```bash
git add src/codegraphagent/bootstrap.py tests/test_bootstrap.py && \
git commit -m "feat(bootstrap): sha256 helper"
```

### Task C3: `bootstrap._load_manifest` — read pinned release info

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/release_manifest.json`
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/bootstrap.py`
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_bootstrap.py`

- [ ] **Step 1: Write placeholder release manifest**

Create `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/release_manifest.json`:

```json
{
  "engine_version": "0.1.0",
  "base_url": "https://github.com/Broccolito/CodeGraphAgent/releases/download/engine-v0.1.0/",
  "platforms": {
    "darwin-arm64":  {"filename": "codegraph-darwin-arm64.tar.gz",  "sha256": "PLACEHOLDER_FILLED_AT_RELEASE"},
    "darwin-x64":    {"filename": "codegraph-darwin-x64.tar.gz",    "sha256": "PLACEHOLDER_FILLED_AT_RELEASE"},
    "linux-x64":     {"filename": "codegraph-linux-x64.tar.gz",     "sha256": "PLACEHOLDER_FILLED_AT_RELEASE"},
    "linux-arm64":   {"filename": "codegraph-linux-arm64.tar.gz",   "sha256": "PLACEHOLDER_FILLED_AT_RELEASE"},
    "win32-x64":     {"filename": "codegraph-win32-x64.zip",        "sha256": "PLACEHOLDER_FILLED_AT_RELEASE"},
    "win32-arm64":   {"filename": "codegraph-win32-arm64.zip",      "sha256": "PLACEHOLDER_FILLED_AT_RELEASE"}
  }
}
```

(The SHA256 placeholders are real strings in the file; they get rewritten to actual hashes by Task L2 once the engine release is built.)

- [ ] **Step 2: Add failing test**

Append to `tests/test_bootstrap.py`:

```python


def test_load_manifest_returns_pinned_info(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setattr(bootstrap, "platform_tag", lambda: "linux-x64")
    info = bootstrap._load_manifest()
    assert "engine_version" in info
    assert info["filename"] == "codegraph-linux-x64.tar.gz"
    assert info["url"].endswith("codegraph-linux-x64.tar.gz")


def test_load_manifest_respects_engine_version_override(
    monkeypatch: pytest.MonkeyPatch
):
    monkeypatch.setattr(bootstrap, "platform_tag", lambda: "linux-x64")
    monkeypatch.setenv("CODEGRAPH_ENGINE_VERSION", "0.9.9")
    info = bootstrap._load_manifest()
    assert "engine-v0.9.9" in info["url"]
```

- [ ] **Step 3: Run, verify fail**

Run:
```bash
pytest tests/test_bootstrap.py::test_load_manifest_returns_pinned_info -v
```

Expected: AttributeError.

- [ ] **Step 4: Implement**

Append to `src/codegraphagent/bootstrap.py`:

```python


def _load_manifest() -> dict:
    """Load the release manifest and resolve the current-platform info.

    Returns dict with keys: engine_version, filename, url, sha256.
    """
    manifest_path = Path(__file__).parent / "release_manifest.json"
    with manifest_path.open() as fh:
        manifest = json.load(fh)

    version = os.environ.get("CODEGRAPH_ENGINE_VERSION") or manifest["engine_version"]
    base_url = manifest["base_url"]
    if os.environ.get("CODEGRAPH_ENGINE_VERSION"):
        # Substitute the version in the URL to the override.
        base_url = (
            f"https://github.com/Broccolito/CodeGraphAgent/releases/"
            f"download/engine-v{version}/"
        )

    tag = platform_tag()
    if tag not in manifest["platforms"]:
        raise BootstrapError(
            f"Manifest does not list a binary for platform {tag}; "
            f"supported: {sorted(manifest['platforms'])}"
        )
    platform_info = manifest["platforms"][tag]
    return {
        "engine_version": version,
        "filename": platform_info["filename"],
        "url": base_url + platform_info["filename"],
        "sha256": platform_info["sha256"],
    }
```

- [ ] **Step 5: Run, verify pass**

Run:
```bash
pytest tests/test_bootstrap.py -v
```

Expected: 9 passed.

- [ ] **Step 6: Commit**

Run:
```bash
git add src/codegraphagent/release_manifest.json src/codegraphagent/bootstrap.py tests/test_bootstrap.py && \
git commit -m "feat(bootstrap): load release manifest with version override"
```

### Task C4: `bootstrap._download_and_verify` — happy path + SHA mismatch

**Files:**
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/bootstrap.py`
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_bootstrap.py`

- [ ] **Step 1: Add failing tests**

Append to `tests/test_bootstrap.py`:

```python


def test_download_and_verify_success(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    payload = b"fake-tarball-bytes-here"
    expected_sha = hashlib.sha256(payload).hexdigest()

    class FakeResponse:
        def __init__(self, content: bytes):
            self._content = content
            self.status_code = 200
        def iter_bytes(self, chunk_size: int = 65536):
            yield self._content
        def raise_for_status(self):
            pass
        def __enter__(self):
            return self
        def __exit__(self, *args):
            return False

    class FakeClient:
        def __init__(self, *args, **kwargs):
            pass
        def __enter__(self):
            return self
        def __exit__(self, *args):
            return False
        def stream(self, method, url):
            return FakeResponse(payload)

    monkeypatch.setattr(bootstrap.httpx, "Client", FakeClient)

    dest = tmp_path / "engine.tar.gz"
    bootstrap._download_and_verify(
        url="https://example.com/engine.tar.gz",
        dest=dest,
        expected_sha=expected_sha,
    )
    assert dest.read_bytes() == payload


def test_download_and_verify_sha_mismatch(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    payload = b"wrong-bytes"

    class FakeResponse:
        def __init__(self):
            self.status_code = 200
        def iter_bytes(self, chunk_size: int = 65536):
            yield payload
        def raise_for_status(self):
            pass
        def __enter__(self):
            return self
        def __exit__(self, *args):
            return False

    class FakeClient:
        def __init__(self, *args, **kwargs):
            pass
        def __enter__(self):
            return self
        def __exit__(self, *args):
            return False
        def stream(self, method, url):
            return FakeResponse()

    monkeypatch.setattr(bootstrap.httpx, "Client", FakeClient)

    dest = tmp_path / "engine.tar.gz"
    with pytest.raises(BootstrapError) as excinfo:
        bootstrap._download_and_verify(
            url="https://example.com/engine.tar.gz",
            dest=dest,
            expected_sha="0" * 64,
        )
    assert excinfo.value.observed_sha == hashlib.sha256(payload).hexdigest()
    assert not dest.exists(), "Partial file must be removed on SHA mismatch"
```

- [ ] **Step 2: Run, verify fail**

Run:
```bash
pytest tests/test_bootstrap.py::test_download_and_verify_success -v
```

Expected: AttributeError.

- [ ] **Step 3: Implement download**

Append to `src/codegraphagent/bootstrap.py`:

```python


def _download_and_verify(*, url: str, dest: Path, expected_sha: str) -> None:
    """Stream-download `url` to `dest`, verifying SHA-256 against
    `expected_sha`. On mismatch, removes the partial file and raises
    BootstrapError carrying both hashes.

    A literal "PLACEHOLDER_FILLED_AT_RELEASE" expected_sha skips verification —
    used only during local development before the first release is cut. The
    bypass is logged.
    """
    dest.parent.mkdir(parents=True, exist_ok=True)
    try:
        with httpx.Client(timeout=120.0, follow_redirects=True) as client:
            with client.stream("GET", url) as resp:
                resp.raise_for_status()
                with dest.open("wb") as fh:
                    for chunk in resp.iter_bytes(64 * 1024):
                        fh.write(chunk)
    except httpx.HTTPError as exc:
        if dest.exists():
            dest.unlink()
        raise BootstrapError(
            f"Engine download failed: {exc}",
            url=url,
        ) from exc

    if expected_sha == "PLACEHOLDER_FILLED_AT_RELEASE":
        import sys
        print(
            f"codegraphagent: skipping SHA verification (placeholder) for {url}",
            file=sys.stderr,
        )
        return

    observed = _sha256_of(dest)
    if observed != expected_sha:
        dest.unlink(missing_ok=True)
        raise BootstrapError(
            "Engine SHA-256 mismatch — refusing to install a tampered or "
            "corrupt bundle. Re-running may help if the download was truncated.",
            url=url,
            expected_sha=expected_sha,
            observed_sha=observed,
        )
```

- [ ] **Step 4: Run, verify pass**

Run:
```bash
pytest tests/test_bootstrap.py -v
```

Expected: 11 passed.

- [ ] **Step 5: Commit**

Run:
```bash
git add src/codegraphagent/bootstrap.py tests/test_bootstrap.py && \
git commit -m "feat(bootstrap): _download_and_verify with SHA gate"
```

### Task C5: `bootstrap._extract` — atomic tarball + zip extraction

**Files:**
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/bootstrap.py`
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_bootstrap.py`

- [ ] **Step 1: Add failing tests**

Append to `tests/test_bootstrap.py`:

```python


def test_extract_tarball(tmp_path: Path):
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    (src_dir / "bin").mkdir()
    (src_dir / "bin" / "codegraph").write_text("#!/bin/sh\necho hi\n")
    (src_dir / "lib").mkdir()
    (src_dir / "lib" / "x").write_text("data")

    archive = tmp_path / "bundle.tar.gz"
    with tarfile.open(archive, "w:gz") as tf:
        tf.add(src_dir, arcname=".")

    dest = tmp_path / "extract-to"
    bootstrap._extract(archive, dest)
    assert (dest / "bin" / "codegraph").read_text() == "#!/bin/sh\necho hi\n"
    assert (dest / "lib" / "x").read_text() == "data"


def test_extract_replaces_existing_dest(tmp_path: Path):
    """An existing engine dir is replaced atomically (rename, not in-place rm)."""
    dest = tmp_path / "engine"
    dest.mkdir()
    (dest / "STALE").write_text("old")

    src_dir = tmp_path / "src"
    src_dir.mkdir()
    (src_dir / "NEW").write_text("new")
    archive = tmp_path / "bundle.tar.gz"
    with tarfile.open(archive, "w:gz") as tf:
        tf.add(src_dir, arcname=".")

    bootstrap._extract(archive, dest)
    assert (dest / "NEW").read_text() == "new"
    assert not (dest / "STALE").exists()
```

- [ ] **Step 2: Run, verify fail**

Run:
```bash
pytest tests/test_bootstrap.py::test_extract_tarball -v
```

Expected: AttributeError.

- [ ] **Step 3: Implement**

Append to `src/codegraphagent/bootstrap.py`:

```python


def _extract(archive: Path, dest: Path) -> None:
    """Extract `archive` (.tar.gz or .zip) into `dest`, atomically.

    Extracts into a sibling temp dir first, then swaps it into place — so a
    partial extraction can't leave `dest` in a broken half-state.
    """
    parent = dest.parent
    parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix=".cga-extract-", dir=parent) as staging:
        staging_path = Path(staging)
        if archive.name.endswith(".zip"):
            with zipfile.ZipFile(archive) as zf:
                zf.extractall(staging_path)
        else:
            with tarfile.open(archive, "r:*") as tf:
                tf.extractall(staging_path)

        if dest.exists():
            old_dest = parent / f".{dest.name}.old"
            if old_dest.exists():
                shutil.rmtree(old_dest)
            dest.rename(old_dest)
            try:
                staging_path.rename(dest)
            except OSError:
                old_dest.rename(dest)
                raise
            shutil.rmtree(old_dest)
        else:
            staging_path.rename(dest)
```

- [ ] **Step 4: Run, verify pass**

Run:
```bash
pytest tests/test_bootstrap.py -v
```

Expected: 13 passed.

- [ ] **Step 5: Commit**

Run:
```bash
git add src/codegraphagent/bootstrap.py tests/test_bootstrap.py && \
git commit -m "feat(bootstrap): atomic tarball/zip extraction"
```

### Task C6: `bootstrap.ensure_engine` — top-level orchestration

**Files:**
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/bootstrap.py`
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_bootstrap.py`

- [ ] **Step 1: Add failing tests**

Append to `tests/test_bootstrap.py`:

```python


def test_ensure_engine_honors_engine_path_override(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    bundle = tmp_path / "user-bundle"
    bin_dir = bundle / "bin"
    bin_dir.mkdir(parents=True)
    launcher = bin_dir / "codegraph"
    launcher.write_text("#!/bin/sh\n")
    launcher.chmod(0o755)

    monkeypatch.setenv("CODEGRAPH_ENGINE_PATH", str(bundle))
    monkeypatch.setattr(bootstrap, "platform_tag", lambda: "linux-x64")

    result = bootstrap.ensure_engine()
    assert result == launcher


def test_ensure_engine_uses_cached_bundle_with_matching_version(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    monkeypatch.setattr(bootstrap, "platform_tag", lambda: "linux-x64")
    monkeypatch.setattr(bootstrap, "_install_dir", lambda: tmp_path / "engine")

    install = tmp_path / "engine"
    (install / "bin").mkdir(parents=True)
    launcher = install / "bin" / "codegraph"
    launcher.write_text("#!/bin/sh\n")
    launcher.chmod(0o755)
    (install / "VERSION").write_text("0.1.0\n")

    monkeypatch.setattr(
        bootstrap, "_load_manifest",
        lambda: {"engine_version": "0.1.0", "filename": "x", "url": "x", "sha256": "x"},
    )

    download_called = MagicMock()
    monkeypatch.setattr(bootstrap, "_download_and_verify", download_called)

    result = bootstrap.ensure_engine()
    assert result == launcher
    download_called.assert_not_called()
```

- [ ] **Step 2: Run, verify fail**

Run:
```bash
pytest tests/test_bootstrap.py::test_ensure_engine_honors_engine_path_override -v
```

Expected: AttributeError.

- [ ] **Step 3: Implement**

Append to `src/codegraphagent/bootstrap.py`:

```python


def _install_dir() -> Path:
    """Return the per-platform install dir inside the extension package."""
    return Path(__file__).parent / "engine"


def _launcher_path(install_dir: Path) -> Path:
    """Return the platform-correct launcher path inside an extracted bundle."""
    name = "codegraph.cmd" if platform_tag().startswith("win32") else "codegraph"
    return install_dir / "bin" / name


def ensure_engine() -> Path:
    """Ensure the engine bundle is present locally and return its launcher.

    Order of preference:
    1. $CODEGRAPH_ENGINE_PATH → use as-is (no download, no verification).
    2. Existing install dir with VERSION matching pinned manifest → reuse.
    3. Download + verify + extract.

    Raises BootstrapError on any failure.
    """
    override = os.environ.get("CODEGRAPH_ENGINE_PATH")
    if override:
        return _launcher_path(Path(override))

    manifest = _load_manifest()
    install = _install_dir()
    version_file = install / "VERSION"
    if (
        install.exists()
        and version_file.exists()
        and version_file.read_text().strip() == manifest["engine_version"]
    ):
        launcher = _launcher_path(install)
        if launcher.exists():
            return launcher

    with tempfile.TemporaryDirectory(prefix=".cga-dl-") as tmp:
        archive = Path(tmp) / manifest["filename"]
        _download_and_verify(
            url=manifest["url"],
            dest=archive,
            expected_sha=manifest["sha256"],
        )
        _extract(archive, install)

    version_file.write_text(manifest["engine_version"] + "\n")
    launcher = _launcher_path(install)
    if not launcher.exists():
        raise BootstrapError(
            f"Extracted bundle is missing the launcher at {launcher}. "
            "The release artifact may be malformed.",
            url=manifest["url"],
        )
    if not launcher.name.endswith(".cmd"):
        launcher.chmod(0o755)
    return launcher
```

- [ ] **Step 4: Run, verify pass**

Run:
```bash
pytest tests/test_bootstrap.py -v
```

Expected: 15 passed.

- [ ] **Step 5: Commit**

Run:
```bash
git add src/codegraphagent/bootstrap.py tests/test_bootstrap.py && \
git commit -m "feat(bootstrap): ensure_engine orchestration with cache + env override"
```

---

## Phase D — Python shim: proxy

### Task D1: `proxy.run` — spawn child and pump stdio

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/proxy.py`
- Create: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_proxy.py`
- Create: `/Users/wgu/Desktop/CodeGraphAgent/tests/fixtures/fake_engine.py`

- [ ] **Step 1: Create the fake engine fixture**

Create `/Users/wgu/Desktop/CodeGraphAgent/tests/fixtures/__init__.py` (empty).

Create `/Users/wgu/Desktop/CodeGraphAgent/tests/fixtures/fake_engine.py`:

```python
"""A fake MCP engine used in proxy tests.

Reads JSON-RPC frames line-by-line from stdin, echoes them back to stdout with
a `"proxied": true` marker injected. Honors a single `{"method": "shutdown"}`
frame to exit cleanly.

Run via: `python -m tests.fixtures.fake_engine`.
"""

from __future__ import annotations

import json
import sys


def main() -> int:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            sys.stderr.write(f"fake_engine: bad json: {line!r}\n")
            sys.stderr.flush()
            continue
        if msg.get("method") == "shutdown":
            return 0
        msg["proxied"] = True
        sys.stdout.write(json.dumps(msg) + "\n")
        sys.stdout.flush()
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: Write failing test**

Create `/Users/wgu/Desktop/CodeGraphAgent/tests/test_proxy.py`:

```python
"""proxy.py — bidirectional stdio piping between BioRouter and the engine."""

from __future__ import annotations

import io
import json
import subprocess
import sys
import threading
import time
from pathlib import Path

import pytest

from codegraphagent import proxy


def test_proxy_run_forwards_stdin_to_child_and_back(tmp_path: Path):
    """Send three JSON-RPC frames into the proxy's stdin, assert the same
    frames come out the proxy's stdout, marked with `proxied: true` by the
    fake engine."""
    parent_in = io.BytesIO(
        b'{"id":1,"method":"initialize"}\n'
        b'{"id":2,"method":"tools/list"}\n'
        b'{"method":"shutdown"}\n'
    )
    parent_out = io.BytesIO()

    # Use the current interpreter to run the fake engine as the "launcher".
    # proxy.run accepts a launcher path + arguments; we override the spawn
    # to use python -m tests.fixtures.fake_engine.
    repo_root = Path(__file__).resolve().parent.parent
    rc = proxy.run(
        launcher=Path(sys.executable),
        cwd=tmp_path,
        argv_override=["-m", "tests.fixtures.fake_engine"],
        env_override={"PYTHONPATH": str(repo_root)},
        stdin=parent_in,
        stdout=parent_out,
        stderr=sys.stderr.buffer,
    )

    assert rc == 0
    lines = [l for l in parent_out.getvalue().decode().splitlines() if l]
    assert len(lines) == 2  # shutdown frame is consumed by the engine
    parsed = [json.loads(l) for l in lines]
    assert parsed[0]["proxied"] is True and parsed[0]["id"] == 1
    assert parsed[1]["proxied"] is True and parsed[1]["id"] == 2
```

- [ ] **Step 3: Run, verify fail**

Run:
```bash
pytest tests/test_proxy.py -v
```

Expected: ImportError on `codegraphagent.proxy`.

- [ ] **Step 4: Implement**

Create `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/proxy.py`:

```python
"""Bidirectional stdio piping between BioRouter (parent) and the CodeGraph
engine (child).

The proxy is byte-level: JSON-RPC frames pass through unchanged in both
directions. Two background threads pump bytes; a third drains stderr into the
parent's stderr (which BioRouter logs).
"""

from __future__ import annotations

import os
import subprocess
import sys
import threading
from pathlib import Path
from typing import BinaryIO


_PUMP_CHUNK = 64 * 1024


def _pump(src: BinaryIO, dst: BinaryIO, *, close_dst_on_eof: bool = False) -> None:
    """Copy bytes from src to dst until src is closed/EOF.

    If `close_dst_on_eof` is True, closes dst when src returns EOF — needed for
    the parent_stdin → child_stdin direction so the engine sees EOF and exits
    cleanly when BioRouter closes its end. The child→parent directions leave
    dst open so the parent process can flush other output.
    """
    try:
        while True:
            chunk = src.read(_PUMP_CHUNK)
            if not chunk:
                break
            dst.write(chunk)
            dst.flush()
    except (BrokenPipeError, ValueError):
        # Either side closed; nothing to do.
        pass
    finally:
        if close_dst_on_eof:
            try:
                dst.close()
            except Exception:
                pass


def run(
    *,
    launcher: Path,
    cwd: Path,
    argv_override: list[str] | None = None,
    env_override: dict[str, str] | None = None,
    stdin: BinaryIO | None = None,
    stdout: BinaryIO | None = None,
    stderr: BinaryIO | None = None,
) -> int:
    """Spawn the engine and pump stdio between parent and child.

    Args:
        launcher: Path to the engine launcher (or Python interpreter for tests).
        cwd: Working directory for the child (the project root).
        argv_override: Replace the default `["serve", "--mcp"]` argv. Tests
            use this to invoke a fake engine.
        env_override: Additional env vars to pass to the child.
        stdin/stdout/stderr: Override the parent's IO streams (tests use this).

    Returns the child's exit code.
    """
    argv = [str(launcher), *(argv_override or ["serve", "--mcp"])]
    env = {**os.environ}
    if env_override:
        env.update(env_override)

    parent_stdin = stdin or sys.stdin.buffer
    parent_stdout = stdout or sys.stdout.buffer
    parent_stderr = stderr or sys.stderr.buffer

    child = subprocess.Popen(
        argv,
        cwd=str(cwd),
        env=env,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        bufsize=0,
    )

    threads = [
        threading.Thread(
            target=_pump,
            args=(parent_stdin, child.stdin),
            kwargs={"close_dst_on_eof": True},
            daemon=True,
        ),
        threading.Thread(target=_pump, args=(child.stdout, parent_stdout), daemon=True),
        threading.Thread(target=_pump, args=(child.stderr, parent_stderr), daemon=True),
    ]
    for t in threads:
        t.start()

    rc = child.wait()
    # Wait briefly for the stdout/stderr pumps to finish flushing.
    for t in threads[1:]:
        t.join(timeout=2.0)
    return rc
```

- [ ] **Step 5: Run, verify pass**

Run:
```bash
pytest tests/test_proxy.py -v
```

Expected: 1 passed.

- [ ] **Step 6: Commit**

Run:
```bash
git add src/codegraphagent/proxy.py tests/test_proxy.py tests/fixtures/ && \
git commit -m "feat(proxy): bidirectional stdio piping with three-thread pump"
```

### Task D2: `proxy.run` — exit-code propagation test

**Files:**
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/tests/fixtures/fake_engine.py`
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_proxy.py`

- [ ] **Step 1: Add an exit-code mode to the fake engine**

Replace `tests/fixtures/fake_engine.py` with:

```python
"""A fake MCP engine used in proxy tests.

- Default mode: echo frames back with `"proxied": true`. Exit 0 on `shutdown`.
- `--exit-code N` mode: read one frame, then exit with code N.
"""

from __future__ import annotations

import json
import sys


def main(argv: list[str]) -> int:
    if len(argv) >= 2 and argv[1] == "--exit-code":
        sys.stdin.readline()
        return int(argv[2])

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            sys.stderr.write(f"fake_engine: bad json: {line!r}\n")
            sys.stderr.flush()
            continue
        if msg.get("method") == "shutdown":
            return 0
        msg["proxied"] = True
        sys.stdout.write(json.dumps(msg) + "\n")
        sys.stdout.flush()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
```

- [ ] **Step 2: Add failing test**

Append to `tests/test_proxy.py`:

```python


def test_proxy_propagates_child_exit_code(tmp_path: Path):
    parent_in = io.BytesIO(b'{"id":1}\n')
    parent_out = io.BytesIO()
    parent_err = io.BytesIO()

    repo_root = Path(__file__).resolve().parent.parent
    rc = proxy.run(
        launcher=Path(sys.executable),
        cwd=tmp_path,
        argv_override=["-m", "tests.fixtures.fake_engine", "--exit-code", "42"],
        env_override={"PYTHONPATH": str(repo_root)},
        stdin=parent_in,
        stdout=parent_out,
        stderr=parent_err,
    )
    assert rc == 42
```

- [ ] **Step 3: Run, verify pass**

Run:
```bash
pytest tests/test_proxy.py -v
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

Run:
```bash
git add tests/fixtures/fake_engine.py tests/test_proxy.py && \
git commit -m "test(proxy): exit-code propagation"
```

---

## Phase E — Python shim: cli orchestration + degraded mode

### Task E1: Degraded-mode error shim

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/error_shim.py`
- Create: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_error_shim.py`

- [ ] **Step 1: Write failing test**

Create `/Users/wgu/Desktop/CodeGraphAgent/tests/test_error_shim.py`:

```python
"""error_shim — minimal MCP server that surfaces a single error tool when the
shim couldn't reach the engine."""

from __future__ import annotations

import io
import json
import threading

from codegraphagent import error_shim
from codegraphagent.errors import BootstrapError, LayoutConflictError


def _send(stream: io.BytesIO, frame: dict) -> None:
    stream.write((json.dumps(frame) + "\n").encode())


def test_error_shim_lists_bootstrap_error_tool():
    parent_in = io.BytesIO()
    _send(parent_in, {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}})
    _send(parent_in, {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
    _send(parent_in, {"jsonrpc": "2.0", "id": 3, "method": "shutdown"})
    parent_in.seek(0)
    parent_out = io.BytesIO()

    exc = BootstrapError(
        "boom",
        url="https://example.com/x.tar.gz",
        expected_sha="abc",
        observed_sha="def",
    )
    error_shim.serve(exc, stdin=parent_in, stdout=parent_out)

    frames = [json.loads(l) for l in parent_out.getvalue().decode().splitlines() if l]
    # frames: initialize response, tools/list response, shutdown response
    assert len(frames) == 3
    tools_list = frames[1]["result"]["tools"]
    assert any(t["name"] == "codegraphagent_bootstrap_error" for t in tools_list)


def test_error_shim_call_returns_error_details():
    parent_in = io.BytesIO()
    _send(parent_in, {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}})
    _send(parent_in, {
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": "codegraphagent_bootstrap_error", "arguments": {}},
    })
    _send(parent_in, {"jsonrpc": "2.0", "id": 3, "method": "shutdown"})
    parent_in.seek(0)
    parent_out = io.BytesIO()

    exc = BootstrapError(
        "download failed",
        url="https://example.com/x.tar.gz",
    )
    error_shim.serve(exc, stdin=parent_in, stdout=parent_out)

    frames = [json.loads(l) for l in parent_out.getvalue().decode().splitlines() if l]
    call_result = frames[1]["result"]
    text = call_result["content"][0]["text"]
    assert "download failed" in text
    assert "https://example.com/x.tar.gz" in text


def test_error_shim_layout_conflict():
    parent_in = io.BytesIO()
    _send(parent_in, {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}})
    _send(parent_in, {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
    _send(parent_in, {"jsonrpc": "2.0", "id": 3, "method": "shutdown"})
    parent_in.seek(0)
    parent_out = io.BytesIO()

    exc = LayoutConflictError("path exists", path="/tmp/foo/.codegraph")
    error_shim.serve(exc, stdin=parent_in, stdout=parent_out)

    frames = [json.loads(l) for l in parent_out.getvalue().decode().splitlines() if l]
    tools_list = frames[1]["result"]["tools"]
    assert any(t["name"] == "codegraphagent_setup_error" for t in tools_list)
```

- [ ] **Step 2: Run, verify fail**

Run:
```bash
pytest tests/test_error_shim.py -v
```

Expected: ImportError on `codegraphagent.error_shim`.

- [ ] **Step 3: Implement the error shim**

Create `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/error_shim.py`:

```python
"""Degraded-mode MCP server.

When bootstrap or layout setup fails, the main shim hands control here. This
server still satisfies the MCP `initialize` + `tools/list` + `tools/call`
handshake — but the only tool it offers returns the underlying error so the
agent (and the user reading the log) can see exactly what went wrong.

Implemented in pure stdlib to keep it functional even if fastmcp's import
itself is what failed.
"""

from __future__ import annotations

import json
import sys
from typing import BinaryIO

from codegraphagent.errors import (
    BootstrapError,
    CodeGraphAgentError,
    LayoutConflictError,
)


def _tool_for(exc: CodeGraphAgentError) -> dict:
    """Return the synthetic tool descriptor matching the given error type."""
    if isinstance(exc, BootstrapError):
        return {
            "name": "codegraphagent_bootstrap_error",
            "description": (
                "CodeGraphAgent failed to download or verify the engine bundle. "
                "Call this tool to see the underlying error."
            ),
            "inputSchema": {"type": "object", "properties": {}, "required": []},
        }
    if isinstance(exc, LayoutConflictError):
        return {
            "name": "codegraphagent_setup_error",
            "description": (
                "CodeGraphAgent could not set up the .biorouter/codegraph "
                "symlink. Call this tool to see the remediation steps."
            ),
            "inputSchema": {"type": "object", "properties": {}, "required": []},
        }
    return {
        "name": "codegraphagent_error",
        "description": "CodeGraphAgent encountered an internal error.",
        "inputSchema": {"type": "object", "properties": {}, "required": []},
    }


def _error_text(exc: CodeGraphAgentError) -> str:
    parts = [f"CodeGraphAgent error: {exc}"]
    if isinstance(exc, BootstrapError):
        if exc.url:
            parts.append(f"URL: {exc.url}")
        if exc.expected_sha and exc.observed_sha:
            parts.append(f"Expected SHA-256: {exc.expected_sha}")
            parts.append(f"Observed SHA-256: {exc.observed_sha}")
        parts.append(
            "Recovery: retry; or set CODEGRAPH_ENGINE_PATH to a pre-downloaded "
            "bundle; or pin CODEGRAPH_ENGINE_VERSION to a different release."
        )
    elif isinstance(exc, LayoutConflictError):
        parts.append(f"Path: {exc.path}")
        parts.append(
            "Recovery: rename or remove the conflicting .codegraph directory, "
            "then restart the CodeGraphAgent extension."
        )
    return "\n".join(parts)


def serve(
    exc: CodeGraphAgentError,
    *,
    stdin: BinaryIO | None = None,
    stdout: BinaryIO | None = None,
) -> None:
    """Serve MCP requests in degraded mode until `shutdown` is received."""
    inp = stdin or sys.stdin.buffer
    out = stdout or sys.stdout.buffer
    tool = _tool_for(exc)

    while True:
        line = inp.readline()
        if not line:
            break
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue

        method = req.get("method")
        req_id = req.get("id")

        if method == "initialize":
            resp = {
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "protocolVersion": req.get("params", {}).get(
                        "protocolVersion", "2024-11-05"
                    ),
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "codegraphagent (degraded)", "version": "0.1.0"},
                },
            }
        elif method == "tools/list":
            resp = {
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {"tools": [tool]},
            }
        elif method == "tools/call":
            resp = {
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "content": [{"type": "text", "text": _error_text(exc)}],
                    "isError": True,
                },
            }
        elif method == "shutdown":
            resp = {"jsonrpc": "2.0", "id": req_id, "result": None}
            out.write((json.dumps(resp) + "\n").encode())
            out.flush()
            break
        else:
            resp = {
                "jsonrpc": "2.0",
                "id": req_id,
                "error": {"code": -32601, "message": f"Method not found: {method}"},
            }
        out.write((json.dumps(resp) + "\n").encode())
        out.flush()
```

- [ ] **Step 4: Run, verify pass**

Run:
```bash
pytest tests/test_error_shim.py -v
```

Expected: 3 passed.

- [ ] **Step 5: Commit**

Run:
```bash
git add src/codegraphagent/error_shim.py tests/test_error_shim.py && \
git commit -m "feat(error-shim): degraded-mode MCP server for setup/bootstrap failures"
```

### Task E2: Wire up `cli.main`

**Files:**
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/cli.py`
- Create: `/Users/wgu/Desktop/CodeGraphAgent/tests/test_cli.py`

- [ ] **Step 1: Write failing tests**

Create `/Users/wgu/Desktop/CodeGraphAgent/tests/test_cli.py`:

```python
"""cli.main — orchestrates paths.ensure_layout → bootstrap.ensure_engine → proxy.run."""

from __future__ import annotations

from pathlib import Path
from unittest.mock import patch, MagicMock

import pytest

from codegraphagent import cli
from codegraphagent.errors import BootstrapError, LayoutConflictError


def test_main_happy_path(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv("BIOROUTER_WORKING_DIR", str(tmp_path))
    (tmp_path / ".git").mkdir()

    fake_launcher = tmp_path / "fake-launcher"
    fake_launcher.write_text("")

    proxy_run = MagicMock(return_value=0)

    with patch.object(cli.bootstrap, "ensure_engine", return_value=fake_launcher), \
         patch.object(cli.proxy, "run", proxy_run):
        rc = cli.main()

    assert rc == 0
    proxy_run.assert_called_once()
    kwargs = proxy_run.call_args.kwargs
    assert kwargs["launcher"] == fake_launcher
    assert kwargs["cwd"] == tmp_path.resolve()


def test_main_falls_back_to_error_shim_on_bootstrap_error(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    monkeypatch.setenv("BIOROUTER_WORKING_DIR", str(tmp_path))
    (tmp_path / ".git").mkdir()

    err = BootstrapError("nope", url="https://example.com/x")
    error_shim_serve = MagicMock()

    with patch.object(cli.bootstrap, "ensure_engine", side_effect=err), \
         patch.object(cli.error_shim, "serve", error_shim_serve):
        rc = cli.main()

    assert rc == 0
    error_shim_serve.assert_called_once()
    assert error_shim_serve.call_args.args[0] is err


def test_main_falls_back_to_error_shim_on_layout_conflict(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    monkeypatch.setenv("BIOROUTER_WORKING_DIR", str(tmp_path))
    (tmp_path / ".git").mkdir()
    (tmp_path / ".codegraph").mkdir()

    error_shim_serve = MagicMock()
    with patch.object(cli.error_shim, "serve", error_shim_serve):
        rc = cli.main()

    assert rc == 0
    error_shim_serve.assert_called_once()
    assert isinstance(error_shim_serve.call_args.args[0], LayoutConflictError)
```

- [ ] **Step 2: Run, verify fail**

Run:
```bash
pytest tests/test_cli.py -v
```

Expected: AttributeError / failure (cli.main is still a stub).

- [ ] **Step 3: Rewrite `cli.py`**

Replace `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/cli.py`:

```python
"""CLI entry point for codegraphagent.

Orchestrates the three pieces of the shim:
  1. paths — resolve the project root and ensure the .biorouter/codegraph
     state dir + .codegraph symlink exist.
  2. bootstrap — ensure the vendored engine bundle is downloaded, verified,
     and extracted.
  3. proxy — spawn the engine and pump MCP traffic.

If either of the first two fails, hand control to the degraded-mode error
shim so the agent gets a structured error frame rather than an opaque crash.
"""

from __future__ import annotations

from codegraphagent import bootstrap, error_shim, paths, proxy
from codegraphagent.errors import CodeGraphAgentError


def main() -> int:
    try:
        root = paths.resolve_project_root()
        paths.ensure_layout(root)
        launcher = bootstrap.ensure_engine()
    except CodeGraphAgentError as exc:
        error_shim.serve(exc)
        return 0

    return proxy.run(launcher=launcher, cwd=root)
```

- [ ] **Step 4: Run, verify pass**

Run:
```bash
pytest tests/test_cli.py -v
```

Expected: 3 passed.

- [ ] **Step 5: Run the full suite**

Run:
```bash
pytest -v
```

Expected: all tests pass (Phase A through E).

- [ ] **Step 6: Commit**

Run:
```bash
git add src/codegraphagent/cli.py tests/test_cli.py && \
git commit -m "feat(cli): wire paths + bootstrap + proxy + degraded mode"
```

---

## Phase F — Engine vendoring (unchanged from upstream)

### Task F1: Vendor upstream CodeGraph at the pinned commit

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/engine/UPSTREAM.md`
- Create: `/Users/wgu/Desktop/CodeGraphAgent/engine/PATCHES.md`
- Create (vendored from upstream): `/Users/wgu/Desktop/CodeGraphAgent/engine/**`

- [ ] **Step 1: Identify the upstream tag to vendor**

Run:
```bash
gh api repos/colbymchenry/codegraph/releases/latest \
  --jq '{tag: .tag_name, sha: .target_commitish, date: .published_at}'
```

Record the tag (e.g. `v0.9.7`) and resolve the SHA:

```bash
LATEST_TAG=$(gh api repos/colbymchenry/codegraph/releases/latest --jq .tag_name)
UPSTREAM_SHA=$(gh api repos/colbymchenry/codegraph/git/refs/tags/$LATEST_TAG --jq .object.sha)
echo "tag=$LATEST_TAG sha=$UPSTREAM_SHA"
```

Save both values; they go into `UPSTREAM.md` shortly.

- [ ] **Step 2: Clone upstream at that tag and copy into `engine/`**

Run:
```bash
cd /tmp && \
gh repo clone colbymchenry/codegraph upstream-codegraph -- --depth 1 --branch $LATEST_TAG && \
cd /Users/wgu/Desktop/CodeGraphAgent && \
rsync -a --exclude='.git' --exclude='.github' --exclude='node_modules' \
  /tmp/upstream-codegraph/ engine/
```

(We strip `.github/` to keep upstream's release workflow from running in our repo; we'll add our own in Phase G.)

- [ ] **Step 3: Write `engine/UPSTREAM.md`**

Create `/Users/wgu/Desktop/CodeGraphAgent/engine/UPSTREAM.md`:

```markdown
# Upstream Provenance

This directory is a flat copy of [CodeGraph](https://github.com/colbymchenry/codegraph)
at a pinned commit. We do not preserve upstream's git history here.

| Field | Value |
| --- | --- |
| Upstream repo | https://github.com/colbymchenry/codegraph |
| Vendored tag | <LATEST_TAG from Step 1> |
| Vendored commit SHA | <UPSTREAM_SHA from Step 1> |
| Vendored on | 2026-05-30 |

## Updating

To pull in newer upstream changes, run `scripts/sync-upstream.sh` (added in
Phase G). It fetches upstream at a target tag, three-way merges into `engine/`,
and updates this file with the new SHA. Conflicts surface as a normal PR.

## What we change

See `engine/PATCHES.md` for the list of modifications layered on top of this
upstream snapshot.
```

Open the file and fill in the actual `<LATEST_TAG>` and `<UPSTREAM_SHA>` values from Step 1.

- [ ] **Step 4: Write `engine/PATCHES.md`**

Create `/Users/wgu/Desktop/CodeGraphAgent/engine/PATCHES.md`:

```markdown
# Patches Layered on Top of Upstream

In this plan (foundation, v0.1.0-rc1) the engine is **unmodified** from the
pinned upstream commit recorded in `UPSTREAM.md`. The only deltas vs upstream
are:

- Upstream's `.github/` workflows have been removed (we run our own in the
  monorepo root's `.github/workflows/`).
- Upstream's `.git/` directory is absent (flat copy, see `UPSTREAM.md`).

Plan 2 (bio-languages) will add per-language patches for R, Julia, MATLAB,
Perl — each documented as a numbered entry below when it lands.
```

- [ ] **Step 5: Sanity-check that the vendored engine builds locally**

Requires Node 22.5+ on the engineer's machine.

Run:
```bash
node --version  # confirm >= 22.5
cd /Users/wgu/Desktop/CodeGraphAgent/engine && \
npm install --silent && \
npm run build  # or whatever upstream's package.json defines
```

Expected: `npm install` and `npm run build` complete without errors. If `npm run build` doesn't exist in upstream's package.json, skip — the build step lives in `scripts/build-bundle.sh`.

- [ ] **Step 6: Commit the vendored engine**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
git add engine/ && \
git commit -m "vendor(engine): import upstream CodeGraph at $LATEST_TAG ($UPSTREAM_SHA)"
```

This will be a large commit (~3 MB + WASM files). That's expected.

### Task F2: Verify upstream's `build-bundle.sh` works locally

**Files:**
- (No new files — verifies existing vendored script.)

- [ ] **Step 1: Identify the local platform tag**

Run:
```bash
uname -sm
# Darwin arm64 → darwin-arm64
# Linux x86_64 → linux-x64
# etc.
```

- [ ] **Step 2: Build the local-platform bundle**

Run (replace `darwin-arm64` with whatever matches your platform):
```bash
cd /Users/wgu/Desktop/CodeGraphAgent/engine && \
bash scripts/build-bundle.sh darwin-arm64
```

Expected: produces `engine/release/codegraph-darwin-arm64.tar.gz` (size ~40-50 MB).

- [ ] **Step 3: Inspect the bundle**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent/engine && \
tar -tzf release/codegraph-darwin-arm64.tar.gz | head -20
```

Expected output includes `bin/codegraph`, `node`, and `lib/dist/...` entries.

- [ ] **Step 4: Smoke-test the bundle**

Run:
```bash
cd /tmp && \
rm -rf cga-bundle-test && \
mkdir cga-bundle-test && \
tar -xzf /Users/wgu/Desktop/CodeGraphAgent/engine/release/codegraph-darwin-arm64.tar.gz -C cga-bundle-test && \
cga-bundle-test/bin/codegraph --version
```

Expected: prints the upstream CodeGraph version (matches `LATEST_TAG`).

- [ ] **Step 5: Commit nothing — but record the working build in a CHANGELOG line**

Create `/Users/wgu/Desktop/CodeGraphAgent/CHANGELOG.md`:

```markdown
# Changelog

## v0.1.0-rc1 (in progress)

- Initial release scaffolding.
- Python proxy shim (paths, bootstrap, proxy, error shim).
- Vendored CodeGraph engine from upstream <LATEST_TAG>.
- Verified `build-bundle.sh` produces a working local-platform bundle.
```

Fill in `<LATEST_TAG>`.

Run:
```bash
git add CHANGELOG.md && git commit -m "docs: add CHANGELOG for v0.1.0-rc1"
```

---

## Phase G — Build scripts and GitHub Actions

### Task G1: `scripts/build-brxt.sh`

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/scripts/build-brxt.sh`

- [ ] **Step 1: Write the script**

Create `/Users/wgu/Desktop/CodeGraphAgent/scripts/build-brxt.sh`:

```bash
#!/usr/bin/env bash
# Build codegraphagent.brxt — a ZIP archive of the .brxt payload.
#
# Excludes:
#   - tests/        (not needed at runtime)
#   - engine/       (the engine is downloaded on first use, not bundled)
#   - .venv/, __pycache__, .pytest_cache, .git
#   - any prior codegraphagent.brxt

set -euo pipefail

cd "$(dirname "$0")/.."
REPO_ROOT="$(pwd)"
OUT="${REPO_ROOT}/codegraphagent.brxt"

rm -f "$OUT"

# zip preserves directory layout; we just include the files the .brxt format requires.
zip -r "$OUT" \
  manifest.json \
  README.md \
  pyproject.toml \
  src/codegraphagent \
  -x '*/__pycache__/*' '*.pyc'

echo
echo "Built: $OUT"
ls -lh "$OUT"
```

- [ ] **Step 2: Make it executable and run it**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
chmod +x scripts/build-brxt.sh && \
scripts/build-brxt.sh
```

Expected: produces `codegraphagent.brxt` (~20-40 KB).

- [ ] **Step 3: Validate the .brxt contents**

Run:
```bash
unzip -l codegraphagent.brxt
```

Expected: lists `manifest.json`, `README.md`, `pyproject.toml`, and `src/codegraphagent/*.py`.

- [ ] **Step 4: Commit**

Run:
```bash
git add scripts/build-brxt.sh && \
git commit -m "build: scripts/build-brxt.sh produces codegraphagent.brxt"
```

### Task G2: `scripts/sync-upstream.sh`

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/scripts/sync-upstream.sh`

- [ ] **Step 1: Write the sync script**

Create `/Users/wgu/Desktop/CodeGraphAgent/scripts/sync-upstream.sh`:

```bash
#!/usr/bin/env bash
# Sync engine/ with a newer upstream CodeGraph release.
#
# Usage: scripts/sync-upstream.sh <upstream-tag>
#
# Workflow:
#   1. Clone upstream at the target tag into a tmp dir.
#   2. rsync into engine/ (preserving our patches isn't fully automatic —
#      conflicts on files we patched must be resolved by hand in the resulting
#      diff).
#   3. Update engine/UPSTREAM.md with the new tag/SHA.
#   4. Leaves the working tree dirty so the user can review + commit.

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <upstream-tag>"
  exit 64
fi

TAG="$1"
cd "$(dirname "$0")/.."
REPO_ROOT="$(pwd)"

TMP="$(mktemp -d -t cga-sync-XXXXXX)"
trap "rm -rf '$TMP'" EXIT

echo "[1/4] Cloning upstream at $TAG..."
gh repo clone colbymchenry/codegraph "$TMP/upstream" -- --depth 1 --branch "$TAG"

SHA="$(cd "$TMP/upstream" && git rev-parse HEAD)"

echo "[2/4] Syncing into engine/..."
rsync -a --delete \
  --exclude='.git' \
  --exclude='.github' \
  --exclude='node_modules' \
  --exclude='release' \
  --exclude='UPSTREAM.md' \
  --exclude='PATCHES.md' \
  "$TMP/upstream/" engine/

echo "[3/4] Updating engine/UPSTREAM.md..."
DATE="$(date +%Y-%m-%d)"
python3 - <<EOF
from pathlib import Path
p = Path("engine/UPSTREAM.md")
text = p.read_text()
text = text.replace(p.read_text().split("Vendored tag")[1].split("\n")[0],
                    f" | $TAG |", 1)
EOF
# Simpler: rewrite the three relevant lines explicitly.
cat > engine/UPSTREAM.md.new <<EOF
# Upstream Provenance

This directory is a flat copy of [CodeGraph](https://github.com/colbymchenry/codegraph)
at a pinned commit. We do not preserve upstream's git history here.

| Field | Value |
| --- | --- |
| Upstream repo | https://github.com/colbymchenry/codegraph |
| Vendored tag | $TAG |
| Vendored commit SHA | $SHA |
| Vendored on | $DATE |

## Updating

Run \`scripts/sync-upstream.sh <new-tag>\`. It fetches upstream at the target
tag, syncs into \`engine/\`, and updates this file. Re-apply our patches as
needed (see \`engine/PATCHES.md\`).

## What we change

See \`engine/PATCHES.md\` for the list of modifications layered on top of this
upstream snapshot.
EOF
mv engine/UPSTREAM.md.new engine/UPSTREAM.md

echo "[4/4] Done. Review with:"
echo "    git status engine/"
echo "    git diff engine/PATCHES.md"
echo
echo "Then re-apply patches per engine/PATCHES.md, run tests, and commit."
```

- [ ] **Step 2: Make it executable**

Run:
```bash
chmod +x /Users/wgu/Desktop/CodeGraphAgent/scripts/sync-upstream.sh
```

- [ ] **Step 3: Commit (do NOT run it now — engine is already at latest)**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
git add scripts/sync-upstream.sh && \
git commit -m "build: scripts/sync-upstream.sh for upstream merges"
```

### Task G3: `.github/workflows/ci.yml`

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/.github/workflows/ci.yml`

- [ ] **Step 1: Write CI workflow**

Create `/Users/wgu/Desktop/CodeGraphAgent/.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  python:
    name: Python shim tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Set up Python
        uses: actions/setup-python@v5
        with:
          python-version: "3.11"
      - name: Install package + dev deps
        run: |
          python -m pip install --upgrade pip
          pip install -e ".[dev]"
      - name: Run pytest
        run: pytest -v

  engine-typecheck:
    name: Engine type-check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Set up Node
        uses: actions/setup-node@v4
        with:
          node-version: "22"
      - name: Install engine deps
        working-directory: engine
        run: npm ci
      - name: Type-check
        working-directory: engine
        run: |
          if [ -f tsconfig.json ]; then
            npx tsc --noEmit
          else
            echo "No tsconfig.json; skipping type-check."
          fi
```

- [ ] **Step 2: Commit and push to trigger CI**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
git add .github/workflows/ci.yml && \
git commit -m "ci: add Python + engine type-check workflow" && \
git push -u origin main
```

- [ ] **Step 3: Watch the CI run succeed**

Run:
```bash
gh run watch
```

Expected: both jobs pass. If type-check fails because upstream uses TS features your Node version doesn't ship — adjust the Node version in the workflow to match what upstream's `package.json` requires.

### Task G4: `.github/workflows/build-engine.yml`

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/.github/workflows/build-engine.yml`

- [ ] **Step 1: Write the workflow**

Create `/Users/wgu/Desktop/CodeGraphAgent/.github/workflows/build-engine.yml`:

```yaml
name: Build engine bundles

on:
  workflow_dispatch:
    inputs:
      release_tag:
        description: "Engine release tag (e.g. engine-v0.1.0)"
        required: true

jobs:
  build:
    name: Build all platform bundles
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Set up Node
        uses: actions/setup-node@v4
        with:
          node-version: "22"
      - name: Install engine deps
        working-directory: engine
        run: npm ci
      - name: Build all bundles
        working-directory: engine
        run: |
          for tgt in darwin-arm64 darwin-x64 linux-x64 linux-arm64 win32-x64 win32-arm64; do
            echo "::group::Build $tgt"
            bash scripts/build-bundle.sh "$tgt"
            echo "::endgroup::"
          done
      - name: Generate SHA256SUMS
        working-directory: engine/release
        run: |
          sha256sum codegraph-*.{tar.gz,zip} 2>/dev/null > SHA256SUMS || \
            shasum -a 256 codegraph-*.{tar.gz,zip} > SHA256SUMS
          cat SHA256SUMS
      - name: Upload artifacts
        uses: actions/upload-artifact@v4
        with:
          name: engine-bundles
          path: |
            engine/release/codegraph-*.tar.gz
            engine/release/codegraph-*.zip
            engine/release/SHA256SUMS
          retention-days: 30
      - name: Create draft GitHub Release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          gh release create "${{ inputs.release_tag }}" \
            --title "Engine ${{ inputs.release_tag }}" \
            --notes "Auto-built engine bundles from $(git rev-parse --short HEAD)." \
            --draft \
            engine/release/codegraph-*.tar.gz \
            engine/release/codegraph-*.zip \
            engine/release/SHA256SUMS
```

- [ ] **Step 2: Commit and push**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
git add .github/workflows/build-engine.yml && \
git commit -m "ci: build-engine workflow for cross-platform bundles" && \
git push
```

- [ ] **Step 3: Verify the workflow file parses**

Run:
```bash
gh workflow list
```

Expected: `Build engine bundles` appears in the list (not yet executed).

### Task G5: `.github/workflows/release-brxt.yml`

**Files:**
- Create: `/Users/wgu/Desktop/CodeGraphAgent/.github/workflows/release-brxt.yml`

- [ ] **Step 1: Write the workflow**

Create `/Users/wgu/Desktop/CodeGraphAgent/.github/workflows/release-brxt.yml`:

```yaml
name: Release .brxt

on:
  workflow_dispatch:
    inputs:
      release_tag:
        description: ".brxt release tag (e.g. v0.1.0-rc1)"
        required: true

jobs:
  release:
    name: Build and publish .brxt
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Set up Python
        uses: actions/setup-python@v5
        with:
          python-version: "3.11"
      - name: Verify release_manifest.json has no placeholder SHAs
        run: |
          if grep -q "PLACEHOLDER_FILLED_AT_RELEASE" src/codegraphagent/release_manifest.json; then
            echo "::error::release_manifest.json still contains PLACEHOLDER_FILLED_AT_RELEASE values."
            echo "Run Task L2 to update SHAs before releasing the .brxt."
            exit 1
          fi
      - name: Build .brxt
        run: bash scripts/build-brxt.sh
      - name: Create GitHub Release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          gh release create "${{ inputs.release_tag }}" \
            --title "CodeGraphAgent ${{ inputs.release_tag }}" \
            --notes-file CHANGELOG.md \
            codegraphagent.brxt
```

- [ ] **Step 2: Commit and push**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
git add .github/workflows/release-brxt.yml && \
git commit -m "ci: release-brxt workflow" && \
git push
```

---

## Phase H — First engine release

### Task H1: Trigger the engine build workflow

- [ ] **Step 1: Trigger workflow_dispatch**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
gh workflow run "Build engine bundles" -f release_tag=engine-v0.1.0
```

Expected: prints `✓ Created workflow_dispatch event for build-engine.yml at main`.

- [ ] **Step 2: Watch the run**

Run:
```bash
gh run watch
```

Expected: completes successfully in ~10-20 min. Produces 6 tarballs/zips + `SHA256SUMS`.

- [ ] **Step 3: Confirm draft release was created**

Run:
```bash
gh release view engine-v0.1.0
```

Expected: shows the draft release with 7 assets (6 bundles + `SHA256SUMS`).

- [ ] **Step 4: Publish the release**

Run:
```bash
gh release edit engine-v0.1.0 --draft=false
```

Expected: release moves from Draft to Published.

### Task H2: Update `release_manifest.json` with real SHAs

**Files:**
- Modify: `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/release_manifest.json`

- [ ] **Step 1: Pull the SHA256SUMS file**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
gh release download engine-v0.1.0 --pattern 'SHA256SUMS' --output SHA256SUMS && \
cat SHA256SUMS
```

Expected output (example — actual SHAs will differ):
```
abc123... codegraph-darwin-arm64.tar.gz
def456... codegraph-darwin-x64.tar.gz
...
```

- [ ] **Step 2: Update `release_manifest.json`**

Open `/Users/wgu/Desktop/CodeGraphAgent/src/codegraphagent/release_manifest.json` and replace each `PLACEHOLDER_FILLED_AT_RELEASE` value with the corresponding SHA from `SHA256SUMS`. Final file looks like:

```json
{
  "engine_version": "0.1.0",
  "base_url": "https://github.com/Broccolito/CodeGraphAgent/releases/download/engine-v0.1.0/",
  "platforms": {
    "darwin-arm64":  {"filename": "codegraph-darwin-arm64.tar.gz",  "sha256": "<from SHA256SUMS>"},
    "darwin-x64":    {"filename": "codegraph-darwin-x64.tar.gz",    "sha256": "<from SHA256SUMS>"},
    "linux-x64":     {"filename": "codegraph-linux-x64.tar.gz",     "sha256": "<from SHA256SUMS>"},
    "linux-arm64":   {"filename": "codegraph-linux-arm64.tar.gz",   "sha256": "<from SHA256SUMS>"},
    "win32-x64":     {"filename": "codegraph-win32-x64.zip",        "sha256": "<from SHA256SUMS>"},
    "win32-arm64":   {"filename": "codegraph-win32-arm64.zip",      "sha256": "<from SHA256SUMS>"}
  }
}
```

- [ ] **Step 3: Sanity-check the file**

Run:
```bash
python3 -c "
import json
d = json.load(open('src/codegraphagent/release_manifest.json'))
for tag, info in d['platforms'].items():
    assert info['sha256'] != 'PLACEHOLDER_FILLED_AT_RELEASE', tag
    assert len(info['sha256']) == 64, tag
print('All SHAs look real.')
"
```

Expected: `All SHAs look real.`

- [ ] **Step 4: Remove the downloaded `SHA256SUMS` file**

Run:
```bash
rm SHA256SUMS
```

- [ ] **Step 5: Commit**

Run:
```bash
git add src/codegraphagent/release_manifest.json && \
git commit -m "release: pin engine-v0.1.0 SHA256s in release_manifest.json"
```

### Task H3: Integration smoke test — shim + real engine

**Files:**
- (No new files — runs the shim against the real downloaded engine.)

- [ ] **Step 1: Create a test project**

Run:
```bash
mkdir -p /tmp/cga-smoke-project && \
cd /tmp/cga-smoke-project && \
git init -q && \
echo 'def hello():\n    return 42' > main.py
```

- [ ] **Step 2: Run the shim and exercise the engine via an MCP initialize/tools-list/shutdown sequence**

Run (note: activates the dev venv so `codegraphagent` is importable):
```bash
source /Users/wgu/Desktop/CodeGraphAgent/.venv/bin/activate && \
cd /tmp/cga-smoke-project && \
BIOROUTER_WORKING_DIR=/tmp/cga-smoke-project \
  python3 -c "
import json, subprocess, sys

p = subprocess.Popen(
    [sys.executable, '-m', 'codegraphagent'],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
)
def send(msg):
    p.stdin.write((json.dumps(msg) + '\n').encode())
    p.stdin.flush()

send({'jsonrpc':'2.0','id':1,'method':'initialize','params':{'protocolVersion':'2024-11-05','capabilities':{},'clientInfo':{'name':'smoke','version':'0'}}})
send({'jsonrpc':'2.0','id':2,'method':'tools/list'})
send({'jsonrpc':'2.0','id':3,'method':'shutdown'})

# Read up to 3 response frames
import select
buf = b''
while p.poll() is None:
    r,_,_ = select.select([p.stdout], [], [], 30)
    if not r: break
    chunk = p.stdout.read1(4096)
    if not chunk: break
    buf += chunk
    if buf.count(b'\n') >= 3: break
print('STDOUT:', buf.decode(errors='replace'))
err = p.stderr.read()
print('STDERR:', err.decode(errors='replace'))
p.wait(timeout=5)
"
```

Expected: STDOUT contains a `tools/list` response listing `codegraph_search`, `codegraph_callers`, etc. (~9 tools).

- [ ] **Step 3: Verify the symlink and DB**

Run:
```bash
ls -la /tmp/cga-smoke-project/.codegraph && \
ls /tmp/cga-smoke-project/.biorouter/codegraph/
```

Expected: `.codegraph` is a symlink → `.biorouter/codegraph/`, and `.biorouter/codegraph/codegraph.db` exists.

- [ ] **Step 4: Clean up**

Run:
```bash
rm -rf /tmp/cga-smoke-project
```

- [ ] **Step 5: Commit the success (no file changes — record in CHANGELOG)**

Edit `/Users/wgu/Desktop/CodeGraphAgent/CHANGELOG.md` to add under `v0.1.0-rc1`:

```markdown
- End-to-end smoke test verified: shim spawns engine, tools list returns ~9
  codegraph_* tools, symlink + DB materialize at expected paths.
```

Run:
```bash
git add CHANGELOG.md && \
git commit -m "docs: record successful E2E smoke test"
```

---

## Phase L — First .brxt release

### Task L1: Build the .brxt locally and install it into a dev BioRouter

**Files:**
- (No new files; uses existing `scripts/build-brxt.sh`.)

- [ ] **Step 1: Build the .brxt**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
scripts/build-brxt.sh
```

Expected: `codegraphagent.brxt` (~20-40 KB) created at repo root.

- [ ] **Step 2: Install into local BioRouter**

In BioRouter (running locally — see `/Users/wgu/Desktop/biorouter/CLAUDE.md` for run commands):
1. Open BioRouter
2. Settings → Extensions → Install from file
3. Select `/Users/wgu/Desktop/CodeGraphAgent/codegraphagent.brxt`
4. Wait for the BioRouter install flow to run `uv sync`

Expected: extension shows as installed but disabled (per BioRouter's default-off bundling behavior — implemented in a separate BioRouter spec, but manual install respects the same default).

- [ ] **Step 3: Enable the extension and test in a real project**

In BioRouter:
1. Settings → Extensions → CodeGraphAgent → Enable
2. Open a session against `/Users/wgu/Desktop/biorouter` (a real Rust + TS project)
3. Ask the agent: "use codegraph_search to find functions named 'analyze'"

Expected: agent calls `codegraph_search`, results list `analyze` from `crates/biorouter-mcp/src/developer/analyze/mod.rs` and related locations.

- [ ] **Step 4: Verify on-disk state**

Run:
```bash
ls -la /Users/wgu/Desktop/biorouter/.biorouter/codegraph/ && \
ls -la /Users/wgu/Desktop/biorouter/.codegraph
```

Expected: `.biorouter/codegraph/codegraph.db` exists; `.codegraph` is a symlink pointing into `.biorouter/codegraph/`.

- [ ] **Step 5: Record results in CHANGELOG**

Append to `/Users/wgu/Desktop/CodeGraphAgent/CHANGELOG.md` under `v0.1.0-rc1`:

```markdown
- Manual E2E in BioRouter: installed .brxt, enabled, opened BioRouter repo,
  ran codegraph_search('analyze'), got correct results. .biorouter/codegraph/
  materialized as expected.
```

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
git add CHANGELOG.md && \
git commit -m "docs: manual E2E success against BioRouter repo"
```

### Task L2: Publish the .brxt release

- [ ] **Step 1: Trigger the release-brxt workflow**

Run:
```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
git push && \
gh workflow run "Release .brxt" -f release_tag=v0.1.0-rc1
```

Expected: workflow starts.

- [ ] **Step 2: Watch the run**

Run:
```bash
gh run watch
```

Expected: completes in <1 min (no build heavy lifting; just builds the zip).

- [ ] **Step 3: Verify the release exists with the .brxt attached**

Run:
```bash
gh release view v0.1.0-rc1
```

Expected: release `v0.1.0-rc1` with `codegraphagent.brxt` asset.

- [ ] **Step 4: Update BioRouter spec status**

In the BioRouter repo, edit [docs/superpowers/specs/2026-05-29-codegraphagent-extension-design.md](../specs/2026-05-29-codegraphagent-extension-design.md) and change the status line from `Draft, pending implementation` to `Foundation released as v0.1.0-rc1; bio languages pending (Plan 2)`.

Run:
```bash
cd /Users/wgu/Desktop/biorouter && \
git add docs/superpowers/specs/2026-05-29-codegraphagent-extension-design.md && \
git commit -m "docs(specs): mark CodeGraphAgent foundation released"
```

---

## Done — what's working after this plan

- `Broccolito/CodeGraphAgent` repo with a Python proxy shim and a vendored CodeGraph engine.
- Shim has full test coverage (paths, bootstrap, proxy, error shim, cli).
- CI runs Python tests + engine type-check on every push.
- `engine-v0.1.0` release published with 6 platform bundles + SHA256SUMS.
- `v0.1.0-rc1` `.brxt` release published.
- Manually verified end-to-end against a real BioRouter session.
- Index materializes at `.biorouter/codegraph/codegraph.db` via the `.codegraph` symlink.
- All 20+ upstream-supported languages work out of the box (Python, Rust, JS/TS, Go, Java, etc.).

## What's next (Plan 2 — bio languages)

The follow-on plan adds R, Julia, MATLAB, and Perl extractors to `engine/`, cuts `engine-v0.2.0`, and releases `.brxt v0.1.0`. Each language is ~6-8 tasks:

1. Vendor the WASM grammar into `engine/wasm/`.
2. Add an entry to `engine/src/extraction/grammars.ts` (WASM_GRAMMAR_FILES + EXTENSION_MAP).
3. Add to `engine/src/types.ts` `Language` union.
4. Write `engine/src/extraction/languages/<lang>.ts` (50-150 lines).
5. Register in `engine/src/extraction/languages/index.ts`.
6. Write vitest fixtures + tests under `engine/__tests__/extraction/`.
7. Document in `engine/PATCHES.md`.

MATLAB additionally needs a content-heuristic for `.m` disambiguation against Objective-C (per spec § "Engine fork — language additions"). Plan 2 will spell out the disambiguation logic.
