# Skill bundles implementation plan

> **What this is.** The task-by-task implementation plan that executed the [skill bundles design](skill-bundles-design.md): treating a parent folder of sub-skill directories as one installable bundle with a single toggle, across the React UI, Electron IPC, and Rust skill discovery.
> **Status:** Historical record — written 2026-05-07 and completed. The work shipped: `SkillBundle` is defined in `ui/desktop/src/components/skills/skillUtils.ts` and `bundle_name` filtering is in `crates/biorouter/src/agents/skills_extension.rs`. The `- [ ]` checkboxes below were never ticked off in the file — read them as the original task list, not as outstanding work.
> **Audience:** agents and developers tracing how skill bundle support was built.

This plan was written to be executed by an agent, one `## Task N` at a time, in test-driven order: write a failing test, run it, implement, re-run, commit. It is unusually code-heavy — several steps say "replace the entire file with" and then inline the whole proposed source. Those blocks are a snapshot of the intended code at authoring time, not a mirror of the current tree; **read the repository, not this document, for what the code does today.** The value that survives is the ordering, the reasoning, and the test cases.

> **Note.** The original file opened with a machine-directed banner naming the `superpowers:subagent-driven-development` and `superpowers:executing-plans` skills as the intended execution harnesses. That instruction is recorded here for provenance and is no longer actionable — the plan is complete.

**Goal:** Add support for skill bundles — a parent folder containing multiple sub-skill directories each with their own `SKILL.md` — treated as a single installable unit with a single toggle.

**Architecture:** Bundle detection runs at discovery time: if a directory entry has no `SKILL.md` at its root but its sub-directories do, it's classified as a `SkillBundle`. Both TypeScript (frontend, via IPC) and Rust (agent runtime) perform two-level discovery. The `skills-config.json` disabled list is unchanged — bundle names coexist with single-skill names as flat strings. The UI renders bundle rows with a single `Switch` and a read-only sub-skill name list.

**Tech Stack:** TypeScript/React 19, Electron IPC, Rust (tokio async), Vitest (unit tests), existing `Switch`/`SkillItem` components, `skillOverrides` store.

---

## File structure

**Modified:**
- `ui/desktop/src/components/skills/skillUtils.ts` — add `SkillBundle` interface, add `bundleName?` to `Skill`, update `loadSkillsFromDirs` return type + discovery
- `ui/desktop/src/components/skills/SkillsView.tsx` — consume `{ singles, bundles }`, render bundle rows
- `ui/desktop/src/components/bottom_menu/BottomMenuSkillSelection.tsx` — consume `{ singles, bundles }`, render bundles as single entries
- `ui/desktop/src/components/skills/AddSkillModal.tsx` — bundle detection in folder and ZIP paths, bundle install, bundle preview UI
- `ui/desktop/src/main.ts` — `write-file` handler creates parent dirs; `skills:extract-zip` adds bundle detection
- `crates/biorouter/src/agents/skills_extension.rs` — `bundle_name: Option<String>` on `Skill`, two-level discovery, disabled check

**Created:**
- `ui/desktop/src/components/skills/skillUtils.test.ts` — unit tests for `parseSkillFrontmatter` and the new bundle detection helpers

---

## Task 1: TypeScript — `SkillBundle` interface + updated `loadSkillsFromDirs`

Teaches the frontend scanner two-level discovery: `loadSkillsFromDirs` stops returning a flat `Skill[]` and returns `{ singles, bundles }` instead, which forces every caller to be updated in Tasks 2 and 3.

**Files:**
- Modify: `ui/desktop/src/components/skills/skillUtils.ts`
- Create: `ui/desktop/src/components/skills/skillUtils.test.ts`

- [ ] **Step 1: Write failing tests for bundle detection helpers**

Create `ui/desktop/src/components/skills/skillUtils.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { parseSkillFrontmatter, toSlug } from './skillUtils';

describe('parseSkillFrontmatter', () => {
  it('returns name and description from valid frontmatter', () => {
    const content = `---\nname: my-skill\ndescription: A test skill\n---\nBody here`;
    expect(parseSkillFrontmatter(content)).toEqual({
      name: 'my-skill',
      description: 'A test skill',
    });
  });

  it('returns null when frontmatter is missing', () => {
    expect(parseSkillFrontmatter('# No frontmatter')).toBeNull();
  });

  it('returns null when name is missing', () => {
    const content = `---\ndescription: only desc\n---\nBody`;
    expect(parseSkillFrontmatter(content)).toBeNull();
  });

  it('ignores extra frontmatter fields (user-invocable, hooks)', () => {
    const content = `---\nname: ralph\ndescription: Test\nuser-invocable: true\n---\nBody`;
    expect(parseSkillFrontmatter(content)).toEqual({ name: 'ralph', description: 'Test' });
  });
});

describe('toSlug', () => {
  it('lowercases and replaces special chars with hyphens', () => {
    expect(toSlug('My Skill!')).toBe('my-skill');
  });

  it('strips .md extension', () => {
    expect(toSlug('my-skill.md')).toBe('my-skill');
  });

  it('collapses multiple hyphens', () => {
    expect(toSlug('a  b')).toBe('a-b');
  });
});
```

- [ ] **Step 2: Run the tests to verify they pass (they test existing code)**

```bash
cd ui/desktop && npm run test:run -- skillUtils
```

Expected: all tests pass (these are pure functions already implemented).

> **Note.** These tests are characterization tests, not red-phase tests — `parseSkillFrontmatter` and `toSlug` already exist and already behave this way, so nothing here can fail. Their job is to pin the existing behaviour before Step 3 rewrites the whole file around them. The genuine test-driven cycle in this plan starts at Task 5, where the Rust tests fail to compile until `bundle_name` exists.

- [ ] **Step 3: Update `skillUtils.ts` — add `bundleName` to `Skill`, add `SkillBundle`, update `loadSkillsFromDirs`**

Replace the entire `skillUtils.ts` with:

```typescript
export interface Skill {
  folderPath: string;
  sourceDir: string;
  name: string;
  description: string;
  content: string;
  bundleName?: string;
}

export interface SkillBundle {
  bundleName: string;
  folderPath: string;
  sourceDir: string;
  skills: Skill[];
}

export const BIOROUTER_SKILLS_DIR = '~/.config/biorouter/skills';
export const OTHER_SKILL_DIRS = [
  '~/.claude/skills',
  '~/.config/agents/skills',
];
export const ALL_SKILL_DIRS = [BIOROUTER_SKILLS_DIR, ...OTHER_SKILL_DIRS];

/**
 * Parse YAML frontmatter from a SKILL.md file.
 * Returns { name, description } if valid, null if missing or malformed.
 */
export function parseSkillFrontmatter(
  content: string
): { name: string; description: string } | null {
  const match = content.match(/^---\r?\n([\s\S]*?)\r?\n---/);
  if (!match) return null;
  const fm = match[1];
  const nameMatch = fm.match(/^name:\s*([^\n]+)$/m);
  const descMatch = fm.match(/^description:\s*([^\n]+)$/m);
  if (!nameMatch?.[1]?.trim() || !descMatch?.[1]?.trim()) return null;
  return { name: nameMatch[1].trim(), description: descMatch[1].trim() };
}

/**
 * Load all skills from a list of directories using Electron IPC.
 *
 * Detection rule per directory entry `<slug>`:
 *   - `<dir>/<slug>/SKILL.md` exists → single skill
 *   - No root SKILL.md, but sub-dirs of `<dir>/<slug>/` contain SKILL.md → bundle
 *   - Otherwise → ignored
 */
export async function loadSkillsFromDirs(
  dirs: string[]
): Promise<{ singles: Skill[]; bundles: SkillBundle[] }> {
  const singles: Skill[] = [];
  const bundles: SkillBundle[] = [];

  for (const dir of dirs) {
    const folders: string[] = await window.electron.listSkillDirs(dir);

    for (const folder of folders) {
      const skillMdPath = `${dir}/${folder}/SKILL.md`;
      const result = await window.electron.readFile(skillMdPath);

      if (result.found && result.file) {
        const parsed = parseSkillFrontmatter(result.file);
        if (!parsed) continue;
        singles.push({
          folderPath: `${dir}/${folder}`,
          sourceDir: dir,
          name: parsed.name,
          description: parsed.description,
          content: result.file,
        });
      } else {
        // No SKILL.md at root — check if sub-dirs have SKILL.md (bundle)
        const subFolders: string[] = await window.electron.listSkillDirs(`${dir}/${folder}`);
        const bundleSkills: Skill[] = [];

        for (const sub of subFolders) {
          const subPath = `${dir}/${folder}/${sub}/SKILL.md`;
          const subResult = await window.electron.readFile(subPath);
          if (!subResult.found || !subResult.file) continue;
          const parsed = parseSkillFrontmatter(subResult.file);
          if (!parsed) continue;
          bundleSkills.push({
            folderPath: `${dir}/${folder}/${sub}`,
            sourceDir: dir,
            name: parsed.name,
            description: parsed.description,
            content: subResult.file,
            bundleName: folder,
          });
        }

        if (bundleSkills.length > 0) {
          bundles.push({
            bundleName: folder,
            folderPath: `${dir}/${folder}`,
            sourceDir: dir,
            skills: bundleSkills,
          });
        }
      }
    }
  }

  return { singles, bundles };
}

/**
 * Derive a safe folder/file slug from a skill name or filename.
 * e.g. "My Skill!" → "my-skill"
 */
export function toSlug(input: string): string {
  return input
    .replace(/\.md$/i, '')
    .replace(/[^a-z0-9-_]/gi, '-')
    .replace(/-{2,}/g, '-')
    .replace(/^-|-$/g, '')
    .toLowerCase();
}
```

- [ ] **Step 4: Run the tests again**

```bash
cd ui/desktop && npm run test:run -- skillUtils
```

Expected: all tests still pass.

- [ ] **Step 5: Run TypeScript type-check**

```bash
cd ui/desktop && npx tsc --noEmit 2>&1 | head -40
```

Expected: errors only about callers of `loadSkillsFromDirs` (return type changed from `Skill[]` to `{ singles, bundles }`) — those get fixed in Tasks 2 and 3.

- [ ] **Step 6: Commit**

```bash
git add ui/desktop/src/components/skills/skillUtils.ts \
        ui/desktop/src/components/skills/skillUtils.test.ts
git commit -m "feat(skills): add SkillBundle interface and two-level discovery in loadSkillsFromDirs"
```

---

## Task 2: Update `SkillsView.tsx` — render bundle rows

Adapts the settings page to the new return shape and adds an inline `BundleItem` row: bundle name, sub-skill count, a read-only list of sub-skill names, one `Switch`, and a delete confirmation that names how many skills will be removed.

**Files:**
- Modify: `ui/desktop/src/components/skills/SkillsView.tsx`

- [ ] **Step 1: Replace the full content of `SkillsView.tsx`**

```tsx
import { useState, useEffect, useCallback } from 'react';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { Button } from '../ui/button';
import { Switch } from '../ui/switch';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { Plus, Upload, Globe, Package } from '../icons/app-icons';
import {
  Skill,
  SkillBundle,
  BIOROUTER_SKILLS_DIR,
  OTHER_SKILL_DIRS,
  loadSkillsFromDirs,
} from './skillUtils';
import SkillItem from './SkillItem';
import AddSkillModal from './AddSkillModal';
import CustomSkillModal from './CustomSkillModal';
import { toastSuccess, toastError } from '../../toasts';
import { SearchView } from '../conversation/SearchView';
import { getSearchShortcutText } from '../../utils/keyboardShortcuts';
import {
  loadSkillOverrides,
  saveSkillOverrides,
  setSkillOverride,
  isSkillEnabled,
} from '../../store/skillOverrides';

export default function SkillsView() {
  const [bioRouterSkills, setBioRouterSkills] = useState<Skill[]>([]);
  const [otherSkills, setOtherSkills] = useState<Skill[]>([]);
  const [bioBundles, setBioBundles] = useState<SkillBundle[]>([]);
  const [otherBundles, setOtherBundles] = useState<SkillBundle[]>([]);
  const [isAddModalOpen, setIsAddModalOpen] = useState(false);
  const [isCustomModalOpen, setIsCustomModalOpen] = useState(false);
  const [skillToDelete, setSkillToDelete] = useState<Skill | null>(null);
  const [bundleToDelete, setBundleToDelete] = useState<SkillBundle | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);
  const [searchTerm, setSearchTerm] = useState('');
  const [overrideTrigger, setOverrideTrigger] = useState(0);

  const loadSkills = useCallback(async () => {
    try {
      const [brResult, otherResult] = await Promise.all([
        loadSkillsFromDirs([BIOROUTER_SKILLS_DIR]),
        loadSkillsFromDirs(OTHER_SKILL_DIRS),
      ]);
      setBioRouterSkills(brResult.singles);
      setBioBundles(brResult.bundles);
      setOtherSkills(otherResult.singles);
      setOtherBundles(otherResult.bundles);
    } catch {
      setBioRouterSkills([]);
      setBioBundles([]);
      setOtherSkills([]);
      setOtherBundles([]);
    }
  }, []);

  useEffect(() => {
    loadSkills();
    loadSkillOverrides();
  }, [loadSkills]);

  const handleToggle = async (skill: Skill, enabled: boolean) => {
    setSkillOverride(skill.name, enabled);
    await saveSkillOverrides();
    setOverrideTrigger((prev) => prev + 1);
  };

  const handleBundleToggle = async (bundle: SkillBundle, enabled: boolean) => {
    setSkillOverride(bundle.bundleName, enabled);
    await saveSkillOverrides();
    setOverrideTrigger((prev) => prev + 1);
  };

  const filterSkill = (skill: Skill) => {
    if (!searchTerm) return true;
    const q = searchTerm.toLowerCase();
    return skill.name.toLowerCase().includes(q) || skill.description.toLowerCase().includes(q);
  };

  const filterBundle = (bundle: SkillBundle) => {
    if (!searchTerm) return true;
    const q = searchTerm.toLowerCase();
    return (
      bundle.bundleName.toLowerCase().includes(q) ||
      bundle.skills.some(
        (s) => s.name.toLowerCase().includes(q) || s.description.toLowerCase().includes(q)
      )
    );
  };

  const handleOpen = async (skill: Skill) => {
    await window.electron.openDirectoryInExplorer(skill.folderPath);
  };

  const handleOpenBundle = async (bundle: SkillBundle) => {
    await window.electron.openDirectoryInExplorer(bundle.folderPath);
  };

  const confirmDeleteSkill = async () => {
    if (!skillToDelete) return;
    setIsDeleting(true);
    const skill = skillToDelete;
    const ok = await window.electron.deleteDirectory(skill.folderPath);
    setIsDeleting(false);
    setSkillToDelete(null);
    if (ok) {
      setBioRouterSkills((prev) => prev.filter((s) => s.folderPath !== skill.folderPath));
      setOtherSkills((prev) => prev.filter((s) => s.folderPath !== skill.folderPath));
      toastSuccess({ title: skill.name, msg: 'Skill deleted' });
    } else {
      toastError({ title: 'Delete failed', msg: `Could not delete ${skill.folderPath}` });
    }
  };

  const confirmDeleteBundle = async () => {
    if (!bundleToDelete) return;
    setIsDeleting(true);
    const bundle = bundleToDelete;
    const ok = await window.electron.deleteDirectory(bundle.folderPath);
    setIsDeleting(false);
    setBundleToDelete(null);
    if (ok) {
      setBioBundles((prev) => prev.filter((b) => b.folderPath !== bundle.folderPath));
      setOtherBundles((prev) => prev.filter((b) => b.folderPath !== bundle.folderPath));
      toastSuccess({ title: bundle.bundleName, msg: 'Bundle deleted' });
    } else {
      toastError({ title: 'Delete failed', msg: `Could not delete ${bundle.folderPath}` });
    }
  };

  const handleShare = async (skill: Skill) => {
    try {
      await navigator.clipboard.writeText(skill.content);
      toastSuccess({ title: skill.name, msg: 'SKILL.md copied to clipboard' });
    } catch {
      toastError({ title: 'Copy failed', msg: 'Could not copy to clipboard' });
    }
  };

  const filteredBR = bioRouterSkills.filter(filterSkill);
  const filteredOther = otherSkills.filter(filterSkill);
  const filteredBRBundles = bioBundles.filter(filterBundle);
  const filteredOtherBundles = otherBundles.filter(filterBundle);

  const totalBR = filteredBR.length + filteredBRBundles.length;
  const totalOther = filteredOther.length + filteredOtherBundles.length;

  return (
    <MainPanelLayout>
      <div className="flex flex-col min-w-0 flex-1 overflow-y-auto relative" data-search-scroll-area>
        {/* Header */}
        <div className="px-8 pt-12 pb-6 flex-shrink-0 border-b border-border-subtle">
          <div className="flex flex-col page-transition">
            <h1 className="text-2xl font-semibold tracking-tight mb-1">Skills</h1>
            <p className="text-sm text-text-muted mb-0">
              Reusable instruction sets that guide BioRouter's behavior.{' '}
              {getSearchShortcutText()} to search.
            </p>
          </div>
          <div className="flex gap-3 mt-5">
            <Button
              className="flex items-center gap-2"
              variant="default"
              onClick={() => setIsAddModalOpen(true)}
            >
              <Upload className="h-4 w-4" />
              Add Skill
            </Button>
            <Button
              className="flex items-center gap-2"
              variant="outline"
              onClick={() =>
                window.open(
                  'http://biorouter.ucsf.edu/baam',
                  '_blank'
                )
              }
            >
              <Globe className="h-4 w-4" />
              Browse Skills
            </Button>
            <Button
              className="flex items-center gap-2"
              variant="outline"
              onClick={() => setIsCustomModalOpen(true)}
            >
              <Plus className="h-4 w-4" />
              Add Custom Skill
            </Button>
          </div>
        </div>

        {/* List */}
        <SearchView onSearch={(term, _caseSensitive) => setSearchTerm(term)} placeholder="Search skills...">
          <div key={overrideTrigger} className="px-6 py-4">
            {totalBR > 0 && (
              <>
                <p className="text-[11px] font-medium text-text-subtle uppercase tracking-widest mb-2 px-2 flex items-center gap-1.5">
                  <span className="inline-block w-1.5 h-1.5 rounded-full bg-blue-500" />
                  BioRouter Skills ({totalBR})
                </p>
                {filteredBRBundles.map((bundle) => (
                  <BundleItem
                    key={bundle.folderPath}
                    bundle={bundle}
                    enabled={isSkillEnabled(bundle.bundleName)}
                    onClick={() => handleOpenBundle(bundle)}
                    onDelete={() => setBundleToDelete(bundle)}
                    onToggle={(e) => handleBundleToggle(bundle, e)}
                  />
                ))}
                {filteredBR.map((skill) => (
                  <SkillItem
                    key={skill.folderPath}
                    skill={skill}
                    enabled={isSkillEnabled(skill.name)}
                    onClick={() => handleOpen(skill)}
                    onDelete={() => setSkillToDelete(skill)}
                    onShare={() => handleShare(skill)}
                    onToggle={(e) => handleToggle(skill, e)}
                  />
                ))}
              </>
            )}

            {totalOther > 0 && (
              <>
                <p className="text-[11px] font-medium text-text-subtle uppercase tracking-widest mt-6 mb-2 px-2 flex items-center gap-1.5">
                  <span className="inline-block w-1.5 h-1.5 rounded-full bg-neutral-400" />
                  Skills From Other Agents ({totalOther})
                </p>
                {filteredOtherBundles.map((bundle) => (
                  <BundleItem
                    key={bundle.folderPath}
                    bundle={bundle}
                    enabled={isSkillEnabled(bundle.bundleName)}
                    onClick={() => handleOpenBundle(bundle)}
                    onDelete={() => setBundleToDelete(bundle)}
                    onToggle={(e) => handleBundleToggle(bundle, e)}
                  />
                ))}
                {filteredOther.map((skill) => (
                  <SkillItem
                    key={skill.folderPath}
                    skill={skill}
                    enabled={isSkillEnabled(skill.name)}
                    onClick={() => handleOpen(skill)}
                    onDelete={() => setSkillToDelete(skill)}
                    onShare={() => handleShare(skill)}
                    onToggle={(e) => handleToggle(skill, e)}
                  />
                ))}
              </>
            )}

            {totalBR === 0 && totalOther === 0 && (
              <p className="text-sm text-text-muted mt-10 text-center">
                {searchTerm
                  ? 'No skills match your search.'
                  : 'No skills found. Add one to get started.'}
              </p>
            )}
          </div>
        </SearchView>
      </div>

      {isAddModalOpen && (
        <AddSkillModal onClose={() => setIsAddModalOpen(false)} onSaved={loadSkills} />
      )}
      {isCustomModalOpen && (
        <CustomSkillModal onClose={() => setIsCustomModalOpen(false)} onSaved={loadSkills} />
      )}

      <ConfirmationModal
        isOpen={skillToDelete !== null}
        title={`Delete "${skillToDelete?.name}"?`}
        message="This will permanently remove the skill folder from disk. This action cannot be undone."
        confirmLabel="Delete"
        cancelLabel="Cancel"
        confirmVariant="destructive"
        isSubmitting={isDeleting}
        onConfirm={confirmDeleteSkill}
        onCancel={() => setSkillToDelete(null)}
      />

      <ConfirmationModal
        isOpen={bundleToDelete !== null}
        title={`Delete bundle "${bundleToDelete?.bundleName}"?`}
        message={`This will permanently remove all ${bundleToDelete?.skills.length ?? 0} skills in this bundle. This action cannot be undone.`}
        confirmLabel="Delete Bundle"
        cancelLabel="Cancel"
        confirmVariant="destructive"
        isSubmitting={isDeleting}
        onConfirm={confirmDeleteBundle}
        onCancel={() => setBundleToDelete(null)}
      />
    </MainPanelLayout>
  );
}

// ---------------------------------------------------------------------------
// Inline bundle row component
// ---------------------------------------------------------------------------
interface BundleItemProps {
  bundle: SkillBundle;
  enabled: boolean;
  onClick: () => void;
  onDelete: () => void;
  onToggle: (enabled: boolean) => void;
}

function BundleItem({ bundle, enabled, onClick, onDelete, onToggle }: BundleItemProps) {
  return (
    <div
      className="flex items-start py-3 border-b border-border-subtle last:border-b-0 hover:bg-background-medium/30 transition-colors group cursor-pointer gap-3 px-2"
      onClick={onClick}
    >
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5">
          <Package className="h-3.5 w-3.5 text-text-subtle flex-shrink-0" />
          <p className="text-sm font-semibold text-text-default">{bundle.bundleName}</p>
          <span className="text-[11px] text-text-subtle">· {bundle.skills.length} skills</span>
        </div>
        <p className="text-xs text-text-subtle mt-1 font-mono leading-relaxed">
          {bundle.skills.map((s) => s.name).join(' · ')}
        </p>
      </div>
      <div className="flex items-center gap-2 flex-shrink-0 mt-0.5">
        <div
          className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity"
          onClick={(e) => e.stopPropagation()}
        >
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0 text-destructive hover:text-destructive hover:bg-destructive/10"
            onClick={() => onDelete()}
            title="Delete bundle"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
        <div onClick={(e) => e.stopPropagation()}>
          <Switch checked={enabled} onCheckedChange={onToggle} variant="mono" />
        </div>
      </div>
    </div>
  );
}
```

> **Note.** `Package` and `Trash2` must be imported. Check `ui/desktop/src/components/icons/app-icons.tsx` for available icons. If `Package` is not exported, use `Layers` or `FolderDot` instead. Import `Trash2` from app-icons (it is already used in `SkillItem.tsx`). Import `Button` from `../ui/button`.
>
> Add these imports at the top of `SkillsView.tsx`:
> ```tsx
> import { Button } from '../ui/button';
> import { Switch } from '../ui/switch';
> import { Plus, Upload, Globe, Package, Trash2 } from '../icons/app-icons';
> ```
> If `Package` is not in `app-icons`, substitute `FolderDot` (already exported).

- [ ] **Step 2: Check which icons are available**

```bash
grep -n "^export" ui/desktop/src/components/icons/app-icons.tsx | grep -i "package\|trash\|folder"
```

If `Package` is not exported, update the import in SkillsView to use `FolderDot` in its place.

- [ ] **Step 3: Run type-check**

```bash
cd ui/desktop && npx tsc --noEmit 2>&1 | head -40
```

Expected: errors only in `BottomMenuSkillSelection.tsx` (fixed in Task 3).

- [ ] **Step 4: Commit**

```bash
git add ui/desktop/src/components/skills/SkillsView.tsx
git commit -m "feat(skills): render bundle rows in SkillsView with single toggle and sub-skill list"
```

---

## Task 3: Update `BottomMenuSkillSelection.tsx` — bundle entries in dropdown

Introduces a `SkillEntry` union (`single` | `bundle`) so the chat-bar dropdown can list, search, sort, and toggle bundles alongside single skills, with each bundle counting as one toward the active-skill badge.

**Files:**
- Modify: `ui/desktop/src/components/bottom_menu/BottomMenuSkillSelection.tsx`

- [ ] **Step 1: Replace the full content of `BottomMenuSkillSelection.tsx`**

```tsx
import { useCallback, useEffect, useMemo, useState, useRef } from 'react';
import { Layers } from '../icons/app-icons';
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from '../ui/dropdown-menu';
import { Input } from '../ui/input';
import { Switch } from '../ui/switch';
import {
  loadSkillOverrides,
  saveSkillOverrides,
  setSkillOverride,
  isSkillEnabled,
  getSkillOverrides,
} from '../../store/skillOverrides';
import { Skill, SkillBundle, ALL_SKILL_DIRS, loadSkillsFromDirs } from '../skills/skillUtils';
import { toastService } from '../../toasts';

interface BottomMenuSkillSelectionProps {
  sessionId: string | null;
}

type SkillEntry =
  | { kind: 'single'; skill: Skill; enabled: boolean }
  | { kind: 'bundle'; bundle: SkillBundle; enabled: boolean };

export const BottomMenuSkillSelection = ({ sessionId }: BottomMenuSkillSelectionProps) => {
  const [searchQuery, setSearchQuery] = useState('');
  const [isOpen, setIsOpen] = useState(false);
  const [allSkills, setAllSkills] = useState<Skill[]>([]);
  const [allBundles, setAllBundles] = useState<SkillBundle[]>([]);
  const [hubUpdateTrigger, setHubUpdateTrigger] = useState(0);
  const [isTransitioning, setIsTransitioning] = useState(false);
  const [pendingSort, setPendingSort] = useState(false);
  const [togglingKey, setTogglingKey] = useState<string | null>(null);
  const [sessionOverrides, setSessionOverrides] = useState<Map<string, boolean>>(new Map());
  const sortTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isHubView = !sessionId;

  const loadAll = useCallback(() => {
    return loadSkillOverrides().then(() => {
      loadSkillsFromDirs(ALL_SKILL_DIRS).then(({ singles, bundles }) => {
        setAllSkills(singles);
        setAllBundles(bundles);
      });
    });
  }, []);

  useEffect(() => {
    loadAll();
  }, [loadAll]);

  useEffect(() => {
    if (isOpen) {
      loadAll().then(() => setHubUpdateTrigger((prev) => prev + 1));
    }
  }, [isOpen, loadAll]);

  useEffect(() => {
    return () => {
      if (sortTimeoutRef.current) clearTimeout(sortTimeoutRef.current);
    };
  }, []);

  const handleToggle = useCallback(
    async (key: string, displayName: string) => {
      if (togglingKey === key) return;

      setIsTransitioning(true);
      setTogglingKey(key);

      const scheduleSort = () => {
        setPendingSort(true);
        if (sortTimeoutRef.current) clearTimeout(sortTimeoutRef.current);
        sortTimeoutRef.current = setTimeout(() => {
          setHubUpdateTrigger((prev) => prev + 1);
          setPendingSort(false);
          setIsTransitioning(false);
          setTogglingKey(null);
        }, 800);
      };

      if (isHubView) {
        const currentEnabled = isSkillEnabled(key);
        setSkillOverride(key, !currentEnabled);
        await saveSkillOverrides();
        scheduleSort();
        toastService.success({
          title: 'Skill Updated',
          msg: `${displayName} will be ${!currentEnabled ? 'enabled' : 'disabled'} in new chats`,
        });
        return;
      }

      // Session view: local state only
      const currentEnabled = sessionOverrides.has(key)
        ? sessionOverrides.get(key)!
        : isSkillEnabled(key);
      const newEnabled = !currentEnabled;
      setSessionOverrides((prev) => {
        const next = new Map(prev);
        next.set(key, newEnabled);
        return next;
      });
      scheduleSort();
      toastService.success({
        title: 'Skill Updated',
        msg: `${displayName} ${newEnabled ? 'enabled' : 'disabled'} for this session`,
      });
    },
    [isHubView, togglingKey, sessionOverrides]
  );

  const entries = useMemo((): SkillEntry[] => {
    const hubOverrides = getSkillOverrides();
    const resolveEnabled = (key: string): boolean => {
      if (!isHubView && sessionOverrides.has(key)) return sessionOverrides.get(key)!;
      if (hubOverrides.has(key)) return hubOverrides.get(key)!;
      return true;
    };

    const singles: SkillEntry[] = allSkills.map((skill) => ({
      kind: 'single',
      skill,
      enabled: resolveEnabled(skill.name),
    }));

    const bundles: SkillEntry[] = allBundles.map((bundle) => ({
      kind: 'bundle',
      bundle,
      enabled: resolveEnabled(bundle.bundleName),
    }));

    return [...bundles, ...singles];
    // hubUpdateTrigger triggers re-evaluation when hub overrides change
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [allSkills, allBundles, isHubView, sessionOverrides, hubUpdateTrigger]);

  const filteredEntries = useMemo(() => {
    const q = searchQuery.toLowerCase();
    if (!q) return entries;
    return entries.filter((e) => {
      if (e.kind === 'single') {
        return (
          e.skill.name.toLowerCase().includes(q) ||
          e.skill.description.toLowerCase().includes(q)
        );
      }
      return (
        e.bundle.bundleName.toLowerCase().includes(q) ||
        e.bundle.skills.some(
          (s) => s.name.toLowerCase().includes(q) || s.description.toLowerCase().includes(q)
        )
      );
    });
  }, [entries, searchQuery]);

  const sortedEntries = useMemo(() => {
    return [...filteredEntries].sort((a, b) => {
      if (a.enabled !== b.enabled) return a.enabled ? -1 : 1;
      const nameA = a.kind === 'single' ? a.skill.name : a.bundle.bundleName;
      const nameB = b.kind === 'single' ? b.skill.name : b.bundle.bundleName;
      return nameA.localeCompare(nameB);
    });
  }, [filteredEntries]);

  const activeCount = useMemo(
    () => entries.filter((e) => e.enabled).length,
    [entries]
  );

  return (
    <DropdownMenu
      open={isOpen}
      onOpenChange={(open) => {
        setIsOpen(open);
        if (!open) {
          setSearchQuery('');
          if (sortTimeoutRef.current) clearTimeout(sortTimeoutRef.current);
          setIsTransitioning(false);
          setPendingSort(false);
          setTogglingKey(null);
        }
      }}
    >
      <DropdownMenuTrigger asChild>
        <button
          className="flex items-center cursor-pointer [&_svg]:size-4 text-text-default/70 hover:text-text-default hover:scale-100 hover:bg-transparent text-xs"
          title="manage skills"
        >
          <Layers className="mr-1 h-4 w-4" />
          <span>{activeCount}</span>
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        side="top"
        align="center"
        className="w-64"
        onCloseAutoFocus={(e) => e.preventDefault()}
      >
        <div className="p-2">
          <Input
            type="text"
            placeholder="search skills..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="h-8 text-sm"
            autoFocus
          />
          <p className="text-xs text-text-default/60 mt-1.5">
            {isHubView ? 'Skills for new chats' : 'Skills for this chat session'}
          </p>
        </div>
        <div
          className={`max-h-[400px] overflow-y-auto transition-opacity duration-300 ${
            isTransitioning && pendingSort ? 'opacity-50' : 'opacity-100'
          }`}
        >
          {sortedEntries.length === 0 ? (
            <div className="px-2 py-4 text-center text-sm text-text-default/70">
              {searchQuery ? 'no skills found' : 'no skills available'}
            </div>
          ) : (
            sortedEntries.map((entry) => {
              if (entry.kind === 'single') {
                const { skill, enabled } = entry;
                const isToggling = togglingKey === skill.name;
                return (
                  <div
                    key={skill.folderPath}
                    className={`flex items-center justify-between px-2 py-2 hover:bg-background-medium transition-all duration-300 ${
                      isToggling ? 'cursor-wait opacity-70' : 'cursor-pointer'
                    }`}
                    onClick={() => !isToggling && handleToggle(skill.name, skill.name)}
                    title={skill.description || skill.name}
                  >
                    <div className="text-sm font-medium text-text-default">{skill.name}</div>
                    <div onClick={(e) => e.stopPropagation()}>
                      <Switch
                        checked={enabled}
                        onCheckedChange={() => handleToggle(skill.name, skill.name)}
                        variant="mono"
                        disabled={isToggling}
                      />
                    </div>
                  </div>
                );
              }

              // Bundle entry
              const { bundle, enabled } = entry;
              const isToggling = togglingKey === bundle.bundleName;
              const subNames = bundle.skills.map((s) => s.name).join(', ');
              return (
                <div
                  key={bundle.folderPath}
                  className={`flex items-start justify-between px-2 py-2 hover:bg-background-medium transition-all duration-300 ${
                    isToggling ? 'cursor-wait opacity-70' : 'cursor-pointer'
                  }`}
                  onClick={() =>
                    !isToggling && handleToggle(bundle.bundleName, bundle.bundleName)
                  }
                  title={`Bundle: ${subNames}`}
                >
                  <div className="flex-1 min-w-0 pr-2">
                    <div className="text-sm font-medium text-text-default">
                      {bundle.bundleName}
                      <span className="ml-1 text-[10px] text-text-subtle font-normal">
                        bundle
                      </span>
                    </div>
                    <div className="text-[10px] text-text-subtle truncate">{subNames}</div>
                  </div>
                  <div onClick={(e) => e.stopPropagation()} className="flex-shrink-0 mt-0.5">
                    <Switch
                      checked={enabled}
                      onCheckedChange={() =>
                        handleToggle(bundle.bundleName, bundle.bundleName)
                      }
                      variant="mono"
                      disabled={isToggling}
                    />
                  </div>
                </div>
              );
            })
          )}
        </div>
      </DropdownMenuContent>
    </DropdownMenu>
  );
};
```

- [ ] **Step 2: Run type-check**

```bash
cd ui/desktop && npx tsc --noEmit 2>&1 | head -40
```

Expected: no errors in `BottomMenuSkillSelection.tsx`. Remaining errors may be in `AddSkillModal.tsx` (fixed in Task 4).

- [ ] **Step 3: Run unit tests**

```bash
cd ui/desktop && npm run test:run
```

Expected: all existing tests pass (no tests exist for BottomMenuSkillSelection).

- [ ] **Step 4: Commit**

```bash
git add ui/desktop/src/components/bottom_menu/BottomMenuSkillSelection.tsx
git commit -m "feat(skills): render bundle entries in BottomMenuSkillSelection dropdown"
```

---

## Task 4: Bundle install — `main.ts` + `AddSkillModal.tsx`

Makes bundles installable from both entry points. Sub-task 4a teaches `write-file` to create parent directories (needed for nested bundle paths) and extends `skills:extract-zip` to recognise a three-level `<bundle>/<sub>/SKILL.md` ZIP. Sub-task 4b adds the matching folder-upload detection and the bundle preview UI.

**Files:**
- Modify: `ui/desktop/src/main.ts`
- Modify: `ui/desktop/src/components/skills/AddSkillModal.tsx`

### 4a — `main.ts`: make `write-file` create parent dirs + add bundle detection to `skills:extract-zip`

- [ ] **Step 1: Update `write-file` IPC handler to create parent directories**

Find this block in `main.ts` (around line 1863):

```typescript
ipcMain.handle('write-file', async (_event, filePath, content) => {
  try {
    // Expand tilde to home directory
    const expandedPath = expandTilde(filePath);
    await fs.writeFile(expandedPath, content, { encoding: 'utf8' });
    return true;
  } catch (error) {
    console.error('Error writing to file:', error);
    return false;
  }
});
```

Replace with:

```typescript
ipcMain.handle('write-file', async (_event, filePath, content) => {
  try {
    const expandedPath = expandTilde(filePath);
    await fs.mkdir(path.dirname(expandedPath), { recursive: true });
    await fs.writeFile(expandedPath, content, { encoding: 'utf8' });
    return true;
  } catch (error) {
    console.error('Error writing to file:', error);
    return false;
  }
});
```

- [ ] **Step 2: Update `skills:extract-zip` IPC handler to detect bundle ZIPs**

Find the `skills:extract-zip` handler (around line 2096–2148). Replace the entire handler with:

```typescript
ipcMain.handle(
  'skills:extract-zip',
  async (_event, { filePath }: { filePath: string }) => {
    try {
      const zip = new AdmZip(filePath);
      const entries = zip.getEntries();

      // --- Single skill: root SKILL.md ---
      let skillEntry = entries.find((e) => e.entryName === 'SKILL.md');
      let prefix = '';

      if (!skillEntry) {
        // Single skill inside a folder: <slug>/SKILL.md
        const single = entries.find((e) => /^[^/]+\/SKILL\.md$/.test(e.entryName));
        if (single) {
          skillEntry = single;
          prefix = single.entryName.replace(/\/SKILL\.md$/, '') + '/';
        }
      }

      if (skillEntry) {
        // --- Single skill install ---
        const parsed = parseFrontmatterFromSkillMd(skillEntry.getData().toString('utf8'));
        if (!parsed) {
          return {
            error: 'SKILL.md must have valid frontmatter with "name" and "description".',
          };
        }

        const slug = parsed.name
          .replace(/[^a-z0-9-_]/gi, '-')
          .replace(/-{2,}/g, '-')
          .replace(/^-|-$/g, '')
          .toLowerCase();

        const TEXT_EXTENSIONS = ['.md', '.txt', '.yaml', '.yml', '.json', '.py', '.sh'];
        const files: [string, string][] = [];
        for (const entry of entries) {
          if (entry.isDirectory) continue;
          if (prefix && !entry.entryName.startsWith(prefix)) continue;
          const relName = prefix ? entry.entryName.slice(prefix.length) : entry.entryName;
          if (!relName) continue;
          const ext = path.extname(relName).toLowerCase();
          if (!TEXT_EXTENSIONS.includes(ext)) continue;
          files.push([relName, entry.getData().toString('utf8')]);
        }

        return { files, name: parsed.name, description: parsed.description, slug, isBundle: false };
      }

      // --- Bundle detection: <bundleName>/<subSlug>/SKILL.md ---
      // Find all entries matching a 3-level SKILL.md pattern
      const bundleSkillEntries = entries.filter((e) =>
        /^[^/]+\/[^/]+\/SKILL\.md$/.test(e.entryName)
      );

      if (bundleSkillEntries.length === 0) {
        return { error: 'No SKILL.md found in the ZIP file.' };
      }

      // Group by bundle folder (first path component)
      const bundleFolder = bundleSkillEntries[0].entryName.split('/')[0];
      const bundlePrefix = bundleFolder + '/';
      const bundleSkills: Array<{ name: string; description: string }> = [];

      for (const entry of bundleSkillEntries) {
        // Only include skills that belong to the same bundle folder
        if (!entry.entryName.startsWith(bundlePrefix)) continue;
        const parsed = parseFrontmatterFromSkillMd(entry.getData().toString('utf8'));
        if (parsed) bundleSkills.push(parsed);
      }

      if (bundleSkills.length === 0) {
        return { error: 'No valid SKILL.md files found in bundle.' };
      }

      // Collect all text files, stripping the top-level bundle folder prefix
      const TEXT_EXTENSIONS = ['.md', '.txt', '.yaml', '.yml', '.json', '.py', '.sh'];
      const bundleFiles: [string, string][] = [];
      for (const entry of entries) {
        if (entry.isDirectory) continue;
        if (!entry.entryName.startsWith(bundlePrefix)) continue;
        const relName = entry.entryName.slice(bundlePrefix.length);
        if (!relName) continue;
        const ext = path.extname(relName).toLowerCase();
        if (!TEXT_EXTENSIONS.includes(ext)) continue;
        bundleFiles.push([relName, entry.getData().toString('utf8')]);
      }

      return {
        isBundle: true,
        bundleName: bundleFolder,
        bundleSkills,
        files: bundleFiles,
        slug: bundleFolder
          .replace(/[^a-z0-9-_]/gi, '-')
          .replace(/-{2,}/g, '-')
          .replace(/^-|-$/g, '')
          .toLowerCase(),
        // For API compatibility with single-skill callers, provide name/description of first sub-skill
        name: bundleFolder,
        description: `Bundle of ${bundleSkills.length} skills`,
      };
    } catch (err) {
      return { error: `Failed to read ZIP: ${(err as Error).message}` };
    }
  }
);
```

- [ ] **Step 3: Update `preload.ts` to reflect the new `extractSkillZip` return type**

Find in `ui/desktop/src/preload.ts` the type declaration for `extractSkillZip` (around line 152). Update its return type to include the bundle fields:

Current:
```typescript
extractSkillZip: (filePath: string) => Promise<{
  files: [string, string][];
  name: string;
  description: string;
  slug: string;
} | { error: string }>;
```

Replace with:
```typescript
extractSkillZip: (filePath: string) => Promise<
  | { isBundle: false; files: [string, string][]; name: string; description: string; slug: string }
  | {
      isBundle: true;
      bundleName: string;
      bundleSkills: Array<{ name: string; description: string }>;
      files: [string, string][];
      slug: string;
      name: string;
      description: string;
    }
  | { error: string }
>;
```

- [ ] **Step 4: Commit main.ts and preload.ts changes**

```bash
git add ui/desktop/src/main.ts ui/desktop/src/preload.ts
git commit -m "feat(skills): write-file creates parent dirs; skills:extract-zip detects bundle ZIPs"
```

### 4b — `AddSkillModal.tsx`: bundle detection for folder upload + bundle preview UI

- [ ] **Step 5: Replace the full content of `AddSkillModal.tsx`**

```tsx
import { useState, useRef, DragEvent } from 'react';
import { Button } from '../ui/button';
import { parseSkillFrontmatter, toSlug, BIOROUTER_SKILLS_DIR } from './skillUtils';
import { toastSuccess, toastError } from '../../toasts';

interface Props {
  onClose: () => void;
  onSaved: () => void;
}

interface SinglePreview {
  isBundle: false;
  name: string;
  description: string;
  slug: string;
  files: [string, string][];
  label: string;
}

interface BundlePreview {
  isBundle: true;
  bundleName: string;
  slug: string;
  bundleSkills: Array<{ name: string; description: string }>;
  files: [string, string][];
  label: string;
}

type Preview = SinglePreview | BundlePreview;

/** Walk a FileList from a webkitdirectory input, find SKILL.md, validate, collect all files. */
function parseUploadedFolder(fileList: FileList): Promise<Preview> {
  return new Promise((resolve, reject) => {
    const files = Array.from(fileList);
    if (files.length === 0) { reject(new Error('No files found in folder.')); return; }

    const topFolder = files[0].webkitRelativePath.split('/')[0] ?? 'skill';

    // Check for root SKILL.md: path like "topFolder/SKILL.md" (2 parts)
    const rootSkillMdFile = files.find((f) => {
      const parts = f.webkitRelativePath.split('/');
      return parts.length === 2 && f.name === 'SKILL.md';
    });

    if (rootSkillMdFile) {
      // --- Single skill ---
      const skillReader = new FileReader();
      skillReader.onerror = () => reject(new Error('Failed to read SKILL.md'));
      skillReader.onload = (e) => {
        const skillMdContent = e.target?.result as string;
        const parsed = parseSkillFrontmatter(skillMdContent);
        if (!parsed) {
          reject(new Error('SKILL.md must have valid YAML frontmatter with "name" and "description".'));
          return;
        }
        const slug = toSlug(parsed.name) || toSlug(topFolder);
        const filePairs: [string, string][] = [];
        let remaining = files.length;
        files.forEach((file) => {
          const rel = file.webkitRelativePath.replace(/^[^/]+\/?/, '') || file.name;
          const fr = new FileReader();
          fr.onerror = () => reject(new Error(`Failed to read ${file.name}`));
          fr.onload = (ev) => {
            filePairs.push([rel, ev.target?.result as string]);
            remaining--;
            if (remaining === 0) {
              resolve({ isBundle: false, name: parsed.name, description: parsed.description, slug, files: filePairs, label: topFolder });
            }
          };
          fr.readAsText(file);
        });
      };
      skillReader.readAsText(rootSkillMdFile);
      return;
    }

    // --- Bundle detection: sub-level SKILL.md files at "topFolder/<sub>/SKILL.md" (3 parts) ---
    const subSkillMdFiles = files.filter((f) => {
      const parts = f.webkitRelativePath.split('/');
      return parts.length === 3 && f.name === 'SKILL.md';
    });

    if (subSkillMdFiles.length === 0) {
      reject(new Error('Folder must contain a SKILL.md file, or sub-folders that each contain a SKILL.md.'));
      return;
    }

    // Read all SKILL.md files to build bundleSkills list
    const bundleSkills: Array<{ name: string; description: string }> = [];
    let pending = subSkillMdFiles.length;

    const filePairs: [string, string][] = [];
    let filePending = files.length;

    const tryResolve = () => {
      if (pending === 0 && filePending === 0) {
        if (bundleSkills.length === 0) {
          reject(new Error('No valid SKILL.md files found in sub-folders.'));
          return;
        }
        resolve({
          isBundle: true,
          bundleName: topFolder,
          slug: toSlug(topFolder),
          bundleSkills,
          files: filePairs,
          label: topFolder,
        });
      }
    };

    subSkillMdFiles.forEach((skillMdFile) => {
      const fr = new FileReader();
      fr.onerror = () => { pending--; tryResolve(); };
      fr.onload = (e) => {
        const content = e.target?.result as string;
        const parsed = parseSkillFrontmatter(content);
        if (parsed) bundleSkills.push(parsed);
        pending--;
        tryResolve();
      };
      fr.readAsText(skillMdFile);
    });

    files.forEach((file) => {
      const rel = file.webkitRelativePath.replace(/^[^/]+\/?/, '') || file.name;
      const fr = new FileReader();
      fr.onerror = () => { filePending--; tryResolve(); };
      fr.onload = (ev) => {
        filePairs.push([rel, ev.target?.result as string]);
        filePending--;
        tryResolve();
      };
      fr.readAsText(file);
    });
  });
}

export default function AddSkillModal({ onClose, onSaved }: Props) {
  const [isDragging, setIsDragging] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [isInstalling, setIsInstalling] = useState(false);
  const mdInputRef = useRef<HTMLInputElement>(null);
  const folderInputRef = useRef<HTMLInputElement>(null);
  const zipInputRef = useRef<HTMLInputElement>(null);

  const processMdFile = (file: File) => {
    if (!file.name.endsWith('.md')) {
      setError('Only .md files or folders with SKILL.md are accepted.');
      setPreview(null);
      return;
    }
    const reader = new FileReader();
    reader.onerror = () => { setError('Failed to read file.'); setPreview(null); };
    reader.onload = (e) => {
      const content = e.target?.result as string;
      const parsed = parseSkillFrontmatter(content);
      if (!parsed) {
        setError('File must have valid YAML frontmatter with "name" and "description" fields.');
        setPreview(null);
        return;
      }
      setError(null);
      setPreview({
        isBundle: false,
        name: parsed.name,
        description: parsed.description,
        slug: toSlug(parsed.name) || toSlug(file.name),
        files: [['SKILL.md', content]],
        label: file.name,
      });
    };
    reader.readAsText(file);
  };

  const handleDrop = async (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setIsDragging(false);
    const file = e.dataTransfer.files[0];
    if (!file) return;
    if (file.name.endsWith('.zip')) {
      await processZipFile(file);
    } else {
      processMdFile(file);
    }
  };

  const handleMdBrowse = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) processMdFile(file);
    e.target.value = '';
  };

  const handleFolderBrowse = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (!files || files.length === 0) return;
    try {
      const p = await parseUploadedFolder(files);
      setError(null);
      setPreview(p);
    } catch (err) {
      setError((err as Error).message);
      setPreview(null);
    }
    e.target.value = '';
  };

  const processZipFile = async (file: File) => {
    const filePath = window.electron.getPathForFile(file);
    const result = await window.electron.extractSkillZip(filePath);
    if ('error' in result) {
      setError(result.error);
      setPreview(null);
      return;
    }
    setError(null);
    if (result.isBundle) {
      setPreview({
        isBundle: true,
        bundleName: result.bundleName,
        slug: result.slug,
        bundleSkills: result.bundleSkills,
        files: result.files,
        label: file.name,
      });
    } else {
      setPreview({
        isBundle: false,
        name: result.name,
        description: result.description,
        slug: result.slug,
        files: result.files,
        label: file.name,
      });
    }
  };

  const handleZipBrowse = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) await processZipFile(file);
    e.target.value = '';
  };

  const handleInstall = async () => {
    if (!preview) return;
    setIsInstalling(true);

    const destFolder = `${BIOROUTER_SKILLS_DIR}/${preview.slug}`;
    await window.electron.ensureDirectory(destFolder);
    let allOk = true;
    for (const [relPath, content] of preview.files) {
      const ok = await window.electron.writeFile(`${destFolder}/${relPath}`, content);
      if (!ok) { allOk = false; break; }
    }
    setIsInstalling(false);

    if (allOk) {
      const displayName = preview.isBundle ? preview.bundleName : preview.name;
      toastSuccess({ title: displayName, msg: 'Installed to BioRouter Skills' });
      onSaved();
      onClose();
    } else {
      toastError({ title: 'Install failed', msg: `Could not write to ${destFolder}` });
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="bg-background-default rounded-xl border border-border-subtle shadow-lg w-[480px] max-h-[80vh] flex flex-col">
        <div className="px-6 pt-5 pb-4 border-b border-border-subtle flex items-center justify-between">
          <h2 className="text-base font-semibold">Add Skill</h2>
          <Button variant="ghost" size="sm" className="h-7 w-7 p-0" onClick={onClose}>✕</Button>
        </div>

        <div className="p-6 flex flex-col gap-4 overflow-y-auto">
          {/* Drop zone */}
          <div
            onDragOver={(e) => { e.preventDefault(); setIsDragging(true); }}
            onDragLeave={() => setIsDragging(false)}
            onDrop={handleDrop}
            onClick={() => mdInputRef.current?.click()}
            className={`border-2 border-dashed rounded-lg p-8 text-center cursor-pointer transition-colors ${
              isDragging
                ? 'border-blue-500 bg-blue-50 dark:bg-blue-900/20'
                : 'border-border-subtle hover:border-blue-400 hover:bg-background-medium/30'
            }`}
          >
            <p className="text-sm text-text-muted">
              Drop a <code>.md</code> or <code>.zip</code> skill file here, or{' '}
              <span className="text-blue-600 underline">browse for file</span>
            </p>
            <p className="text-xs text-text-subtle mt-1">
              File needs YAML frontmatter with <code>name</code> and <code>description</code>.
              A folder named after the skill with <code>SKILL.md</code> inside will be created.
            </p>
          </div>
          <input ref={mdInputRef} type="file" accept=".md" className="hidden" onChange={handleMdBrowse} />

          {/* Folder upload */}
          <div className="flex items-center gap-3">
            <div className="h-px flex-1 bg-border-subtle" />
            <span className="text-xs text-text-subtle">or</span>
            <div className="h-px flex-1 bg-border-subtle" />
          </div>
          <Button
            variant="outline"
            className="w-full"
            onClick={() => folderInputRef.current?.click()}
          >
            Browse for Skill Folder
          </Button>
          <p className="text-xs text-text-subtle -mt-2 text-center">
            Folder with <code>SKILL.md</code> (single skill) or sub-folders each with <code>SKILL.md</code> (bundle)
          </p>
          <input
            ref={folderInputRef}
            type="file"
            // @ts-expect-error -- webkitdirectory is non-standard but supported by Electron/Chromium
            webkitdirectory=""
            className="hidden"
            onChange={handleFolderBrowse}
          />
          <Button
            variant="outline"
            className="w-full"
            onClick={() => zipInputRef.current?.click()}
          >
            Browse for Skill ZIP
          </Button>
          <p className="text-xs text-text-subtle -mt-2 text-center">
            ZIP with <code>SKILL.md</code> (single skill) or a bundle folder containing sub-skills
          </p>
          <input
            ref={zipInputRef}
            type="file"
            accept=".zip"
            className="hidden"
            onChange={handleZipBrowse}
          />

          {error && (
            <div className="text-sm text-destructive bg-destructive/10 rounded-lg px-4 py-3">
              {error}
            </div>
          )}

          {preview && !preview.isBundle && (
            <div className="bg-background-medium/30 rounded-lg px-4 py-3">
              <p className="text-sm font-semibold">{preview.name}</p>
              <p className="text-xs text-text-muted mt-0.5">{preview.description}</p>
              <p className="text-[11px] text-text-subtle mt-1 font-mono">
                {preview.files.length} file{preview.files.length !== 1 ? 's' : ''} · from {preview.label}
              </p>
            </div>
          )}

          {preview && preview.isBundle && (
            <div className="bg-background-medium/30 rounded-lg px-4 py-3">
              <p className="text-sm font-semibold">
                Bundle: {preview.bundleName}
                <span className="ml-2 text-xs text-text-subtle font-normal">
                  {preview.bundleSkills.length} skills
                </span>
              </p>
              <div className="mt-1.5 max-h-[120px] overflow-y-auto">
                {preview.bundleSkills.map((s, i) => (
                  <p key={i} className="text-xs text-text-muted leading-relaxed">
                    · {s.name}
                    {s.description && (
                      <span className="text-text-subtle"> — {s.description}</span>
                    )}
                  </p>
                ))}
              </div>
              <p className="text-[11px] text-text-subtle mt-1.5 font-mono">
                {preview.files.length} file{preview.files.length !== 1 ? 's' : ''} · from {preview.label}
              </p>
            </div>
          )}
        </div>

        <div className="px-6 py-4 border-t border-border-subtle flex justify-end gap-2">
          <Button variant="outline" onClick={onClose}>Cancel</Button>
          <Button variant="default" onClick={handleInstall} disabled={!preview || isInstalling}>
            {isInstalling
              ? 'Installing…'
              : preview?.isBundle
              ? `Install Bundle (${preview.bundleSkills.length} skills)`
              : 'Install Skill'}
          </Button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 6: Run type-check**

```bash
cd ui/desktop && npx tsc --noEmit 2>&1 | head -40
```

Expected: no errors.

- [ ] **Step 7: Run all unit tests**

```bash
cd ui/desktop && npm run test:run
```

Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add ui/desktop/src/components/skills/AddSkillModal.tsx
git commit -m "feat(skills): AddSkillModal detects and installs skill bundles from folder or ZIP"
```

---

## Task 5: Rust — bundle discovery in `skills_extension.rs`

Brings the agent runtime in line with the UI: adds `bundle_name: Option<String>` to the `Skill` struct, threads it through `parse_skill_file`, rewrites `discover_skills_in_directories` for two-level detection, and makes the disabled-skills filter honour a disabled bundle name. This is the only task with genuinely failing tests up front — the three new tests will not compile until the struct field exists.

**Files:**
- Modify: `crates/biorouter/src/agents/skills_extension.rs`

- [ ] **Step 1: Write failing Rust tests for bundle discovery**

Append to the `tests` module at the bottom of `skills_extension.rs` (after the last existing test, before the closing `}`):

```rust
    #[test]
    fn test_discover_single_skill() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join("my-skill");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: A test skill\n---\nBody",
        )
        .unwrap();

        let skills = SkillsClient::discover_skills_in_directories(&[temp_dir.path().to_path_buf()]);
        assert_eq!(skills.len(), 1);
        let skill = skills.get("my-skill").unwrap();
        assert_eq!(skill.metadata.name, "my-skill");
        assert!(skill.bundle_name.is_none());
    }

    #[test]
    fn test_discover_bundle() {
        let temp_dir = TempDir::new().unwrap();
        let bundle_dir = temp_dir.path().join("superpowers");
        fs::create_dir(&bundle_dir).unwrap();

        // Two sub-skills in the bundle
        let sub1 = bundle_dir.join("brainstorming");
        fs::create_dir(&sub1).unwrap();
        fs::write(
            sub1.join("SKILL.md"),
            "---\nname: brainstorming\ndescription: Brainstorm ideas\n---\nBody",
        )
        .unwrap();

        let sub2 = bundle_dir.join("debugging");
        fs::create_dir(&sub2).unwrap();
        fs::write(
            sub2.join("SKILL.md"),
            "---\nname: debugging\ndescription: Debug code\n---\nBody",
        )
        .unwrap();

        let skills = SkillsClient::discover_skills_in_directories(&[temp_dir.path().to_path_buf()]);
        assert_eq!(skills.len(), 2);

        let br = skills.get("brainstorming").unwrap();
        assert_eq!(br.bundle_name.as_deref(), Some("superpowers"));

        let dbg = skills.get("debugging").unwrap();
        assert_eq!(dbg.bundle_name.as_deref(), Some("superpowers"));
    }

    #[test]
    fn test_bundle_disabled_by_bundle_name() {
        let temp_dir = TempDir::new().unwrap();
        let bundle_dir = temp_dir.path().join("superpowers");
        fs::create_dir(&bundle_dir).unwrap();

        let sub = bundle_dir.join("brainstorming");
        fs::create_dir(&sub).unwrap();
        fs::write(
            sub.join("SKILL.md"),
            "---\nname: brainstorming\ndescription: Brainstorm ideas\n---\nBody",
        )
        .unwrap();

        let skills = SkillsClient::discover_skills_in_directories(&[temp_dir.path().to_path_buf()]);
        assert_eq!(skills.len(), 1);

        // Simulate filtering with bundle name in disabled set
        let mut disabled = std::collections::HashSet::new();
        disabled.insert("superpowers".to_string());

        let filtered: Vec<_> = skills
            .into_iter()
            .filter(|(name, skill)| {
                !disabled.contains(name)
                    && !skill
                        .bundle_name
                        .as_deref()
                        .map_or(false, |b| disabled.contains(b))
            })
            .collect();

        assert!(filtered.is_empty(), "bundle skill should be filtered out when bundle name is disabled");
    }
```

- [ ] **Step 2: Run the new tests to verify they fail**

```bash
cargo test -p biorouter --test-thread=1 test_discover_bundle test_bundle_disabled 2>&1 | tail -20
```

Expected: compilation error (`bundle_name` field doesn't exist yet).

- [ ] **Step 3: Add `bundle_name` field to the `Skill` struct**

Find the `Skill` struct (around line 31):

```rust
#[derive(Debug, Clone)]
struct Skill {
    metadata: SkillMetadata,
    body: String,
    directory: PathBuf,
    supporting_files: Vec<PathBuf>,
}
```

Replace with:

```rust
#[derive(Debug, Clone)]
struct Skill {
    metadata: SkillMetadata,
    body: String,
    directory: PathBuf,
    supporting_files: Vec<PathBuf>,
    bundle_name: Option<String>,
}
```

- [ ] **Step 4: Update `parse_skill_file` to accept and thread `bundle_name`**

Find `parse_skill_file` (around line 138):

```rust
fn parse_skill_file(path: &Path) -> Result<Skill> {
    let content = std::fs::read_to_string(path)?;
    let (metadata, body) = Self::parse_frontmatter(&content)?;
    let directory = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Skill file has no parent directory"))?
        .to_path_buf();
    let supporting_files = Self::find_supporting_files(&directory, path)?;
    Ok(Skill {
        metadata,
        body,
        directory,
        supporting_files,
    })
}
```

Replace with:

```rust
fn parse_skill_file(path: &Path, bundle_name: Option<String>) -> Result<Skill> {
    let content = std::fs::read_to_string(path)?;
    let (metadata, body) = Self::parse_frontmatter(&content)?;
    let directory = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Skill file has no parent directory"))?
        .to_path_buf();
    let supporting_files = Self::find_supporting_files(&directory, path)?;
    Ok(Skill {
        metadata,
        body,
        directory,
        supporting_files,
        bundle_name,
    })
}
```

- [ ] **Step 5: Update `discover_skills_in_directories` to detect bundles**

Find the method (around line 197):

```rust
fn discover_skills_in_directories(directories: &[PathBuf]) -> HashMap<String, Skill> {
    let mut skills = HashMap::new();

    for dir in directories {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let skill_file = path.join("SKILL.md");
                    if skill_file.exists() {
                        if let Ok(skill) = Self::parse_skill_file(&skill_file) {
                            skills.insert(skill.metadata.name.clone(), skill);
                        }
                    }
                }
            }
        }
    }

    skills
}
```

Replace with:

```rust
fn discover_skills_in_directories(directories: &[PathBuf]) -> HashMap<String, Skill> {
    let mut skills = HashMap::new();

    for dir in directories {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }

                let skill_file = path.join("SKILL.md");
                if skill_file.exists() {
                    // Single skill
                    if let Ok(skill) = Self::parse_skill_file(&skill_file, None) {
                        skills.insert(skill.metadata.name.clone(), skill);
                    }
                } else {
                    // Bundle: check if sub-directories contain SKILL.md
                    let bundle_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(str::to_string);

                    if let (Some(bundle_name), Ok(sub_entries)) =
                        (bundle_name, std::fs::read_dir(&path))
                    {
                        for sub_entry in sub_entries.flatten() {
                            let sub_path = sub_entry.path();
                            if !sub_path.is_dir() {
                                continue;
                            }
                            let sub_skill_file = sub_path.join("SKILL.md");
                            if sub_skill_file.exists() {
                                if let Ok(skill) = Self::parse_skill_file(
                                    &sub_skill_file,
                                    Some(bundle_name.clone()),
                                ) {
                                    skills.insert(skill.metadata.name.clone(), skill);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    skills
}
```

- [ ] **Step 6: Update the disabled-skills filter in `new()` to check `bundle_name`**

Find this block in `SkillsClient::new()` (around line 72):

```rust
let skills = skills
    .into_iter()
    .filter(|(name, _)| !disabled.contains(name))
    .collect();
```

Replace with:

```rust
let skills = skills
    .into_iter()
    .filter(|(name, skill)| {
        !disabled.contains(name)
            && !skill
                .bundle_name
                .as_deref()
                .map_or(false, |b| disabled.contains(b))
    })
    .collect();
```

- [ ] **Step 7: Run all Rust tests**

```bash
cargo test -p biorouter 2>&1 | tail -20
```

Expected: all tests pass, including the three new bundle tests.

- [ ] **Step 8: Build to confirm no compile errors**

```bash
cargo build 2>&1 | tail -20
```

Expected: clean build.

- [ ] **Step 9: Commit**

```bash
git add crates/biorouter/src/agents/skills_extension.rs
git commit -m "feat(skills): Rust bundle discovery — two-level SKILL.md detection with bundle_name filtering"
```

---

## Final verification

Not a task — a closing gate. Re-runs both sides of the build once Tasks 1 to 5 have landed, to confirm the TypeScript and Rust halves agree.

- [ ] **Run frontend type-check and tests one more time**

```bash
cd ui/desktop && npx tsc --noEmit 2>&1 | head -20 && npm run test:run
```

Expected: zero type errors, all tests pass.

- [ ] **Run Rust tests**

```bash
cargo test -p biorouter 2>&1 | tail -10
```

Expected: all tests pass.

## Related documentation

- [Skill bundles design](skill-bundles-design.md) — the spec this plan executes; read it first for the detection rule and the single-toggle rationale.
- [.brxt bundled skills implementation plan](brxt-bundled-skills-plan.md) — the sibling plan from the same day; it created the `skills:extract-zip` handler that Task 4a here extends with bundle detection.
- [.brxt bundled skills design](brxt-bundled-skills-design.md) — the design behind that handler and extension-local skill storage.
- [Desktop bug fix batch v1.72.1](../desktop-ui-fixes/v1-72-1-bug-fix-batch.md) — a later plan that builds on the `bundle_name` filtering landed in Task 5.
- [Skills extension](../../extensions/built-in/skills.md) — the user-facing view of the skill discovery this plan changed.
