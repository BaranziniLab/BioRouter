# Skills packaging

This folder holds the paper trail for two features that changed how *skills* — folders
containing a `SKILL.md` file of YAML frontmatter plus a markdown body, which the agent
loads as procedural guidance — are packaged, installed, and discovered. Both were
specified and executed on **2026-05-07**, and **both shipped**: `brxt:uninstall` and
`skills:extract-zip` are IPC handlers in `ui/desktop/src/main.ts`, `bundle_name` is a
field on the Rust `Skill` struct in `crates/biorouter/src/agents/skills_extension.rs`,
and `SkillBundle` is defined in `ui/desktop/src/components/skills/skillUtils.ts`. This
is a historical record kept for the reasoning and the build sequence — not current
guidance.

Come here to trace *why* extension packages carry skills, or why a folder of sub-skills
collapses into one row with one toggle. If you instead want to know where BioRouter
looks for skills today, how to add one, or how to author one, you are in the wrong place
and should read the [Skills extension guide](../../extensions/built-in/skills.md) or the
[extensions, skills, and MCP agents guide](../../extensions/extensions-and-skills-guide.md)
— those describe shipped behaviour and are maintained; the paths and file layouts quoted
in the documents below are a snapshot of authoring time. The two plans are unusually
code-heavy and inline whole proposed source files; where the shipped code diverged, the
repository is authoritative. Their `- [ ]` checkboxes were never ticked off in the files
— read them as the original task lists, not as outstanding work.

| Document | What it covers |
|---|---|
| [`.brxt` bundled skills design](brxt-bundled-skills-design.md) | The design spec for letting a `.brxt` extension package carry its own skills, so that installing an extension installs its skills and removing the extension removes them atomically — plus ZIP file support in the standalone skill import UI. Approved 2026-05-07; every element shipped. |
| [`.brxt` bundled skills implementation plan](brxt-bundled-skills-plan.md) | The task-by-task, test-driven execution record for that design: extension-local storage under `~/.config/biorouter/extensions/<name>/skills/<slug>/`, the `brxt:uninstall` and `skills:extract-zip` IPC handlers, and the Rust scanner change that discovers them. Completed. |
| [Skill bundles design](skill-bundles-design.md) | The design spec for *skill bundles*: treating a parent folder that holds several sub-skill directories as one installable unit with a single on/off toggle, across the TypeScript and Rust skill scanners and the settings UI. Motivated by public collections such as `superpowers`, which under the old one-level rule produced dozens of separate rows. Implemented. |
| [Skill bundles implementation plan](skill-bundles-plan.md) | The task-by-task execution record for bundle support across the React UI, Electron IPC, and Rust discovery — two-level detection at discovery time, the `{ singles, bundles }` return shape, and the bundle rows in the settings UI. Completed. |

The two features were written on the same day and overlap at exactly one point: the
`.brxt` spec added ZIP import to the Add Skill modal, and the bundles spec taught that
import path to recognise a bundle. They are otherwise independent, which is why each
design carries its own plan rather than sharing one.

## Related documentation

- [Skills extension](../../extensions/built-in/skills.md) — the current user-facing truth about where skills are discovered from and how to get more of them.
- [Extensions, skills, and MCP agents](../../extensions/extensions-and-skills-guide.md) — the current guide to adding, configuring, and authoring extensions and skills, including the `.brxt` package format these documents extend.
- [Institutional providers archive](../institutional-providers/README.md) — the other 2026-05-07 design-and-plan pair, written against the same `feat/institutional-providers` branch these specs were split off from.
- [Historical records index](../README.md) — the archive this folder sits in, and how to check any document's standing from its `Status:` line.
