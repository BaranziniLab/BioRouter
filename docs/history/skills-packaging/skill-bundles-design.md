# Skill bundles design

> **What this is.** The design spec for *skill bundles*: treating a parent folder that holds several sub-skill directories as one installable unit with a single on/off toggle, across the TypeScript and Rust skill scanners and the settings UI.
> **Status:** Historical record — implemented. `bundle_name: Option<String>` is a field on the Rust `Skill` struct in `crates/biorouter/src/agents/skills_extension.rs`, the two-level scan in `discover_skills_in_directories` matches the detection rule below, and `SkillBundle` is defined in `ui/desktop/src/components/skills/skillUtils.ts`.
> **Audience:** developers working on skill discovery and the skills settings UI.

A *skill* in BioRouter is a folder containing a `SKILL.md` file — YAML frontmatter plus a markdown body — that the agent loads as procedural guidance. Until this change, discovery was one level deep: one folder meant exactly one skill. Skill collections published as a single repository therefore arrived as dozens of unrelated entries in the skills list, each with its own toggle.

This spec was written on 2026-05-07 alongside the [`.brxt` bundled skills design](brxt-bundled-skills-design.md). The two overlap at the ZIP import path — that spec added ZIP import, this one teaches it to recognise a bundle — but they are otherwise independent features. The task-by-task execution record is in the [companion implementation plan](skill-bundles-plan.md).

## Overview

BioRouter currently supports single-skill directories (one folder → one `SKILL.md`). This spec adds support for **skill bundles**: a parent folder containing multiple sub-skill directories, each with its own `SKILL.md`. The bundle is treated as a single installable unit with a single on/off toggle.

The motivating example is the `superpowers` skills collection at `https://github.com/obra/superpowers/tree/main/skills` — a public repository whose `skills/` directory contains dozens of sub-skill folders, each holding its own `SKILL.md`. Installed under today's one-level rule it would produce dozens of separate rows and toggles rather than one.

## Data model

### TypeScript (`skillUtils.ts`)

The `Skill` interface gains one optional field:

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

A new `SkillBundle` interface:

```typescript
export interface SkillBundle {
  bundleName: string;   // folder name; also the key used in the disabled list
  folderPath: string;   // absolute path to the bundle root folder
  sourceDir: string;    // parent skills directory (e.g. ~/.config/biorouter/skills)
  skills: Skill[];      // all sub-skills in the bundle
  enabled: boolean;     // true when bundleName is NOT in the disabled list
}
```

The `loadSkillsFromDirs()` return type changes:

```typescript
async function loadSkillsFromDirs(dirs: string[]): Promise<{
  singles: Skill[];
  bundles: SkillBundle[];
}>
```

All callers (SkillsView, BottomMenuSkillSelection, skills IPC handler) are updated to destructure `{ singles, bundles }`.

### The `skills-config.json` disabled list

No schema change. The `disabled` array holds names — both single skill names and bundle names coexist as flat strings. A bundle is disabled by adding its `bundleName` to the array. Individual sub-skill names are never written to the disabled list.

### Rust (`skills_extension.rs`)

The `Skill` struct gains one field:

```rust
pub struct Skill {
    pub metadata: SkillMetadata,
    pub content: String,
    pub file_path: PathBuf,
    pub bundle_name: Option<String>,
}
```

The disabled check becomes:

```rust
let is_disabled = disabled.contains(&skill.metadata.name)
    || skill.bundle_name.as_deref()
        .map_or(false, |b| disabled.contains(b));
```

## Discovery logic

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

> **Note.** This spec gives the Rust side one sentence against a full TypeScript listing, but the two scanners are equally load-bearing — the TypeScript scan drives the settings UI, and the Rust scan is what the agent runtime actually sees. The concrete Rust changes (struct field, `parse_skill_file` signature, the rewritten `discover_skills_in_directories`, and the disabled-name filter) are spelled out in Task 5 of the [implementation plan](skill-bundles-plan.md), which is also where the three Rust unit tests covering single-skill discovery, bundle discovery, and bundle-level disabling live. This spec carries no testing section of its own.

## UI changes

### SkillsView (settings page)

Bundle rows render in the same list as single-skill rows (alphabetical by name/bundle name).

The bundle row shows:

- Bold bundle name + muted sub-skill count badge (e.g., "superpowers · 12 skills")
- One `Switch` toggle — checked when `bundleName` is NOT in the disabled list; toggling writes/removes `bundleName` from `skills-config.json`
- Read-only list of sub-skill names in muted small text below (always visible, no expand/collapse)
- No individual sub-skill toggles

Single-skill rows are unchanged from today.

### BottomMenuSkillSelection (chat bar dropdown)

Bundles appear as single entries in the skill picker dropdown:

- Primary line: bundle name
- Secondary line (muted): comma-separated sub-skill names, truncated with "…" if long
- Clicking toggles the whole bundle on/off (adds/removes `bundleName` from the active set)

The active-skill count badge on the chat bar button counts each bundle as 1.

## Installation

### ZIP import (AddSkillModal)

After extracting a ZIP, detection runs on the extracted folder:

- Root `SKILL.md` found → single skill install (existing path)
- No root `SKILL.md` + sub-dirs with `SKILL.md` → bundle install

For bundle installs the modal shows a preview before confirming:

> "Bundle detected: **superpowers** — 12 skills included"
> [scrollable list of sub-skill names]

The bundle folder is moved as-is into the target skills directory, preserving all sub-directories.

### Directory picker / folder drag-drop

The same detection logic is applied to the picked or dropped folder. The folder is copied into the skills dir under its folder name (for example, `skills/superpowers/`).

### Uninstall

Uninstalling a bundle removes the entire bundle folder (`rm -rf skills/<bundleName>/`). The bundle name is removed from the disabled list on uninstall (same behavior as single-skill uninstall today). Per-sub-skill uninstall is not supported.

## Out of scope

The following were deferred at design time. This document does not record whether any were revisited later.

- Per-sub-skill enable/disable toggles
- Partial bundle install (user selects which sub-skills to include)
- Nested bundles (bundles of bundles)
- Remote bundle install from URL (only ZIP and folder picker)

## Related documentation

- [Skill bundles implementation plan](skill-bundles-plan.md) — the task-by-task execution of this design, including the Rust changes and tests this spec only summarises.
- [.brxt bundled skills design](brxt-bundled-skills-design.md) — the sibling spec from the same day; it introduces the ZIP import path that bundle detection extends.
- [.brxt bundled skills implementation plan](brxt-bundled-skills-plan.md) — where the `skills:extract-zip` IPC handler this spec builds on was first added.
- [Skills extension](../../extensions/built-in/skills.md) — how skill discovery behaves today from a user's point of view.
- [Extensions, skills, and MCP agents](../../extensions/extensions-and-skills-guide.md) — the current end-user guide to adding and authoring skills.
