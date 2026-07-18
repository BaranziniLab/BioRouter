# .brxt bundled skills implementation plan

> **What this is.** The task-by-task implementation plan that executed the [.brxt bundled skills design](brxt-bundled-skills-design.md): bundling skills inside `.brxt` extension packages so installing an extension installs its skills and removing it removes them, plus ZIP import in the Add Skill modal.
> **Status:** Historical record — written 2026-05-07 and completed. The work shipped: `brxt:uninstall` and `skills:extract-zip` are IPC handlers in `ui/desktop/src/main.ts`, `brxt:validate-and-read` returns `skillsPreview`, and `crates/biorouter/src/agents/skills_extension.rs` scans `~/.config/biorouter/extensions/*/skills/`. The `- [ ]` checkboxes below were never ticked off in the file — read them as the original task list, not as outstanding work.
> **Audience:** agents and developers tracing how extension-bundled skill discovery was built.

This plan was written to be executed by an agent, task by task, in test-driven order: write a failing test, run it, implement, re-run, commit. Each `## Task N` heading is one commit-sized unit. The code blocks are the literal patches proposed at authoring time; where the shipped code diverged, the repository is authoritative.

> **Note.** The original file opened with a machine-directed banner naming the `superpowers:subagent-driven-development` and `superpowers:executing-plans` skills as the intended execution harnesses. That instruction is recorded here for provenance and is no longer actionable — the plan is complete.

**Goal:** Bundle skills inside `.brxt` extension packages so that installing an extension auto-installs its skills, removing an extension removes its skills, and the standalone skill import UI gains ZIP file support.

**Architecture:** Extension-local storage: skills from a `.brxt` install into `~/.config/biorouter/extensions/<name>/skills/<slug>/`, which the Rust `SkillsClient` discovers by scanning `~/.config/biorouter/extensions/*/skills/`. A new `brxt:uninstall` IPC handler removes the extension directory atomically (Python + skills in one `rm -rf`). Skill ZIP import is handled by a new `skills:extract-zip` IPC handler using `adm-zip` (already a dependency).

**Tech Stack:** Rust (skills_extension.rs), TypeScript + Electron IPC (main.ts, preload.ts), React (BrxtInstallModal, ExtensionsSection, AddSkillModal), adm-zip (already in dependencies), Playwright E2E tests, Rust `#[test]` with `tempfile`.

---

## File map

| File | Change |
|------|--------|
| `crates/biorouter/src/agents/skills_extension.rs` | Add `extensions/*/skills/` discovery in `get_default_skill_directories()` |
| `ui/desktop/src/types/brxt.ts` | Add `BrxtSkillMeta` interface; add `skills?` to `BrxtManifest` |
| `ui/desktop/src/main.ts` | Extend `brxt:validate-and-read`; add `brxt:uninstall`; add `skills:extract-zip` |
| `ui/desktop/src/preload.ts` | Update `validateBrxtBundle` return type; add `uninstallBrxtExtension` + `extractSkillZip` |
| `ui/desktop/src/components/BrxtInstallModal.tsx` | Show `skillsPreview` in manifest preview card |
| `ui/desktop/src/components/settings/extensions/ExtensionsSection.tsx` | Call `uninstallBrxtExtension` on `.brxt`-installed extension deletion |
| `ui/desktop/src/components/skills/AddSkillModal.tsx` | Add ZIP import path (drag, browse button, hidden input) |
| `ui/desktop/tests/e2e/brxt.spec.ts` | Add `createValidBrxtWithSkills` helper + skills-preview test |
| `ui/desktop/tests/fixtures/skills/` (new dir) | Sample SKILL.md files for CDWAgent, UCSFOMOPAgent, SPOKEAgent |

---

## Task 1: Rust — extend skill scanner to discover extension skills

**Files:**
- Modify: `crates/biorouter/src/agents/skills_extension.rs:79-96` (function body)
- Test: same file, `mod tests` section

- [ ] **Step 1: Write the failing test**

  Add this test inside `mod tests` in `crates/biorouter/src/agents/skills_extension.rs`:

  ```rust
  #[test]
  fn test_discover_extension_skills() {
      let temp_dir = TempDir::new().unwrap();
      let skill_dir = temp_dir
          .path()
          .join("extensions")
          .join("myext")
          .join("skills")
          .join("my-ext-skill");
      fs::create_dir_all(&skill_dir).unwrap();
      fs::write(
          skill_dir.join("SKILL.md"),
          "---\nname: my-ext-skill\ndescription: An extension skill\n---\n\nBody here.",
      )
      .unwrap();

      let ext_skills_dir = temp_dir.path().join("extensions").join("myext").join("skills");
      let skills = SkillsClient::discover_skills_in_directories(&[ext_skills_dir]);
      assert!(skills.contains_key("my-ext-skill"), "extension skill not found");
      assert_eq!(
          skills["my-ext-skill"].metadata.description,
          "An extension skill"
      );
  }
  ```

- [ ] **Step 2: Run the test, then add the one that actually fails**

  ```bash
  cargo test -p biorouter test_discover_extension_skills -- --nocapture
  ```

  Expected: PASS, not FAIL. `discover_skills_in_directories` already accepts arbitrary directories, so it discovers skills under an `extensions/<name>/skills/` layout without any change. That makes `test_discover_extension_skills` a characterization test — worth keeping, but it does not drive the new code.

  The behaviour that is genuinely missing is that `get_default_skill_directories()` includes the extension skills directories at all. Add this second test, which does fail before the change, to cover the integration:

  ```rust
  #[test]
  fn test_get_default_skill_directories_includes_extensions() {
      let temp_dir = TempDir::new().unwrap();
      // Create extensions/myext/skills/ directory
      let ext_skills = temp_dir
          .path()
          .join("config")
          .join("extensions")
          .join("myext")
          .join("skills");
      fs::create_dir_all(&ext_skills).unwrap();
      // Create a skill inside it
      let skill_dir = ext_skills.join("my-ext-skill");
      fs::create_dir_all(&skill_dir).unwrap();
      fs::write(
          skill_dir.join("SKILL.md"),
          "---\nname: my-ext-skill\ndescription: test\n---\nbody",
      )
      .unwrap();

      // BIOROUTER_PATH_ROOT redirects Paths::config_dir() to temp_dir/config
      // We call get_default_skill_directories() with env var set, capture result, unset
      std::env::set_var("BIOROUTER_PATH_ROOT", temp_dir.path());
      let dirs = SkillsClient::get_default_skill_directories();
      std::env::remove_var("BIOROUTER_PATH_ROOT");

      assert!(
          dirs.iter().any(|d| d == &ext_skills),
          "extension skills dir not in default dirs: {:?}",
          dirs
      );
  }
  ```

- [ ] **Step 3: Implement the change**

  In `crates/biorouter/src/agents/skills_extension.rs`, replace the body of `get_default_skill_directories()` (currently lines 79-96):

  ```rust
  fn get_default_skill_directories() -> Vec<PathBuf> {
      let mut dirs = Vec::new();

      if let Some(home) = dirs::home_dir() {
          dirs.push(home.join(".claude/skills"));
          dirs.push(home.join(".config/agents/skills"));
      }

      dirs.push(Paths::config_dir().join("skills"));

      // Scan installed .brxt extension skills subdirectories
      let extensions_dir = Paths::config_dir().join("extensions");
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

      if let Ok(working_dir) = std::env::current_dir() {
          dirs.push(working_dir.join(".claude/skills"));
          dirs.push(working_dir.join(".biorouter/skills"));
          dirs.push(working_dir.join(".agents/skills"));
      }

      dirs
  }
  ```

- [ ] **Step 4: Run both tests**

  ```bash
  cargo test -p biorouter test_discover_extension_skills test_get_default_skill_directories_includes_extensions -- --nocapture
  ```

  Expected: both PASS.

- [ ] **Step 5: Run full Rust test suite**

  ```bash
  cargo test -p biorouter
  ```

  Expected: all existing tests still pass.

- [ ] **Step 6: Commit**

  ```bash
  git add crates/biorouter/src/agents/skills_extension.rs
  git commit -m "feat(skills): discover skills from installed .brxt extension directories"
  ```

---

## Task 2: TypeScript types — add BrxtSkillMeta

**Files:**
- Modify: `ui/desktop/src/types/brxt.ts`

- [ ] **Step 1: Update the file**

  Replace the entire content of `ui/desktop/src/types/brxt.ts`:

  ```typescript
  export interface BrxtEnvVar {
    key: string;
    required: boolean;
    auto_propagate: boolean;
    default?: string;
    description: string;
    secret: boolean;
  }

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
    skills?: BrxtSkillMeta[];
  }
  ```

- [ ] **Step 2: Type-check**

  ```bash
  cd ui/desktop && npm run typecheck
  ```

  Expected: no errors (only type addition, no breaking changes yet).

- [ ] **Step 3: Commit**

  ```bash
  git add ui/desktop/src/types/brxt.ts
  git commit -m "feat(types): add BrxtSkillMeta and optional skills field to BrxtManifest"
  ```

---

## Task 3: main.ts — extend brxt:validate-and-read + add brxt:uninstall + skills:extract-zip

**Files:**
- Modify: `ui/desktop/src/main.ts` (three changes near line 1983–2048)

- [ ] **Step 1: Add `parseFrontmatterFromSkillMd` helper**

  Find the line just before `ipcMain.handle('brxt:open-file-dialog'` (around line 1967) and insert this helper function before it:

  ```typescript
  function parseFrontmatterFromSkillMd(
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
  ```

- [ ] **Step 2: Extend `brxt:validate-and-read` to scan skills**

  In the `brxt:validate-and-read` handler, replace the final `return { manifest };` (currently the last line before the `catch`) with:

  ```typescript
  // Scan for bundled skills in skills/<slug>/SKILL.md
  const skillsPreview: Array<{ slug: string; name: string; description: string }> = [];
  for (const entry of zip.getEntries()) {
    const m = entry.entryName.match(/^skills\/([^/]+)\/SKILL\.md$/);
    if (m) {
      const slug = m[1];
      const parsed = parseFrontmatterFromSkillMd(entry.getData().toString('utf8'));
      if (parsed) skillsPreview.push({ slug, name: parsed.name, description: parsed.description });
    }
  }

  return { manifest, skillsPreview };
  ```

- [ ] **Step 3: Add `brxt:uninstall` handler**

  Immediately after the closing `);` of the `brxt:install` handler (around line 2048), add:

  ```typescript
  ipcMain.handle(
    'brxt:uninstall',
    async (_event, { extensionName }: { extensionName: string }) => {
      try {
        const installDir = path.join(
          os.homedir(),
          '.config',
          'biorouter',
          'extensions',
          extensionName
        );
        if (fsSync.existsSync(installDir)) {
          fsSync.rmSync(installDir, { recursive: true, force: true });
        }
        return { success: true as const };
      } catch (err) {
        return { error: `Uninstall failed: ${(err as Error).message}` };
      }
    }
  );
  ```

- [ ] **Step 4: Add `skills:extract-zip` handler**

  Immediately after the `brxt:uninstall` handler, add:

  ```typescript
  ipcMain.handle(
    'skills:extract-zip',
    async (_event, { filePath }: { filePath: string }) => {
      try {
        const zip = new AdmZip(filePath);
        const entries = zip.getEntries();

        // Find SKILL.md at root or one level deep (<slug>/SKILL.md)
        let skillEntry = entries.find((e) => e.entryName === 'SKILL.md');
        let prefix = '';

        if (!skillEntry) {
          skillEntry = entries.find((e) => /^[^/]+\/SKILL\.md$/.test(e.entryName));
          if (skillEntry) {
            prefix = skillEntry.entryName.replace(/\/SKILL\.md$/, '') + '/';
          }
        }

        if (!skillEntry) {
          return { error: 'No SKILL.md found in the ZIP file.' };
        }

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

        const files: [string, string][] = [];
        for (const entry of entries) {
          if (entry.isDirectory) continue;
          const relName = prefix ? entry.entryName.slice(prefix.length) : entry.entryName;
          if (!relName) continue;
          files.push([relName, entry.getData().toString('utf8')]);
        }

        return { files, name: parsed.name, description: parsed.description, slug };
      } catch (err) {
        return { error: `Failed to read ZIP: ${(err as Error).message}` };
      }
    }
  );
  ```

- [ ] **Step 5: Build check**

  ```bash
  cd ui/desktop && npx tsc --noEmit
  ```

  Expected: no errors.

- [ ] **Step 6: Commit**

  ```bash
  git add ui/desktop/src/main.ts
  git commit -m "feat(ipc): extend brxt:validate-and-read with skills preview; add brxt:uninstall and skills:extract-zip handlers"
  ```

---

## Task 4: preload.ts — update type declarations and add IPC bindings

**Files:**
- Modify: `ui/desktop/src/preload.ts`

- [ ] **Step 1: Update `validateBrxtBundle` return type in the `ElectronAPI` interface**

  Find the line (around line 147):
  ```typescript
  validateBrxtBundle: (filePath: string) => Promise<{ manifest: import('./types/brxt').BrxtManifest } | { error: string }>;
  ```

  Replace with:
  ```typescript
  validateBrxtBundle: (filePath: string) => Promise<{
    manifest: import('./types/brxt').BrxtManifest;
    skillsPreview: Array<{ slug: string; name: string; description: string }>;
  } | { error: string }>;
  uninstallBrxtExtension: (extensionName: string) => Promise<{ success: true } | { error: string }>;
  extractSkillZip: (filePath: string) => Promise<{
    files: [string, string][];
    name: string;
    description: string;
    slug: string;
  } | { error: string }>;
  ```

  (The existing `installBrxtBundle` line directly after should remain unchanged.)

- [ ] **Step 2: Add IPC implementations in the `electronAPI` object**

  Find the implementation line (around line 301):
  ```typescript
  installBrxtBundle: (filePath: string, extensionName: string) =>
    ipcRenderer.invoke('brxt:install', { filePath, extensionName }),
  ```

  Add after it:
  ```typescript
  uninstallBrxtExtension: (extensionName: string) =>
    ipcRenderer.invoke('brxt:uninstall', { extensionName }),
  extractSkillZip: (filePath: string) =>
    ipcRenderer.invoke('skills:extract-zip', { filePath }),
  ```

- [ ] **Step 3: Type-check**

  ```bash
  cd ui/desktop && npm run typecheck
  ```

  Expected: no errors.

- [ ] **Step 4: Commit**

  ```bash
  git add ui/desktop/src/preload.ts
  git commit -m "feat(preload): expose uninstallBrxtExtension and extractSkillZip IPC bindings"
  ```

---

## Task 5: BrxtInstallModal.tsx — show skills preview

**Files:**
- Modify: `ui/desktop/src/components/BrxtInstallModal.tsx`

- [ ] **Step 1: Add `skillsPreview` state**

  Find the existing state declarations (around line 38–44). After `const [envEntries, setEnvEntries] = useState<EnvEntry[]>([]);`, add:

  ```typescript
  const [skillsPreview, setSkillsPreview] = useState<
    Array<{ slug: string; name: string; description: string }>
  >([]);
  ```

- [ ] **Step 2: Update `processFile` to capture skillsPreview**

  In `processFile` (around line 54–71), replace the `else` branch:

  ```typescript
  } else {
    setManifest(result.manifest);
    setSkillsPreview(result.skillsPreview);
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
  ```

- [ ] **Step 3: Add skills count to the info line in the manifest preview card**

  Find the info line inside the manifest preview card (around line 252–254):
  ```tsx
  <p className="text-xs text-text-muted mt-0.5">
    v{manifest.version}
    {manifest.tools_count ? ` · ${manifest.tools_count} tools` : ''}
    {' · '}
    {requiredVars.length} required env var
    {requiredVars.length !== 1 ? 's' : ''}
  </p>
  ```

  Replace with:
  ```tsx
  <p className="text-xs text-text-muted mt-0.5">
    v{manifest.version}
    {manifest.tools_count ? ` · ${manifest.tools_count} tools` : ''}
    {skillsPreview.length > 0
      ? ` · ${skillsPreview.length} skill${skillsPreview.length !== 1 ? 's' : ''}`
      : ''}
    {' · '}
    {requiredVars.length} required env var
    {requiredVars.length !== 1 ? 's' : ''}
  </p>
  ```

- [ ] **Step 4: Add skills preview list below the description in the manifest preview card**

  Find the description line in the manifest preview card (around line 258):
  ```tsx
  <p className="text-sm text-text-default mt-2">{manifest.description}</p>
  ```

  Add immediately after it:
  ```tsx
  {skillsPreview.length > 0 && (
    <div className="mt-2 pt-2 border-t border-border-subtle">
      <p className="text-xs font-semibold text-text-muted uppercase tracking-wide mb-1">
        Skills included
      </p>
      {skillsPreview.map((skill) => (
        <p key={skill.slug} className="text-xs text-text-muted leading-relaxed">
          · <span className="font-medium">{skill.name}</span> — {skill.description}
        </p>
      ))}
    </div>
  )}
  ```

- [ ] **Step 5: Type-check**

  ```bash
  cd ui/desktop && npm run typecheck
  ```

  Expected: no errors.

- [ ] **Step 6: Commit**

  ```bash
  git add ui/desktop/src/components/BrxtInstallModal.tsx
  git commit -m "feat(ui): show bundled skills preview in .brxt install modal"
  ```

---

## Task 6: ExtensionsSection.tsx — uninstall brxt extension files on deletion

**Files:**
- Modify: `ui/desktop/src/components/settings/extensions/ExtensionsSection.tsx`

- [ ] **Step 1: Add toastService import**

  At the top of `ExtensionsSection.tsx`, add to the existing imports:

  ```typescript
  import { toastService } from '../../../toasts';
  ```

- [ ] **Step 2: Replace `handleDeleteExtension`**

  Find `handleDeleteExtension` (around line 155–168) and replace its entire body:

  ```typescript
  const handleDeleteExtension = async (name: string) => {
    handleModalClose();

    // Detect .brxt-installed extensions by their --directory arg pointing to extensions/
    const config = extensionsList.find((e) => e.name === name);
    const isBrxtInstalled =
      config?.type === 'stdio' &&
      Array.isArray(config.args) &&
      config.args.some(
        (arg) => typeof arg === 'string' && arg.includes('biorouter/extensions/')
      );

    try {
      if (isBrxtInstalled) {
        const uninstallResult = await window.electron.uninstallBrxtExtension(name);
        if ('error' in uninstallResult) {
          toastService.error({
            title: name,
            msg: `Failed to remove extension files: ${uninstallResult.error}`,
          });
          return;
        }
      }
      await deleteExtension({
        name,
        removeFromConfig: removeExtension,
        extensionConfig: config,
      });
      if (isBrxtInstalled) {
        toastService.success({ title: name, msg: 'Extension and its skills removed' });
      }
    } catch (error) {
      console.error('Failed to delete extension:', error);
    } finally {
      await fetchExtensions();
    }
  };
  ```

- [ ] **Step 3: Type-check**

  ```bash
  cd ui/desktop && npm run typecheck
  ```

  Expected: no errors.

- [ ] **Step 4: Commit**

  ```bash
  git add ui/desktop/src/components/settings/extensions/ExtensionsSection.tsx
  git commit -m "feat(extensions): uninstall .brxt filesystem on extension deletion (removes skills)"
  ```

---

## Task 7: AddSkillModal.tsx — add ZIP import

**Files:**
- Modify: `ui/desktop/src/components/skills/AddSkillModal.tsx`

- [ ] **Step 1: Add ZIP ref and handler**

  After the existing `const folderInputRef = useRef<HTMLInputElement>(null);` (line 69), add:

  ```typescript
  const zipInputRef = useRef<HTMLInputElement>(null);
  ```

  After the `handleFolderBrowse` function (around line 124), add:

  ```typescript
  const processZipFile = async (file: File) => {
    const filePath = window.electron.getPathForFile(file);
    const result = await window.electron.extractSkillZip(filePath);
    if ('error' in result) {
      setError(result.error);
      setPreview(null);
      return;
    }
    setError(null);
    setPreview({
      name: result.name,
      description: result.description,
      slug: result.slug,
      files: result.files,
      label: file.name,
    });
  };

  const handleZipBrowse = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) await processZipFile(file);
    e.target.value = '';
  };
  ```

- [ ] **Step 2: Update drop handler to detect `.zip`**

  Replace the existing `handleDrop` function:

  ```typescript
  const handleDrop = (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setIsDragging(false);
    const file = e.dataTransfer.files[0];
    if (!file) return;
    if (file.name.endsWith('.zip')) {
      processZipFile(file);
    } else {
      processMdFile(file);
    }
  };
  ```

- [ ] **Step 3: Update drop zone label text**

  Find (around line 169):
  ```tsx
  <p className="text-sm text-text-muted">
    Drop a <code>.md</code> skill file here, or{' '}
    <span className="text-blue-600 underline">browse for file</span>
  </p>
  ```

  Replace with:
  ```tsx
  <p className="text-sm text-text-muted">
    Drop a <code>.md</code> or <code>.zip</code> skill file here, or{' '}
    <span className="text-blue-600 underline">browse for file</span>
  </p>
  ```

- [ ] **Step 4: Add ZIP browse button and hidden input**

  After the existing folder section (after the `<input ref={folderInputRef} .../>` at around line 201), add:

  ```tsx
  <Button
    variant="outline"
    className="w-full"
    onClick={() => zipInputRef.current?.click()}
  >
    Browse for Skill ZIP
  </Button>
  <p className="text-xs text-text-subtle -mt-2 text-center">
    ZIP must contain a <code>SKILL.md</code> file
  </p>
  <input
    ref={zipInputRef}
    type="file"
    accept=".zip"
    className="hidden"
    onChange={handleZipBrowse}
  />
  ```

- [ ] **Step 5: Type-check**

  ```bash
  cd ui/desktop && npm run typecheck
  ```

  Expected: no errors.

- [ ] **Step 6: Commit**

  ```bash
  git add ui/desktop/src/components/skills/AddSkillModal.tsx
  git commit -m "feat(skills): add ZIP file import to Add Skill modal"
  ```

---

## Task 8: Sample skills + E2E test for skills-in-brxt validation

**Files:**
- Create: `ui/desktop/tests/fixtures/skills/cdwagent/cdw-query-cohorts/SKILL.md`
- Create: `ui/desktop/tests/fixtures/skills/cdwagent/cdw-explore-schema/SKILL.md`
- Create: `ui/desktop/tests/fixtures/skills/ucsfomopagent/omop-phenotype-query/SKILL.md`
- Create: `ui/desktop/tests/fixtures/skills/spokeagent/spoke-knowledge-graph/SKILL.md`
- Modify: `ui/desktop/tests/e2e/brxt.spec.ts`

- [ ] **Step 1: Create sample skill files**

  Create `ui/desktop/tests/fixtures/skills/cdwagent/cdw-query-cohorts/SKILL.md`:
  ```markdown
  ---
  name: cdw-query-cohorts
  description: Systematically build and refine patient cohorts from CDW clinical data
  ---

  Use this skill when the user wants to identify a group of patients based on
  clinical criteria (diagnoses, medications, lab results, procedures, encounter types).

  ## When to activate
  - User mentions "cohort", "patient population", "inclusion criteria", "exclusion criteria"
  - User asks to find patients with specific conditions, treatments, or clinical characteristics

  ## Approach
  1. Clarify the clinical definition of the cohort with the user
  2. Use `CDW-get_available_tables` to identify relevant tables
  3. Use `CDW-explore_table_schema` on candidate tables
  4. Build an initial SQL query and validate with `CDW-execute_cdw_query`
  5. Iterate based on row counts and user feedback
  6. Present the final cohort size and sample records

  ## Notes
  - All queries are read-only against de-identified data
  - Prefer date range filters to keep queries fast
  - Always show the patient count before presenting full results
  ```

  Create `ui/desktop/tests/fixtures/skills/cdwagent/cdw-explore-schema/SKILL.md`:
  ```markdown
  ---
  name: cdw-explore-schema
  description: Explore the CDW clinical schema, tables, and column relationships
  ---

  Use this skill when the user wants to understand what data is available in the
  Clinical Data Warehouse before writing queries.

  ## When to activate
  - User asks "what tables are available", "what data do you have", "show me the schema"
  - User is unfamiliar with the CDW and needs orientation

  ## Approach
  1. Call `CDW-get_available_tables` to list all available tables with descriptions
  2. For tables of interest, call `CDW-explore_table_schema` to show columns and types
  3. Use `CDW-search_clinical_concepts` to find relevant medical codes (ICD, CPT, LOINC)
  4. Summarize the relevant tables and suggest next steps for the user's research goal

  ## Notes
  - The CDW schema reference is bundled — fast lookups without hitting the database
  - Always orient the user to what is de-identified vs. potentially re-identifiable
  ```

  Create `ui/desktop/tests/fixtures/skills/ucsfomopagent/omop-phenotype-query/SKILL.md`:
  ```markdown
  ---
  name: omop-phenotype-query
  description: Query the OMOP CDM to identify patient phenotypes and clinical concepts
  ---

  Use this skill when the user wants to define or query patient phenotypes using
  the OMOP Common Data Model at UCSF.

  ## When to activate
  - User asks about OMOP, phenotypes, standard concepts, or the OHDSI framework
  - User wants to count patients with specific conditions using standard vocabularies

  ## Approach
  1. Use `list_ucsf_omop_tables` to show the available OMOP domain tables
  2. Write a phenotyping query using standard OMOP concepts (condition_occurrence,
     drug_exposure, measurement, observation, procedure_occurrence)
  3. Run with `query_ucsf_omop` and present results with concept names
  4. Suggest refinements using concept ancestors for broader/narrower definitions

  ## Notes
  - OMOP uses standard concept IDs — avoid local source codes
  - All queries are read-only on de-identified data
  - concept_id = 0 means unmapped — filter these out for clean phenotypes
  ```

  Create `ui/desktop/tests/fixtures/skills/spokeagent/spoke-knowledge-graph/SKILL.md`:
  ```markdown
  ---
  name: spoke-knowledge-graph
  description: Traverse the SPOKE biomedical knowledge graph to find entity relationships
  ---

  Use this skill when the user wants to explore relationships between biomedical
  entities (diseases, genes, compounds, pathways, side effects) in the SPOKE graph.

  ## When to activate
  - User asks about connections between diseases, genes, drugs, or biological processes
  - User wants to find drug repurposing candidates, disease mechanisms, or biomarkers
  - User asks "what is related to X" or "how are X and Y connected"

  ## Approach
  1. Call `get_spoke_schema` to understand available node and edge types
  2. Identify the entities the user is asking about (use standard identifiers where possible)
  3. Write a Cypher query with `query_spoke` to traverse the relevant subgraph
  4. Explain the biological meaning of each relationship type found
  5. Summarize findings with the most biologically relevant connections highlighted

  ## Notes
  - SPOKE integrates: OMIM, DrugBank, DisGeNET, SIDER, Reactome, UniProt, and more
  - Node labels: Disease, Gene, Compound, Pathway, SideEffect, Anatomy, etc.
  - Limit Cypher queries to avoid full-graph traversals (use LIMIT and specific labels)
  ```

- [ ] **Step 2: Write the failing E2E test**

  In `ui/desktop/tests/e2e/brxt.spec.ts`, add after the existing `VALID_MANIFEST_NO_ENV` constant (around line 62) and after `createInvalidBrxt`:

  ```typescript
  /** Create a .brxt with bundled skills for testing. */
  function createValidBrxtWithSkills(manifest: object, skills: Array<{ slug: string; name: string; description: string }>, outPath: string): void {
    const zip = new AdmZip();
    zip.addFile('manifest.json', Buffer.from(JSON.stringify(manifest)));
    zip.addFile('README.md', Buffer.from('# Test extension'));
    zip.addFile('pyproject.toml', Buffer.from('[project]\nname = "test"\nversion = "0.1.0"'));
    zip.addFile('src/__init__.py', Buffer.from(''));
    for (const skill of skills) {
      zip.addFile(
        `skills/${skill.slug}/SKILL.md`,
        Buffer.from(`---\nname: ${skill.name}\ndescription: ${skill.description}\n---\n\nSkill body.`)
      );
    }
    zip.writeZip(outPath);
  }
  ```

  Then add this test in the test suite (after the existing tests):

  ```typescript
  test('brxt:validate-and-read returns skillsPreview for bundle with skills', async () => {
    const brxtPath = path.join(tmpDir, 'with-skills.brxt');
    createValidBrxtWithSkills(
      VALID_MANIFEST_NO_ENV,
      [
        { slug: 'cdw-query-cohorts', name: 'cdw-query-cohorts', description: 'Build patient cohorts' },
        { slug: 'cdw-explore-schema', name: 'cdw-explore-schema', description: 'Explore CDW schema' },
      ],
      brxtPath
    );

    const result = await electronApp.evaluate(
      async ({ ipcMain: _ipcMain }, fp) => {
        const { ipcMain } = await import('electron');
        return new Promise<unknown>((resolve) => {
          ipcMain.emit('brxt:validate-and-read', { sender: {} }, { filePath: fp });
          // Use direct handler invocation via app.evaluate
          resolve(null);
        });
      },
      brxtPath
    );

    // Prefer testing via the page IPC bridge
    await page.evaluate((fp) => window.electron.validateBrxtBundle(fp), brxtPath);
    const validateResult = await page.evaluate(
      (fp) => (window as unknown as { electron: { validateBrxtBundle: (fp: string) => Promise<unknown> } }).electron.validateBrxtBundle(fp),
      brxtPath
    );

    expect(validateResult).not.toHaveProperty('error');
    const typed = validateResult as { manifest: object; skillsPreview: Array<{ slug: string; name: string; description: string }> };
    expect(typed.skillsPreview).toHaveLength(2);
    expect(typed.skillsPreview[0].slug).toBe('cdw-query-cohorts');
    expect(typed.skillsPreview[1].slug).toBe('cdw-explore-schema');
    expect(result).toBeNull(); // suppress unused var warning
  });
  ```

  > **Note:** The E2E test directly calls `window.electron.validateBrxtBundle` via `page.evaluate`, matching the pattern in the rest of `brxt.spec.ts`. If the test harness doesn't expose `window.electron` in evaluate context, use the `electronApp.evaluate` approach to invoke the IPC handler directly (see `brxt.spec.ts` existing patterns for the exact invocation style).

- [ ] **Step 3: Run the E2E test**

  ```bash
  cd ui/desktop && npm run test-e2e -- --grep "skillsPreview"
  ```

  Expected: test passes if the `brxt:validate-and-read` handler now returns `skillsPreview`.

- [ ] **Step 4: Commit**

  ```bash
  git add ui/desktop/tests/fixtures/ ui/desktop/tests/e2e/brxt.spec.ts
  git commit -m "test: add sample skills for CDWAgent/UCSFOMOPAgent/SPOKEAgent and brxt skills E2E test"
  ```

---

## Task 9: Final build verification

- [ ] **Step 1: Run full Rust test suite**

  ```bash
  cargo test -p biorouter
  ```

  Expected: all tests pass.

- [ ] **Step 2: Run TypeScript type-check**

  ```bash
  cd ui/desktop && npm run typecheck
  ```

  Expected: no errors.

- [ ] **Step 3: Run frontend unit tests**

  ```bash
  cd ui/desktop && npm run test:run
  ```

  Expected: all pass.

- [ ] **Step 4: Run linter**

  ```bash
  cd ui/desktop && npm run lint:check
  ```

  Expected: no errors. If there are fixable warnings, run `npm run lint` and commit.

- [ ] **Step 5: Commit lint fixes if any**

  ```bash
  git add -p
  git commit -m "chore: fix lint warnings from brxt skills integration"
  ```

- [ ] **Step 6: Final commit summary**

  ```bash
  git log --oneline -8
  ```

  Expected output (newest first):
  ```text
  chore: fix lint warnings from brxt skills integration (if needed)
  test: add sample skills for CDWAgent/UCSFOMOPAgent/SPOKEAgent and brxt skills E2E test
  feat(skills): add ZIP file import to Add Skill modal
  feat(extensions): uninstall .brxt filesystem on extension deletion (removes skills)
  feat(ui): show bundled skills preview in .brxt install modal
  feat(preload): expose uninstallBrxtExtension and extractSkillZip IPC bindings
  feat(ipc): extend brxt:validate-and-read with skills preview; add brxt:uninstall and skills:extract-zip handlers
  feat(types): add BrxtSkillMeta and optional skills field to BrxtManifest
  feat(skills): discover skills from installed .brxt extension directories
  ```

---

## Self-review against the design spec

Every element of the [design spec](brxt-bundled-skills-design.md) maps to a task in this plan:

| Design element | Covered by |
|---|---|
| `.brxt` format extension (`skills/` dir + `manifest.json` skills array) | Tasks 2, 3, 5 |
| Skills storage strategy (`extensions/<name>/skills/`) | Task 1 (Rust discovery) |
| `brxt:validate-and-read` extended | Task 3 |
| `brxt:install` unchanged (zip extraction already lands skills naturally) | No task needed |
| `brxt:uninstall` new handler | Task 3 |
| `BrxtInstallModal` skills preview | Task 5 |
| Extension removal uninstalls filesystem | Task 6 |
| Skill import ZIP support | Task 7 |
| Test fixtures (4 `SKILL.md` files) | Task 8 |
| E2E test for `skillsPreview` | Task 8 |

Placeholder scan: no TBDs or incomplete steps found.

Type consistency:

- `skillsPreview: Array<{ slug: string; name: string; description: string }>` used consistently across Tasks 3, 4, 5.
- `{ success: true as const }` in `brxt:uninstall` matches `{ success: true }` in the preload type.
- `files: [string, string][]` in `skills:extract-zip` matches `Preview.files` in `AddSkillModal.tsx`.

## Related documentation

- [.brxt bundled skills design](brxt-bundled-skills-design.md) — the approved spec this plan executes; read it first for the rationale behind extension-local skill storage.
- [Skill bundles implementation plan](skill-bundles-plan.md) — the sibling plan from the same day; it later extends the `skills:extract-zip` handler added in Task 3 to detect multi-skill bundle ZIPs.
- [Skill bundles design](skill-bundles-design.md) — defines what a skill *bundle* is, the concept layered on top of this work.
- [Skills extension](../../extensions/built-in/skills.md) — the user-facing view of the skill discovery paths this plan extended.
