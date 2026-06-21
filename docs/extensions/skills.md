The Skills extension loads *skills* &mdash; reusable sets of instructions that teach biorouter how to perform specific tasks or follow particular workflows.

biorouter automatically discovers skills at startup and uses them when relevant to your request. biorouter loads skills from `.agents/skills/` in your project directory and `~/.config/agents/skills/` globally, making skills portable across different AI coding agents. To learn about creating skills and how biorouter uses them, see [Using Skills](/docs/guides/context-engineering/using-skills).

## Configuration

  
  
  
  

  1. Run the `configure` command:
  ```sh
  biorouter configure
  ```

  2. Choose to `Toggle Extensions`
  ```sh
  ┌   biorouter-configure 
  │
  ◇  What would you like to configure?
  │  Toggle Extensions 
  │
  ◆  Enable extensions: (use "space" to toggle and "enter" to submit)
  // highlight-start    
  │  ● skills
  // highlight-end  
  └  Extension settings updated successfully
  ```
  

## Example Usage

Let's say you have a skill that biorouter discovers on startup in `~/.config/agents/skills/run-gwas/SKILL.md`:

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

### biorouter Prompt

```
Run the GWAS on the current dataset
```

### biorouter Output

> **Note:** I'll help you run the GWAS on the current dataset. Let me first load the GWAS pipeline skill to follow the safe analysis procedure.

I can see you're working in the **`new-cohort`** dataset with some unprocessed files. Before proceeding with the association analysis, I need to follow the safe analysis procedure.

#### ⚠️ Pre-analysis Confirmation Required

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

---

**Before I proceed, please confirm:**

1. ✅ Has genotype QC passed for the `new-cohort` dataset?
2. ✅ Is the phenotype file finalized?
3. ✅ Are the covariates (age, sex, ancestry PCs) defined?
4. ✅ Are you sure you want to run the GWAS on the `new-cohort` dataset?

Please confirm these items and I'll proceed with the analysis.
