# Creating Skills: package your preferred methods as reusable instruction sets

## Purpose

Teach the user to create, test, and manage skills — folders containing a
`SKILL.md` whose instructions the agent loads on demand when the task matches.
Skills are how a user teaches Biorouter *their* way of doing something once,
instead of re-explaining it every session.

## Concepts to convey first (briefly)

- A skill is a directory with a `SKILL.md` file: YAML frontmatter (`name`,
  `description`) followed by markdown instructions. Supporting files
  (scripts, templates, examples) can sit next to it and are listed when the
  skill loads.
- The agent sees only each skill's name + description up front; it calls
  `loadSkill` to pull in the full instructions when relevant. **The
  description is the trigger** — write it to say *when* the skill applies.
- Skills live in `~/.config/biorouter/skills/<slug>/` (plus shared locations
  like `~/.claude/skills` and project-local `.biorouter/skills`, which lets
  skills travel between agent tools and live alongside a repo).
- Enable/disable from the **Skills** page; state persists in
  `skills-config.json`. The built-in `about-biorouter` skill ships with the
  app — it can be toggled off but is restored if deleted.

## Phase 1: Pick a real candidate

Ask what the user finds themselves re-explaining. Good skills are method-
shaped, not fact-shaped: "how I want figures formatted", "our lab's analysis
QC checklist", "how to structure a literature review". Facts and documents
belong in a knowledge base instead.

## Phase 2: Write the skill

Create the folder and file (the **Skills** page → Add Custom Skill does this
in the app; in the CLI, create the directory by hand):

```markdown
---
name: figure-style
description: Apply the lab's figure formatting conventions whenever creating or revising plots and figures.
---

# Lab figure style

- Use colorblind-safe palettes (Okabe-Ito).
- Label axes with units; sentence case; no chart junk.
- Export at 300 dpi, PDF + PNG.
...
```

Rules of thumb to share:
- Frontmatter `name` should match the folder slug.
- Keep the body imperative and concrete — it becomes operating instructions.
- Put long reference material in supporting files next to SKILL.md rather
  than inflating the body.

## Phase 3: Test it

Start a fresh session and give a task that *should* trigger the skill without
naming it (e.g. "plot this distribution"). Verify the agent loads it and
follows the instructions. If it doesn't trigger, sharpen the description —
that's the matching surface. Users can always force it with "use the
figure-style skill".

## Phase 4: Manage and share

- Toggle skills per the session's needs from the **Skills** page; too many
  always-on skills dilute each other.
- Share: copy SKILL.md to the clipboard from the Skills page, or zip the
  folder — others install it with `biorouter skill install <zip>` or the Add
  Skill button. Related skills can be grouped as a bundle (a parent folder of
  skill folders) and toggled as a unit.
- Workflows can pin skills via their `skills:` field so an automation always
  runs with the right methods loaded.

## Notes for the agent

- Help users keep skills small and single-purpose; suggest splitting a
  sprawling skill into a bundle.
- Skill instructions can conflict with each other or with workflow
  instructions — if behavior looks confused, check what's enabled.
