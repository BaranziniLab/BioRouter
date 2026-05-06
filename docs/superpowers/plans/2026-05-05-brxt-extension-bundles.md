# .brxt Extension Bundle System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the full .brxt extension bundle system — create bundles for CDWAgent, UCSFOMOPAgent, and SPOKEAgent; add Electron IPC handlers for bundle validation and installation; add a drag-and-drop "Add extension" UI in BioRouter's Extensions tab; publish releases; and test end-to-end with Playwright.

**Architecture:** A `.brxt` file is a zip archive containing `manifest.json`, `README.md`, `pyproject.toml`, and `src/`. Electron's main process handles zip extraction and `uv sync` via two IPC handlers. The React renderer presents a two-step Dialog (drop → configure env vars). After install, the extension runs via `uv run --directory ~/.config/biorouter/extensions/<name> <entry-point>` registered as a `stdio` extension in BioRouter's config.

**Tech Stack:** TypeScript, React 19, Electron, adm-zip (zip extraction in Node.js main process), uv (Python venv management), gh CLI (GitHub releases), Playwright via playwright-electron MCP (E2E tests)

**Spec:** `docs/superpowers/specs/2026-05-05-brxt-extension-bundles-design.md`

---

## File Map

### New files (BioRouter repo)
| File | Purpose |
|------|---------|
| `ui/desktop/src/types/brxt.ts` | TypeScript interfaces: `BrxtManifest`, `BrxtEnvVar` |
| `ui/desktop/src/components/BrxtInstallModal.tsx` | Two-step drag-and-drop install dialog |

### Modified files (BioRouter repo)
| File | Change |
|------|--------|
| `ui/desktop/package.json` | Add `adm-zip` + `@types/adm-zip` |
| `ui/desktop/src/main.ts` | Add `brxt:validate-and-read` and `brxt:install` IPC handlers; add `spawnSync` import |
| `ui/desktop/src/preload.ts` | Add `validateBrxtBundle` and `installBrxtBundle` to `ElectronAPI` type + implementation |
| `ui/desktop/src/components/extensions/ExtensionsView.tsx` | Button reorder: Add extension (black) → Browse extensions (outline) → Add custom extension (outline) |
| `ui/desktop/src/components/settings/extensions/ExtensionsSection.tsx` | Same button reorder |

### External repos (separate git operations per repo)
| Repo | Files changed |
|------|--------------|
| CDWAgent | `manifest.json` (new), `README.md` (updated), `cdwagent.brxt` (release asset) |
| UCSFOMOPAgent | `manifest.json` (new), `README.md` (updated), `ucsfomopagent.brxt` (release asset) |
| SPOKEAgent | `manifest.json` (new), `README.md` (updated), `spokeagent.brxt` (release asset) |

---

## Phase A: BioRouter UI Infrastructure

### Task 1: TypeScript types and adm-zip dependency

**Files:**
- Create: `ui/desktop/src/types/brxt.ts`
- Modify: `ui/desktop/package.json`

- [ ] **Step 1: Create brxt types file**

```typescript
// ui/desktop/src/types/brxt.ts

export interface BrxtEnvVar {
  key: string;
  required: boolean;
  auto_propagate: boolean;
  default?: string;
  description: string;
  secret: boolean;
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
}
```

- [ ] **Step 2: Install adm-zip**

```bash
cd /Users/wgu/Desktop/BioRouter/ui/desktop
npm install adm-zip
npm install --save-dev @types/adm-zip
```

- [ ] **Step 3: Verify it loads**

```bash
cd /Users/wgu/Desktop/BioRouter/ui/desktop
node -e "const AdmZip = require('adm-zip'); console.log('adm-zip ok');"
```

Expected: `adm-zip ok`

- [ ] **Step 4: Commit**

```bash
cd /Users/wgu/Desktop/BioRouter
git add ui/desktop/src/types/brxt.ts ui/desktop/package.json ui/desktop/package-lock.json
git commit -m "feat: add BrxtManifest types and adm-zip dependency"
```

---

### Task 2: Preload bridge — expose brxt IPC methods

**Files:**
- Modify: `ui/desktop/src/preload.ts`

The preload exposes typed methods on `window.electron`. Every new IPC call needs: (1) a type entry in `ElectronAPI`, (2) an implementation in `electronAPI`. Use `window.electron.getPathForFile(file)` (already present via `webUtils.getPathForFile`) to get the real filesystem path from a `File` object.

- [ ] **Step 1: Add types to ElectronAPI interface**

In `ui/desktop/src/preload.ts`, find the `ElectronAPI` type definition (around line 60–143) and add these two entries before the closing `};`:

```typescript
  validateBrxtBundle: (filePath: string) => Promise<{ manifest: import('./types/brxt').BrxtManifest } | { error: string }>;
  installBrxtBundle: (filePath: string, extensionName: string) => Promise<{ success: true; installDir: string } | { error: string }>;
```

- [ ] **Step 2: Add implementations to electronAPI object**

Find the `electronAPI` object (around line 150) and add these two entries before the closing `};`:

```typescript
  validateBrxtBundle: (filePath: string) =>
    ipcRenderer.invoke('brxt:validate-and-read', { filePath }),
  installBrxtBundle: (filePath: string, extensionName: string) =>
    ipcRenderer.invoke('brxt:install', { filePath, extensionName }),
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
cd /Users/wgu/Desktop/BioRouter/ui/desktop
npx tsc --noEmit 2>&1 | grep -v "node_modules" | head -20
```

Expected: No new errors. (Pre-existing errors unrelated to this change are OK.)

- [ ] **Step 4: Commit**

```bash
cd /Users/wgu/Desktop/BioRouter
git add ui/desktop/src/preload.ts
git commit -m "feat: expose validateBrxtBundle and installBrxtBundle in preload bridge"
```

---

### Task 3: Electron IPC — brxt:validate-and-read handler

**Files:**
- Modify: `ui/desktop/src/main.ts`

`path`, `os`, `fs`, `fsSync` are already imported. `spawn` from `child_process` is already imported. Only `AdmZip` and `spawnSync` need to be added.

- [ ] **Step 1: Add missing imports to main.ts**

Find the existing import block at the top of `ui/desktop/src/main.ts`. Add after line 25 (`import { spawn } from 'child_process';`):

```typescript
import { spawnSync } from 'child_process';
import AdmZip from 'adm-zip';
```

- [ ] **Step 2: Add the validate-and-read handler**

Find the block of `ipcMain.handle` calls (starting around line 1230) and add this handler after the last one in the file (before any non-IPC code at the bottom):

```typescript
ipcMain.handle(
  'brxt:validate-and-read',
  async (_event, { filePath }: { filePath: string }) => {
    try {
      const zip = new AdmZip(filePath);
      const entries = zip.getEntries().map((e) => e.entryName);

      if (!entries.some((e) => e === 'manifest.json'))
        return { error: 'Missing manifest.json — not a valid .brxt bundle' };
      if (!entries.some((e) => e.toLowerCase() === 'readme.md'))
        return { error: 'Missing README.md — not a valid .brxt bundle' };
      if (!entries.some((e) => e === 'pyproject.toml'))
        return { error: 'Missing pyproject.toml — not a valid .brxt bundle' };
      if (!entries.some((e) => e.startsWith('src/')))
        return { error: 'Missing src/ directory — not a valid .brxt bundle' };

      const manifestEntry = zip.getEntry('manifest.json');
      if (!manifestEntry) return { error: 'Could not read manifest.json' };

      const manifest = JSON.parse(manifestEntry.getData().toString('utf8'));

      for (const field of ['name', 'display_name', 'description', 'version', 'entry_point', 'repository']) {
        if (!manifest[field])
          return { error: `manifest.json missing required field: "${field}"` };
      }

      if (!Array.isArray(manifest.env_vars))
        return { error: 'manifest.json "env_vars" must be an array' };

      return { manifest };
    } catch (err) {
      return { error: `Failed to read bundle: ${(err as Error).message}` };
    }
  }
);
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
cd /Users/wgu/Desktop/BioRouter/ui/desktop
npx tsc --noEmit 2>&1 | grep -v "node_modules" | head -20
```

Expected: No new errors.

- [ ] **Step 4: Commit**

```bash
cd /Users/wgu/Desktop/BioRouter
git add ui/desktop/src/main.ts
git commit -m "feat: add brxt:validate-and-read IPC handler"
```

---

### Task 4: Electron IPC — brxt:install handler

**Files:**
- Modify: `ui/desktop/src/main.ts`

- [ ] **Step 1: Add the install handler immediately after the validate handler**

```typescript
ipcMain.handle(
  'brxt:install',
  async (_event, { filePath, extensionName }: { filePath: string; extensionName: string }) => {
    try {
      const installDir = path.join(os.homedir(), '.config', 'biorouter', 'extensions', extensionName);

      // Create install directory
      fsSync.mkdirSync(installDir, { recursive: true });

      // Extract bundle
      const zip = new AdmZip(filePath);
      zip.extractAllTo(installDir, /* overwrite */ true);

      // Pre-build the virtual environment
      const uvResult = spawnSync('uv', ['sync'], {
        cwd: installDir,
        encoding: 'utf8',
        timeout: 120_000,
      });

      if (uvResult.status !== 0) {
        throw new Error(`uv sync failed:\n${uvResult.stderr || uvResult.stdout}`);
      }

      return { success: true, installDir };
    } catch (err) {
      return { error: `Installation failed: ${(err as Error).message}` };
    }
  }
);
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
cd /Users/wgu/Desktop/BioRouter/ui/desktop
npx tsc --noEmit 2>&1 | grep -v "node_modules" | head -20
```

Expected: No new errors.

- [ ] **Step 3: Commit**

```bash
cd /Users/wgu/Desktop/BioRouter
git add ui/desktop/src/main.ts
git commit -m "feat: add brxt:install IPC handler with uv sync"
```

---

### Task 5: BrxtInstallModal component

**Files:**
- Create: `ui/desktop/src/components/BrxtInstallModal.tsx`

This is a two-step Dialog. Step 1: drop zone + file picker + validation preview. Step 2: env var form + install.

- [ ] **Step 1: Create the component**

```tsx
// ui/desktop/src/components/BrxtInstallModal.tsx
import { useState, useCallback, useRef } from 'react';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { Button } from './ui/button';
import { BrxtEnvVar, BrxtManifest } from '../types/brxt';
import { useConfig } from './ConfigContext';
import { activateExtensionDefault } from './settings/extensions';
import { upsertConfig } from '../api';
import { toastService } from '../toasts';

interface EnvEntry {
  key: string;
  value: string;
  secret: boolean;
  required: boolean;
  description: string;
  auto_propagate: boolean;
}

type Step = 'drop' | 'configure';

interface Props {
  onClose: () => void;
  onInstalled: () => void;
}

export function BrxtInstallModal({ onClose, onInstalled }: Props) {
  const [step, setStep] = useState<Step>('drop');
  const [filePath, setFilePath] = useState<string | null>(null);
  const [manifest, setManifest] = useState<BrxtManifest | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isValidating, setIsValidating] = useState(false);
  const [isInstalling, setIsInstalling] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [envEntries, setEnvEntries] = useState<EnvEntry[]>([]);
  const [showOptional, setShowOptional] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const { addExtension } = useConfig();

  const processFile = useCallback(async (file: File) => {
    setError(null);
    setManifest(null);
    setIsValidating(true);
    const fp = window.electron.getPathForFile(file);
    setFilePath(fp);

    const result = await window.electron.validateBrxtBundle(fp);
    setIsValidating(false);

    if ('error' in result) {
      setError(result.error);
    } else {
      setManifest(result.manifest);
      setEnvEntries(
        result.manifest.env_vars.map((v: BrxtEnvVar) => ({
          key: v.key,
          value: v.auto_propagate && v.default ? v.default : '',
          secret: v.secret,
          required: v.required,
          description: v.description,
          auto_propagate: v.auto_propagate,
        }))
      );
    }
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setIsDragging(false);
      const file = e.dataTransfer.files[0];
      if (!file) return;
      if (!file.name.endsWith('.brxt')) {
        setError('Please drop a .brxt file');
        return;
      }
      processFile(file);
    },
    [processFile]
  );

  const handleFileInput = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) processFile(file);
      // reset so same file can be re-selected after an error
      e.target.value = '';
    },
    [processFile]
  );

  const setEnvValue = (key: string, value: string) =>
    setEnvEntries((prev) => prev.map((e) => (e.key === key ? { ...e, value } : e)));

  const handleInstall = async () => {
    if (!filePath || !manifest) return;
    setIsInstalling(true);
    setError(null);

    const result = await window.electron.installBrxtBundle(filePath, manifest.name);

    if ('error' in result) {
      setError(result.error);
      setIsInstalling(false);
      return;
    }

    const { installDir } = result;

    // Store secrets in BioRouter's secret store
    for (const entry of envEntries.filter((e) => e.secret && e.value)) {
      await upsertConfig({
        body: { is_secret: true, key: entry.key, value: entry.value },
      }).catch(() => {});
    }

    // Build envs map for extension config
    const envs: Record<string, string> = {};
    envEntries.forEach(({ key, value }) => { if (value) envs[key] = value; });

    const extensionConfig = {
      name: manifest.name,
      display_name: manifest.display_name,
      description: manifest.description,
      type: 'stdio' as const,
      cmd: `uv run --directory "${installDir}" ${manifest.entry_point}`,
      envs,
      timeout: 300,
    };

    try {
      await activateExtensionDefault({ addToConfig: addExtension, extensionConfig });
      toastService.success({
        title: manifest.display_name,
        msg: 'Extension installed and enabled',
      });
      onInstalled();
      onClose();
    } catch {
      setError('Extension installed but failed to register. Try adding it manually from the Extensions tab.');
      setIsInstalling(false);
    }
  };

  const requiredVars = envEntries.filter((e) => e.required);
  const optionalVars = envEntries.filter((e) => !e.required);
  const requiredMissing = requiredVars.some((e) => !e.value.trim());

  return (
    <Dialog open={true} onOpenChange={onClose}>
      <DialogContent className="sm:max-w-[560px] max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {step === 'drop' ? 'Add Extension' : `Configure ${manifest?.display_name ?? ''}`}
          </DialogTitle>
        </DialogHeader>

        {step === 'drop' && (
          <div className="py-4 space-y-4">
            <p className="text-sm text-text-muted">
              Install a BioRouter extension bundle (.brxt file).
            </p>

            {/* Drop zone */}
            <div
              className={[
                'border-2 border-dashed rounded-xl p-10 text-center transition-colors cursor-pointer select-none',
                isDragging
                  ? 'border-blue-400 bg-blue-50 dark:bg-blue-900/10'
                  : error
                  ? 'border-red-400 bg-red-50 dark:bg-red-900/10'
                  : 'border-border-subtle hover:border-border-strong',
              ].join(' ')}
              onDragOver={(e) => { e.preventDefault(); setIsDragging(true); }}
              onDragLeave={() => setIsDragging(false)}
              onDrop={handleDrop}
              onClick={() => fileInputRef.current?.click()}
            >
              <input
                ref={fileInputRef}
                type="file"
                accept=".brxt"
                className="hidden"
                onChange={handleFileInput}
              />
              {isValidating ? (
                <p className="text-sm text-text-muted animate-pulse">Reading bundle…</p>
              ) : (
                <>
                  <p className="text-3xl mb-2">📦</p>
                  <p className="text-sm font-medium mb-1">Drop your .brxt file here</p>
                  <p className="text-xs text-text-muted mb-3">or click to browse</p>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={(e) => { e.stopPropagation(); fileInputRef.current?.click(); }}
                  >
                    Browse file…
                  </Button>
                </>
              )}
            </div>

            {/* Error banner */}
            {error && (
              <div className="bg-red-50 dark:bg-red-900/10 border border-red-300 dark:border-red-700 rounded-lg p-3">
                <p className="text-sm text-red-600 dark:text-red-400">{error}</p>
              </div>
            )}

            {/* Manifest preview card */}
            {manifest && !error && (
              <div className="bg-background-medium border border-border-subtle rounded-lg p-4">
                <p className="text-xs text-text-muted uppercase tracking-wide mb-2">
                  Detected from bundle
                </p>
                <p className="text-sm font-semibold">{manifest.display_name}</p>
                <p className="text-xs text-text-muted mt-0.5">
                  v{manifest.version}
                  {manifest.tools_count ? ` · ${manifest.tools_count} tools` : ''}
                  {' · '}
                  {requiredVars.length} required env var
                  {requiredVars.length !== 1 ? 's' : ''}
                </p>
                <p className="text-sm text-text-default mt-2">{manifest.description}</p>
              </div>
            )}

            <DialogFooter>
              <Button variant="outline" onClick={onClose}>
                Cancel
              </Button>
              <Button
                disabled={!manifest || !!error || isValidating}
                onClick={() => setStep('configure')}
              >
                Next: Configure →
              </Button>
            </DialogFooter>
          </div>
        )}

        {step === 'configure' && manifest && (
          <div className="py-4 space-y-4">
            <p className="text-sm text-text-muted">
              Fill in required credentials. Optional fields are pre-filled with defaults.
            </p>

            {requiredVars.length > 0 && (
              <div className="space-y-3">
                <p className="text-xs font-semibold text-text-default uppercase tracking-wide">
                  Required
                </p>
                {requiredVars.map((entry) => (
                  <div key={entry.key}>
                    <label className="block text-xs font-semibold mb-1">
                      {entry.key} <span className="text-red-500">*</span>
                    </label>
                    <input
                      type={entry.secret ? 'password' : 'text'}
                      className="w-full border border-border-subtle rounded-md px-3 py-2 text-sm bg-background-default focus:outline-none focus:ring-2 focus:ring-blue-400"
                      placeholder={entry.description}
                      value={entry.value}
                      onChange={(e) => setEnvValue(entry.key, e.target.value)}
                      autoComplete="off"
                    />
                  </div>
                ))}
              </div>
            )}

            {optionalVars.length > 0 && (
              <div>
                <button
                  type="button"
                  className="text-xs text-text-muted underline"
                  onClick={() => setShowOptional((v) => !v)}
                >
                  {showOptional ? 'Hide' : 'Show'} {optionalVars.length} optional variable
                  {optionalVars.length !== 1 ? 's' : ''} (pre-filled with defaults)
                </button>
                {showOptional && (
                  <div className="space-y-3 mt-3">
                    <p className="text-xs font-semibold text-text-muted uppercase tracking-wide">
                      Optional
                    </p>
                    {optionalVars.map((entry) => (
                      <div key={entry.key}>
                        <label className="block text-xs font-medium text-text-muted mb-1">
                          {entry.key}
                        </label>
                        <input
                          type={entry.secret ? 'password' : 'text'}
                          className="w-full border border-border-subtle rounded-md px-3 py-2 text-sm bg-background-subtle focus:outline-none focus:ring-2 focus:ring-blue-400"
                          placeholder={entry.description}
                          value={entry.value}
                          onChange={(e) => setEnvValue(entry.key, e.target.value)}
                          autoComplete="off"
                        />
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}

            {error && (
              <div className="bg-red-50 dark:bg-red-900/10 border border-red-300 dark:border-red-700 rounded-lg p-3">
                <p className="text-sm text-red-600 dark:text-red-400">{error}</p>
              </div>
            )}

            <DialogFooter>
              <Button
                variant="outline"
                onClick={() => { setStep('drop'); setError(null); }}
              >
                ← Back
              </Button>
              <Button variant="outline" onClick={onClose}>
                Cancel
              </Button>
              <Button disabled={requiredMissing || isInstalling} onClick={handleInstall}>
                {isInstalling ? 'Installing…' : 'Install Extension'}
              </Button>
            </DialogFooter>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Verify TypeScript compiles**

```bash
cd /Users/wgu/Desktop/BioRouter/ui/desktop
npx tsc --noEmit 2>&1 | grep -v "node_modules" | head -20
```

Expected: No new errors.

- [ ] **Step 3: Commit**

```bash
cd /Users/wgu/Desktop/BioRouter
git add ui/desktop/src/components/BrxtInstallModal.tsx
git commit -m "feat: add BrxtInstallModal two-step drag-and-drop install dialog"
```

---

### Task 6: Update button layout in ExtensionsView and ExtensionsSection

**Files:**
- Modify: `ui/desktop/src/components/extensions/ExtensionsView.tsx`
- Modify: `ui/desktop/src/components/settings/extensions/ExtensionsSection.tsx`

New order: **Add extension** (black/default) → **Browse extensions** (outline) → **Add custom extension** (outline)

- [ ] **Step 1: Update ExtensionsView.tsx**

Read `ui/desktop/src/components/extensions/ExtensionsView.tsx`.

Add `BrxtInstallModal` import at the top:
```typescript
import { BrxtInstallModal } from '../BrxtInstallModal';
```

Add state for the brxt modal alongside existing state (after `const [isAddModalOpen, setIsAddModalOpen] = useState(false);`):
```typescript
const [isBrxtModalOpen, setIsBrxtModalOpen] = useState(false);
```

Replace the existing `<div className="flex gap-3 mt-5">` buttons block (the two-button block containing "Add custom extension" and "Browse extensions") with:

```tsx
<div className="flex gap-3 mt-5">
  <Button
    className="flex items-center gap-2"
    variant="default"
    onClick={() => setIsBrxtModalOpen(true)}
  >
    <Plus className="h-4 w-4" />
    Add extension
  </Button>
  <Button
    className="flex items-center gap-2"
    variant="outline"
    onClick={() =>
      window.open('https://baranzinilab.github.io/biorouter-landing/baam.html', '_blank')
    }
  >
    <GPSIcon size={12} />
    Browse extensions
  </Button>
  <Button
    className="flex items-center gap-2"
    variant="outline"
    onClick={() => setIsAddModalOpen(true)}
  >
    <Plus className="h-4 w-4" />
    Add custom extension
  </Button>
</div>
```

Add the `BrxtInstallModal` render alongside the existing `ExtensionModal` render (after the closing `)}` of the existing modal):

```tsx
{isBrxtModalOpen && (
  <BrxtInstallModal
    onClose={() => setIsBrxtModalOpen(false)}
    onInstalled={() => setRefreshKey((prev) => prev + 1)}
  />
)}
```

- [ ] **Step 2: Update ExtensionsSection.tsx**

Read `ui/desktop/src/components/settings/extensions/ExtensionsSection.tsx`.

Add import:
```typescript
import { BrxtInstallModal } from '../../BrxtInstallModal';
```

Add state (alongside existing `isAddModalOpen`):
```typescript
const [isBrxtModalOpen, setIsBrxtModalOpen] = useState(false);
```

Replace the `{!hideButtons && ...}` buttons block with:

```tsx
{!hideButtons && (
  <div className="flex gap-4 pt-4 w-full">
    <Button
      className="flex items-center gap-2 justify-center"
      variant="default"
      onClick={() => setIsBrxtModalOpen(true)}
    >
      <Plus className="h-4 w-4" />
      Add extension
    </Button>
    <Button
      className="flex items-center gap-2 justify-center"
      variant="outline"
      onClick={() =>
        window.open('https://baranzinilab.github.io/biorouter-landing/baam.html', '_blank')
      }
    >
      <GPSIcon size={12} />
      Browse extensions
    </Button>
    <Button
      className="flex items-center gap-2 justify-center"
      variant="outline"
      onClick={() => setIsAddModalOpen(true)}
    >
      <Plus className="h-4 w-4" />
      Add custom extension
    </Button>
  </div>
)}
```

Add `BrxtInstallModal` render right before the closing `</section>` tag:

```tsx
{isBrxtModalOpen && (
  <BrxtInstallModal
    onClose={() => setIsBrxtModalOpen(false)}
    onInstalled={fetchExtensions}
  />
)}
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
cd /Users/wgu/Desktop/BioRouter/ui/desktop
npx tsc --noEmit 2>&1 | grep -v "node_modules" | head -20
```

Expected: No new errors.

- [ ] **Step 4: Commit**

```bash
cd /Users/wgu/Desktop/BioRouter
git add ui/desktop/src/components/extensions/ExtensionsView.tsx \
        ui/desktop/src/components/settings/extensions/ExtensionsSection.tsx
git commit -m "feat: add Add extension button and reorder Extensions tab buttons"
```

---

## Phase B: Create the Three Extension Bundles

> For each bundle: clone the repo, write manifest.json, update README, package the .brxt, test it locally.

### Task 7: CDWAgent bundle

**Working dir:** `/tmp/bundle-work/CDWAgent` (temporary; not committed to BioRouter repo)

- [ ] **Step 1: Clone CDWAgent**

```bash
mkdir -p /tmp/bundle-work
cd /tmp/bundle-work
gh repo clone BaranziniLab/CDWAgent
```

- [ ] **Step 2: Create manifest.json**

Create `/tmp/bundle-work/CDWAgent/manifest.json`:

```json
{
  "name": "cdwagent",
  "display_name": "CDWAgent",
  "description": "Read-only MCP access to the UCSF Epic Caboodle Clinical Data Warehouse. Provides 21 tools for schema discovery, clinical queries, notes NLP, concept search, data export, and cohort statistics.",
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
      "description": "Logging verbosity (DEBUG, INFO, WARNING, ERROR)",
      "secret": false
    }
  ]
}
```

- [ ] **Step 3: Add BioRouter Extension section to README.md**

Open `/tmp/bundle-work/CDWAgent/README.md` and insert this section immediately after the first `# CDWAgent` heading (before the existing content):

```markdown
## BioRouter Extension

CDWAgent is available as a BioRouter extension bundle (`.brxt`). Download and drag it into BioRouter's **Extensions → Add extension** dialog — no manual setup required.

**[⬇ Download cdwagent.brxt](https://github.com/BaranziniLab/CDWAgent/releases/latest)**

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `CLINICAL_RECORDS_USERNAME` | **Yes** | — | UCSF network username (e.g. `CAMPUS\youruser`) |
| `CLINICAL_RECORDS_PASSWORD` | **Yes** | — | UCSF network password |
| `CLINICAL_RECORDS_SERVER` | No | `QCDIDDWDB001.ucsfmedicalcenter.org` | SQL Server hostname |
| `CLINICAL_RECORDS_DATABASE` | No | `CDW_NEW` | Database name |
| `CDW_NAMESPACE` | No | `CDW` | Tool namespace prefix |
| `CDW_SCHEMA` | No | `deid_uf` | SQL schema name |
| `CDW_LOG_LEVEL` | No | `INFO` | Logging verbosity |

BioRouter will prompt for required variables on install and pre-fill optional ones with defaults.

---
```

- [ ] **Step 4: Commit manifest.json and README to CDWAgent repo**

```bash
cd /tmp/bundle-work/CDWAgent
git add manifest.json README.md
git commit -m "feat: add BioRouter extension bundle manifest and install instructions"
git push origin main
```

- [ ] **Step 5: Package the .brxt bundle**

```bash
cd /tmp/bundle-work/CDWAgent
zip -r /tmp/bundle-work/cdwagent.brxt manifest.json README.md pyproject.toml src/
```

- [ ] **Step 6: Verify the bundle locally**

```bash
# Unpack to a temp dir
mkdir -p /tmp/brxt-test/cdwagent
unzip /tmp/bundle-work/cdwagent.brxt -d /tmp/brxt-test/cdwagent

# Verify all required files exist
ls /tmp/brxt-test/cdwagent/manifest.json /tmp/brxt-test/cdwagent/README.md \
   /tmp/brxt-test/cdwagent/pyproject.toml /tmp/brxt-test/cdwagent/src/
echo "All required files present"

# Validate manifest JSON
python3 -c "
import json, sys
m = json.load(open('/tmp/brxt-test/cdwagent/manifest.json'))
for f in ['name','display_name','description','version','entry_point','repository','env_vars']:
    assert f in m, f'Missing field: {f}'
print('manifest.json valid, entry_point:', m['entry_point'])
"

# uv sync
cd /tmp/brxt-test/cdwagent
uv sync
echo "uv sync exit code: $?"
```

Expected:
```
All required files present
manifest.json valid, entry_point: cdwagent
uv sync exit code: 0
```

- [ ] **Step 7: Smoke-test server start with dummy credentials**

```bash
cd /tmp/brxt-test/cdwagent
CLINICAL_RECORDS_USERNAME="dummy" CLINICAL_RECORDS_PASSWORD="dummy" \
  timeout 5 uv run cdwagent 2>&1 || true
```

Expected: Process starts and exits (timeout kills it). No import errors or crashes before the DB connection attempt.

---

### Task 8: UCSFOMOPAgent bundle

**Working dir:** `/tmp/bundle-work/UCSFOMOPAgent`

- [ ] **Step 1: Clone UCSFOMOPAgent**

```bash
cd /tmp/bundle-work
gh repo clone BaranziniLab/UCSFOMOPAgent
```

- [ ] **Step 2: Create manifest.json**

Create `/tmp/bundle-work/UCSFOMOPAgent/manifest.json`:

```json
{
  "name": "ucsfomopagent",
  "display_name": "UCSFOMOPAgent",
  "description": "Read-only MCP access to the UCSF OMOP de-identified EHR database. Provides 2 tools: SQL query execution and table listing against the OMOP Common Data Model.",
  "version": "0.1.0",
  "entry_point": "ucsfomopagent",
  "repository": "https://github.com/BaranziniLab/UCSFOMOPAgent",
  "tools_count": 2,
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
      "key": "OMOP_LOG_LEVEL",
      "required": false,
      "auto_propagate": true,
      "default": "INFO",
      "description": "Logging verbosity (DEBUG, INFO, WARNING, ERROR)",
      "secret": false
    }
  ]
}
```

- [ ] **Step 3: Add BioRouter Extension section to README.md**

Open `/tmp/bundle-work/UCSFOMOPAgent/README.md`. If the file is minimal (just a title), replace with a full README. Insert or prepend:

```markdown
## BioRouter Extension

UCSFOMOPAgent is available as a BioRouter extension bundle (`.brxt`). Download and drag it into BioRouter's **Extensions → Add extension** dialog.

**[⬇ Download ucsfomopagent.brxt](https://github.com/BaranziniLab/UCSFOMOPAgent/releases/latest)**

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `CLINICAL_RECORDS_USERNAME` | **Yes** | — | UCSF network username (e.g. `CAMPUS\youruser`) |
| `CLINICAL_RECORDS_PASSWORD` | **Yes** | — | UCSF network password |
| `OMOP_LOG_LEVEL` | No | `INFO` | Logging verbosity |

---
```

- [ ] **Step 4: Commit and push**

```bash
cd /tmp/bundle-work/UCSFOMOPAgent
git add manifest.json README.md
git commit -m "feat: add BioRouter extension bundle manifest and install instructions"
git push origin main
```

- [ ] **Step 5: Package the bundle**

```bash
cd /tmp/bundle-work/UCSFOMOPAgent
zip -r /tmp/bundle-work/ucsfomopagent.brxt manifest.json README.md pyproject.toml src/
```

- [ ] **Step 6: Verify the bundle locally**

```bash
mkdir -p /tmp/brxt-test/ucsfomopagent
unzip /tmp/bundle-work/ucsfomopagent.brxt -d /tmp/brxt-test/ucsfomopagent

python3 -c "
import json
m = json.load(open('/tmp/brxt-test/ucsfomopagent/manifest.json'))
for f in ['name','display_name','description','version','entry_point','repository','env_vars']:
    assert f in m, f'Missing field: {f}'
print('manifest.json valid, entry_point:', m['entry_point'])
"

cd /tmp/brxt-test/ucsfomopagent
uv sync
echo "uv sync exit code: $?"
```

- [ ] **Step 7: Smoke-test**

```bash
cd /tmp/brxt-test/ucsfomopagent
CLINICAL_RECORDS_USERNAME="dummy" CLINICAL_RECORDS_PASSWORD="dummy" \
  timeout 5 uv run ucsfomopagent 2>&1 || true
```

Expected: Starts without Python import errors before DB connection.

---

### Task 9: SPOKEAgent bundle

**Working dir:** `/tmp/bundle-work/SPOKEAgent`

- [ ] **Step 1: Clone SPOKEAgent**

```bash
cd /tmp/bundle-work
gh repo clone BaranziniLab/SPOKEAgent
```

- [ ] **Step 2: Create manifest.json**

Create `/tmp/bundle-work/SPOKEAgent/manifest.json`:

```json
{
  "name": "spokeagent",
  "display_name": "SPOKEAgent",
  "description": "Read-only Cypher queries against the SPOKE biomedical knowledge graph (Neo4j). Provides 2 tools: schema introspection and query execution. Access requires a UCSF-issued passcode.",
  "version": "0.1.0",
  "entry_point": "spokeagent",
  "repository": "https://github.com/BaranziniLab/SPOKEAgent",
  "tools_count": 2,
  "env_vars": [
    {
      "key": "SPOKEAGENT_PASSCODE",
      "required": true,
      "auto_propagate": false,
      "description": "UCSF-issued passcode for SPOKE access. Contact the Baranzini Lab to request access.",
      "secret": true
    },
    {
      "key": "SPOKE_LOG_LEVEL",
      "required": false,
      "auto_propagate": true,
      "default": "INFO",
      "description": "Logging verbosity (DEBUG, INFO, WARNING, ERROR)",
      "secret": false
    }
  ]
}
```

- [ ] **Step 3: Add BioRouter Extension section to README.md**

Open `/tmp/bundle-work/SPOKEAgent/README.md` and insert/prepend:

```markdown
## BioRouter Extension

SPOKEAgent is available as a BioRouter extension bundle (`.brxt`). Download and drag it into BioRouter's **Extensions → Add extension** dialog.

**[⬇ Download spokeagent.brxt](https://github.com/BaranziniLab/SPOKEAgent/releases/latest)**

> **Access:** The `SPOKEAGENT_PASSCODE` required to use this extension is issued by the UCSF Baranzini Lab. Contact the lab to request access.

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `SPOKEAGENT_PASSCODE` | **Yes** | — | UCSF-issued passcode for SPOKE Neo4j access |
| `SPOKE_LOG_LEVEL` | No | `INFO` | Logging verbosity |

---
```

- [ ] **Step 4: Commit and push**

```bash
cd /tmp/bundle-work/SPOKEAgent
git add manifest.json README.md
git commit -m "feat: add BioRouter extension bundle manifest and install instructions"
git push origin main
```

- [ ] **Step 5: Package the bundle**

```bash
cd /tmp/bundle-work/SPOKEAgent
zip -r /tmp/bundle-work/spokeagent.brxt manifest.json README.md pyproject.toml src/
```

- [ ] **Step 6: Verify the bundle locally**

```bash
mkdir -p /tmp/brxt-test/spokeagent
unzip /tmp/bundle-work/spokeagent.brxt -d /tmp/brxt-test/spokeagent

python3 -c "
import json
m = json.load(open('/tmp/brxt-test/spokeagent/manifest.json'))
for f in ['name','display_name','description','version','entry_point','repository','env_vars']:
    assert f in m, f'Missing field: {f}'
print('manifest.json valid, entry_point:', m['entry_point'])
"

cd /tmp/brxt-test/spokeagent
uv sync
echo "uv sync exit code: $?"
```

- [ ] **Step 7: Smoke-test**

```bash
cd /tmp/brxt-test/spokeagent
# SPOKEAgent refuses to start without the passcode — verify the error is the expected one
SPOKEAGENT_PASSCODE="" timeout 5 uv run spokeagent 2>&1 || true
```

Expected: Process exits with a `RuntimeError` or similar about missing/invalid passcode (not a Python import error or crash).

---

## Phase C: GitHub Releases

### Task 10: CDWAgent GitHub release

- [ ] **Step 1: Create the release**

```bash
gh release create v0.4.3-brxt \
  --repo BaranziniLab/CDWAgent \
  --title "CDWAgent v0.4.3 — BioRouter Extension Bundle" \
  --notes "$(cat <<'EOF'
## BioRouter Extension Bundle

This release introduces the `.brxt` bundle format for one-click installation via BioRouter's **Extensions → Add extension** dialog.

### How to install
1. Download `cdwagent.brxt` from the assets below
2. Open BioRouter → Extensions tab
3. Click **Add extension** and drop the `.brxt` file
4. Enter your UCSF credentials when prompted
5. Click **Install Extension** — BioRouter handles the rest

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `CLINICAL_RECORDS_USERNAME` | **Yes** | — | UCSF network username |
| `CLINICAL_RECORDS_PASSWORD` | **Yes** | — | UCSF network password |
| `CLINICAL_RECORDS_SERVER` | No | `QCDIDDWDB001.ucsfmedicalcenter.org` | SQL Server hostname |
| `CLINICAL_RECORDS_DATABASE` | No | `CDW_NEW` | Database name |
| `CDW_NAMESPACE` | No | `CDW` | Tool namespace prefix |
| `CDW_SCHEMA` | No | `deid_uf` | SQL schema name |
| `CDW_LOG_LEVEL` | No | `INFO` | Logging verbosity |

### Manual uv run
```bash
git clone https://github.com/BaranziniLab/CDWAgent.git
cd CDWAgent
uv sync
CLINICAL_RECORDS_USERNAME="CAMPUS\\youruser" CLINICAL_RECORDS_PASSWORD="yourpass" uv run cdwagent
```
EOF
)" \
  /tmp/bundle-work/cdwagent.brxt
```

- [ ] **Step 2: Verify the release**

```bash
gh release view v0.4.3-brxt --repo BaranziniLab/CDWAgent
```

Expected: Release shown with `cdwagent.brxt` as an asset.

---

### Task 11: UCSFOMOPAgent GitHub release

- [ ] **Step 1: Create the release**

```bash
gh release create v0.1.0-brxt \
  --repo BaranziniLab/UCSFOMOPAgent \
  --title "UCSFOMOPAgent v0.1.0 — BioRouter Extension Bundle" \
  --notes "$(cat <<'EOF'
## BioRouter Extension Bundle

### How to install
1. Download `ucsfomopagent.brxt` from the assets below
2. Open BioRouter → Extensions tab → **Add extension**
3. Drop the file, enter UCSF credentials, click **Install Extension**

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `CLINICAL_RECORDS_USERNAME` | **Yes** | — | UCSF network username |
| `CLINICAL_RECORDS_PASSWORD` | **Yes** | — | UCSF network password |
| `OMOP_LOG_LEVEL` | No | `INFO` | Logging verbosity |

### Manual uv run
```bash
git clone https://github.com/BaranziniLab/UCSFOMOPAgent.git
cd UCSFOMOPAgent && uv sync
CLINICAL_RECORDS_USERNAME="CAMPUS\\youruser" CLINICAL_RECORDS_PASSWORD="yourpass" uv run ucsfomopagent
```
EOF
)" \
  /tmp/bundle-work/ucsfomopagent.brxt
```

- [ ] **Step 2: Verify**

```bash
gh release view v0.1.0-brxt --repo BaranziniLab/UCSFOMOPAgent
```

---

### Task 12: SPOKEAgent GitHub release

- [ ] **Step 1: Create the release**

```bash
gh release create v0.1.0-brxt \
  --repo BaranziniLab/SPOKEAgent \
  --title "SPOKEAgent v0.1.0 — BioRouter Extension Bundle" \
  --notes "$(cat <<'EOF'
## BioRouter Extension Bundle

### How to install
1. Download `spokeagent.brxt` from the assets below
2. Open BioRouter → Extensions tab → **Add extension**
3. Drop the file, enter your UCSF-issued passcode, click **Install Extension**

> **Access:** The `SPOKEAGENT_PASSCODE` is issued by the UCSF Baranzini Lab. Contact the lab to request access.

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `SPOKEAGENT_PASSCODE` | **Yes** | — | UCSF-issued passcode |
| `SPOKE_LOG_LEVEL` | No | `INFO` | Logging verbosity |

### Manual uv run
```bash
git clone https://github.com/BaranziniLab/SPOKEAgent.git
cd SPOKEAgent && uv sync
SPOKEAGENT_PASSCODE="your-passcode" uv run spokeagent
```
EOF
)" \
  /tmp/bundle-work/spokeagent.brxt
```

- [ ] **Step 2: Verify**

```bash
gh release view v0.1.0-brxt --repo BaranziniLab/SPOKEAgent
```

---

## Phase D: Playwright UI Tests

> Use the `playwright-electron` MCP tools. See `claude_debug_ui.md` for full setup instructions. Start BioRouter in debug mode before running tests.

### Task 13: Start the app and take baseline snapshot

- [ ] **Step 1: Build and launch BioRouter in debug mode**

```bash
cd /Users/wgu/Desktop/BioRouter
just run-dev
```

Wait for the app window to appear.

- [ ] **Step 2: Navigate to Extensions tab and take snapshot**

Use `mcp__playwright-electron__browser_snapshot` to capture the current state of the Extensions tab. Navigate there first via `mcp__playwright-electron__browser_click` on the Extensions nav item in the sidebar.

- [ ] **Step 3: Verify baseline loads without errors**

Check the snapshot for any console errors via `mcp__playwright-electron__browser_console_messages`.

---

### Task 14: Button layout tests

- [ ] **Step 1: Verify three buttons are present in correct order**

Use `mcp__playwright-electron__browser_snapshot` on the Extensions tab header. Confirm the accessibility tree or visible text contains, in order:
1. "Add extension" button
2. "Browse extensions" button
3. "Add custom extension" button

- [ ] **Step 2: Verify button styles**

Take a screenshot with `mcp__playwright-electron__browser_take_screenshot`. Visually confirm:
- "Add extension" is filled/black (variant="default")
- "Browse extensions" and "Add custom extension" are outlined

- [ ] **Step 3: Click "Browse extensions" — verify external link opens**

Use `mcp__playwright-electron__browser_click` on "Browse extensions" button. Verify no crash occurs (the `window.open` call goes to baam.html externally).

- [ ] **Step 4: Click "Add custom extension" — verify existing ExtensionModal opens**

Use `mcp__playwright-electron__browser_click` on "Add custom extension". Verify a dialog appears with the title "Add custom extension" via `mcp__playwright-electron__browser_snapshot`.

Close the dialog with Escape: `mcp__playwright-electron__browser_press_key` with key `Escape`.

---

### Task 15: BrxtInstallModal flow tests

- [ ] **Step 1: Open Add extension dialog**

Click "Add extension" button. Verify the `BrxtInstallModal` opens with:
- Title: "Add Extension"
- Drop zone with "Drop your .brxt file here" text
- "Browse file…" button
- "Next: Configure →" button (disabled)

Use `mcp__playwright-electron__browser_snapshot` to confirm.

- [ ] **Step 2: Test invalid file rejection**

Create a test invalid bundle:
```bash
echo "not a valid zip" > /tmp/invalid.brxt
```

Use `mcp__playwright-electron__browser_file_upload` to upload `/tmp/invalid.brxt` to the file input (`input[type="file"][accept=".brxt"]`).

Verify the error banner appears with a message about invalid/unreadable bundle. "Next: Configure →" must remain disabled.

- [ ] **Step 3: Test missing-manifest rejection**

```bash
cd /tmp
mkdir -p missing-manifest-test/src/pkg
echo "def main(): pass" > missing-manifest-test/src/pkg/cli.py
echo "[project]" > missing-manifest-test/pyproject.toml
echo "# README" > missing-manifest-test/README.md
# Intentionally omit manifest.json
zip -r /tmp/missing-manifest.brxt missing-manifest-test/src missing-manifest-test/pyproject.toml missing-manifest-test/README.md
```

Upload `/tmp/missing-manifest.brxt` to the file input.

Verify error: "Missing manifest.json — not a valid .brxt bundle" appears.

- [ ] **Step 4: Test valid bundle — Step 1 success state**

Upload a real bundle (e.g. `/tmp/bundle-work/cdwagent.brxt`) to the file input.

Verify:
- No error banner
- Manifest preview card appears showing "CDWAgent", "v0.4.3", "2 required env vars"
- "Next: Configure →" button is enabled

Take a screenshot.

- [ ] **Step 5: Test Step 2 — env var form**

Click "Next: Configure →".

Verify:
- Title changes to "Configure CDWAgent"
- `CLINICAL_RECORDS_USERNAME *` field is empty (required, red asterisk)
- `CLINICAL_RECORDS_PASSWORD *` field is empty (required, red asterisk)
- "Show N optional variables" toggle is present
- "Install Extension" button is disabled (required fields empty)

- [ ] **Step 6: Test optional vars toggle**

Click the "Show optional variables" toggle.

Verify `CLINICAL_RECORDS_SERVER` field appears pre-filled with `QCDIDDWDB001.ucsfmedicalcenter.org`.

- [ ] **Step 7: Test Install button enables on required var fill**

Fill `CLINICAL_RECORDS_USERNAME` with `dummy_user` and `CLINICAL_RECORDS_PASSWORD` with `dummy_pass` via `mcp__playwright-electron__browser_fill_form`.

Verify "Install Extension" button becomes enabled.

- [ ] **Step 8: Test full install with dummy credentials**

Click "Install Extension". Wait for the install to complete (the `uv sync` step takes a few seconds — wait up to 30s).

Verify:
- Modal closes
- A success toast appears with "Extension installed and enabled"
- The Extensions list now contains "CDWAgent" (or "cdwagent") as an entry

Use `mcp__playwright-electron__browser_snapshot` to confirm.

- [ ] **Step 9: Verify extension is listed and toggleable**

In the Extensions list, find the newly installed CDWAgent entry. Verify it has an enabled toggle.

- [ ] **Step 10: Commit final BioRouter changes**

```bash
cd /Users/wgu/Desktop/BioRouter
git add -A
git commit -m "feat: complete .brxt extension bundle system

- Add BrxtInstallModal with drag-and-drop and file picker
- Add brxt:validate-and-read and brxt:install Electron IPC handlers
- Reorder Extensions tab buttons: Add extension (black), Browse extensions, Add custom extension (outline)
- Add adm-zip for zip extraction in main process
- TypeScript types for BrxtManifest and BrxtEnvVar"
```

---

## Self-Review Checklist

- [x] Spec section 1 (bundle format + manifest schema) → Tasks 7–9 (manifest.json creation)
- [x] Spec section 2 (install architecture, uv sync) → Task 4 (brxt:install handler)
- [x] Spec section 3a (button reorder) → Task 6
- [x] Spec section 3b (BrxtInstallModal, both steps) → Task 5
- [x] Spec section 3c (Electron IPC handlers) → Tasks 3–4
- [x] Spec section 4a (GitHub releases) → Tasks 10–12
- [x] Spec section 4b (README updates) → Tasks 7–9 step 3
- [x] Spec section 4c (testing strategy) → Tasks 13–15
- [x] `window.electron.getPathForFile(file)` used (not `file.path`) → Task 5
- [x] `installDir` returned from main process (not constructed in renderer) → Tasks 4 + 5
- [x] `spawnSync` used instead of `execSync` (no shell injection risk) → Task 4
- [x] Secret env vars stored via `upsertConfig` → Task 5
