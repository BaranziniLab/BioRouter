# v1.72.1 bug fix batch

> **What this is.** The implementation plan for the four fixes batched into the BioRouter v1.72.1 release: runtime enforcement of disabled skills in the Rust agent, plus a drop-zone-only redesign of the Import Session, Import Workflow, and Add Skill modals.
> **Status:** Historical record — all four fixes were implemented and shipped in v1.72.1 (May 2026). The runtime disabled-skill check is present in `crates/biorouter/src/agents/skills_extension.rs` today, and the three modals are drop-zone-only in the current desktop build. The repository has since moved well past v1.72.1, so treat the code samples below as a record of what was written then, not as current source.
> **Audience:** agents and maintainers reconstructing why the skills-enforcement and import-modal code looks the way it does.

The four items in this batch share nothing but a release train. Two unrelated concerns are batched here because they were the outstanding bug reports when v1.72.1 was cut: one Rust change in the skills extension, and three React modal refactors in the desktop UI. Read the tasks independently — Task 1 stands alone, and Tasks 2 through 4 apply the same drop-zone pattern to three different modals.

Version numbers here (`v1.72.1`) are BioRouter release tags; release notes for later versions live in `docs/releases/notes/`. No `v1.72.1.md` was ever written there, so the release-notes block at the end of Task 5 is the only surviving record of what that release announced.

**Goal.** Fix four bugs:

1. Disabled skills remain loadable in chat mid-session.
2. The Import Session modal offers a path input alongside its drop zone.
3. The Import Workflow modal offers a path input alongside its drop zone.
4. The Add Skill modal offers separate browse buttons alongside its drop zone.

Items 2 through 4 should all use the `BrxtInstallModal` drop-zone UI pattern, with no path input text area.

> **Note.** The v1.72.1 release also shipped two fixes that this plan does not cover — a GPT-5.5 routing fix and institutional provider visibility — plus a Current Model re-render fix. All three appear only in the release-notes block in Task 5.

**Architecture.** Backend skill enforcement via runtime `get_disabled_skills()` checks in `skills_extension.rs`. Frontend UI refactor to remove the `"or"` divider and path input block from three modals, making click-on-drop-zone the only file selector.

**Tech stack.** Rust (tokio/async), React 19/TypeScript, Electron IPC.

## Files modified

| File | Change |
|---|---|
| `crates/biorouter/src/agents/skills_extension.rs` | Runtime disabled check in `handle_load_skill`; dynamic disabled filter in `generate_instructions` |
| `ui/desktop/src/components/sessions/ImportSessionModal.tsx` | Remove path input section; keep only drop zone |
| `ui/desktop/src/components/workflows/ImportWorkflowForm.tsx` | Remove path input section; keep only drop zone |
| `ui/desktop/src/components/skills/AddSkillModal.tsx` | Remove browse button row; clicking drop area triggers file picker |

## Task 1: Enforce disabled skills at runtime in the backend

**Files:**
- Modify: `crates/biorouter/src/agents/skills_extension.rs`

### Context

`SkillsClient::new()` filters out disabled skills at init time. But mid-session, if the user disables a skill, the running `SkillsClient` still has it. The LLM can still call `loadSkill` to retrieve it.

Fix: Keep ALL discovered skills in `self.skills` at init. Filter disabled skills (1) in `generate_instructions` at session-start time (so new sessions get correct instructions) and (2) in `handle_load_skill` at call time (so mid-session disabling is enforced).

**Step 1: Remove the disabled filter from `SkillsClient::new()`**

In `crates/biorouter/src/agents/skills_extension.rs`, find the `new()` function (around line 69) and change:

```rust
let skills = Self::discover_skills_in_directories(&directories);
let disabled = Self::get_disabled_skills();
let skills = skills
    .into_iter()
    .filter(|(name, skill)| {
        !disabled.contains(name)
            && !skill
                .bundle_name
                .as_deref()
                .is_some_and(|b| disabled.contains(b))
    })
    .collect();
```

To (remove the filter, keep all skills):

```rust
let skills = Self::discover_skills_in_directories(&directories);
```

**Step 2: Add disabled filter to `generate_instructions`**

Find `generate_instructions` (around line 256). Change it to dynamically filter:

```rust
fn generate_instructions(&self) -> String {
    if self.skills.is_empty() {
        return String::new();
    }

    let disabled = Self::get_disabled_skills();
    let mut skill_list: Vec<_> = self.skills.iter()
        .filter(|(name, skill)| {
            !disabled.contains(*name)
                && !skill.bundle_name.as_deref().is_some_and(|b| disabled.contains(b))
        })
        .collect();

    if skill_list.is_empty() {
        return String::new();
    }

    let mut instructions = String::from(
        "You have these skills at your disposal, when it is clear they can help you solve a problem or you are asked to use them:\n\n"
    );
    skill_list.sort_by_key(|(name, _)| *name);
    for (name, skill) in skill_list {
        instructions.push_str(&format!("- {}: {}\n", name, skill.metadata.description));
    }
    instructions
}
```

**Step 3: Add runtime disabled check in `handle_load_skill`**

Find `handle_load_skill` (around line 273). Add a disabled check before returning skill content:

```rust
async fn handle_load_skill(
    &self,
    arguments: Option<JsonObject>,
) -> Result<Vec<Content>, String> {
    let skill_name = arguments
        .as_ref()
        .ok_or("Missing arguments")?
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: name")?;

    // Runtime check: reject disabled skills even mid-session
    let disabled = Self::get_disabled_skills();
    if let Some(skill) = self.skills.get(skill_name) {
        let is_disabled = disabled.contains(skill_name)
            || skill.bundle_name.as_deref().is_some_and(|b| disabled.contains(b));
        if is_disabled {
            return Err(format!(
                "Skill '{}' is currently disabled. Enable it in BioRouter's Skills settings to use it.",
                skill_name
            ));
        }
    }

    let skill = self
        .skills
        .get(skill_name)
        .ok_or_else(|| format!("Skill '{}' not found", skill_name))?;

    let mut response = format!("# Skill: {}\n\n{}\n\n", skill.metadata.name, skill.body);

    if !skill.supporting_files.is_empty() {
        response.push_str(&format!(
            "## Supporting Files\n\nSkill directory: {}\n\n",
            skill.directory.display()
        ));
        response.push_str("The following supporting files are available:\n");
        for file in &skill.supporting_files {
            if let Ok(relative) = file.strip_prefix(&skill.directory) {
                response.push_str(&format!("- {}\n", relative.display()));
            }
        }
        response.push_str("\nUse the view file tools to access these files as needed, or run scripts as directed with dev extension.\n");
    }

    Ok(vec![Content::text(response)])
}
```

**Step 4: Also update `list_tools` to exclude disabled skills**

The `list_tools` returns only a `loadSkill` tool (no per-skill tools), but we should also hide that tool entirely if all skills are disabled:

```rust
async fn list_tools(
    &self,
    _next_cursor: Option<String>,
    _cancellation_token: CancellationToken,
) -> Result<ListToolsResult, Error> {
    let disabled = Self::get_disabled_skills();
    let has_enabled_skills = self.skills.iter().any(|(name, skill)| {
        !disabled.contains(name)
            && !skill.bundle_name.as_deref().is_some_and(|b| disabled.contains(b))
    });
    let tools = if has_enabled_skills {
        Self::get_tools()
    } else {
        Vec::new()
    };
    Ok(ListToolsResult {
        tools,
        next_cursor: None,
        meta: None,
    })
}
```

**Step 5: Build and verify**

```bash
source bin/activate-hermit
cargo build -p biorouter 2>&1 | tail -5
```

Expected: `Finished` with no errors.

**Step 6: Run existing tests to confirm no regressions**

```bash
source bin/activate-hermit
cargo test -p biorouter --lib agents::skills_extension 2>&1 | tail -10
```

Expected: all tests pass.

**Step 7: Commit**

```bash
git add crates/biorouter/src/agents/skills_extension.rs
git commit -m "fix(skills): enforce disabled skills at runtime in backend loadSkill handler"
```

## Task 2: Redesign the Import Session modal

**Files:**
- Modify: `ui/desktop/src/components/sessions/ImportSessionModal.tsx`

### Context

The modal has a drop zone, a divider, a path input field, and a Load button. The user wants only the drop zone (click to browse). Remove the `"or"` divider, path input, and the browse/Load buttons below it.

**Step 1: Remove path state, browse handler, path import handler**

Replace the entire file with this content:

```tsx
import React, { useState, useCallback, useRef } from 'react';
import { Upload } from '../icons/app-icons';
import { Button } from '../ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '../ui/dialog';

interface ImportSessionModalProps {
  isOpen: boolean;
  onClose: () => void;
  onImport: (json: string) => Promise<void>;
}

export function ImportSessionModal({ isOpen, onClose, onImport }: ImportSessionModalProps) {
  const [isDragging, setIsDragging] = useState(false);
  const [error, setError] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const reset = () => {
    setError('');
    setIsDragging(false);
    setIsSubmitting(false);
  };

  const handleClose = () => {
    reset();
    onClose();
  };

  const processFile = useCallback(
    async (file: File) => {
      if (!file.name.endsWith('.json') && file.type !== 'application/json') {
        setError('Please provide a JSON file.');
        return;
      }
      setError('');
      setIsSubmitting(true);
      try {
        const json = await file.text();
        JSON.parse(json);
        await onImport(json);
        reset();
        onClose();
      } catch (e) {
        setError(e instanceof SyntaxError ? 'Invalid JSON file.' : String(e));
        setIsSubmitting(false);
      }
    },
    [onImport, onClose]
  );

  const handleDrop = useCallback(
    (e: React.DragEvent<HTMLDivElement>) => {
      e.preventDefault();
      setIsDragging(false);
      const file = e.dataTransfer.files[0];
      if (file) processFile(file);
    },
    [processFile]
  );

  const handleFileInputChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) processFile(file);
      e.target.value = '';
    },
    [processFile]
  );

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && handleClose()}>
      <DialogContent className="sm:max-w-[480px]">
        <DialogHeader>
          <DialogTitle>Import Session</DialogTitle>
          <DialogDescription>
            Drag and drop a session JSON file, or click to browse.
          </DialogDescription>
        </DialogHeader>

        <div
          onDragOver={(e) => { e.preventDefault(); setIsDragging(true); }}
          onDragLeave={() => setIsDragging(false)}
          onDrop={handleDrop}
          onClick={() => !isSubmitting && fileInputRef.current?.click()}
          className={[
            'flex flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed py-10 cursor-pointer transition-colors duration-150 select-none',
            isDragging
              ? 'border-[#cf6d47] bg-[#cf6d47]/5'
              : error
              ? 'border-red-400 bg-red-50 dark:bg-red-900/10'
              : 'border-border-subtle bg-background-muted hover:border-border-strong hover:bg-background-medium',
          ].join(' ')}
        >
          <input
            ref={fileInputRef}
            type="file"
            accept=".json,application/json"
            onChange={handleFileInputChange}
            className="hidden"
          />
          {isSubmitting ? (
            <p className="text-sm text-text-muted animate-pulse">Importing…</p>
          ) : (
            <>
              <Upload className="w-8 h-8 text-text-muted" />
              <p className="text-sm font-medium text-text-default">Drop a JSON file here</p>
              <p className="text-xs text-text-muted">or click to browse</p>
            </>
          )}
        </div>

        {error && (
          <p className="text-sm text-red-500 dark:text-red-400">{error}</p>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={handleClose} disabled={isSubmitting}>
            Cancel
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

**Step 2: Run type-check**

```bash
cd ui/desktop && npm run typecheck 2>&1 | tail -10
```

Expected: no errors.

**Step 3: Commit**

```bash
git add ui/desktop/src/components/sessions/ImportSessionModal.tsx
git commit -m "fix(ui): simplify Import Session modal — drop zone only, no path input"
```

## Task 3: Redesign the Import Workflow modal

**Files:**
- Modify: `ui/desktop/src/components/workflows/ImportWorkflowForm.tsx`

### Context

Same treatment as the Import Session modal in Task 2: remove the `"or"` divider and path input section. Keep only the drop zone.

**Step 1: Rewrite the `ImportWorkflowForm` component**

Replace the function body of `ImportWorkflowForm` (lines 39–232) with:

```tsx
export default function ImportWorkflowForm({ isOpen, onClose, onSuccess }: ImportWorkflowFormProps) {
  const [isDragging, setIsDragging] = useState(false);
  const [error, setError] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const reset = () => {
    setError('');
    setIsDragging(false);
    setIsSubmitting(false);
  };

  const handleClose = () => {
    reset();
    onClose();
  };

  const processContent = useCallback(
    async (content: string, filename: string) => {
      const isYaml = /\.(ya?ml)$/i.test(filename);
      const isJson = /\.json$/i.test(filename);
      if (!isYaml && !isJson) {
        setError('Please provide a YAML or JSON workflow file.');
        return;
      }
      setError('');
      setIsSubmitting(true);
      try {
        const workflow = await parseWorkflowFromFile(content);
        await saveWorkflow(workflow, null);
        reset();
        onClose();
        onSuccess();
        toastSuccess({ title: workflow.title?.trim() || 'Workflow', msg: 'Workflow imported successfully' });
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        setIsSubmitting(false);
      }
    },
    [onClose, onSuccess]
  );

  const processFile = useCallback(
    async (file: File) => {
      const content = await file.text();
      await processContent(content, file.name);
    },
    [processContent]
  );

  const handleDrop = useCallback(
    (e: React.DragEvent<HTMLDivElement>) => {
      e.preventDefault();
      setIsDragging(false);
      const file = e.dataTransfer.files[0];
      if (file) processFile(file);
    },
    [processFile]
  );

  const handleFileInputChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) processFile(file);
      e.target.value = '';
    },
    [processFile]
  );

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && handleClose()}>
      <DialogContent className="sm:max-w-[480px]">
        <DialogHeader>
          <DialogTitle>Import Workflow</DialogTitle>
          <DialogDescription>
            Drag and drop a workflow YAML or JSON file, or click to browse.
          </DialogDescription>
        </DialogHeader>

        <div
          onDragOver={(e) => { e.preventDefault(); setIsDragging(true); }}
          onDragLeave={() => setIsDragging(false)}
          onDrop={handleDrop}
          onClick={() => !isSubmitting && fileInputRef.current?.click()}
          className={[
            'flex flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed py-10 cursor-pointer transition-colors duration-150 select-none',
            isDragging
              ? 'border-[#cf6d47] bg-[#cf6d47]/5'
              : error
              ? 'border-red-400 bg-red-50 dark:bg-red-900/10'
              : 'border-border-subtle bg-background-muted hover:border-border-strong hover:bg-background-medium',
          ].join(' ')}
        >
          <input
            ref={fileInputRef}
            type="file"
            accept=".yaml,.yml,.json"
            onChange={handleFileInputChange}
            className="hidden"
          />
          {isSubmitting ? (
            <p className="text-sm text-text-muted animate-pulse">Importing…</p>
          ) : (
            <>
              <Upload className="w-8 h-8 text-text-muted" />
              <p className="text-sm font-medium text-text-default">Drop a YAML or JSON file here</p>
              <p className="text-xs text-text-muted">or click to browse</p>
            </>
          )}
        </div>

        {error && (
          <p className="text-sm text-red-500 dark:text-red-400">{error}</p>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={handleClose} disabled={isSubmitting}>
            Cancel
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

Also remove unused imports: `Folder` from `app-icons`. Keep `Upload`, `Download`, `Button`, Dialog parts, `toastSuccess`, `saveWorkflow`, `parseWorkflow`, `Workflow`.

**Step 2: Run type-check**

```bash
cd ui/desktop && npm run typecheck 2>&1 | tail -10
```

Expected: no errors.

**Step 3: Commit**

```bash
git add ui/desktop/src/components/workflows/ImportWorkflowForm.tsx
git commit -m "fix(ui): simplify Import Workflow modal — drop zone only, no path input"
```

## Task 4: Redesign the Add Skill modal

**Files:**
- Modify: `ui/desktop/src/components/skills/AddSkillModal.tsx`

### Context

The modal has separate folder picker and ZIP file picker buttons. The user wants clicking the drop area to be the only way to trigger the file picker — no separate "Browse folder" / "Browse ZIP" buttons. Keep the drag-and-drop plus click-to-open-picker pattern, matching `BrxtInstallModal`.

**Step 1: Read the current `AddSkillModal` to identify the browse buttons**

```bash
grep -n "Browse\|button\|onClick.*picker\|selectFolder\|openFile" ui/desktop/src/components/skills/AddSkillModal.tsx | head -30
```

**Step 2: Remove the separate browse/picker button row**

In `ui/desktop/src/components/skills/AddSkillModal.tsx`, find the section that renders standalone browse buttons (separate from the drop zone). Remove that section. The drop zone's `onClick` handler already triggers the file picker — no separate buttons needed.

After removing the buttons, verify the drop zone still has `onClick={() => fileInputRef.current?.click()}` (or equivalent for both folder and ZIP inputs).

**Step 3: Run type-check**

```bash
cd ui/desktop && npm run typecheck 2>&1 | tail -10
```

Expected: no errors.

**Step 4: Commit**

```bash
git add ui/desktop/src/components/skills/AddSkillModal.tsx
git commit -m "fix(ui): simplify Add Skill modal — remove separate browse buttons, click drop zone to pick"
```

## Task 5: Build and release v1.72.1

Run every command in this task from the repository root unless a step says otherwise.

**Step 1: Build macOS ARM64 release binary**

```bash
source bin/activate-hermit
cargo build --release 2>&1 | tail -5
strip -x target/release/biorouter target/release/biorouterd
cp target/release/biorouter ui/desktop/src/bin/biorouter
cp target/release/biorouterd ui/desktop/src/bin/biorouterd
```

Expected: binaries around 97MB and 86MB.

**Step 2: Push all commits to main**

```bash
git push origin main
```

**Step 3: Build and notarize macOS ARM64 package**

```bash
cd ui/desktop
APPLE_ID=<apple-id> APPLE_APP_SPECIFIC_PASSWORD=<app-specific-password> npm run make -- --targets @electron-forge/maker-zip
cd out/BioRouter-darwin-arm64 && ditto -c -k --sequesterRsrc --keepParent BioRouter.app BioRouter.zip
```

**Step 4: Build macOS Intel package**

```bash
source bin/activate-hermit
just release-intel  # builds target/x86_64-apple-darwin/release/ binaries
strip -x target/x86_64-apple-darwin/release/biorouter target/x86_64-apple-darwin/release/biorouterd
cp target/x86_64-apple-darwin/release/biorouter ui/desktop/src/bin/biorouter
cp target/x86_64-apple-darwin/release/biorouterd ui/desktop/src/bin/biorouterd
cd ui/desktop
APPLE_ID=<apple-id> APPLE_APP_SPECIFIC_PASSWORD=<app-specific-password> npm run make -- --arch=x64 --targets @electron-forge/maker-zip
```

**Step 5: Build Linux packages (Docker)**

Restore ARM64 binaries first, then build Linux:

```bash
cp target/release/biorouter ui/desktop/src/bin/biorouter
cp target/release/biorouterd ui/desktop/src/bin/biorouterd
just make-ui-linux
```

**Step 6: Build Windows package**

```bash
cd ui/desktop && npm run bundle:windows
```

**Step 7: Create GitHub release v1.72.1**

```bash
gh release create v1.72.1 \
  "ui/desktop/out/BioRouter-darwin-arm64/BioRouter.zip#BioRouter-macOS-arm64.zip" \
  "ui/desktop/out/make/zip/darwin/x64/BioRouter-darwin-x64-1.72.1.zip#BioRouter-macOS-intel.zip" \
  --title "BioRouter v1.72.1" \
  --notes "$(cat <<'EOF'
## Bug Fixes

- **GPT-5.5 fix:** Resolved 400 error when using GPT-5.5 via OpenAI (reasoning_effort was incorrectly sent to /v1/chat/completions; these models require /v1/responses)
- **Institutional models:** UCSF Versa Azure and Versa Bedrock providers now correctly appear in Provider Configuration
- **Disabled skills enforcement:** Disabling a skill in BioRouter's Skills settings now prevents the LLM from loading it mid-session (backend-level enforcement)
- **Current Model UI:** Fixed blinking/infinite re-render in the Switch Models settings panel
- **Import modals:** Simplified Import Session, Import Workflow, and Add Skill modals — clean drop-zone UI, no path input field
EOF
)"
```

> **Note.** Later releases record their notes as a file under `docs/releases/notes/` rather than inline in a `gh release create` heredoc. This block is preserved because no `v1.72.1.md` was ever written there.

## Related documentation

- [Skill bundles plan](../skills-packaging/skill-bundles-plan.md) — the bundle packaging work that introduced the `bundle_name` field this task's disabled-skill filter checks.
- [BRXT bundled skills plan](../skills-packaging/brxt-bundled-skills-plan.md) — how skills reach the Add Skill modal in the first place, via `.brxt` extension bundles.
- [Skills extension reference](../../extensions/built-in/skills.md) — the current behaviour of the skills extension, superseding the code samples in Task 1.
- [Versa institutional providers plan](../institutional-providers/versa-providers-plan.md) — the UCSF Versa provider work whose fix also shipped in v1.72.1.
- [Desktop reliability defects](../subsystem-reviews-2026/desktop-reliability-defects.md) — a later, broader sweep of desktop UI defects in the same subsystem.
