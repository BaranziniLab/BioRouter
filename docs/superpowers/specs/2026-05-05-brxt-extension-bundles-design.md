# Design: .brxt Extension Bundle System

**Date:** 2026-05-05
**Author:** Wanjun Gu / Baranzini Lab
**Status:** Approved

---

## Overview

This spec covers:
1. The `.brxt` file format — a zip-based BioRouter extension bundle
2. Three initial extension bundles: CDWAgent, UCSFOMOPAgent, SPOKEAgent
3. BioRouter UI changes — new "Add extension" button and drag-and-drop install dialog
4. Electron IPC handlers for bundle validation and installation
5. GitHub release publishing and README updates for all three repos
6. Local testing strategy

---

## 1. The `.brxt` Bundle Format

### Structure

A `.brxt` file is a **standard zip archive** with the extension `.brxt`. Required contents:

```
<name>.brxt (zip)
├── manifest.json       ← required: extension metadata + env var schema
├── README.md           ← required: human-readable description
├── pyproject.toml      ← required: uv project definition
└── src/
    └── <package>/      ← required: Python source code
        ├── __init__.py
        ├── cli.py
        ├── server.py
        └── ...
```

**Validation:** the installer checks for all four required entries (`manifest.json`, `README.md`, `pyproject.toml`, and at least one file under `src/`). If any are missing, the installer shows an inline error and prevents installation from proceeding.

### manifest.json Schema

```json
{
  "name": "cdwagent",
  "display_name": "CDWAgent",
  "description": "Read-only MCP access to UCSF Epic Caboodle Clinical Data Warehouse.",
  "version": "0.4.3",
  "entry_point": "cdwagent",
  "repository": "https://github.com/BaranziniLab/CDWAgent",
  "tools_count": 21,
  "env_vars": [
    {
      "key": "CLINICAL_RECORDS_USERNAME",
      "required": true,
      "auto_propagate": false,
      "description": "UCSF network username (e.g. CAMPUS\\youruser)",
      "secret": true
    },
    {
      "key": "CLINICAL_RECORDS_PASSWORD",
      "required": true,
      "auto_propagate": false,
      "description": "UCSF network password",
      "secret": true
    },
    {
      "key": "CLINICAL_RECORDS_SERVER",
      "required": false,
      "auto_propagate": true,
      "default": "QCDIDDWDB001.ucsfmedicalcenter.org",
      "description": "SQL Server hostname",
      "secret": false
    },
    {
      "key": "CLINICAL_RECORDS_DATABASE",
      "required": false,
      "auto_propagate": true,
      "default": "CDW_NEW",
      "description": "Database name",
      "secret": false
    },
    {
      "key": "CDW_NAMESPACE",
      "required": false,
      "auto_propagate": true,
      "default": "CDW",
      "description": "Tool namespace prefix",
      "secret": false
    },
    {
      "key": "CDW_SCHEMA",
      "required": false,
      "auto_propagate": true,
      "default": "deid_uf",
      "description": "SQL schema name",
      "secret": false
    },
    {
      "key": "CDW_LOG_LEVEL",
      "required": false,
      "auto_propagate": true,
      "default": "INFO",
      "description": "Logging level",
      "secret": false
    }
  ]
}
```

**Field definitions:**

| Field | Type | Meaning |
|-------|------|---------|
| `name` | string | Python package name / extension identifier (no spaces) |
| `display_name` | string | Human-readable name shown in BioRouter UI |
| `description` | string | One-sentence description shown in install dialog and extension list |
| `version` | string | Semver string matching pyproject.toml |
| `entry_point` | string | Console script name registered in pyproject.toml |
| `repository` | string | GitHub repo URL for future update checking |
| `env_vars[].key` | string | Environment variable name |
| `env_vars[].required` | bool | If true, Install button is disabled until a non-empty value is provided |
| `env_vars[].auto_propagate` | bool | If true, the `default` is pre-filled in the UI; user may edit |
| `env_vars[].default` | string? | Default value (only meaningful when `auto_propagate: true`) |
| `env_vars[].description` | string | Shown as placeholder/tooltip in the env var form |
| `env_vars[].secret` | bool | If true, rendered as `type="password"` and stored via BioRouter's secret store |

---

## 2. The Three Extension Bundles

### CDWAgent (`cdwagent.brxt`)
- **Version:** 0.4.3
- **Repo:** https://github.com/BaranziniLab/CDWAgent
- **Description:** Read-only MCP access to UCSF Epic Caboodle Clinical Data Warehouse (SQL Server). 21 tools across schema discovery, clinical queries, notes NLP, concept search, export, and statistics.
- **Entry point:** `cdwagent`
- **Required env vars:** `CLINICAL_RECORDS_USERNAME`, `CLINICAL_RECORDS_PASSWORD`
- **Optional env vars (auto-propagated):** `CLINICAL_RECORDS_SERVER`, `CLINICAL_RECORDS_DATABASE`, `CDW_NAMESPACE`, `CDW_SCHEMA`, `CDW_LOG_LEVEL`

### UCSFOMOPAgent (`ucsfomopagent.brxt`)
- **Version:** 0.1.0
- **Repo:** https://github.com/BaranziniLab/UCSFOMOPAgent
- **Description:** Read-only MCP access to the UCSF OMOP de-identified EHR database (SQL Server). 2 tools: query execution and table listing.
- **Entry point:** `ucsfomopagent`
- **Required env vars:** `CLINICAL_RECORDS_USERNAME`, `CLINICAL_RECORDS_PASSWORD`
- **Optional env vars (auto-propagated):** `OMOP_LOG_LEVEL`

### SPOKEAgent (`spokeagent.brxt`)
- **Version:** 0.1.0
- **Repo:** https://github.com/BaranziniLab/SPOKEAgent
- **Description:** Read-only Cypher queries against the SPOKE biomedical knowledge graph (Neo4j). Access requires a passcode from UCSF. 2 tools: schema introspection and query execution.
- **Entry point:** `spokeagent`
- **Required env vars:** `SPOKEAGENT_PASSCODE`
- **Optional env vars (auto-propagated):** `SPOKE_LOG_LEVEL`

---

## 3. Installation Architecture

### Filesystem Layout

```
~/.config/biorouter/
├── config.yaml                          ← existing BioRouter config
├── sessions/                            ← existing sessions
└── extensions/
    ├── cdwagent/
    │   ├── manifest.json
    │   ├── README.md
    │   ├── pyproject.toml
    │   ├── src/
    │   │   └── cdwagent/
    │   └── .venv/                       ← created by `uv sync` at install time
    ├── ucsfomopagent/
    │   └── ...
    └── spokeagent/
        └── ...
```

### Install Sequence

1. User drops `.brxt` file (or selects via file picker) into the BrxtInstallModal
2. Renderer sends file path to main process via IPC: `brxt:validate-and-read`
3. Main process:
   - Unzips the archive in memory
   - Validates presence of `manifest.json`, `README.md`, `pyproject.toml`, `src/`
   - Returns manifest JSON to renderer on success, or error message on failure
4. Renderer shows extension info (name, version, description, env var count)
5. User clicks "Next: Configure →" → Step 2 shows env var form
6. User fills required env vars, optionally edits auto-propagated defaults
7. User clicks "Install Extension" → renderer sends IPC: `brxt:install`
8. Main process:
   - Extracts zip to `~/.config/biorouter/extensions/<name>/`
   - Runs `uv sync` in that directory (blocks until complete; shows spinner in UI)
   - Returns success or error
9. Renderer calls `addExtension()` with the extension config
10. Modal closes, success toast shown, extension appears in list

### BioRouter Extension Config (written to `~/.config/biorouter/config.yaml`)

```yaml
extensions:
  cdwagent:
    type: stdio
    cmd: "uv run --directory ~/.config/biorouter/extensions/cdwagent cdwagent"
    envs:
      CLINICAL_RECORDS_USERNAME: "CAMPUS\\youruser"
      CLINICAL_RECORDS_PASSWORD: "••••••••"     # stored in secret store
      CLINICAL_RECORDS_SERVER: "QCDIDDWDB001.ucsfmedicalcenter.org"
      CLINICAL_RECORDS_DATABASE: "CDW_NEW"
      CDW_NAMESPACE: "CDW"
      CDW_SCHEMA: "deid_uf"
      CDW_LOG_LEVEL: "INFO"
    enabled: true
    timeout: 300
```

Secret values (`secret: true`) are stored via BioRouter's existing `upsertConfig` secret store, not in plaintext in `config.yaml`.

### Electron IPC Handlers (in `main.ts`)

**`brxt:validate-and-read`**
- Input: `{ filePath: string }` — the renderer uses Electron's `File.path` property (available in Electron, not standard web) to get the real filesystem path
- Output: `{ manifest: BrxtManifest } | { error: string }`
- Opens the zip, validates structure, parses and returns manifest

**`brxt:install`**
- Input: `{ filePath: string, extensionName: string }` — same `File.path` passed through
- Output: `{ success: true } | { error: string }`
- Extracts zip to `~/.config/biorouter/extensions/<name>/`, runs `uv sync` (blocking; renderer shows spinner during this step)

### TypeScript Types

```typescript
interface BrxtEnvVar {
  key: string;
  required: boolean;
  auto_propagate: boolean;
  default?: string;
  description: string;
  secret: boolean;
}

interface BrxtManifest {
  name: string;
  display_name: string;
  description: string;
  version: string;
  entry_point: string;
  repository: string;
  env_vars: BrxtEnvVar[];
}
```

---

## 4. UI Changes

### 4a. Button Layout (`ExtensionsView.tsx` and `ExtensionsSection.tsx`)

New order and styling:

| Position | Label | Variant | Action |
|----------|-------|---------|--------|
| 1st | `Add extension` | `default` (black) | Opens `BrxtInstallModal` |
| 2nd | `Browse extensions` | `outline` | Opens baam.html (unchanged) |
| 3rd | `Add custom extension` | `outline` | Opens existing `ExtensionModal` |

Changes from current:
- "Add custom extension" changes from `variant="default"` to `variant="outline"`
- New "Add extension" button added before it with `variant="default"`
- Order swapped so Browse comes before Add custom extension

Both `ExtensionsView.tsx` (header buttons) and `ExtensionsSection.tsx` (bottom buttons, shown when `hideButtons` is false) get this change.

### 4b. New `BrxtInstallModal` Component

**File:** `ui/desktop/src/components/BrxtInstallModal.tsx`

Uses existing `Dialog`/`DialogContent` pattern. Two steps controlled by local `step` state:

**Step 1 — Drop & Validate:**
- `<input type="file" accept=".brxt">` hidden, triggered by "Browse file…" button
- Drag-and-drop event handlers on the drop zone div
- On file received: show filename + loading spinner, call `window.electron.invoke('brxt:validate-and-read', { filePath })`
- On success: show manifest info card (name, version, description, `tools_count` if present, required env var count)
- On error: show red error banner with the error message; allow trying a different file
- "Next: Configure →" button disabled until a valid manifest is loaded

**Step 2 — Configure Env Vars:**
- Required vars rendered first, with red `*` label, `type="password"` for `secret: true`
- Optional vars with `auto_propagate: true` rendered below, pre-filled with `default`, de-emphasized styling
- "Show/hide optional vars" toggle (optional vars hidden by default if all have defaults)
- Install button disabled until all `required` vars have non-empty values
- On Install click: call `window.electron.invoke('brxt:install', { filePath, extensionName: manifest.name })`, show spinner
- On success: call `addExtension(name, extensionConfig, true)`, close modal, show toast
- On error: show error banner, stay on Step 2

### 4c. Preload Bridge (`preload.ts`)

Add `brxt:validate-and-read` and `brxt:install` to the IPC invoke allowlist.

---

## 5. GitHub Releases & README Updates

### Release Process (for each of the 3 repos)

Using `gh release create` with:
- Tag: `v<version>-brxt` (e.g. `v0.4.3-brxt`)
- Title: `<DisplayName> v<version> — BioRouter Extension Bundle`
- Attached asset: `<name>.brxt`
- Release notes (markdown):
  - What's new (bundle format introduced)
  - How to install: drag `.brxt` into BioRouter Extensions tab → Add extension
  - Env var reference table (key, required, default, description)
  - Manual `uv` run instructions

### README Updates (each repo)

Add a "## BioRouter Extension" section near the top containing:
- Download badge/link pointing to latest `.brxt` release asset
- One-paragraph summary: "Drag the `.brxt` file into BioRouter's Extensions → Add extension dialog. BioRouter will install the virtual environment automatically and prompt for required credentials."
- Env var table: key | required | default | description
- Existing `uv` manual-run section preserved below

---

## 6. Testing Strategy

### Bundle Integrity Tests (local, no credentials needed)

For each `.brxt`:
1. `unzip <name>.brxt -d /tmp/<name>-test` → verify all 4 required paths exist
2. `python -c "import json; json.load(open('/tmp/<name>-test/manifest.json'))"` → valid JSON
3. Validate manifest has required fields: `name`, `display_name`, `description`, `version`, `entry_point`, `repository`, `env_vars`
4. `cd /tmp/<name>-test && uv sync` → all dependencies install cleanly
5. Launch with dummy env vars and verify process starts (FastMCP servers start and wait for stdio even without live DB/credentials):
   ```bash
   CLINICAL_RECORDS_USERNAME="test" CLINICAL_RECORDS_PASSWORD="test" \
     uv run --directory /tmp/cdwagent-test cdwagent &
   sleep 2; kill %1
   ```

### UI Tests (Playwright via BioRouter's `playwright-electron` MCP)

1. **Button layout test:** Verify 3 buttons present in correct order; verify "Add extension" is black/default, "Browse extensions" and "Add custom extension" are outline
2. **Drop zone test:** Simulate file drop of a valid `.brxt`; verify extension info card appears with correct name/version
3. **Validation error test:** Drop an invalid zip (missing manifest.json); verify error banner appears
4. **Env var form test:** After valid bundle loaded, click Next; verify required fields have red asterisk, optional fields are pre-filled
5. **Install flow test:** Fill required fields with dummy values, click Install; verify extension appears in extensions list
6. **Full install smoke test:** Complete install → toggle extension on → verify extension is listed as enabled

---

## 7. Out of Scope

- Automatic update checking via the `repository` field (field is stored for future use)
- Bundle signing or verification
- Windows path handling differences (future work)
- Publishing `.brxt` files to a central registry
