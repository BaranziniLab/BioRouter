---
date: 2026-05-07
status: approved
---

# Docs Consolidation

Merge `documentation/` and `docs/` into a single, clean, plain-markdown `docs/` folder. Drop the Docusaurus infrastructure entirely. Purge all Goose/Block branding. Rename `recipe` → `workflow` throughout. Keep only built-in BioRouter extensions; discard all third-party MCP pages.

---

## Source Inventory

### `documentation/` — 7 authoritative plain-markdown files (zero Goose references)

| File | Target |
| --- | --- |
| `architecture.md` | `docs/architecture/overview.md` |
| `data-privacy.md` | `docs/guides/data-privacy.md` |
| `extensions-skills-mcp.md` | `docs/guides/extensions-skills.md` |
| `installation-setup.md` | `docs/getting-started/installation.md` |
| `providers-and-models.md` | `docs/getting-started/providers.md` |
| `recipes.md` | `docs/guides/workflows/index.md` (recipe→workflow) |
| `schedulers.md` | `docs/guides/schedulers.md` |

### `docs/docs/` — Selected pages to carry forward (strip JSX, fix branding)

| Source | Target |
| --- | --- |
| `docs/quickstart.md` | `docs/getting-started/quickstart.md` |
| `biorouter-architecture/extensions-design.md` | `docs/architecture/extensions-design.md` |
| `biorouter-architecture/error-handling.md` | `docs/architecture/error-handling.md` |
| `guides/config-files.md` | `docs/guides/config-files.md` |
| `guides/biorouter-cli-commands.md` | `docs/guides/cli-commands.md` |
| `guides/biorouter-permissions.md` | `docs/guides/permissions.md` |
| `guides/sessions/index.md` | `docs/guides/sessions.md` |
| `guides/context-engineering/index.md` | `docs/guides/context-engineering.md` |
| `guides/environment-variables.md` | `docs/guides/environment-variables.md` |
| `guides/security/index.md` | `docs/guides/security.md` |
| `guides/subagents.md` | `docs/guides/subagents.md` |
| `guides/tips.md` | `docs/guides/tips.md` |
| `guides/recipes/recipe-reference.md` | `docs/guides/workflows/reference.md` |
| `guides/recipes/subrecipes.md` | `docs/guides/workflows/subworkflows.md` |
| `guides/recipes/session-recipes.md` | `docs/guides/workflows/session-workflows.md` |
| `guides/recipes/storing-recipes.md` | `docs/guides/workflows/storing-workflows.md` |
| `troubleshooting/index.md` | `docs/troubleshooting/index.md` |
| `troubleshooting/known-issues.md` | `docs/troubleshooting/known-issues.md` |
| `troubleshooting/diagnostics-and-reporting.md` | `docs/troubleshooting/diagnostics.md` |
| `mcp/developer-mcp.md` | `docs/extensions/developer.md` |
| `mcp/computer-controller-mcp.md` | `docs/extensions/computer-controller.md` |
| `mcp/memory-mcp.md` | `docs/extensions/memory.md` |
| `mcp/tutorial-mcp.md` | `docs/extensions/tutorial.md` |
| `mcp/autovisualiser-mcp.md` | `docs/extensions/auto-visualiser.md` |
| `mcp/skills-mcp.md` | `docs/extensions/skills.md` |
| `mcp/extension-manager-mcp.md` | `docs/extensions/extension-manager.md` |
| `mcp/chatrecall-mcp.md` | `docs/extensions/chat-recall.md` |
| `mcp/code-execution-mcp.md` | `docs/extensions/code-execution.md` |
| `mcp/todo-mcp.md` | `docs/extensions/todo.md` |

---

## Target Structure

```text
docs/
  getting-started/
    installation.md
    providers.md
    quickstart.md
  architecture/
    overview.md
    extensions-design.md
    error-handling.md
  guides/
    data-privacy.md
    extensions-skills.md
    schedulers.md
    config-files.md
    cli-commands.md
    permissions.md
    sessions.md
    context-engineering.md
    environment-variables.md
    security.md
    subagents.md
    tips.md
    workflows/
      index.md
      reference.md
      subworkflows.md
      session-workflows.md
      storing-workflows.md
  extensions/
    developer.md
    computer-controller.md
    memory.md
    tutorial.md
    auto-visualiser.md
    todo.md
    skills.md
    extension-manager.md
    chat-recall.md
    code-execution.md
  troubleshooting/
    index.md
    known-issues.md
    diagnostics.md
  superpowers/             (untouched)
    plans/
    specs/
```

---

## What Gets Deleted

All of the following are removed from `docs/`:

- All generated HTML files (`*.html`) and `*/index.html` subdirectories
- `docs/blog/` — blog posts not relevant to app docs
- `docs/community/` — external community pages
- `docs/audio/` — `elevenlabs-mcp-demo.mp3` (third-party demo, not built-in)
- `docs/videos/` — all `.mp4` files (hero videos, demo clips)
- `docs/assets/` — Docusaurus CSS, hashed images, media assets
- `docs/deeplink-generator/`, `docs/recipe-generator/`, `docs/prompt-library/`
- `docs/grants/`, `docs/extension/`, `docs/files/`, `docs/v1/`
- `docs/inkeepChatButton.js`, `docs/inkeepSearchBar.js`
- `docs/sitemap.xml`, `docs/robots.txt`, `docs/atom.xml`, `docs/atom.css`, `docs/atom.xsl`
- `docs/rss.xml`, `docs/rss.css`, `docs/rss.xsl`
- `docs/.nojekyll`, `docs/llms.txt`, `docs/servers.json`
- `docs/docs/mcp/` — all 50+ third-party MCP pages (not built-in to BioRouter)
- `docs/docs/blog/`, `docs/docs/experimental/`, `docs/docs/tutorials/`
- `docs/docs/guides/` subdirectories not in the carry-forward list
- `documentation/` folder entirely (all content merged into `docs/`)

---

## Content Transformations

Applied uniformly to every file carried forward:

### 1. Strip JSX / MDX

Remove all React component syntax and Docusaurus-specific markup:

- `<Card ... />`, `<div className={...}>`, `className={styles.xxx}`
- YouTube `<iframe>` embeds and `<div className="video-container">` wrappers
- Import statements (`import Card from ...`, `import styles from ...`)
- Frontmatter fields only used by Docusaurus (`sidebar_label`, `sidebar_position`, `custom_edit_url`, `pagination_prev`, `pagination_next`)

Replace card grids with plain markdown link lists. Drop embedded videos entirely (link to external resource in text if essential).

### 2. Branding replacements

| Find (case-insensitive) | Replace with |
| --- | --- |
| `goose` / `Goose` / `GOOSE` | `BioRouter` |
| `geese` / `Geese` / `GEESE` | `BioRouter` |
| `block.xyz`, `block.github.io` | `https://github.com/BaranziniLab/BioRouter` |
| `sq.github.io/goose` | `https://github.com/BaranziniLab/BioRouter` |
| "Block" as a company name | remove sentence or rewrite without company attribution |

### 3. Recipe → Workflow rename

| Find | Replace |
| --- | --- |
| `recipe` / `Recipe` | `workflow` / `Workflow` |
| `recipes` / `Recipes` | `workflows` / `Workflows` |
| `sub_recipe` | `sub_workflow` |
| `subrecipe` / `subrecipes` | `subworkflow` / `subworkflows` |
| `BIOROUTER_RECIPE_PATH` | `BIOROUTER_WORKFLOW_PATH` |
| `BIOROUTER_RECIPE_GITHUB_REPO` | `BIOROUTER_WORKFLOW_GITHUB_REPO` |
| `biorouter recipe validate` | `biorouter workflow validate` |
| `biorouter run --recipe` | `biorouter run --workflow` |
| `.biorouter/recipes/` | `.biorouter/workflows/` |
| `~/.config/biorouter/recipes/` | `~/.config/biorouter/workflows/` |

### 4. Path / link cleanup

- Update all internal doc cross-links to reflect new file paths
- Remove links to deleted pages (blog posts, external tools, Docusaurus-only routes)

---

## Out of Scope

- Custom/domain-specific extensions: OMOP agent, SPOKE agent, CDW agent — not included
- Experimental features (`biorouter-mobile`, `vs-code-extension`) — not carried forward
- Tutorial pages — dropped (can be recreated fresh if needed)
- `docs/superpowers/` — untouched; plans and specs remain as-is

---

## Verification Criteria

After implementation:

1. `find docs/ -name "*.html"` → zero results
2. `find docs/ -name "*.mp4" -o -name "*.mp3"` → zero results
3. `grep -ri "goose\|geese" docs/ --include="*.md"` → zero results (outside `superpowers/`)
4. `grep -ri "recipe" docs/ --include="*.md"` → zero results (outside `superpowers/`)
5. `ls docs/docs/mcp/` → directory does not exist
6. `ls documentation/` → directory does not exist
7. Every file in `docs/` (outside `superpowers/`) is a `.md` file
