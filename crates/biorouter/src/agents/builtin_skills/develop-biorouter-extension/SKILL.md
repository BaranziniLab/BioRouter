---
name: develop-biorouter-extension
description: "Guide for building a Biorouter extension (.brxt file). Covers the required ZIP structure, manifest.json schema, the Python MCP server (pyproject.toml + console-script entry point), cross-platform dependency pitfalls, and optional bundled skills. Load this skill whenever the user wants to build, package, or publish a Biorouter extension."
user-invocable: true
---

# Developing a Biorouter Extension (.brxt)

A `.brxt` file is a standard ZIP archive renamed with the `.brxt` extension.
Biorouter validates and installs it by unzipping it to
`~/.config/biorouter/extensions/<name>/`, running `uv sync` to build the Python
virtual environment, and registering a stdio MCP server that Biorouter launches
with `uv run --directory <dir> <entry_point>`.

## Required ZIP Contents

```
my-extension.brxt (ZIP archive)
├── manifest.json          ← Biorouter metadata (required)
├── README.md              ← Extension documentation (required)
├── pyproject.toml         ← Python project config (required)
└── src/                   ← Python source directory (required)
    └── myextension/
        └── __init__.py
```

Validation fails if any of the four required entries are missing
(`manifest.json`, `README.md`, `pyproject.toml`, and a `src/` directory). The
same four-entry check runs in both the desktop app and the `biorouter extension
install` CLI.

## manifest.json Schema

```json
{
  "name": "myextension",
  "display_name": "My Extension",
  "description": "What this extension does.",
  "version": "1.0.0",
  "entry_point": "myextension",
  "repository": "https://github.com/you/myextension",
  "env_vars": [
    {
      "key": "MY_API_KEY",
      "required": true,
      "auto_propagate": false,
      "description": "API key for the external service.",
      "secret": true,
      "default": ""
    }
  ]
}
```

**Required to be present:** `name`, `display_name`, `description`, `version`,
`entry_point`, `repository`. All six are plain (non-defaulted) string fields, so
the manifest fails to parse at all if any one is missing.

**Checked non-empty:** only `name` and `entry_point`. The other four may be
empty strings and still install — ship real values anyway, because they are what
the user reads in the Extensions page.

**Optional:** `env_vars` (defaults to an empty list, so it may be omitted
entirely; write `[]` for clarity if you prefer) and `tools_count`. Inside an
`env_vars` entry only `key` is required; `required`, `auto_propagate`,
`description`, `secret` and `default` all default.

**`env_vars` field reference:**

| Field | Type | Purpose |
|---|---|---|
| `key` | string | Environment variable name |
| `required` | bool | Whether Biorouter warns/blocks if missing |
| `auto_propagate` | bool | Whether to pass the var through automatically |
| `description` | string | Shown to the user in the UI |
| `secret` | bool | Stored in the OS keyring and masked in the UI |
| `default` | string | Optional fallback value |

Secret env vars (`"secret": true`) are stored in the OS credential store
(keyring) and referenced by key; non-secret vars are saved with the extension's
config.

## Python Package (pyproject.toml + src/)

The extension is a Python package that implements an MCP server. Use `uv` for
dependency management. Biorouter runs `uv sync` on install.

**`entry_point` is a console-script command, not a module.** At runtime
Biorouter starts the server with `uv run --directory <install-dir>
<entry_point>`, so `entry_point` must name a console script your package
declares under `[project.scripts]`. It is **not** run as `python -m
<entry_point>`.

Minimal `pyproject.toml`:

```toml
[project]
name = "myextension"
version = "1.0.0"
requires-python = ">=3.11"
dependencies = [
  "mcp>=1.0.0",
]

# The command Biorouter runs via `uv run <entry_point>`. The left-hand name
# MUST match manifest.json's "entry_point"; the right-hand side is
# "<module>:<callable>", naming the function that starts your MCP server.
[project.scripts]
myextension = "myextension:main"

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"
```

With the `src/` layout above, the importable module is `src/myextension/` and
your `__init__.py` should expose a `main()` entry that runs the MCP server's
stdio loop (e.g. `mcp.run()` / `server.run()`).

## Cross-Platform Dependencies (avoid source builds)

`uv sync` runs on the **end user's machine** at install time. Every dependency
(direct *and* transitive) must therefore either ship a prebuilt wheel for the
user's OS + CPU + Python, or be compilable there. When no wheel matches, uv
falls back to building from source, which silently requires a C and/or Rust
toolchain the user may not have, turning "install an extension" into a compiler
error. This is the single most common cause of install failures, and Biorouter
surfaces uv's output plus a targeted hint (missing/broken Rust toolchain, no
wheel for the platform, the `cryptography<49` Intel-Mac case) when it happens.

**Real example:** `cryptography` ≥ 49 (2026-06-12) removed x86_64 (Intel) macOS
wheels. Any extension that pulls it in, for example transitively via
`fastmcp > fastmcp-slim[server] > joserfc > cryptography`, forces Intel-Mac
users into a from-source Rust build that fails on most machines.

**Mitigation: cap the dependency only where wheels are missing.** Use a uv
constraint with an environment marker so other platforms keep the newest
version. This works even for *transitive* deps (a constraint narrows a version
that's already in the tree; it does not add a new dependency):

```toml
[tool.uv]
# cryptography >=49 dropped x86_64 macOS wheels (2026-06-12), forcing a
# from-source Rust build on Intel Macs. Cap it to <49 there so Intel users get
# a prebuilt wheel; arm64/Linux/Windows are unaffected.
constraint-dependencies = [
    "cryptography<49; sys_platform == 'darwin' and platform_machine == 'x86_64'",
]
```

**Verify resolution per platform** before you publish. `uv pip compile`
resolves for a target without installing:

```bash
uv pip compile pyproject.toml --python-platform x86_64-apple-darwin  | grep -i cryptography   # Intel mac → must be <49
uv pip compile pyproject.toml --python-platform aarch64-apple-darwin | grep -i cryptography   # Apple silicon
uv pip compile pyproject.toml --python-platform x86_64-unknown-linux-gnu | grep -i cryptography
```

Guidelines:
- Prefer dependency versions that ship wheels for **every** platform you
  support. Check a package's "Download files" page on PyPI for the wheel tags.
- Commit a `uv.lock` for reproducible installs. After adding a constraint, run
  `uv lock` and `uv lock --check`. The lock is what `uv sync` consumes, so an
  invalid lock breaks installs outright. Always validate it.
- Pure-Python packages (no compiled extension) are always safe. The risk is
  packages with native code: `cryptography`, `pymssql`, `pydantic-core`,
  anything built with maturin/setuptools-rust/cffi.

## Optional: Bundled Skills

Skills can be shipped inside the `.brxt` and are installed atomically alongside
the extension:

```
skills/
  my-skill-slug/
    SKILL.md
  another-skill/
    SKILL.md
    supporting-file.md
```

Each `SKILL.md` must begin with valid YAML frontmatter:

```markdown
---
name: my-skill-slug
description: One-line description of what this skill does.
---

Skill body in markdown...
```

**`name` must equal the skill's folder slug** — lowercase, hyphenated, no
spaces. A title-cased display name in `name` does not match the directory it
sits in, and the two views of the catalog then disagree about what the skill is
called.

Skills install to `~/.config/biorouter/extensions/<name>/skills/<slug>/SKILL.md`
and are auto-discovered by the agent on startup (Biorouter scans the `skills/`
subdirectory of every installed extension).

## Building the .brxt File

```bash
# Create the ZIP and rename to .brxt
cd my-extension-dir
zip -r ../myextension.brxt manifest.json README.md pyproject.toml src/ skills/
```

Or using Python:

```python
import zipfile, pathlib

with zipfile.ZipFile("myextension.brxt", "w", zipfile.ZIP_DEFLATED) as zf:
    for path in pathlib.Path(".").rglob("*"):
        if path.is_file() and ".venv" not in path.parts:
            zf.write(path)
```

Exclude `.venv/`, `__pycache__/`, and any build artifacts.

## Installation & Uninstallation

Biorouter installs by:
1. Unzipping all contents to `~/.config/biorouter/extensions/<name>/`.
2. Running `uv sync` to build the virtual environment. If it fails, Biorouter
   surfaces uv's output along with a hint for common causes (missing/broken
   Rust toolchain, no wheel for the platform).
3. Registering a stdio MCP extension that runs `uv run --directory <dir>
   <entry_point>`, storing any secret env vars in the OS keyring.

The desktop app and the `biorouter extension install` CLI run the same three
steps and share the same four-entry validation, but they **differ on the `uv
sync` timeout**: the desktop app caps it at 10 minutes (generous, so a
legitimate from-source build has time to finish) and reports a timeout as such;
the CLI runs `uv sync` to completion with no timeout at all, so a wedged
resolve there hangs until you interrupt it.

**Uninstalling does not delete files by default.** Removing an extension from
the desktop **Extensions** page, or `biorouter extension remove <name>`,
*unregisters* it — the entry leaves the config and the server stops being
launched, while `~/.config/biorouter/extensions/<name>/` (the venv, the source
and any bundled skills) stays on disk. Pass `biorouter extension remove <name>
--purge` to delete that directory as well.

## Validation Checklist

Before distributing a `.brxt`, verify:

```
[ ] ZIP contains manifest.json at root
[ ] ZIP contains README.md at root
[ ] ZIP contains pyproject.toml at root
[ ] ZIP contains src/ directory
[ ] manifest.json has all six required keys present (name, display_name,
    description, version, entry_point, repository), with name and entry_point
    non-empty and the other four filled in for the user's sake
[ ] env_vars, if present, is a JSON array; omit it or use [] when there are none
[ ] pyproject.toml declares [project.scripts] with an entry whose name equals
    manifest.json's entry_point (Biorouter runs `uv run <entry_point>`)
[ ] Each SKILL.md has --- frontmatter with name and description, and name is
    the folder slug (lowercase, hyphenated)
[ ] .venv/ and __pycache__/ are excluded from the ZIP
[ ] uv sync completes cleanly from the unzipped directory
[ ] Every dependency has a wheel for each target platform, OR a marker-scoped
    constraint caps it to a version that does (verify with `uv pip compile
    --python-platform ...`, including x86_64-apple-darwin for Intel Macs)
[ ] If a uv.lock is committed, `uv lock --check` passes
```
