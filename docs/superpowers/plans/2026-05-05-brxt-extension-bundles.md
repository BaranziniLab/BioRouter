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

## Phase E: File Association (Double-click .brxt → BioRouter)

> When a user double-clicks a `.brxt` file, BioRouter opens, navigates to the Extensions tab, and auto-loads the file into `BrxtInstallModal` ready for the configure step. This requires platform-specific file type registration **and** a runtime handler in main.ts.

### Task 16: BrxtInstallModal — preloaded file path + skip-configure when no env vars

**Files:**
- Modify: `ui/desktop/src/components/BrxtInstallModal.tsx`

Two changes: (1) accept optional `preloadedFilePath` prop and auto-validate on mount; (2) skip Step 2 entirely when the manifest has no env vars.

- [ ] **Step 1: Add preloadedFilePath prop and auto-validation**

Update the `Props` interface and add a `useEffect` that runs when `preloadedFilePath` is provided:

```tsx
// Update Props interface
interface Props {
  onClose: () => void;
  onInstalled: () => void;
  preloadedFilePath?: string;   // <-- new
}

// Add inside the component, after state declarations:
useEffect(() => {
  if (preloadedFilePath) {
    setFilePath(preloadedFilePath);
    setIsValidating(true);
    window.electron.validateBrxtBundle(preloadedFilePath).then((result) => {
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
    });
  }
}, [preloadedFilePath]);
```

- [ ] **Step 2: Skip configure step when no env vars**

In the Step 1 footer, replace the single "Next: Configure →" button with conditional logic:

```tsx
<DialogFooter>
  <Button variant="outline" onClick={onClose}>
    Cancel
  </Button>
  {manifest && !error && manifest.env_vars.length === 0 ? (
    // No env vars at all — install directly from step 1
    <Button
      disabled={isInstalling}
      onClick={handleInstall}
    >
      {isInstalling ? 'Installing…' : 'Install Extension'}
    </Button>
  ) : (
    <Button
      disabled={!manifest || !!error || isValidating}
      onClick={() => setStep('configure')}
    >
      Next: Configure →
    </Button>
  )}
</DialogFooter>
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
cd /Users/wgu/Desktop/BioRouter/ui/desktop
npx tsc --noEmit 2>&1 | grep -v "node_modules" | head -20
```

- [ ] **Step 4: Commit**

```bash
cd /Users/wgu/Desktop/BioRouter
git add ui/desktop/src/components/BrxtInstallModal.tsx
git commit -m "feat: skip configure step when no env vars; support preloaded file path"
```

---

### Task 17: Main process — handle .brxt file opens

**Files:**
- Modify: `ui/desktop/src/main.ts`

macOS fires `app.on('open-file')`. Windows and Linux pass the file path as a CLI argument (`process.argv`). In both cases, the main process sends `open-brxt-file` to the renderer window.

- [ ] **Step 1: Add a helper function to send brxt open event to renderer**

Find where `mainWindow` is created in `ui/desktop/src/main.ts`. Add this helper function near the other helper functions (e.g., near `handleProtocolUrl`):

```typescript
function handleBrxtFileOpen(filePath: string) {
  if (!filePath.endsWith('.brxt')) return;
  // If window isn't ready yet, queue it; otherwise send immediately
  const sendEvent = () => {
    const win = BrowserWindow.getAllWindows()[0];
    if (win) {
      win.webContents.send('open-brxt-file', filePath);
      win.show();
      win.focus();
    }
  };

  const win = BrowserWindow.getAllWindows()[0];
  if (win && !win.webContents.isLoading()) {
    sendEvent();
  } else {
    app.once('browser-window-created', () => {
      setTimeout(sendEvent, 1000);
    });
  }
}
```

- [ ] **Step 2: Handle macOS open-file event**

Find `app.on('open-file', ...)` in `main.ts` (search for `open-file` — it may already exist for other file types). If it already exists, add a `.brxt` branch inside it. If it doesn't exist, add it in the `app.whenReady()` block or near app initialization:

```typescript
app.on('open-file', (event, filePath) => {
  event.preventDefault();
  if (filePath.endsWith('.brxt')) {
    handleBrxtFileOpen(filePath);
  }
});
```

- [ ] **Step 3: Handle Windows/Linux CLI argument**

Near the existing startup code that handles `process.argv` (search for `process.argv` in `main.ts`), add a check for `.brxt` files passed as arguments. Add this after `app.whenReady()` resolves:

```typescript
// Check if launched with a .brxt file argument (Windows/Linux double-click)
const brxtArg = process.argv.slice(app.isPackaged ? 1 : 2).find((a) => a.endsWith('.brxt'));
if (brxtArg) {
  app.whenReady().then(() => handleBrxtFileOpen(brxtArg));
}
```

- [ ] **Step 4: Expose open-brxt-file in preload allowlist**

In `ui/desktop/src/preload.ts`, the `window.electron.on` method already forwards any channel. Verify that `on` / `off` do not filter channels — if they have an allowlist, add `'open-brxt-file'` to it.

- [ ] **Step 5: Verify TypeScript compiles**

```bash
cd /Users/wgu/Desktop/BioRouter/ui/desktop
npx tsc --noEmit 2>&1 | grep -v "node_modules" | head -20
```

- [ ] **Step 6: Commit**

```bash
cd /Users/wgu/Desktop/BioRouter
git add ui/desktop/src/main.ts ui/desktop/src/preload.ts
git commit -m "feat: handle .brxt file open via macOS open-file event and CLI args"
```

---

### Task 18: BrxtFileOpenHandler — renderer listens for open-brxt-file

**Files:**
- Read then modify: `ui/desktop/src/App.tsx`

The renderer needs to: (1) listen for `open-brxt-file`, (2) navigate to the Extensions view, (3) open `BrxtInstallModal` with the preloaded path.

- [ ] **Step 1: Read App.tsx to understand navigation and view state**

```bash
cat /Users/wgu/Desktop/BioRouter/ui/desktop/src/App.tsx
```

Look for: how `setView` or navigation works, where `ExtensionInstallModal` is rendered, how the current view is tracked. The `BrxtFileOpenHandler` will follow the same pattern as `ExtensionInstallModal`.

- [ ] **Step 2: Add BrxtFileOpenHandler component**

Add this component to `App.tsx` (or as a separate file `ui/desktop/src/components/BrxtFileOpenHandler.tsx` if App.tsx is large). It mirrors the pattern of `ExtensionInstallModal`:

```tsx
// Add to App.tsx (or a new BrxtFileOpenHandler.tsx imported into App.tsx)
import { useEffect, useState } from 'react';
import { IpcRendererEvent } from 'electron';
import { BrxtInstallModal } from './BrxtInstallModal';
import { View, ViewOptions } from '../utils/navigationUtils';
import { useConfig } from './ConfigContext';

interface BrxtFileOpenHandlerProps {
  setView: (view: View, options?: ViewOptions) => void;
}

export function BrxtFileOpenHandler({ setView }: BrxtFileOpenHandlerProps) {
  const [pendingFilePath, setPendingFilePath] = useState<string | null>(null);
  const { addExtension } = useConfig();  // needed by BrxtInstallModal via context

  useEffect(() => {
    const handler = (_event: IpcRendererEvent, filePath: string) => {
      // Navigate to Extensions tab first, then open the modal
      setView('extensions');
      setPendingFilePath(filePath);
    };
    window.electron.on('open-brxt-file', handler);
    return () => window.electron.off('open-brxt-file', handler);
  }, [setView]);

  if (!pendingFilePath) return null;

  return (
    <BrxtInstallModal
      preloadedFilePath={pendingFilePath}
      onClose={() => setPendingFilePath(null)}
      onInstalled={() => setPendingFilePath(null)}
    />
  );
}
```

- [ ] **Step 3: Mount BrxtFileOpenHandler in App.tsx**

In `App.tsx`, find where `ExtensionInstallModal` is rendered and add `BrxtFileOpenHandler` next to it, passing the same `setView` prop:

```tsx
<BrxtFileOpenHandler setView={setView} />
```

- [ ] **Step 4: Verify TypeScript compiles**

```bash
cd /Users/wgu/Desktop/BioRouter/ui/desktop
npx tsc --noEmit 2>&1 | grep -v "node_modules" | head -20
```

- [ ] **Step 5: Commit**

```bash
cd /Users/wgu/Desktop/BioRouter
git add ui/desktop/src/App.tsx ui/desktop/src/components/BrxtFileOpenHandler.tsx 2>/dev/null; \
git add ui/desktop/src/components/BrxtInstallModal.tsx
git commit -m "feat: add BrxtFileOpenHandler — navigate to Extensions on .brxt double-click"
```

---

### Task 19: File association — macOS (forge.config.ts)

**Files:**
- Modify: `ui/desktop/forge.config.ts`

The forge config already has `extendInfo.CFBundleDocumentTypes` with a folder entry. Add `.brxt` as a recognized document type.

- [ ] **Step 1: Add .brxt to CFBundleDocumentTypes**

In `ui/desktop/forge.config.ts`, find the `CFBundleDocumentTypes` array inside `extendInfo`. Add a new entry:

```javascript
{
  CFBundleTypeName: 'BioRouter Extension Bundle',
  CFBundleTypeExtensions: ['brxt'],
  CFBundleTypeRole: 'Viewer',
  LSHandlerRank: 'Owner',
  CFBundleTypeIconFile: 'AppIcon',
},
```

The full `CFBundleDocumentTypes` array should look like:

```javascript
CFBundleDocumentTypes: [
  {
    CFBundleTypeName: 'Folders',
    CFBundleTypeRole: 'Viewer',
    LSHandlerRank: 'Alternate',
    LSItemContentTypes: ['public.directory', 'public.folder'],
  },
  {
    CFBundleTypeName: 'BioRouter Extension Bundle',
    CFBundleTypeExtensions: ['brxt'],
    CFBundleTypeRole: 'Viewer',
    LSHandlerRank: 'Owner',
    CFBundleTypeIconFile: 'AppIcon',
  },
],
```

- [ ] **Step 2: Commit**

```bash
cd /Users/wgu/Desktop/BioRouter
git add ui/desktop/forge.config.ts
git commit -m "feat: register .brxt file association for macOS"
```

---

### Task 20: File association — Linux deb + rpm (.desktop files + MIME type)

**Files:**
- Modify: `ui/desktop/forge.deb.desktop`
- Modify: `ui/desktop/forge.rpm.desktop`
- Create: `ui/desktop/brxt-mime.xml`

Linux file association requires: (1) a MIME type XML definition, (2) the `.desktop` file advertising the MIME type.

- [ ] **Step 1: Create MIME type XML**

Create `ui/desktop/brxt-mime.xml`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="application/x-brxt">
    <comment>BioRouter Extension Bundle</comment>
    <glob pattern="*.brxt"/>
  </mime-type>
</mime-info>
```

- [ ] **Step 2: Update forge.deb.desktop**

Read `ui/desktop/forge.deb.desktop`. Add `application/x-brxt;` to the `MimeType` line:

```ini
MimeType=x-scheme-handler/biorouter;application/x-brxt;
```

- [ ] **Step 3: Update forge.rpm.desktop**

Read `ui/desktop/forge.rpm.desktop`. Apply the same change:

```ini
MimeType=x-scheme-handler/biorouter;application/x-brxt;
```

- [ ] **Step 4: Add MIME XML to deb maker config in forge.config.ts**

In `forge.config.ts`, find the `@electron-forge/maker-deb` config block and add `mimeType` and the XML file reference. The deb maker supports injecting files via `fpm` arguments:

```javascript
{
  name: '@electron-forge/maker-deb',
  config: {
    name: 'BioRouter',
    bin: 'BioRouter',
    maintainer: 'BaranziniLab',
    homepage: 'https://github.com/BaranziniLab/BioRouter',
    categories: ['Development'],
    desktopTemplate: './forge.deb.desktop',
    options: {
      icon: 'src/images/icon.png',
      prefix: '/opt',
      fpm: [
        '--after-install', 'scripts/post-install.sh',  // only if script exists
      ],
    },
  },
},
```

Actually, for the MIME XML on deb/rpm, the cleanest approach is to create a post-install script that runs `xdg-mime install`:

- [ ] **Step 5: Create post-install script**

Create `ui/desktop/scripts/post-install.sh`:

```bash
#!/bin/bash
# Register .brxt MIME type with the system
if command -v xdg-mime >/dev/null 2>&1; then
  xdg-mime install /opt/BioRouter/resources/brxt-mime.xml --novendor 2>/dev/null || true
  update-mime-database /usr/share/mime 2>/dev/null || true
fi
```

Make it executable:
```bash
chmod +x /Users/wgu/Desktop/BioRouter/ui/desktop/scripts/post-install.sh
```

Add `brxt-mime.xml` to `extraResource` in `forge.config.ts` so it's bundled inside the app:

```javascript
extraResource: ['src/bin', 'src/images', 'brxt-mime.xml'],
```

Update the deb and rpm maker `fpm` arrays to reference the post-install script:

```javascript
// In maker-deb options:
fpm: ['--after-install', './scripts/post-install.sh']

// In maker-rpm options:
fpm: ['--after-install', './scripts/post-install.sh', '--rpm-rpmbuild-define', '_build_id_links none']
```

- [ ] **Step 6: Commit**

```bash
cd /Users/wgu/Desktop/BioRouter
git add ui/desktop/forge.deb.desktop ui/desktop/forge.rpm.desktop \
        ui/desktop/brxt-mime.xml ui/desktop/forge.config.ts \
        ui/desktop/scripts/post-install.sh
git commit -m "feat: register .brxt MIME type for Linux deb and rpm packages"
```

---

### Task 21: File association — Windows

**Files:**
- Modify: `ui/desktop/forge.config.ts`

BioRouter's Windows release is a zip (not an NSIS/WiX installer), so there's no installer-time registry step. The double-click flow works via CLI argument passing (already handled in Task 17, Step 3). For the file icon/association, add to `packagerConfig`:

- [ ] **Step 1: Add Windows file association to forge.config.ts**

In `forge.config.ts`, inside the top-level `cfg` object (the `packagerConfig`), there is already a `win32` key. Add `fileAssociations` to `cfg`:

```javascript
// Add alongside the existing `protocols` and `extendInfo` in cfg:
fileAssociations: [
  {
    ext: 'brxt',
    name: 'BioRouter Extension Bundle',
    description: 'BioRouter Extension Bundle',
    role: 'Viewer',
    icon: 'src/images/icon.ico',
  },
],
```

Note: `fileAssociations` in Electron Forge's packagerConfig maps to `build.fileAssociations` in electron-builder terms and is processed by `@electron/packager`. It writes the registry entries for the packaged app on Windows.

- [ ] **Step 2: Verify forge.config.ts is valid JavaScript**

```bash
cd /Users/wgu/Desktop/BioRouter/ui/desktop
node -e "require('./forge.config.ts')" 2>&1 || node -e "const cfg = require('./forge.config.js')" 2>&1 | head -5
```

If the config is TypeScript and requires compilation, just do a syntax check:
```bash
npx tsc --noEmit 2>&1 | grep "forge.config" | head -5
```

- [ ] **Step 3: Commit**

```bash
cd /Users/wgu/Desktop/BioRouter
git add ui/desktop/forge.config.ts
git commit -m "feat: register .brxt file association for Windows"
```

---

### Task 22: Playwright test — double-click file open flow

> Tests the end-to-end file association flow by simulating the `open-brxt-file` IPC event (since we can't actually double-click a file in the OS during automated tests).

- [ ] **Step 1: Start BioRouter in debug mode**

```bash
cd /Users/wgu/Desktop/BioRouter
just run-dev
```

- [ ] **Step 2: Simulate open-brxt-file event via browser console**

Use `mcp__playwright-electron__browser_evaluate` to simulate the IPC event that double-clicking would produce:

```javascript
// In browser console:
window.electron.emit('open-brxt-file', '/tmp/bundle-work/cdwagent.brxt')
```

- [ ] **Step 3: Verify navigation and modal**

After emitting the event, use `mcp__playwright-electron__browser_snapshot` to verify:
- The Extensions view is now active
- `BrxtInstallModal` is open
- The manifest preview card shows "CDWAgent" (auto-validated from preloaded path)

- [ ] **Step 4: Test no-env-var skip**

Create a minimal test bundle with no env vars:

```bash
mkdir -p /tmp/noenv-test/src/noenvpkg
cat > /tmp/noenv-test/manifest.json << 'EOF'
{
  "name": "noenvtest",
  "display_name": "No Env Test",
  "description": "Test extension with no env vars",
  "version": "0.1.0",
  "entry_point": "noenvtest",
  "repository": "https://github.com/example/noenvtest",
  "env_vars": []
}
EOF
echo "# NoEnvTest" > /tmp/noenv-test/README.md
echo '[project]\nname = "noenvtest"\nversion = "0.1.0"' > /tmp/noenv-test/pyproject.toml
echo "def main(): pass" > /tmp/noenv-test/src/noenvpkg/__init__.py
cd /tmp/noenv-test
zip -r /tmp/noenv.brxt manifest.json README.md pyproject.toml src/
```

Upload `/tmp/noenv.brxt` via the file picker in `BrxtInstallModal` (click "Add extension" → file picker → select `/tmp/noenv.brxt`).

Verify:
- Manifest preview shows "No Env Test"
- The button reads **"Install Extension"** (not "Next: Configure →")
- No Step 2 configure dialog appears when clicking it

- [ ] **Step 5: Commit any test artifacts**

```bash
cd /Users/wgu/Desktop/BioRouter
# No test files committed — all test bundles are in /tmp
git status
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
- [x] Skip configure step when no env vars → Task 16
- [x] preloadedFilePath prop for double-click flow → Tasks 16 + 18
- [x] open-file macOS handler + CLI args Windows/Linux → Task 17
- [x] BrxtFileOpenHandler navigates to Extensions view → Task 18
- [x] macOS CFBundleDocumentTypes for .brxt → Task 19
- [x] Linux deb + rpm MIME type + post-install → Task 20
- [x] Windows fileAssociations in packagerConfig → Task 21
- [x] Playwright test for double-click simulation + no-env-var skip → Task 22

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
