# Skills extension

> **What this is.** User guide to the built-in Skills extension: where skills are discovered from on disk, how to get more of them, and a worked GWAS-pipeline example showing a skill steering the agent.
> **Status:** Current — discovery from `.agents/skills/` and `~/.config/agents/skills/` is the shipped behaviour, and the extension is enabled by default.
> **Audience:** end users.

The Skills extension loads *skills* — reusable sets of instructions that teach BioRouter how to perform a specific task or follow a particular workflow. BioRouter discovers skills at startup and applies them when they are relevant to your request, without you having to name them.

Skills are read from `.agents/skills/` in your project directory and `~/.config/agents/skills/` globally. That layout is shared with other AI coding agents, so a skill you write is portable rather than BioRouter-specific.

> **Note.** This extension is **enabled by default**. `crates/biorouter/src/agents/extension.rs` registers `skills` as a platform extension with `default_enabled: true`, matching the default-enabled list in the [installation guide](../../getting-started/installation.md). The configuration walkthrough below is only needed if you previously disabled it, or want to confirm its state.

## Configuration

1. Run the `configure` command:

   ```bash
   biorouter configure
   ```

2. Choose `Toggle Extensions`, then confirm `skills` is enabled:

   ```text
   ┌   biorouter-configure
   │
   ◇  What would you like to configure?
   │  Toggle Extensions
   │
   ◆  Enable extensions: (use "space" to toggle and "enter" to submit)
   │  ● skills
   └  Extension settings updated successfully
   ```

## Where skills come from

You can write skills yourself, or install ready-made ones. Most users do the latter: BioRouter's marketplace, **BAAM**, publishes a catalogue of installable extensions and skills that the app can install in one click, and the skills themselves live in the `BaranziniLab/biorouter-skills` repository — around 85 of them, grouped into core, developer and biomedical categories (single-cell, variant calling, differential expression, pathway analysis, clinical biostatistics, and so on). Browse them on the [BAAM skills gallery](http://biorouter.ucsf.edu/skills.html).

> **Note.** A skill's identity comes from the `name` in its frontmatter, not from its directory name. BioRouter parses each `SKILL.md` and keys the skill by `metadata.name`, so a skill stored in `run-gwas/` but declaring `name: gwas-pipeline` is known to BioRouter as `gwas-pipeline`. Keeping the two the same avoids confusion, but the frontmatter is what wins.

## Example usage

Suppose BioRouter discovers this skill on startup at `~/.config/agents/skills/run-gwas/SKILL.md`:

```markdown
---
name: gwas-pipeline
description: Safe procedure for running a genome-wide association analysis
---

# GWAS Analysis

## Pre-analysis Checklist
1. Ensure genotype QC has passed
2. Check that the phenotype file is finalized
3. Confirm covariates (age, sex, ancestry PCs) are defined

## Analysis Steps
1. Create an analysis branch from main
2. Run sample- and variant-level QC filters
3. Run the association tests, then apply genomic control
4. Generate a Manhattan plot and review for genomic inflation
```

Because of the rule above, BioRouter registers this skill as `gwas-pipeline`.

### BioRouter prompt

```text
Run the GWAS on the current dataset
```

### BioRouter output

I'll help you run the GWAS on the current dataset. Let me first load the GWAS pipeline skill to follow the safe analysis procedure.

I can see you're working in the **`new-cohort`** dataset with some unprocessed files. Before proceeding with the association analysis, I need to follow the safe analysis procedure.

#### ⚠️ Pre-analysis confirmation required

According to the GWAS pipeline skill, I need to complete the following checklist:

**Pre-analysis Checklist**

1. **Ensure genotype QC has passed** - Has QC been run?
2. **Check that the phenotype file is finalized** - Is the phenotype definition locked?
3. **Confirm covariates are defined** - Are age, sex, and ancestry PCs available?

**Analysis Steps (once confirmed)**

1. Create an analysis branch from main
2. Run sample- and variant-level QC filters
3. Run the association tests, then apply genomic control
4. Generate a Manhattan plot and review for genomic inflation

**Before I proceed, please confirm:**

1. ✅ Has genotype QC passed for the `new-cohort` dataset?
2. ✅ Is the phenotype file finalized?
3. ✅ Are the covariates (age, sex, ancestry PCs) defined?
4. ✅ Are you sure you want to run the GWAS on the `new-cohort` dataset?

Please confirm these items and I'll proceed with the analysis.

The transcript ends there: the skill's whole purpose in this example is to stop BioRouter before an expensive, hard-to-undo analysis and make it ask for confirmation first.

## Related documentation

- [Extensions and skills guide](../extensions-and-skills-guide.md) — the same ground from the configuration side: installing skills, and where they are stored.
- [Context engineering](../../agent-loop/context-engineering.md) — how skills are selected and injected into the model's context.
- [Tutorial extension](tutorial.md) — the interactive walkthroughs, including one on creating skills.
- [Installation](../../getting-started/installation.md) — the list of extensions enabled out of the box.
