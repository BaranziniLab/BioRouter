---
name: develop-biorouter-skill
description: "Guide for authoring a Biorouter skill: the SKILL.md format and frontmatter, how Biorouter discovers and loads skills, writing a description that triggers reliably, progressive disclosure with supporting files, composing with subagents/workflows/hooks, testing for consistency and robustness, and packaging a skill as a zip. Load this skill whenever the user wants to create, write, improve, test, or publish a Biorouter (or Claude) skill."
user-invocable: true
---

# Developing a Biorouter Skill

A **skill** is a folder containing a `SKILL.md` file: YAML frontmatter
(`name`, `description`) followed by markdown instructions, plus optional
supporting files (scripts, templates, reference docs) beside it. Skills are how
a user teaches Biorouter *their* way of doing something once, instead of
re-explaining it every session. This guide teaches you to author, test, and
package one well.

> Skills encode **method and environment-specific knowledge** ("how *we* do X",
> our endpoints/conventions/quirks), not facts or documents. Those belong in a
> Knowledge base. And not generic knowledge the model already has.

## How Biorouter loads skills (why structure matters)

Biorouter uses **progressive disclosure**: load the cheap thing always, the
expensive thing on demand:

1. **Discovery (always on, and it is one sentence).** The always-on system
   prompt carries a *count* of the enabled skills and the names of the tools
   that reach them — not the catalog. The agent finds a skill by calling
   **`searchSkills`** (with no query, to page the whole catalog), and what it
   matches against there is the `name` + `description`. So the description
   still does all the triggering work; it simply has to survive a search rather
   than sit permanently in front of the model, which makes concrete trigger
   keywords more important, not less.
2. **Body (loaded on trigger).** Once a description matches the task, the agent
   calls the **`loadSkill`** tool to pull in the full `SKILL.md` body. Keep it
   lean, because it competes with the live conversation for context.
3. **Supporting files (loaded as needed).** Files next to `SKILL.md` are listed
   when the skill loads; the agent reads or executes them only when required, so
   they cost nothing until used.

**Implication:** put the trigger logic in the `description`, the core procedure
in the body, and bulky reference material / large examples / scripts in
supporting files.

## Where skills live

Biorouter auto-discovers skills at session start from several locations
(portable across agent tools that share the convention):

- `~/.config/biorouter/skills/<slug>/`: user-global (the default install dir)
- `~/.claude/skills/<slug>/` and `~/.config/agents/skills/<slug>/`: shared
- Project-local: `.biorouter/skills/`, `.claude/skills/`, `.agents/skills/`
- Bundled inside an installed extension: `extensions/<name>/skills/<slug>/`

A project-local skill overrides a global one of the same slug. Enable/disable
per skill from the **Skills** page or with `biorouter skill enable|disable
<name>` in the CLI (or in `biorouter configure` → Toggle Extensions for the
`skills` extension as a whole); per-skill disabled state
persists in `~/.config/biorouter/skills-config.json`. Built-in skills that ship
with the app (like `about-biorouter`) can be toggled off but are restored if the
folder is deleted.

## SKILL.md format and standards

```markdown
---
name: figure-style
description: Apply the lab's figure formatting conventions whenever creating or revising plots, charts, or figures.
---

# Lab figure style

- Use colorblind-safe palettes (Okabe-Ito).
- Label axes with units; sentence case; no chart junk.
- Export at 300 dpi, PDF + PNG.
```

**Frontmatter rules** (Biorouter parses `name` + `description`; extra keys such
as `user-invocable: true` are tolerated and ignored by the loader):

| Field | Required | Standard |
|---|---|---|
| `name` | yes | Make it equal the folder slug. Lowercase letters, numbers, hyphens; ≤ 64 chars. Avoid reserved words "anthropic"/"claude". Prefer a gerund or action phrase (`analyzing-spreadsheets`, `figure-style`), never `helper`/`utils`/`tools`. |
| `description` | yes | Non-empty, ≤ ~1024 chars. **Third person.** State both *what it does* and *when to use it*, with concrete trigger keywords. |

⚠ **Those standards are authoring conventions, not validation.** The loader
requires the frontmatter to parse with both `name` and `description` present —
`SkillMetadata` has neither as an `Option`, so serde rejects a missing one — and
its line-based fallback parser additionally requires `name` to be non-empty.
That is all. Nothing checks the length of `name` or `description`, nothing rejects
a reserved word, and nothing enforces that `name` matches the folder — a skill
whose `name` disagrees with its directory installs happily and then answers to a
different label than the one on disk. Hold yourself to the table; the tooling
will not.

**Body standards:**
- Keep the body **under ~500 lines**. Past that, split detail into supporting
  files in the same folder and link to them (one level deep, because the agent may
  only preview deeply nested files and read them incompletely).
- Write **imperative operating instructions for the agent**, not human-facing
  prose explaining concepts the model already knows. ("Run the linter, then
  fix any errors", not "Linters are tools that check code style.")
- Use **consistent terminology** (pick one word per concept and stick to it).
- Avoid **time-sensitive phrasing** ("before August 2025…") and hardcoded
  values that will drift; they rot the skill. Verify against the live codebase
  instead of pinning specifics.
- A reference file over ~100 lines should start with a short table of contents
  so a partial read still reveals its scope.

## Writing a description that triggers reliably

The `description` is the single matching surface across potentially many
installed skills, and it is the most important thing you write.

- **Third person, what + when.** "Generates commit messages by analyzing git
  diffs. Use when the user asks for help writing a commit message or reviewing
  staged changes." Not "I can help you write commit messages."
- **Include the words the user will actually say**: file types, formats,
  domain terms, task verbs.
- **Counter under-triggering.** Models tend to under-load skills. Name the
  contexts explicitly ("…even when the user doesn't say 'dashboard' but asks to
  display metrics"). For a skill that loses to a tempting inline default, an
  imperative description ("Always load this skill before X; do not do X
  directly") triggers far more reliably, but reserve forceful language for
  skills that need it, or overlapping "always" directives dilute each other.
- Keep all the "when" guidance **in the description**, not scattered in the body.

## Set the right degree of freedom

Match how prescriptive the instructions are to how fragile the task is:

- **High freedom (prose):** multiple valid approaches, judgment-driven (e.g. a
  code-review checklist). Describe goals and principles.
- **Medium freedom (parameterized steps / pseudocode):** a preferred pattern
  with acceptable variation.
- **Low freedom (exact commands, "run this verbatim"):** fragile or
  consistency-critical operations (migrations, releases). Say "do not modify the
  command or add flags."

## Composing with Biorouter's other primitives

A skill is *guidance loaded inline*. Have it delegate when another primitive
fits better, and know when **not** to:

- **Subagents.** When a step is high-volume, parallelizable, or produces
  throwaway intermediate output (broad search, log triage, multi-file audit),
  instruct the agent to spawn a subagent so that work stays out of the main
  context. Keep the orchestration/decision logic in the skill.
- **Workflows.** For deterministic multi-step automation (loops, fan-out,
  verify stages), point the user at a workflow. A workflow can **pin skills**
  via its `skills:` field so the automation always runs with the right methods
  loaded. Use a skill for the *method*; a workflow for the *orchestration*.
- **Hooks (harness).** Instructions are followed *most* of the time but can be
  reasoned around under pressure. For a **non-negotiable guarantee** ("always
  run the formatter after editing", "block this dangerous command"), a hook is
  deterministic where skill prose is probabilistic. If a skill rule keeps being
  skipped, that rule probably belongs in a `PostToolUse`/`PreToolUse` hook, not
  the body. Rule of thumb: **skills guide judgment; hooks enforce guarantees.**

## Supporting files

Put bulky or optional material beside `SKILL.md`, keep it one level deep, and
reference it from the body:

```
my-skill/
  SKILL.md
  reference.md          ← detailed schemas / API docs (read on demand)
  scripts/validate.py   ← run for deterministic, repeatable steps
  templates/report.md   ← output template
```

Prefer a pre-written script over asking the agent to regenerate the same code:
more reliable, consistent, and the script's *source* never enters context,
only its output. Make execution intent explicit ("**Run** `validate.py`" vs
"**See** `validate.py` for the algorithm"), have scripts handle their own
errors, and justify any magic numbers.

## Testing a skill (robustness & consistency)

There are **two separate failure modes**, and you test each:

**1. Does it trigger?** Start a *fresh* session and give a task that *should*
load the skill **without naming it** (e.g. "plot this distribution" for a
figure-style skill). Confirm the agent calls `loadSkill` and follows the
instructions. Also try tasks that *should not* trigger it, to check for false
positives. If it under-triggers, sharpen the `description`, which is the match
surface, not the body. (The user can always force it: "use the figure-style
skill".)

**2. Does it execute every step?** A loaded skill can still silently skip
internal steps, especially invisible verification. Make critical steps produce
**visible output** ("before delivering, output a checklist of each rule and
whether you applied it") so you and the agent can confirm compliance.

For robustness:
- Run the trigger test **several times**: skills are probabilistic; judge a
  *rate*, not a single pass. Tighten wording until it's reliable.
- Test under realistic conditions, including with your usual hooks/project
  instructions present (unrelated context can suppress a weak description).
- If you deploy across model tiers, sanity-check on the smaller one too, since it may
  need more explicit guidance than the largest model.
- Keep each skill **small and single-purpose.** Too many always-on or
  overlapping skills dilute each other; split a sprawling skill into a bundle.

## Bundles (grouping related skills)

A **bundle** is a parent folder containing multiple skill folders; Biorouter
discovers each and toggles them as a unit:

```
my-bundle/
  skill-one/
    SKILL.md
  skill-two/
    SKILL.md
    helper.md
```

## Packaging & installing (zip export, the Biorouter way)

Share a skill by zipping its folder. Accepted layouts:

- **Single skill:** `SKILL.md` at the zip root, **or** one folder deep
  `<slug>/SKILL.md`.
- **Bundle:** `<bundle>/<slug>/SKILL.md` (two levels deep).
- **Declared by a manifest:** a `biorouter-package.json`,
  `.codex-plugin/plugin.json`, `.claude-plugin/plugin.json` or
  `skills-manifest.json` names the components explicitly, and that beats the
  shape on disk.
- **Ambiguous — refused until you answer:** two or more skill folders sitting
  *loose at the archive root* with **no** manifest. That is equally the shape of
  one package and of a folder somebody happened to zip, so Biorouter asks
  instead of guessing. Resolve it with `--as bundle` or `--as individual` (over
  HTTP the question comes back as a 200 with `needsChoice`, not an error). A
  named parent directory is **not** ambiguous — putting the skills inside
  `my-bundle/` has already said they belong together.

```bash
# Single skill
cd my-skill && zip -r ../my-skill.zip SKILL.md reference.md scripts/

# Or zip the folder itself
zip -r my-skill.zip my-skill/
```

Install it with the CLI or the GUI:

```bash
biorouter skill install my-skill.zip                    # a local .zip
biorouter skill install https://github.com/you/skills   # or a repository URL
biorouter skill install my-skills.zip --as bundle       # answer the ambiguity above
biorouter skill list                                    # verify it registered
```

`install` takes one `source` argument — a path to a `.zip` **or** a repository
URL — plus `--force` to replace an already-installed slug and
`--as bundle|individual` for a source that could be either.

In the desktop app, the **Skills** page → **Add Skill** (or drag the zip in)
does the same, unpacking into `~/.config/biorouter/skills/<slug>/`. You can also
copy a skill's `SKILL.md` to the clipboard from the Skills page to share it
quickly.

## Authoring checklist

```
[ ] Folder slug == frontmatter name (lowercase, hyphens, ≤64 chars)
[ ] description is third person, says what AND when, with trigger keywords
[ ] Body is imperative agent instructions, < ~500 lines, no generic filler
[ ] Bulky/optional material moved to supporting files, one level deep
[ ] Degree of freedom matches task fragility (prose vs exact commands)
[ ] Delegates to subagents/workflows where appropriate; guarantees → hooks
[ ] No time-sensitive phrasing or hardcoded values that will drift
[ ] Trigger-tested in a fresh session without naming the skill (and a
    negative case), repeated for consistency
[ ] Critical steps emit visible output so execution can be verified
[ ] Packs as SKILL.md at root, <slug>/SKILL.md, or a named parent for a
    bundle — never several skills loose at the zip root with no manifest,
    which install refuses as ambiguous
[ ] Installs cleanly via `biorouter skill install <zip-or-repo-url>`
```
