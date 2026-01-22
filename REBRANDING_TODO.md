# BioRouter Documentation Rebranding TODO

This document outlines the remaining work needed to complete the rebranding from "Goose" to "BioRouter" in the documentation.

## Current Status

### ✅ Completed
1. **Installation Documentation Updated**
   - Marked Windows and Linux installations as "Coming Soon"
   - Only macOS installation instructions are now shown as available
   - Updated both `/docs/getting-started/installation.md` and `/docs/quickstart.md`

2. **GitHub Actions Workflow Created**
   - Created `.github/workflows/deploy-docs.yml`
   - Workflow will build and deploy documentation to GitHub Pages
   - Configured for: https://baranzinilab.github.io/BioRouter/

3. **Image Assets Partially Fixed**
   - Renamed critical blog post images from `goose_*` to `biorouter_*`:
     - `blog/2025-03-18-goose-langfuse/biorouter_aierrors.png`
     - `blog/2025-03-21-goose-boston-meetup/biorouter_*` (3 images)
     - `blog/2026-01-20-goose-mobile-apps/biorouter-mobile-apps-banner.png`
   - Renamed video: `static/videos/biorouter_makes_a_call.mp4`

### 🔴 Remaining Work

## 1. Fix Broken Links (CRITICAL - Blocking Build)

The build currently fails due to broken links. Blog post directories have "goose" in their names but references use "biorouter":

**Broken Links:**
- `/blog/2025/12/19/biorouter-mobile-terminal/` (directory is still named `goose-mobile-terminal`)
- `/blog/2025/03/14/biorouter-ollama` (directory is still named `goose-ollama`)
- `/blog/2025/03/31/biorouter-benchmark` (directory is still named `goose-benchmark`)
- `/blog/2025/06/05/whats-in-my-biorouterhints-file`

**Required Actions:**
```bash
cd documentation/blog
# Either rename directories or update all references
mv 2025-12-19-goose-mobile-terminal 2025-12-19-biorouter-mobile-terminal
mv 2025-03-14-goose-ollama 2025-03-14-biorouter-ollama
mv 2025-03-31-goose-benchmark 2025-03-31-biorouter-benchmark
# And update all references throughout the docs
```

## 2. Complete Image Asset Rebranding (HIGH PRIORITY)

### Static Images
Located in `documentation/static/img/`:
- `goose-grant-program.png` → `biorouter-grant-program.png`
- `goose-logo-black.png` → `biorouter-logo-black.png`
- `goose-logo-white.png` → `biorouter-logo-white.png`
- `goose.svg` → `biorouter.svg`
- `logo_codename_goose.png` → (delete or rename)
- `logo-codename-goose.svg` → (delete or rename)

### Blog Post Images
Approximately **45+ blog post images** with "goose" in filename need renaming.
Example directories with goose images:
```
blog/2024-11-22-screenshot-driven-development/goose-prototypes-calendar.png
blog/2024-12-06-previewing-goose-v10-beta/goose-*.png
blog/2024-12-10-connecting-ai-agents-to-your-systems-with-mcp/goose-*.png
blog/2024-12-11-resolving-ci-issues-with-goose-a-practical-walkthrough/goose-*.png
blog/2025-01-28-introducing-codename-goose/introducing-codename-goose.png
blog/2025-02-21-gooseteam-mcp/gooseteam-*.png
blog/2025-03-06-goose-tips/goose-tips.png
blog/2025-03-10-goose-calls-vyop/goose-voyp.png
blog/2025-03-12-goose-figma-mcp/goosefigma.png
blog/2025-03-14-goose-ollama/gooseollama.png
... (30+ more files)
```

### Documentation Assets
Located in `documentation/docs/assets/`:
- `goose-in-action.mp4`
- `goose.png`
- `goose-in-action.gif`
- `guides/goose-providers-cli.png`

## 3. Update React Components (HIGH PRIORITY)

### Component Files to Update
Located in `documentation/src/components/`:

1. **GooseLogo.tsx** → Rename to `BioRouterLogo.tsx`
   - Update component name: `GooseLogo` → `BioRouterLogo`
   - Update image references

2. **biorouterWordmark.tsx**
   - Update export: `GooseWordmark` → `BioRouterWordmark`

3. **icons/biorouter.tsx**
   - Update function name: `Goose` → `BioRouter`

4. **GooseDesktopInstaller.tsx** → Rename to `BioRouterDesktopInstaller.tsx`
   - Update interface: `GooseDesktopInstallerProps` → `BioRouterDesktopInstallerProps`
   - Update function: `GooseDesktopInstaller` → `BioRouterDesktopInstaller`
   - Update internal function: `buildGooseUrl` → `buildBioRouterUrl`

5. **GooseBuiltinInstaller.tsx** → Rename to `BioRouterBuiltinInstaller.tsx`
   - Update interface and component names
   - Update CSS class: `goose-builtin-installer` → `biorouter-builtin-installer`

6. **server-card.tsx**
   - Line 83: Update text from "goose" to "biorouter"

7. **ContentCard.tsx**
   - Line 248: Update image path
   - Line 249: Update alt text

8. **RateLimits.js**
   - Line 21: Update link from `/docs/guides/handling-llm-rate-limits-with-goose` to `/docs/guides/handling-llm-rate-limits-with-biorouter`

### Update Component Imports
After renaming components, update all imports in:
- `src/pages/index.tsx`
- Any other files importing these components

## 4. Update Blog Post Content (170 FILES)

**Statistics:**
- 170 blog post files contain "goose" references
- ~12,302 lines total across all blog posts

### Critical Blog Posts to Update:

1. **Most Recent:**
   - `blog/2026-01-20-goose-mobile-apps/index.md`
   - `blog/2025-12-28-goose-maintains-goose/index.md`

2. **Foundational:**
   - `blog/2025-01-28-introducing-codename-goose/index.md`

3. **High Traffic Posts** (based on naming):
   - All tutorial posts referencing "goose" in title or content

### Blog Directory Names
**30+ blog directories** have "goose" in their name and should be renamed or kept for historical URLs with redirects.

Consider: Keep old URLs but add redirects in `docusaurus.config.ts`

## 5. Update Documentation Content (88 FILES)

**Statistics:**
- 88 documentation files
- 227 occurrences of "goose"

### High-Impact Documentation Files:
- `docs/quickstart.md`
- `docs/tutorials/cicd.md` (10 occurrences)
- `docs/tutorials/ralph-loop.md` (25 occurrences)
- `docs/guides/subagents.mdx` (8 occurrences)
- 80+ MCP tutorial files

**Common patterns to replace:**
- "using goose" → "using biorouter"
- "goose provides" → "biorouter provides"
- "with goose" → "with biorouter"
- "goose can" → "biorouter can"
- "ask goose" → "ask biorouter"

## 6. Update JSON Data Files (MEDIUM PRIORITY)

### Community Content
`src/pages/community/data/community-content.json`:
- Multiple entries reference "goose"
- Titles like "Goose with Canva MCP"
- Descriptions mentioning "goose's extension system"

### Prompt Library
- `src/pages/prompt-library/data/prompts/js-express-setup.json`
- `src/pages/prompt-library/data/prompts/github-branch-pr.json`

## 7. Remove CLI References (USER REQUEST)

**User wants Desktop UI only** - remove all CLI-specific content:

### Files/Sections to Update:
1. **Remove CLI tabs from:**
   - All installation guides
   - Quickstart guide
   - Configuration guides
   - Any "biorouter CLI" vs "biorouter Desktop" comparisons

2. **Remove or update:**
   - Commands like `biorouter session`, `biorouter configure`, etc.
   - Terminal/shell examples
   - CLI-specific troubleshooting sections

3. **Update language:**
   - References to "CLI or Desktop" → just "Desktop"
   - "Run biorouter from terminal" → "Launch BioRouter Desktop"

## 8. Remove YouTube Links (USER REQUEST)

Search and remove all YouTube embeds and links:
```bash
cd documentation
grep -r "youtube\|youtu.be" --include="*.md" --include="*.mdx" --include="*.tsx" --include="*.jsx"
```

Components to check:
- `src/components/YouTubeShortEmbed` or similar
- Any embedded video components

## 9. Update Installation/Update Instructions

As requested by user:
- Remove "Update biorouter" sections that reference CLI commands
- Replace with: "Check the [releases section](https://github.com/BaranziniLab/BioRouter/releases) on GitHub for the most recent release"
- Update macOS instructions to show only Desktop app updates

## 10. Additional Cleanup

### Low Priority - "Bird" and "Flying" References
Most are unrelated to branding:
- "Self-flying" in grants.md (line 30) - could stay as metaphor
- "Flappy Bird" in benchmark (technical reference) - OK to keep
- "Little Bird" product name (example data) - OK to keep

### Configuration Files
- `AGENTS.md` - Update brand guidelines
- `docusaurus.config.ts` - Already correct
- `package.json` - Already uses "biorouter"

## Implementation Strategy

### Phase 1: Fix Build-Blocking Issues (IMMEDIATE)
1. Rename blog directories causing broken links
2. Update all internal references to those directories
3. Verify build passes: `npm run build`

### Phase 2: Critical Rebranding (HIGH)
1. Rename and update React components
2. Rename static assets (images, videos)
3. Update component imports throughout codebase

### Phase 3: Content Updates (MEDIUM)
1. Bulk update blog post content
2. Bulk update documentation content
3. Update JSON data files

### Phase 4: User-Requested Changes (MEDIUM)
1. Remove CLI references
2. Remove YouTube links
3. Update update/installation instructions

### Phase 5: Testing & Deployment (FINAL)
1. Full build test
2. Visual inspection of key pages
3. Test GitHub Actions workflow
4. Deploy to GitHub Pages

## Automation Suggestions

Consider creating scripts for bulk operations:

```bash
# Rename blog directories
#!/bin/bash
cd documentation/blog
for dir in *goose*; do
  newdir=$(echo "$dir" | sed 's/goose/biorouter/g')
  mv "$dir" "$newdir"
done

# Update text references
find documentation -type f \( -name "*.md" -o -name "*.mdx" \) \
  -exec sed -i '' 's/\bgoose\b/biorouter/g' {} \;

# Rename image files
find documentation -type f -name "*goose*" | while read file; do
  newfile=$(echo "$file" | sed 's/goose/biorouter/g')
  mv "$file" "$newfile"
done
```

**WARNING:** Test these scripts on a backup first. Some "goose" references may be intentional (e.g., historical blog posts).

## Testing Checklist

- [ ] Build passes without errors: `npm run build`
- [ ] No broken links
- [ ] All images load correctly
- [ ] Components render properly
- [ ] Desktop UI instructions are clear
- [ ] No CLI references remain
- [ ] No YouTube embeds remain
- [ ] GitHub Actions workflow succeeds
- [ ] Site deployed successfully to GitHub Pages

## Notes

- The docusaurus configuration is already correct for GitHub Pages deployment
- Consider keeping some blog post URLs with redirects for SEO
- Historical blog posts might intentionally reference the "goose" name
- The project repository is: https://github.com/BaranziniLab/BioRouter
