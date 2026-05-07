# Skill Bundles Design

## Overview

BioRouter currently supports single-skill directories (one folder → one `SKILL.md`). This spec adds support for **skill bundles**: a parent folder containing multiple sub-skill directories, each with its own `SKILL.md`. The bundle is treated as a single installable unit with a single on/off toggle.

**Motivating example:** `https://github.com/obra/superpowers/tree/main/skills` — a repo whose `skills/` directory contains dozens of sub-skill folders, each with `SKILL.md`.

---

## Section 1: Data Model

### TypeScript (`skillUtils.ts`)

**`Skill` interface gains one optional field:**

```typescript
export interface Skill {
  name: string;
  description: string;
  content: string;
  filePath: string;
  sourceDir: string;
  enabled: boolean;
  bundleName?: string;   // set when skill belongs to a bundle
}
```

**New `SkillBundle` interface:**

```typescript
export interface SkillBundle {
  bundleName: string;   // folder name; also the key used in the disabled list
  folderPath: string;   // absolute path to the bundle root folder
  sourceDir: string;    // parent skills directory (e.g. ~/.config/biorouter/skills)
  skills: Skill[];      // all sub-skills in the bundle
  enabled: boolean;     // true when bundleName is NOT in the disabled list
}
```

**`loadSkillsFromDirs()` return type changes:**

```typescript
async function loadSkillsFromDirs(dirs: string[]): Promise<{
  singles: Skill[];
  bundles: SkillBundle[];
}>
```

All callers (SkillsView, BottomMenuSkillSelection, skills IPC handler) are updated to destructure `{ singles, bundles }`.

### `skills-config.json` disabled list

No schema change. The `disabled` array holds names — both single skill names and bundle names coexist as flat strings. A bundle is disabled by adding its `bundleName` to the array. Individual sub-skill names are never written to the disabled list.

### Rust (`skills_extension.rs`)

**`Skill` struct gains one field:**

```rust
pub struct Skill {
    pub metadata: SkillMetadata,
    pub content: String,
    pub file_path: PathBuf,
    pub bundle_name: Option<String>,
}
```

**Disabled check becomes:**

```rust
let is_disabled = disabled.contains(&skill.metadata.name)
    || skill.bundle_name.as_deref()
        .map_or(false, |b| disabled.contains(b));
```

---

## Section 2: Discovery Logic

### Detection rule

When scanning a skills directory entry `<slug>`:

1. If `<dir>/<slug>/SKILL.md` exists → **single skill** (current behavior, unchanged).
2. Else if `<dir>/<slug>/` is a directory and contains at least one sub-directory `<name>/SKILL.md` → **bundle**. The bundle name is `<slug>`.
3. Otherwise → ignore.

No new file format. No new IPC handlers.

### TypeScript scan (`loadSkillsFromDirs`)

```typescript
for (const entry of fs.readdirSync(dir)) {
  const entryPath = path.join(dir, entry);
  const singleSkillMd = path.join(entryPath, 'SKILL.md');

  if (fs.existsSync(singleSkillMd)) {
    // single skill — existing logic
    singles.push(loadSingleSkill(entryPath, dir));
  } else if (fs.statSync(entryPath).isDirectory()) {
    // check for bundle
    const subSkills = fs.readdirSync(entryPath)
      .map(sub => path.join(entryPath, sub, 'SKILL.md'))
      .filter(p => fs.existsSync(p));

    if (subSkills.length > 0) {
      bundles.push(loadBundle(entry, entryPath, dir, subSkills, disabled));
    }
  }
}
```

### Rust scan

Same two-level detection in `skills_extension.rs`: try `<slug>/SKILL.md` first; if absent and `<slug>/*/SKILL.md` exists, create bundle entries with `bundle_name: Some(slug.to_string())`.

---

## Section 3: UI Changes

### SkillsView (Settings page)

Bundle rows render in the same list as single-skill rows (alphabetical by name/bundle name).

**Bundle row:**
- Bold bundle name + muted sub-skill count badge (e.g., "superpowers · 12 skills")
- One `Switch` toggle — checked when `bundleName` is NOT in the disabled list; toggling writes/removes `bundleName` from `skills-config.json`
- Read-only list of sub-skill names in muted small text below (always visible, no expand/collapse)
- No individual sub-skill toggles

**Single-skill rows:** unchanged from today.

### BottomMenuSkillSelection (chat bar dropdown)

Bundles appear as single entries in the skill picker dropdown:
- Primary line: bundle name
- Secondary line (muted): comma-separated sub-skill names, truncated with "…" if long
- Clicking toggles the whole bundle on/off (adds/removes `bundleName` from the active set)

The active-skill count badge on the chat bar button counts each bundle as 1.

---

## Section 4: Installation

### ZIP import (AddSkillModal)

After extracting a ZIP, detection runs on the extracted folder:
- Root `SKILL.md` found → single skill install (existing path)
- No root `SKILL.md` + sub-dirs with `SKILL.md` → bundle install

For bundle installs the modal shows a preview before confirming:
> "Bundle detected: **superpowers** — 12 skills included"
> [scrollable list of sub-skill names]

The bundle folder is moved as-is into the target skills directory, preserving all sub-directories.

### Directory picker / folder drag-drop

Same detection logic applied to the picked/dropped folder. The folder is copied into the skills dir under its folder name (e.g., `skills/superpowers/`).

### Uninstall

Uninstalling a bundle removes the entire bundle folder (`rm -rf skills/<bundleName>/`). The bundle name is removed from the disabled list on uninstall (same behavior as single-skill uninstall today). Per-sub-skill uninstall is not supported.

---

## Out of Scope

- Per-sub-skill enable/disable toggles
- Partial bundle install (user selects which sub-skills to include)
- Nested bundles (bundles of bundles)
- Remote bundle install from URL (only ZIP and folder picker)
