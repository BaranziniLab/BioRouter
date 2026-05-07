#!/usr/bin/env python3
"""
Migrate docs from Docusaurus/MDX to plain markdown.

Transformations applied to every file:
  1. Strip Docusaurus-only frontmatter keys
  2. Strip JSX/MDX (imports, component tags, admonitions wrappers)
  3. Replace Goose/Block branding → BioRouter
  4. Replace recipe/recipes → workflow/workflows
  5. Basic link cleanup (remove /BioRouter/ prefix, fix recipe→workflow paths)

Usage:
  python3 scripts/migrate-docs.py          # run migration
  python3 scripts/migrate-docs.py --dry-run  # preview without writing
"""
import re
import sys
from pathlib import Path

REPO = Path(__file__).parent.parent
DRY_RUN = '--dry-run' in sys.argv

# ── Frontmatter keys to remove ────────────────────────────────────────────────
STRIP_FM_KEYS = {
    'sidebar_label', 'sidebar_position', 'custom_edit_url',
    'pagination_prev', 'pagination_next', 'slug', 'displayed_sidebar',
    'tags', 'image', 'keywords', 'hide_title', 'hide_table_of_contents',
    'toc_min_heading_level', 'toc_max_heading_level',
}

# ── Branding replacements (order matters — longer patterns first) ──────────────
BRANDING = [
    # URLs first (before bare word replacements)
    ('block.gitmcp.io',          'https://github.com/BaranziniLab/BioRouter'),
    ('block.xyz',                'https://github.com/BaranziniLab/BioRouter'),
    ('block.github.io',          'https://github.com/BaranziniLab/BioRouter'),
    ('sq.github.io/goose',       'https://github.com/BaranziniLab/BioRouter'),
    # Bare word branding (case variants)
    (re.compile(r'\bGOOSE\b'),   'BIOROUTER'),
    (re.compile(r'\bGoose\b'),   'BioRouter'),
    (re.compile(r'\bgoose\b'),   'BioRouter'),
    (re.compile(r'\bGEESE\b'),   'BIOROUTER'),
    (re.compile(r'\bGeese\b'),   'BioRouter'),
    (re.compile(r'\bgeese\b'),   'BioRouter'),
    # Remove "by Block" / "from Block" attributions
    (re.compile(r'\s*[Bb]y\s+Block\b'),  ''),
    (re.compile(r'\s*[Ff]rom\s+Block\b'), ''),
    (re.compile(r',?\s*[Bb]lock\s+[Ii]nc\.?'), ''),
]

# ── Recipe → Workflow renames ─────────────────────────────────────────────────
WORKFLOW = [
    # Env vars and CLI flags first (exact string, no word-boundary needed)
    ('BIOROUTER_RECIPE_PATH',       'BIOROUTER_WORKFLOW_PATH'),
    ('BIOROUTER_RECIPE_GITHUB_REPO','BIOROUTER_WORKFLOW_GITHUB_REPO'),
    ('biorouter recipe validate',   'biorouter workflow validate'),
    ('biorouter run --recipe',      'biorouter run --workflow'),
    ('.biorouter/recipes/',         '.biorouter/workflows/'),
    ('~/.config/biorouter/recipes/','~/.config/biorouter/workflows/'),
    # Path segments in links
    ('/recipes/',                   '/workflows/'),
    ('/recipe-reference',           '/reference'),
    ('/session-recipes',            '/session-workflows'),
    ('/storing-recipes',            '/storing-workflows'),
    ('/subrecipes',                 '/subworkflows'),
    # YAML field names
    ('sub_recipes:',                'sub_workflows:'),
    ('sub_recipe:',                 'sub_workflow:'),
    # Plural forms first
    (re.compile(r'\bSubrecipes\b'),  'Subworkflows'),
    (re.compile(r'\bsubrecipes\b'),  'subworkflows'),
    (re.compile(r'\bSubrecipe\b'),   'Subworkflow'),
    (re.compile(r'\bsubrecipe\b'),   'subworkflow'),
    (re.compile(r'\bRecipes\b'),     'Workflows'),
    (re.compile(r'\brecipes\b'),     'workflows'),
    (re.compile(r'\bRecipe\b'),      'Workflow'),
    (re.compile(r'\brecipe\b'),      'workflow'),
]


def strip_frontmatter(text: str) -> str:
    """Remove Docusaurus-only frontmatter keys; preserve title/date/status."""
    if not text.startswith('---'):
        return text
    end = text.find('\n---', 3)
    if end == -1:
        return text
    fm_block = text[3:end]
    body = text[end + 4:]  # skip closing ---\n
    kept = []
    for line in fm_block.splitlines():
        key = line.split(':')[0].strip().lstrip('-').strip()
        if not key or key not in STRIP_FM_KEYS:
            kept.append(line)
    new_fm = '\n'.join(kept).strip()
    if new_fm:
        return f'---\n{new_fm}\n---\n{body}'
    return body.lstrip('\n')


def strip_jsx(text: str) -> str:
    """Remove JSX/MDX constructs; preserve substantive content."""
    # Remove import/export lines
    text = re.sub(r'^(?:import|export)\s+.+\n', '', text, flags=re.MULTILINE)

    # Convert Docusaurus admonitions: :::type[title]\ncontent\n::: → > **Type:** content
    def admonition_sub(m):
        kind = m.group(1).strip().split()[0].capitalize()
        content = m.group(2).strip()
        return f'\n> **{kind}:** {content}\n'
    text = re.sub(
        r':::(\w+[^\n]*)\n(.*?):::',
        admonition_sub,
        text,
        flags=re.DOTALL,
    )

    # Strip <details><summary>…</summary>…</details> — keep inner content
    text = re.sub(
        r'<details[^>]*>\s*<summary[^>]*>(.*?)</summary>(.*?)</details>',
        lambda m: f'\n**{m.group(1).strip()}**\n{m.group(2).strip()}\n',
        text,
        flags=re.DOTALL,
    )

    # Convert JSX heading/paragraph tags to markdown (e.g. from landing pages)
    text = re.sub(r'<h1[^>]*>(.*?)</h1>', r'# \1', text, flags=re.DOTALL)
    text = re.sub(r'<h2[^>]*>(.*?)</h2>', r'## \1', text, flags=re.DOTALL)
    text = re.sub(r'<h3[^>]*>(.*?)</h3>', r'### \1', text, flags=re.DOTALL)
    text = re.sub(r'<p[^>]*>(.*?)</p>', r'\1\n', text, flags=re.DOTALL)

    # Remove Tabs/TabItem wrappers — keep plain content inside TabItem
    text = re.sub(r'<Tabs[^>]*>', '', text)
    text = re.sub(r'</Tabs>', '', text)
    text = re.sub(r'<TabItem[^>]*>', '', text)
    text = re.sub(r'</TabItem>', '', text)

    # Remove self-closing JSX components (uppercase tag name)
    text = re.sub(r'<[A-Z][a-zA-Z]*(?:\s[^>]*)?\s*/>', '', text)

    # Remove opening/closing JSX component pairs with content
    text = re.sub(r'<[A-Z][a-zA-Z]*[^>]*>.*?</[A-Z][a-zA-Z]*>', '', text, flags=re.DOTALL)

    # Remove iframe embeds
    text = re.sub(r'<iframe[^>]*>.*?</iframe>', '', text, flags=re.DOTALL)

    # Remove remaining div/span with className
    text = re.sub(r'<(?:div|span)[^>]*className[^>]*>', '', text)
    text = re.sub(r'</(?:div|span)>', '', text)

    # Remove className attributes on remaining HTML tags
    text = re.sub(r'\s+className=(?:"[^"]*"|\{[^}]*\})', '', text)

    # Remove /BioRouter/ URL prefix (GitHub Pages base path, not needed in plain docs)
    text = re.sub(r'\(/BioRouter/', '(/', text)

    # Fix doc cross-links: /docs/guides/ → relative paths are fine as-is
    # (internal links are best-effort; broken ones can be fixed manually)

    # Collapse 3+ blank lines to 2
    text = re.sub(r'\n{3,}', '\n\n', text)
    return text.strip() + '\n'


def apply_subs(text: str, subs: list) -> str:
    for pattern, replacement in subs:
        if isinstance(pattern, re.Pattern):
            text = pattern.sub(replacement, text)
        else:
            text = text.replace(pattern, replacement)
    return text


def migrate(src: Path, dst: Path):
    text = src.read_text(encoding='utf-8')
    text = strip_frontmatter(text)
    text = strip_jsx(text)
    text = apply_subs(text, BRANDING)
    text = apply_subs(text, WORKFLOW)
    if DRY_RUN:
        print(f'  DRY-RUN: {src.relative_to(REPO)} -> {dst.relative_to(REPO)}')
        return
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text(text, encoding='utf-8')
    print(f'  {src.relative_to(REPO)} -> {dst.relative_to(REPO)}')


# ── Mapping: source path → destination path (both relative to REPO) ───────────
MAPPING = [
    # documentation/ → docs/ (already clean markdown, recipe→workflow only)
    ('documentation/architecture.md',           'docs/architecture/overview.md'),
    ('documentation/data-privacy.md',           'docs/guides/data-privacy.md'),
    ('documentation/extensions-skills-mcp.md',  'docs/guides/extensions-skills.md'),
    ('documentation/installation-setup.md',     'docs/getting-started/installation.md'),
    ('documentation/providers-and-models.md',   'docs/getting-started/providers.md'),
    ('documentation/recipes.md',                'docs/guides/workflows/index.md'),
    ('documentation/schedulers.md',             'docs/guides/schedulers.md'),

    # getting-started
    ('docs/docs/quickstart.md',                         'docs/getting-started/quickstart.md'),

    # architecture
    ('docs/docs/biorouter-architecture/extensions-design.md',  'docs/architecture/extensions-design.md'),
    ('docs/docs/biorouter-architecture/error-handling.md',     'docs/architecture/error-handling.md'),

    # guides — core
    ('docs/docs/guides/config-files.md',              'docs/guides/config-files.md'),
    ('docs/docs/guides/biorouter-cli-commands.md',    'docs/guides/cli-commands.md'),
    ('docs/docs/guides/biorouter-permissions.md',     'docs/guides/permissions.md'),
    ('docs/docs/guides/sessions/index.md',            'docs/guides/sessions.md'),
    ('docs/docs/guides/context-engineering/index.md', 'docs/guides/context-engineering.md'),
    ('docs/docs/guides/environment-variables.md',     'docs/guides/environment-variables.md'),
    ('docs/docs/guides/security/index.md',            'docs/guides/security.md'),
    ('docs/docs/guides/subagents.md',                 'docs/guides/subagents.md'),
    ('docs/docs/guides/tips.md',                      'docs/guides/tips.md'),

    # guides — workflows (formerly recipes)
    ('docs/docs/guides/recipes/recipe-reference.md',   'docs/guides/workflows/reference.md'),
    ('docs/docs/guides/recipes/subrecipes.md',         'docs/guides/workflows/subworkflows.md'),
    ('docs/docs/guides/recipes/session-recipes.md',    'docs/guides/workflows/session-workflows.md'),
    ('docs/docs/guides/recipes/storing-recipes.md',    'docs/guides/workflows/storing-workflows.md'),

    # troubleshooting
    ('docs/docs/troubleshooting/index.md',                    'docs/troubleshooting/index.md'),
    ('docs/docs/troubleshooting/known-issues.md',             'docs/troubleshooting/known-issues.md'),
    ('docs/docs/troubleshooting/diagnostics-and-reporting.md','docs/troubleshooting/diagnostics.md'),

    # extensions — built-in BioRouter extensions only
    ('docs/docs/mcp/developer-mcp.md',           'docs/extensions/developer.md'),
    ('docs/docs/mcp/computer-controller-mcp.md', 'docs/extensions/computer-controller.md'),
    ('docs/docs/mcp/memory-mcp.md',              'docs/extensions/memory.md'),
    ('docs/docs/mcp/tutorial-mcp.md',            'docs/extensions/tutorial.md'),
    ('docs/docs/mcp/autovisualiser-mcp.md',      'docs/extensions/auto-visualiser.md'),
    ('docs/docs/mcp/skills-mcp.md',              'docs/extensions/skills.md'),
    ('docs/docs/mcp/extension-manager-mcp.md',   'docs/extensions/extension-manager.md'),
    ('docs/docs/mcp/chatrecall-mcp.md',          'docs/extensions/chat-recall.md'),
    ('docs/docs/mcp/code-execution-mcp.md',      'docs/extensions/code-execution.md'),
    ('docs/docs/mcp/todo-mcp.md',                'docs/extensions/todo.md'),
]

if __name__ == '__main__':
    print(f'{"DRY-RUN: " if DRY_RUN else ""}Migrating {len(MAPPING)} files...')
    errors = []
    for src_rel, dst_rel in MAPPING:
        src = REPO / src_rel
        dst = REPO / dst_rel
        if not src.exists():
            print(f'  MISSING: {src_rel}')
            errors.append(src_rel)
            continue
        try:
            migrate(src, dst)
        except Exception as e:
            print(f'  ERROR: {src_rel}: {e}')
            errors.append(src_rel)
    print(f'\nDone. {len(MAPPING) - len(errors)}/{len(MAPPING)} files migrated.')
    if errors:
        print(f'Errors ({len(errors)}): {errors}')
        sys.exit(1)
