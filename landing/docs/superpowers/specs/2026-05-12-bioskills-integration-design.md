# BioSkills Integration Design

**Date:** 2026-05-12
**Scope:** Integrate the GPTomics/bioSkills bioinformatics skill collection (63 categories, ~457 skills) into `biorouter-skills/`, and surface them on the BAAM page of the BioRouter landing site.

## Goal

Bring the full bioSkills library into BioRouter's skill marketplace without losing fidelity, advertise every category on `baam.html`, and add a skills-side search bar mirroring the existing extensions search.

## Repo changes (`biorouter-skills/`)

1. Add a new bundle directory: `biorouter-skills/bio-skills/`. Mirrors the `superpowers/` bundle pattern — a single top-level directory whose subdirectories are skills (or, here, *categories of* skills).
2. Preserve the upstream structure exactly: 63 category dirs, each containing one or more skill subdirectories with a `SKILL.md`.
3. **Naming.** Directory names already use kebab-case and match the biorouter convention. Strip the `bio-` prefix from the `name:` field of each SKILL.md frontmatter so directory name == skill name (matches the `superpowers/` convention). Add `user-invocable: false` since these are auto-applied domain skills, not slash commands.
4. **Preserve content.** Do not rewrite install commands, code blocks, or examples. Skills stay intact.
5. **pip → uv guidance.** Two-layer approach:
   - Bundle-level `bio-skills/README.md` with a "Prefer uv over pip" section.
   - For each SKILL.md that contains a `pip install` line, prepend a short tip block at the top of the file pointing at `uv pip install` / `uv add`.
6. Add a top-level `bio-skills/INDEX.md` listing every category and skill count for discoverability.

## Website changes (`biorouter-landing/baam.html`)

1. Replace the agent-only search bar at the top with a unified one OR add a second skills-specific search input. Decision: add a **second search bar** dedicated to the Skills section so the existing extension search keeps its scroll-into-view behavior.
2. Keep the 7 existing skill cards as-is.
3. Add a new section header **"BioSkills Library — 63 categories"** under the existing skills section.
4. Render 63 **compact** category cards in a grid. Each card shows:
   - Category title (Title Cased from kebab-case)
   - Skill count chip ("N skills")
   - One-line description
   - GitHub link to that category's directory
5. Add a top-of-section install note pointing at the bundle install path and the uv tip.
6. Search bar JS filters all skill cards (existing 7 + 63 new) by visible text. Live filter.

## Out of scope

- Generating per-category `.zip` releases. We link to the GitHub tree for now; the existing release workflow can be applied later.
- Renaming/rewriting individual skill content. Keep upstream content as authoritative.
- Backporting changes upstream to GPTomics/bioSkills.

## Approval

Design choices approved via brainstorming questions on 2026-05-12:
- 63 compact category cards with search bar.
- Bundle README + per-file tip banner for pip→uv (no in-place command rewrites).
