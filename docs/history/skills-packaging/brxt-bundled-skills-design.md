# .brxt bundled skills design

> **What this is.** The design spec for letting a `.brxt` extension package carry its own skills, so that installing an extension installs its skills and removing the extension removes them atomically — plus ZIP file support in the standalone skill import UI.
> **Status:** Historical record — approved 2026-05-07 and implemented. Every element of this design shipped: `brxt:uninstall` and `skills:extract-zip` are IPC handlers in `ui/desktop/src/main.ts`, `brxt:validate-and-read` returns `skillsPreview`, and `crates/biorouter/src/agents/skills_extension.rs` scans `~/.config/biorouter/extensions/*/skills/`.
> **Audience:** developers working on extension packaging and skill discovery.

A `.brxt` file is BioRouter's extension package format: a ZIP archive holding a Python MCP server plus a `manifest.json` of BioRouter metadata. A *skill* is a `SKILL.md` file — YAML frontmatter plus a markdown body — that the Rust `SkillsClient` discovers from a fixed set of directories and offers to the agent as procedural guidance. This spec joins the two systems, which until now knew nothing about each other.

The document was written on 2026-05-07 against the `feat/institutional-providers` branch and approved there before being split onto its own branch. It is preserved as the record of what was decided; the task-by-task execution record lives in the [companion implementation plan](brxt-bundled-skills-plan.md).

## Overview

Extend BioRouter's `.brxt` extension package format to carry bundled skills. When a `.brxt` is installed, its skills are automatically installed alongside the MCP server. When the extension is removed, its skills are removed too. Separately, the standalone skill import UI gains ZIP file support.

## Background

A `.brxt` file is a standard ZIP archive containing a Python MCP server package plus a `manifest.json` BioRouter metadata file. Skills in BioRouter are `SKILL.md` files (YAML frontmatter + markdown body) discovered by the Rust `SkillsClient` from a set of hardcoded directories. Currently these two systems are completely independent — no `.brxt` ships skills, and skill install/remove has no awareness of the extension that may have produced it.

## Goals

1. `.brxt` files can optionally bundle skills under a `skills/` directory.
2. Installing a `.brxt` automatically installs its skills; no user action required.
3. Removing an extension removes its skills atomically.
4. The skill import UI accepts `.zip` files in addition to `.md` files and folders.
5. Test skills added to CDWAgent, UCSFOMOPAgent, and SPOKEAgent fixture `.brxt` files.

## Non-goals

- No plugin manifest system (`.claude-plugin/plugin.json`) — not needed yet.
- No `.mcp.json` auto-loading — handled separately by the extension config system.
- No skill versioning or conflict resolution.
- No UI to browse/preview skills before install.

## Design

### `.brxt` ZIP format extension

Add an optional `skills/` directory inside the ZIP:

```text
manifest.json
README.md
pyproject.toml
src/<name>/
  __init__.py
  __main__.py
  cli.py
  server.py
skills/                        ← NEW (optional)
  <skill-slug>/
    SKILL.md                   ← standard frontmatter: name, description
    [supporting files]         ← images, data, referenced markdown
```

`manifest.json` gains an optional `skills` array for install-preview UI only:

```json
{
  "name": "cdwagent",
  "display_name": "CDWAgent",
  "description": "...",
  "version": "0.4.3",
  "entry_point": "cdwagent",
  "repository": "...",
  "tools_count": 21,
  "env_vars": [...],
  "skills": [
    { "name": "cdw-query-cohorts", "description": "Build patient cohorts from CDW data" },
    { "name": "cdw-explore-schema", "description": "Explore the CDW clinical schema" }
  ]
}
```

The `skills` array is optional metadata for display; actual installed skills come from the zip's `skills/` directory. If `skills/` is absent, no skills are installed.

### Skills storage strategy (Approach A: extension-local)

Skills from a `.brxt` install into:

```text
~/.config/biorouter/extensions/<name>/skills/<slug>/SKILL.md
```

This is the same root directory as the Python source code. A single `rm -rf ~/.config/biorouter/extensions/<name>/` removes the Python code, virtual environment, and all skills atomically. No separate tracking file is needed.

The Rust `SkillsClient` gains one additional search pattern:

```text
~/.config/biorouter/extensions/*/skills/
```

This is appended to the existing list of 6 skill directories in `get_default_skill_directories()` in `crates/biorouter/src/agents/skills_extension.rs`.

### Electron main process changes

All three handlers live in `ui/desktop/src/main.ts`.

#### `brxt:validate-and-read` (existing handler, extended)

After reading `manifest.json`, also scan `skills/*/SKILL.md` within the zip. For each found skill, parse its frontmatter and add to a `skills_preview` array returned to the renderer:

```typescript
{
  manifest: BrxtManifest,           // existing
  skills_preview: Array<{           // NEW
    slug: string,
    name: string,
    description: string,
  }>
}
```

If `manifest.json` already has a `skills` array and the zip has no `skills/` dir, return the manifest skills array as the preview (display-only). If both exist, the zip content takes precedence.

#### `brxt:install` (existing handler, unchanged in behavior)

The zip is extracted in full to `~/.config/biorouter/extensions/<name>/`. If the zip contains `skills/`, those directories land at `~/.config/biorouter/extensions/<name>/skills/<slug>/` automatically. No extra extraction step needed.

#### `brxt:uninstall` (new handler)

```typescript
// Input: { extensionName: string }
// Output: { success: true } | { error: string }
// Action: rm -rf ~/.config/biorouter/extensions/<extensionName>
```

Called before the API `DELETE /config/extensions/<name>` when removing an extension that was installed from a `.brxt` (detected by checking if the extension `cmd` path is inside `~/.config/biorouter/extensions/`).

### TypeScript types

In `ui/desktop/src/types/brxt.ts`:

```typescript
export interface BrxtSkillMeta {
  name: string;
  description: string;
}

export interface BrxtManifest {
  name: string;
  display_name: string;
  description: string;
  version: string;
  entry_point: string;
  repository: string;
  tools_count?: number;
  env_vars: BrxtEnvVar[];
  skills?: BrxtSkillMeta[];   // NEW — optional metadata for install preview
}
```

### Install modal changes

In `BrxtInstallModal.tsx`:

- Display skills that will be installed (from `skills_preview`) in the install confirmation step.
- If `skills_preview.length > 0`, show a "Skills included" section listing skill names and descriptions.
- No user toggle — skills always install with the extension.

### Extension removal wiring

In `ExtensionsView.tsx` or `extension-manager.ts`. When the user removes an extension:

1. Check if the extension config has `cmd: 'uv'` and `args` containing a path under `~/.config/biorouter/extensions/` — this identifies `.brxt`-installed extensions.
2. If yes, call `window.electron.uninstallBrxtExtension(extensionName)` **first** (filesystem), then call `DELETE /config/extensions/<name>` (config). If filesystem removal fails, abort and show an error — do not remove from config so the entry remains retryable.
3. Show a toast: "Extension and its skills removed."

### Skill import ZIP support

In `AddSkillModal.tsx`, add `.zip` to accepted file types alongside `.md` and folder.

The ZIP processing flow:

1. The user selects a `.zip` file.
2. The renderer sends the file path to the Electron main process via a new `skills:extract-zip` IPC handler, which uses the already-present `adm-zip` library to extract files and return their contents to the renderer.
3. The handler looks for `SKILL.md` at the zip root, or inside exactly one subdirectory (for example, `myskill/SKILL.md`).
4. All files are extracted. If `SKILL.md` is found, its frontmatter is parsed.
5. Files are written to `~/.config/biorouter/skills/<slug>/` via Electron IPC — the same path as folder import.

If no `SKILL.md` is found in the zip, the UI shows an error: "No SKILL.md found in the ZIP file."

### Rust skill scanner change

In `get_default_skill_directories()` in `skills_extension.rs`, append the extension skills glob after the existing 6 directories:

```rust
// Scan each installed extension's skills/ subdirectory
let extensions_dir = platform_config_dir.join("extensions");
if extensions_dir.exists() {
    if let Ok(entries) = std::fs::read_dir(&extensions_dir) {
        for entry in entries.flatten() {
            let skills_subdir = entry.path().join("skills");
            if skills_subdir.is_dir() {
                dirs.push(skills_subdir);
            }
        }
    }
}
```

This requires no changes to the skill parsing or tool exposure logic.

### Test fixtures

Create test `.brxt` fixture files in `ui/desktop/tests/fixtures/` that bundle sample skills. Each fixture is a ZIP built by a test helper script.

CDWAgent skills (2):

- `cdw-query-cohorts` — "Systematically build and refine patient cohorts using CDW clinical data"
- `cdw-explore-schema` — "Explore the CDW clinical schema, tables, and relationships"

UCSFOMOPAgent skills (1):

- `omop-phenotype-query` — "Query the OMOP CDM to identify patient phenotypes and clinical concepts"

SPOKEAgent skills (1):

- `spoke-knowledge-graph` — "Traverse the SPOKE biomedical knowledge graph to find relationships between entities"

Test fixture script: `ui/desktop/tests/fixtures/build-test-brxts.ts` — builds four `.brxt` files, one without skills (existing tests) and three with skills.

> **Note on what shipped.** The four sample `SKILL.md` files exist on disk under `ui/desktop/tests/fixtures/skills/{cdwagent,ucsfomopagent,spokeagent}/`, but the standalone `build-test-brxts.ts` script proposed here was never created. The fixture `.brxt` archives are instead built in-test by a `createValidBrxtWithSkills` helper in `ui/desktop/tests/e2e/brxt.spec.ts`.

## File change summary

| File | Change |
|------|--------|
| `crates/biorouter/src/agents/skills_extension.rs` | Add `~/.config/biorouter/extensions/*/skills/` to discovery |
| `ui/desktop/src/types/brxt.ts` | Add `BrxtSkillMeta`, extend `BrxtManifest` |
| `ui/desktop/src/main.ts` | Extend `brxt:validate-and-read`; add `brxt:uninstall` handler |
| `ui/desktop/src/preload.ts` | Expose `uninstallBrxtExtension` IPC binding |
| `ui/desktop/src/components/BrxtInstallModal.tsx` | Show skills preview section |
| `ui/desktop/src/components/extensions/ExtensionsView.tsx` | Call `uninstallBrxtExtension` on removal |
| `ui/desktop/src/components/skills/AddSkillModal.tsx` | Add ZIP import support |
| `ui/desktop/tests/fixtures/build-test-brxts.ts` | Script to build test `.brxt` fixtures with skills |
| `ui/desktop/tests/fixtures/skills/` | Sample SKILL.md files for each test agent |

## Error handling

- **ZIP with no `skills/` dir**: silently skip skills installation (not an error).
- **Malformed `SKILL.md` in zip**: log a warning, skip that skill, continue installing others.
- **`brxt:uninstall` failure**: show error toast, do not proceed with config removal (keep extension listed so user can retry).
- **Skills ZIP import with no `SKILL.md`**: show error in UI "No SKILL.md found in ZIP".
- **Skills ZIP import with multiple `SKILL.md` files**: use the one at the shortest path (shallowest level).

## Testing

- Unit: extend `brxt.spec.ts` with cases for skills-bundled `.brxt` install and removal.
- Unit: `AddSkillModal` — add test for ZIP file import path.
- Manual: install CDWAgent test fixture, verify skills appear in skill list; remove extension, verify skills gone.

## Related documentation

- [.brxt bundled skills implementation plan](brxt-bundled-skills-plan.md) — the task-by-task execution of this design, with the exact code that landed.
- [Skill bundles design](skill-bundles-design.md) — the sibling spec from the same day; it defines multi-skill *bundle* folders, which this spec's ZIP import path later had to detect.
- [Skill bundles implementation plan](skill-bundles-plan.md) — extends the `skills:extract-zip` handler introduced here to recognise bundle ZIPs.
- [Extensions, skills, and MCP agents](../../extensions/extensions-and-skills-guide.md) — the current end-user guide to installing extensions and skills.
- [Skills extension](../../extensions/built-in/skills.md) — how skill discovery behaves today from a user's point of view.
