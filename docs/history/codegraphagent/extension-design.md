# CodeGraphAgent — BioRouter Extension Design

**Date:** 2026-05-29 (revised 2026-05-30)
**Status:** Draft, pending implementation
**Repo (extension + engine fork):** [Broccolito/CodeGraphAgent](https://github.com/Broccolito/CodeGraphAgent)
**Upstream engine (fork source):** [colbymchenry/codegraph](https://github.com/colbymchenry/codegraph) (MIT)

## Goal

Ship a BioRouter extension (`.brxt`) that gives BioRouter agents a pre-indexed code knowledge graph: persistent SQLite-backed call graph, framework-aware routing, cross-language bridges, and typed query tools (`codegraph_search`, `codegraph_callers`, `codegraph_callees`, `codegraph_trace`, `codegraph_impact`, `codegraph_node`, `codegraph_explore`, `codegraph_context`, `codegraph_status`).

Beyond upstream's 20+ languages, **v0.1 adds four bioinformatics-relevant languages** by vendoring CodeGraph's source into our own repo and extending it: **R, Julia, MATLAB, Perl**.

Constraints:

- Distributed as a single `.brxt` file (per BioRouter extension format).
- Per-project index stored under `<project-root>/.biorouter/codegraph/` (not upstream's `.codegraph/`).
- Off by default when bundled with BioRouter.
- Self-contained at runtime: no system Node.js, no upstream npm install at use time.
- The shim downloads engine bundles from **our** repo's releases, not upstream's. Users never reach colbymchenry/codegraph at runtime.
- May vendor / re-use code from upstream CodeGraph (MIT-licensed).

## Naming convention (matches `BaranziniLab/UCSFOMOPAgent`)

| Surface | Value |
| --- | --- |
| GitHub repo | `Broccolito/CodeGraphAgent` |
| `.brxt` filename | `codegraphagent.brxt` |
| `manifest.json` `name` | `codegraphagent` |
| `manifest.json` `display_name` | `CodeGraphAgent` |
| `manifest.json` `entry_point` | `codegraphagent` |
| `manifest.json` `repository` | `https://github.com/Broccolito/CodeGraphAgent` |
| `pyproject.toml` `[project].name` | `codegraphagent` |
| `[project.scripts]` | `codegraphagent = "codegraphagent.cli:main"` |
| Python package | `src/codegraphagent/` |
| Install dir | `~/.config/biorouter/extensions/codegraphagent/` |
| MCP framework | `fastmcp>=2.11.2` |
| MCP tool names on the wire | Upstream names unchanged (`codegraph_search`, `codegraph_callers`, ...) |

## Architecture

The Python entry point is a **transparent MCP proxy** in front of a vendored, forked CodeGraph engine that **we** build and release.

```text
BioRouter agent
   │ MCP over stdio
   ▼
~/.config/biorouter/extensions/codegraphagent/
   src/codegraphagent/__main__.py       ← Python proxy
        │
        ├── bootstrap: download our engine tarball on first use, verify SHA256, extract
        ├── paths:     ensure <root>/.biorouter/codegraph/ + symlink
        │              <root>/.codegraph → <root>/.biorouter/codegraph/
        ├── spawn:     subprocess.Popen([bin/codegraph, "serve", "--mcp"], cwd=root)
        └── proxy:     bidirectional stdio piping, byte-level
                │
                ▼
        engine/ tarball extracted into install dir
        ├── node (or node.exe)             ← official Node runtime
        ├── lib/dist/                       ← our forked engine + tree-sitter WASMs
        └── bin/codegraph                   ← launcher
                │
                ▼
        <project-root>/.biorouter/codegraph/codegraph.db
                                  └── codegraph.db-wal, .db-shm,
                                      .gitignore, codegraph.lock, cache/
```

Three layers in our repo:

1. **Python shim** (`src/codegraphagent/`) — the .brxt entry point. Bootstrap, path injection, MCP proxy. ~500 lines of Python.
2. **Engine fork** (`engine/`) — flat copy of upstream CodeGraph TypeScript source, modified to add R/Julia/MATLAB/Perl extractors. ~3 MB.
3. **Build/release pipeline** (`scripts/`, `.github/workflows/`) — runs upstream's `engine/scripts/build-bundle.sh` per platform, attaches tarballs to GitHub Releases on our repo.

## Repo structure (full)

```text
Broccolito/CodeGraphAgent/
├── README.md
├── manifest.json                          # .brxt manifest
├── pyproject.toml                         # .brxt python package
├── src/
│   └── codegraphagent/                    # Python proxy shim
│       ├── __init__.py
│       ├── __main__.py                    # MCP server entry
│       ├── cli.py                         # [project.scripts] target
│       ├── bootstrap.py                   # tarball download + extract + SHA256
│       ├── paths.py                       # project-root + symlink setup
│       ├── proxy.py                       # bidirectional stdio piping
│       └── release_manifest.json          # pinned engine version + per-platform SHA256s
├── tests/                                 # pytest suite for the shim
├── engine/                                # vendored CodeGraph (flat copy, our fork)
│   ├── UPSTREAM.md                        # records upstream commit we forked from
│   ├── PATCHES.md                         # our additions on top of upstream
│   ├── src/                               # upstream TS sources, modified
│   │   └── extraction/
│   │       ├── grammars.ts                # + WASM grammar entries for new langs
│   │       └── languages/
│   │           ├── ... upstream langs ...
│   │           ├── r.ts                   # NEW (we add)
│   │           ├── julia.ts               # NEW
│   │           ├── matlab.ts              # NEW
│   │           └── perl.ts                # NEW
│   ├── wasm/                              # tree-sitter WASMs (incl. r/julia/matlab/perl)
│   ├── package.json
│   ├── tsconfig.json
│   └── scripts/build-bundle.sh            # upstream's recipe, unchanged
├── scripts/
│   ├── build-brxt.sh                      # produces codegraphagent.brxt
│   ├── sync-upstream.sh                   # merges upstream into engine/
│   └── add-language.sh                    # scaffolds a new engine/src/extraction/languages/<lang>.ts
└── .github/workflows/
    ├── ci.yml                             # Python tests + engine type-check
    ├── build-engine.yml                   # cross-platform tarballs (build-bundle.sh)
    └── release.yml                        # tags + attaches assets + builds .brxt
```

## Vendoring and upstream sync

**Method:** flat copy, no upstream git history. Upstream's `.git` is dropped when we vendor.

**Provenance:** `engine/UPSTREAM.md` records the upstream commit SHA we forked from. `engine/PATCHES.md` lists every change we apply on top (currently: 4 new language extractors + 4 WASM grammars + grammar/extension-map entries).

**Sync workflow:** `scripts/sync-upstream.sh` does:

1. Fetches upstream tarball at a target commit/tag into a temp dir.
2. Three-way merges into `engine/` using `git merge-file` per file, with our `PATCHES.md`-tracked files as the "ours" side.
3. Surfaces conflicts as a normal pull request.
4. On merge, updates `engine/UPSTREAM.md` to the new SHA.

We do not attempt automated nightly merges in v0.1 — too risky without test coverage on the engine side. Manual sync is triggered when we want to pull in upstream changes (new languages, framework patterns, bug fixes).

**Upstream attribution:** upstream's `LICENSE` file is preserved verbatim at `engine/LICENSE`. Our top-level `README.md` credits upstream prominently.

## .brxt structure

```text
codegraphagent.brxt   (ZIP, ~hundreds of KB)
├── manifest.json
├── README.md
├── pyproject.toml
└── src/
    └── codegraphagent/
        ├── __init__.py
        ├── __main__.py
        ├── cli.py
        ├── bootstrap.py
        ├── paths.py
        ├── proxy.py
        └── release_manifest.json
```

The .brxt **does not** contain the engine — the engine is downloaded on first use from our GitHub Releases. Keeps .brxt small and platform-agnostic.

### `manifest.json`

```json
{
  "name": "codegraphagent",
  "display_name": "CodeGraphAgent",
  "description": "Pre-indexed code knowledge graph (callers, callees, impact, trace) via vendored CodeGraph engine with R/Julia/MATLAB/Perl support added. Per-project index stored at <project>/.biorouter/codegraph/.",
  "version": "0.1.0",
  "entry_point": "codegraphagent",
  "repository": "https://github.com/Broccolito/CodeGraphAgent",
  "tools_count": 9,
  "env_vars": [
    {"key": "CODEGRAPH_NO_WATCH", "required": false, "auto_propagate": true, "default": "", "description": "Set to 1 to disable the file watcher (slow filesystems like WSL2)", "secret": false},
    {"key": "CODEGRAPH_ENGINE_PATH", "required": false, "auto_propagate": false, "default": "", "description": "Path to an already-extracted engine bundle (skips first-use download for air-gapped/CI)", "secret": false},
    {"key": "CODEGRAPH_ENGINE_VERSION", "required": false, "auto_propagate": false, "default": "", "description": "Override the pinned CodeGraphAgent engine release", "secret": false}
  ]
}
```

### `pyproject.toml`

```toml
[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[project]
name = "codegraphagent"
version = "0.1.0"
description = "BioRouter extension wrapping a vendored CodeGraph engine (with R/Julia/MATLAB/Perl additions) as an MCP server with per-project index in .biorouter/codegraph/"
readme = "README.md"
license = {text = "MIT"}
authors = [{name = "Wanjun Gu", email = "wanjun.gu@ucsf.edu"}]
requires-python = ">=3.11"
dependencies = [
  "fastmcp>=2.11.2",
  "httpx>=0.27",
]

[project.scripts]
codegraphagent = "codegraphagent.cli:main"
```

## Python shim — component responsibilities

### `paths.py` — project root + symlink setup

- `resolve_project_root() -> Path` — read `BIOROUTER_WORKING_DIR`, else walk up from `Path.cwd()` looking for `.biorouter/`, `.git/`, or `pyproject.toml`. Fall back to CWD.
- `ensure_layout(root: Path)`:
  - `mkdir -p root/.biorouter/codegraph`
  - if `root/.codegraph` doesn't exist: create as symlink (Unix `os.symlink`, Windows directory junction via `subprocess.run(["cmd", "/c", "mklink", "/J", ...])`)
  - if `root/.codegraph` exists as a real directory → raise `LayoutConflictError`
  - if it exists as a symlink → leave it
  - add `.codegraph` to `root/.gitignore` if writable and not present
  - add `*.db*`, `*.lock`, `.dirty`, `cache/` to `root/.biorouter/codegraph/.gitignore`

### `bootstrap.py` — engine tarball fetch and extract

Upstream ships `codegraph-<target>.tar.gz` containing `node` + `lib/dist/` + `bin/codegraph` launcher (Windows: `.zip` with `node.exe` + `.cmd` launcher).

- `ensure_engine() -> Path` (returns path to launcher):
  - if `CODEGRAPH_ENGINE_PATH` is set and points to a valid bundle dir → return its launcher
  - else: install_dir = `Path(__file__).parent / "engine"`; expected launcher = `install_dir / "bin" / ("codegraph" or "codegraph.cmd")`
  - if launcher exists and bundle's `VERSION` file matches the pinned version → return launcher
  - else: download `release_manifest["base_url"] + f"codegraph-{platform_tag()}.{archive_suffix()}"`, verify SHA256, extract into install_dir (atomic via temp-dir + rename), `chmod +x` on the launcher, write VERSION file, return launcher
  - on failure: raise `BootstrapError` with URL, expected/observed SHA, underlying exception
- `platform_tag()` returns one of: `darwin-arm64`, `darwin-x64`, `linux-x64`, `linux-arm64`, `win32-x64`, `win32-arm64`. Reject unsupported.
- `archive_suffix()`: `tar.gz` for Unix, `zip` for Windows.

`release_manifest.json` schema:

```json
{
  "engine_version": "0.1.0",
  "base_url": "https://github.com/Broccolito/CodeGraphAgent/releases/download/engine-v0.1.0/",
  "platforms": {
    "darwin-arm64":  {"filename": "codegraph-darwin-arm64.tar.gz",  "sha256": "..."},
    "darwin-x64":    {"filename": "codegraph-darwin-x64.tar.gz",    "sha256": "..."},
    "linux-x64":     {"filename": "codegraph-linux-x64.tar.gz",     "sha256": "..."},
    "linux-arm64":   {"filename": "codegraph-linux-arm64.tar.gz",   "sha256": "..."},
    "win32-x64":     {"filename": "codegraph-win32-x64.zip",        "sha256": "..."},
    "win32-arm64":   {"filename": "codegraph-win32-arm64.zip",      "sha256": "..."}
  }
}
```

`engine_version` is **our** version, not upstream's. Engine releases are tagged `engine-vX.Y.Z` in our repo and published independently of `.brxt` releases (`vX.Y.Z`). The .brxt's `release_manifest.json` pins exactly one engine release.

### `proxy.py` — bidirectional stdio piping

- Spawn: `subprocess.Popen([launcher, "serve", "--mcp"], cwd=root, env=merged_env, stdin=PIPE, stdout=PIPE, stderr=PIPE)`
- Two threads: `parent.stdin → child.stdin` and `child.stdout → parent.stdout`, byte-for-byte
- Third thread: drains child's stderr into the BioRouter MCP log
- On child exit: log exit code, close pipes, exit the Python process with the same code

### `cli.py` and `__main__.py` — entry points

`cli.py` is `[project.scripts]`' target (`codegraphagent` command). It calls `main()` in `__main__.py`. Two-file split mirrors UCSFOMOPAgent's pattern.

```python
def main() -> None:
    try:
        root = paths.resolve_project_root()
        paths.ensure_layout(root)
        launcher = bootstrap.ensure_engine()
    except (LayoutConflictError, BootstrapError) as exc:
        serve_error_shim(exc)
        return
    proxy.run(launcher, root)
```

### `serve_error_shim(exc)` — degraded mode

If bootstrap or layout setup fails, the shim still serves MCP but exposes only synthetic error tools:

- `codegraphagent_bootstrap_error` — engine download/extract/verify failed
- `codegraphagent_setup_error` — symlink / layout setup failed

These use the `codegraphagent_` prefix (not `codegraph_`) so the agent can tell at a glance which tools come from our shim versus the engine.

## Engine fork — language additions for v0.1

Per upstream's [`BUNDLING.md`](https://github.com/colbymchenry/codegraph/blob/main/BUNDLING.md), the engine has zero native deps (uses Node 22.5's built-in `node:sqlite`), so "any target builds on any OS" via `build-bundle.sh`.

Adding a language to the engine takes four edits, well-isolated:

1. **WASM grammar** — drop `tree-sitter-<lang>.wasm` into `engine/wasm/` (precompiled via `tree-sitter build --wasm` from the official grammar repo).
2. **`engine/src/extraction/grammars.ts`** — add entry to `WASM_GRAMMAR_FILES` and `EXTENSION_MAP`.
3. **`engine/src/extraction/languages/<lang>.ts`** — define `LanguageExtractor` for AST node kinds (functions, classes, imports, calls). Python's reference is **54 lines**; expect 50–150 lines per language depending on AST quirks.
4. **`engine/src/extraction/languages/index.ts`** — import and register in `EXTRACTORS` map.
5. **`engine/src/types.ts`** — add the language tag to the `Language` union type.

**v0.1 deliverable: R, Julia, MATLAB, Perl.**

| Language | Grammar | Extension(s) | Priority |
| --- | --- | --- | --- |
| R | `tree-sitter-r` (active, well-maintained) | `.R`, `.r`, `.Rmd` (code chunks) | High — primary bioinformatics ask |
| Julia | `tree-sitter-julia` (active) | `.jl` | Medium — growing in computational bio |
| MATLAB | `tree-sitter-matlab` (active) | `.m` (conflicts with Objective-C `.m` — heuristic disambiguation needed; default to MATLAB unless `#import`/`@interface` present) | Medium — legacy but real |
| Perl | `tree-sitter-perl` (active, less mature) | `.pl`, `.pm`, `.t` | Lower — legacy BioPerl |

Each language extractor lives in its own file with its own tests in `engine/__tests__/extraction/<lang>.test.ts`. Upstream's existing test scaffold accepts this pattern with no changes.

**The `.m` ambiguity** between MATLAB and Objective-C is real and not solvable purely by extension. We add a content heuristic in `engine/src/extraction/grammars.ts`: if the first non-empty / non-comment line contains `@interface`, `@implementation`, `#import`, or `#include`, treat as Objective-C; otherwise MATLAB. Documented in `PATCHES.md`.

## Data flow

```text
1. BioRouter starts the extension subprocess.
2. codegraphagent.cli:main()
   ├── resolve project root
   ├── ensure .biorouter/codegraph/ + .codegraph symlink
   ├── ensure engine present (download + extract on first run)
   └── spawn engine launcher with cwd=root
3. Engine performs its own init: opens .codegraph/codegraph.db (which is the
   symlinked .biorouter/codegraph/codegraph.db), starts watcher if not disabled.
4. BioRouter sends MCP initialize → proxy forwards → engine responds with
   capabilities + tool list (incl. R/Julia/MATLAB/Perl support).
5. Subsequent MCP frames (tools/list, tools/call, etc.) pass byte-for-byte.
6. On shutdown: parent stdin closes → engine detects EOF → exits → supervisor
   thread propagates exit code.
```

## Build & release pipeline

### `.github/workflows/build-engine.yml`

- Triggered on push to `main` with changes under `engine/`, or manually.
- Single Linux runner. For each target in `[darwin-arm64, darwin-x64, linux-x64, linux-arm64, win32-x64, win32-arm64]`:
  - Run `engine/scripts/build-bundle.sh <target>` → produces `release/codegraph-<target>.{tar.gz,zip}`
  - Compute SHA256, write to artifact
- Uploads all six tarballs/zips as workflow artifacts.

### `.github/workflows/release.yml`

- Triggered manually by a maintainer on a tag push.
- Two release types:
  - **`engine-vX.Y.Z`** — pulls artifacts from `build-engine.yml`, creates a GitHub Release with the six bundles attached and a `release-manifest.json` (the same one the shim reads).
  - **`vX.Y.Z`** — builds the `.brxt`: validates `src/codegraphagent/release_manifest.json` points to an existing `engine-vX.Y.Z` release, zips into `codegraphagent.brxt`, creates a GitHub Release.

### `.github/workflows/ci.yml`

- pytest for `tests/` (Python shim)
- `npm test` in `engine/` (TypeScript engine, including new language extractors)
- `tsc --noEmit` in `engine/` for type-check

## Error handling

| Failure | Surface | Recovery |
| --- | --- | --- |
| Network down / engine release unreachable | `codegraphagent_bootstrap_error` returns URL + error | Set `CODEGRAPH_ENGINE_PATH` or retry |
| SHA256 mismatch | `codegraphagent_bootstrap_error` with expected vs observed hashes | Re-download or pin a different `CODEGRAPH_ENGINE_VERSION` |
| Tarball extract failure (corrupt archive) | `codegraphagent_bootstrap_error` | Re-download |
| `<root>/.codegraph` exists as real dir | `codegraphagent_setup_error` with remediation message | User-action: rename/remove, restart |
| Symlink/junction creation denied | `codegraphagent_setup_error` | Enable Developer Mode (Windows) or run with elevated perms once |
| Engine process crash mid-session | All subsequent tool calls return `"engine exited unexpectedly: {code}"` | Restart extension |
| Unsupported platform tag | `codegraphagent_bootstrap_error` listing supported tags | Open an issue on our repo |

No auto-restart in v0.1.

## Testing strategy

### Python shim (pytest)

- `tests/test_paths.py` — project root resolution, symlink idempotency, junction fallback on Windows, gitignore writes
- `tests/test_bootstrap.py` — mocked tarball download, SHA verification, atomic extract, `CODEGRAPH_ENGINE_PATH` short-circuit, partial-download cleanup
- `tests/test_proxy.py` — stdio framing against a fake engine, exit-code propagation

### Engine (TS, vitest — upstream's framework)

- `engine/__tests__/extraction/r.test.ts` — sample R fixtures parse into expected functions/calls/imports
- `engine/__tests__/extraction/julia.test.ts`
- `engine/__tests__/extraction/matlab.test.ts` — incl. `.m` disambiguation cases
- `engine/__tests__/extraction/perl.test.ts`
- `engine/__tests__/grammars.test.ts` — verifies all WASM grammars load

### Integration

- Build engine locally, run shim against it, send MCP `initialize` + `tools/list`, assert ≥9 `codegraph_*` tools and that calling `codegraph_search` on a fixture R repo returns R symbols.

### Manual E2E (deferred)

- Install `codegraphagent.brxt` into a dev BioRouter, enable, open against this repo, verify `.biorouter/codegraph/codegraph.db` materializes.
- Open against an R bioinformatics repo (e.g. a Bioconductor package), confirm `codegraph_callers` finds callers of an exported function across multiple R files.

## Bundling with BioRouter (default-off mechanism)

Out of scope for this .brxt repo, tracked here so we don't lose the thread.

Approach: ship `codegraphagent.brxt` as a resource inside the BioRouter app bundle (`ui/desktop/resources/bundled-extensions/codegraphagent.brxt`). On first launch, BioRouter:

1. Detects bundled extensions not yet installed
2. Auto-installs into `~/.config/biorouter/extensions/codegraphagent/`
3. Writes the config entry with `enabled: false` (default-off)
4. Surfaces the extension in UI under "Recommended extensions" with a one-click toggle

The bundled-extensions loader is a separate BioRouter-side spec.

## Out of scope for v0.1

- Rewriting **upstream** tool names. (Our synthetic shim tools use the `codegraphagent_*` prefix; no proxy-level rewriting is involved.)
- Auto-restart on engine crash. User reloads manually.
- Multi-project / cross-repo unified index. Each project has its own DB.
- Migration tool for users who already have `.codegraph/` in their project.
- Automated nightly upstream merges. Manual sync only.
- Framework-pattern additions for R (Shiny), Julia (Pluto/Genie), etc. Pure extraction first; framework awareness later.

## Risks

| Risk | Likelihood | Mitigation |
| --- | --- | --- |
| Upstream changes engine internals such that our four language extractors break on sync | Medium | `engine/PATCHES.md` documents every change; `sync-upstream.sh` surfaces conflicts in PRs; engine vitest suite catches regressions |
| Tree-sitter grammar for one of R/Julia/MATLAB/Perl produces unstable ASTs across releases | Medium | Pin specific grammar repo SHAs in `engine/wasm/SOURCES.md`; bump deliberately |
| `.m` ambiguity (MATLAB vs Objective-C) misroutes files | Low-Medium | Content-heuristic with documented disambiguation; falsifiable with fixture tests |
| Engine binary build pipeline regresses for one platform | Medium | CI matrix runs `build-bundle.sh` for all 6 platforms on each engine release |
| Repo size grows uncomfortably with vendored engine + WASMs | Low | engine/ is ~3 MB + WASMs ~10 MB; acceptable for a public repo |
| Fork maintenance burden (we own bug fixes for our 4 langs forever) | Medium | Document scope as bioinformatics-targeted; reject feature creep |
| Upstream license changes from MIT to something restrictive | Very Low | MIT is irrevocable for the vendored snapshot; we'd freeze and continue independently if upstream relicenses |
| Symlink leaks into a user's git commits | Medium | Auto-add `.codegraph` to root `.gitignore`; documented in README |
| Windows symlink/junction permission edge cases | Medium | Specific Windows test path with junction fallback + clear error message |
| First-use download blocked by corporate firewall | Medium | `CODEGRAPH_ENGINE_PATH` escape hatch lets users hand-place the bundle |

## Open questions

None blocking implementation. Two follow-on tracks:

1. **BioRouter-side bundled-extensions loader** (separate spec).
2. **R / Julia / MATLAB / Perl framework patterns** — Shiny app routing, Pluto notebooks, etc. Defer until after v0.1 extraction is stable in real bioinformatics codebases.

## Plan handoff

After this design is approved, the writing-plans skill will turn it into a step-by-step implementation plan covering:

1. Repo scaffold (top-level files, src layout, engine vendor)
2. Vendor CodeGraph: clone upstream at pinned SHA, copy `src/`, `package.json`, `tsconfig.json`, `scripts/`, `__tests__/`, `wasm/`, `LICENSE` into `engine/`; drop `.git`; write `UPSTREAM.md` + `PATCHES.md`
3. Python shim — `paths.py` + tests
4. Python shim — `bootstrap.py` (tarball, atomic extract) + tests
5. Python shim — `proxy.py` + tests
6. Python shim — `cli.py` / `__main__.py` glue
7. Engine language extractors — R, Julia, MATLAB (with `.m` disambiguation), Perl, plus WASM grammars + tests
8. Build pipeline — `engine/scripts/build-bundle.sh` verification, `.github/workflows/build-engine.yml`, `release.yml`, `ci.yml`
9. `scripts/build-brxt.sh` (zip the shim into `.brxt`)
10. `scripts/sync-upstream.sh` (periodic merge tool)
11. README + LICENSE + attribution
12. Cut `engine-v0.1.0` and `v0.1.0` releases, verify the .brxt installs cleanly into a local BioRouter and runs against this repo as a fixture
