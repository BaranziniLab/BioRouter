# CodeGraphAgent bio-language extractors plan

> **What this is.** The task-by-task implementation plan for adding R, Julia, MATLAB and Perl tree-sitter extractors to CodeGraphAgent's vendored CodeGraph engine, and shipping them as `engine-v0.2.0` plus a `.brxt v0.1.0` release.
> **Status:** Historical record — this work was completed. CodeGraphAgent ships today as a marketplace extension covering 23 languages "incl. R/Julia/MATLAB/Perl", which is exactly this plan's deliverable. The unticked checkboxes below are the plan as authored, not an indication of outstanding work.
> **Audience:** agents executing the plan, and developers tracing how the four bio-language extractors were added.

This plan describes work in a **separate repository**, [`Broccolito/CodeGraphAgent`](https://github.com/Broccolito/CodeGraphAgent), not in the BioRouter tree. It is filed under BioRouter's docs because BioRouter is the consuming application. It is called **"Plan 2"** because it is the second of two plans derived from [the CodeGraphAgent extension design](extension-design.md): [the foundation plan](foundation-plan.md) ("Plan 1") built the repo scaffold, Python shim and release pipeline, and this one adds the languages on top.

> **How to read the identifiers.** Work is grouped into lettered **phases** named after their subject — **Q** (acquire WASM grammars), **R** (R extractor), **J** (Julia), **M** (MATLAB), **Pe** (Perl), **Z** (release) — and each phase contains numbered **tasks** (`Q1`, `R3`, `Pe5`, …). The letters carry no ordering meaning beyond the sequence in which the phases appear here.

> **Note.** Every command block below hardcodes the original author's checkout path, `/Users/wgu/Desktop/CodeGraphAgent/`. Read it as "your CodeGraphAgent checkout". The pre-work commit `a5e62e6` likewise refers to that external repository and cannot be resolved from the BioRouter tree.

> **For agentic workers:** Use superpowers:subagent-driven-development to execute task-by-task.

**Goal:** Add R, Julia, MATLAB, and Perl extractors to the vendored CodeGraph engine. Ship as `engine-v0.2.0` + `.brxt v0.1.0`.

**Architecture:** For each language, follow upstream's well-factored pattern: vendor the tree-sitter WASM grammar into `engine/src/extraction/wasm/`, register it in `engine/src/types.ts` + `engine/src/extraction/grammars.ts`, write a `LanguageExtractor` TS file in `engine/src/extraction/languages/<lang>.ts`, and import it from `engine/src/extraction/languages/index.ts`. Add vitest fixtures. Document in `engine/PATCHES.md`. The `.m` extension is shared between MATLAB and Objective-C, resolved by a content heuristic.

**Tech stack:** TypeScript (engine), tree-sitter / web-tree-sitter, vitest. No Python changes — the Python shim is language-agnostic.

**Spec:** [CodeGraphAgent BioRouter extension design](extension-design.md) (this plan delivers its "Engine fork — language additions for v0.1" section).

**Working directory:** `/Users/wgu/Desktop/CodeGraphAgent/`. Engine source under `engine/`. HEAD before this plan starts: `a5e62e6` (post-v0.1.0-rc1 release).

---

## Phase Q — Acquire WASM grammars

### Task Q1: Download `tree-sitter-r.wasm`

**Files:** Create `engine/src/extraction/wasm/tree-sitter-r.wasm`

- [ ] **Step 1:** Download from r-lib/tree-sitter-r v1.2.0

```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
gh release download v1.2.0 \
  --repo r-lib/tree-sitter-r \
  --pattern 'tree-sitter-r.wasm' \
  --output engine/src/extraction/wasm/tree-sitter-r.wasm && \
ls -lh engine/src/extraction/wasm/tree-sitter-r.wasm
```

Expected: file size ~481 KB.

- [ ] **Step 2:** No commit yet — Q-phase commits batch at end of Q4.

### Task Q2: Download `tree-sitter-julia.wasm`

**Files:** Create `engine/src/extraction/wasm/tree-sitter-julia.wasm`

- [ ] **Step 1:** Download from tree-sitter/tree-sitter-julia v0.25.0

```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
gh release download v0.25.0 \
  --repo tree-sitter/tree-sitter-julia \
  --pattern 'tree-sitter-julia.wasm' \
  --output engine/src/extraction/wasm/tree-sitter-julia.wasm && \
ls -lh engine/src/extraction/wasm/tree-sitter-julia.wasm
```

Expected: ~2.6 MB.

### Task Q3: Build `tree-sitter-matlab.wasm`

**Files:** Create `engine/src/extraction/wasm/tree-sitter-matlab.wasm`

acristoffers/tree-sitter-matlab doesn't ship WASM as a release asset (only Python wheels). Build it locally via the tree-sitter CLI + Docker.

- [ ] **Step 1:** Clone the grammar and build the WASM

```bash
cd /tmp && \
gh repo clone acristoffers/tree-sitter-matlab matlab-grammar -- --depth 1 && \
cd matlab-grammar && \
npx -y tree-sitter-cli@latest build --wasm --docker -o /tmp/tree-sitter-matlab.wasm && \
cp /tmp/tree-sitter-matlab.wasm /Users/wgu/Desktop/CodeGraphAgent/engine/src/extraction/wasm/tree-sitter-matlab.wasm && \
ls -lh /Users/wgu/Desktop/CodeGraphAgent/engine/src/extraction/wasm/tree-sitter-matlab.wasm
```

If the build fails, fall back to using emscripten directly or pin a different MATLAB grammar repo that publishes WASMs.

### Task Q4: Download `tree-sitter-perl.wasm`

**Files:** Create `engine/src/extraction/wasm/tree-sitter-perl.wasm`

- [ ] **Step 1:** Download from tree-sitter-perl/tree-sitter-perl v1.0.2

```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
gh release download v1.0.2 \
  --repo tree-sitter-perl/tree-sitter-perl \
  --pattern 'tree-sitter-perl.wasm' \
  --output engine/src/extraction/wasm/tree-sitter-perl.wasm && \
ls -lh engine/src/extraction/wasm/tree-sitter-perl.wasm
```

Expected: ~4.1 MB.

### Task Q5: Commit all four WASMs

- [ ] **Step 1:** Stage and commit the vendored grammars

```bash
cd /Users/wgu/Desktop/CodeGraphAgent && \
git add engine/src/extraction/wasm/tree-sitter-{r,julia,matlab,perl}.wasm && \
git commit -m "vendor: tree-sitter WASM grammars for R/Julia/MATLAB/Perl"
```

---

## Phase R — R extractor

### Task R1: Register R in `engine/src/types.ts`

- [ ] Add `'r'` to the `Language` union type.

Find the existing `export type Language = '...'` definition. Add `'r'`.

### Task R2: Register R in `engine/src/extraction/grammars.ts`

- [ ] Add `r: 'tree-sitter-r.wasm'` to `WASM_GRAMMAR_FILES`.
- [ ] Add R file extensions to `EXTENSION_MAP`:

```typescript
'.R': 'r',
'.r': 'r',
'.Rmd': 'r',  // R Markdown — code chunks
```

### Task R3: Create `engine/src/extraction/languages/r.ts`

R AST node kinds (from tree-sitter-r grammar):

- functions: `function_definition` (also `left_assignment` with function value, but `function_definition` is the canonical node)
- calls: `call`
- imports: `library` / `require` calls (R has no `import` statement — modules are loaded with `library(pkg)`, `require(pkg)`)

```typescript
import { getNodeText, getChildByField } from '../tree-sitter-helpers';
import type { LanguageExtractor } from '../tree-sitter-types';

export const rExtractor: LanguageExtractor = {
  functionTypes: ['function_definition'],
  classTypes: [],  // R uses S3/S4/R5/R6 — no first-class class node
  methodTypes: [],
  interfaceTypes: [],
  structTypes: [],
  enumTypes: [],
  typeAliasTypes: [],
  importTypes: [],  // library() detection handled below
  callTypes: ['call'],
  variableTypes: ['left_assignment', 'right_assignment', 'equals_assignment'],
  nameField: 'name',
  bodyField: 'body',
  paramsField: 'parameters',
  returnField: undefined,
  getSignature: (node, source) => {
    const params = getChildByField(node, 'parameters');
    return params ? getNodeText(params, source) : undefined;
  },
  isAsync: () => false,
  isStatic: () => false,
  extractImport: (node, source) => {
    // R has no import statement; this is wired through callTypes for library()/require().
    return null;
  },
};
```

### Task R4: Register in `engine/src/extraction/languages/index.ts`

Add import + EXTRACTORS entry:

```typescript
import { rExtractor } from './r';
// ...
export const EXTRACTORS: Partial<Record<Language, LanguageExtractor>> = {
  // ... existing entries
  r: rExtractor,
};
```

### Task R5: Add vitest fixture and test

Create `engine/__tests__/extraction/r.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { extractFromSource } from './_helpers';  // or whatever helper upstream uses

describe('R extractor', () => {
  it('extracts function definitions', async () => {
    const source = `
hello <- function(name) {
  paste0("Hello, ", name)
}
greet <- function() hello("world")
`;
    const result = await extractFromSource(source, 'r');
    const names = result.functions.map(f => f.name);
    expect(names).toContain('hello');
    expect(names).toContain('greet');
  });

  it('extracts call edges', async () => {
    const source = `
hello <- function() paste0("hi")
greet <- function() hello()
`;
    const result = await extractFromSource(source, 'r');
    const callees = result.calls.map(c => c.callee);
    expect(callees).toContain('hello');
  });
});
```

If `extractFromSource` doesn't exist in upstream's test helpers, look at an existing language test (e.g. `python.test.ts`) and copy its pattern.

### Task R6: Update `engine/PATCHES.md`

Add an entry under a new "Language additions" section listing R, its grammar source SHA, and the files we added.

---

## Phase J — Julia extractor

Identical pattern to Phase R. Per-step files/code:

- **J1** `engine/src/types.ts` — add `'julia'` to Language
- **J2** `engine/src/extraction/grammars.ts` — `julia: 'tree-sitter-julia.wasm'`, extension `'.jl': 'julia'`
- **J3** `engine/src/extraction/languages/julia.ts` — Julia AST: `function_definition`, `short_function_definition`, `macro_definition`. Imports: `using` / `import` statements.
- **J4** Register in `index.ts`
- **J5** `__tests__/extraction/julia.test.ts` with Julia fixture (function definitions, call edges)
- **J6** Update PATCHES.md

Commit as one task: `feat(engine): Julia extractor`.

---

## Phase M — MATLAB extractor and `.m` disambiguation

MATLAB shares `.m` extension with Objective-C. Existing engine maps `.m` → `objc`. We add a content heuristic.

- **M1** `engine/src/types.ts` — add `'matlab'`
- **M2** `engine/src/extraction/grammars.ts` — `matlab: 'tree-sitter-matlab.wasm'`. **Do NOT add `.m` to EXTENSION_MAP** — that stays mapped to `objc` as the default. Instead, add a new function `detectLanguage(filePath: string, content: string): Language` that uses content sniffing for `.m`:

```typescript
export function detectLanguage(filePath: string, content: string): Language {
  const ext = path.extname(filePath);
  if (ext === '.m') {
    // ObjC if Objective-C markers present; MATLAB otherwise (the more common
    // case in scientific repos).
    const head = content.slice(0, 4096);
    const objcMarkers = /^\s*(@interface|@implementation|#import|#include)/m;
    return objcMarkers.test(head) ? 'objc' : 'matlab';
  }
  return EXTENSION_MAP[ext] ?? 'unknown';
}
```

Update the engine code path that currently uses `EXTENSION_MAP[ext]` directly to call `detectLanguage(path, content)` instead.

- **M3** `engine/src/extraction/languages/matlab.ts` — MATLAB AST: `function_definition`. No classes (MATLAB classes are `classdef`, may add later). Imports via `import` statement (rare).
- **M4** Register in `index.ts`
- **M5** Tests — incl. `.m` disambiguation: one fixture with `@interface` → objc; one without → matlab
- **M6** Update PATCHES.md

Commit: `feat(engine): MATLAB extractor with .m disambiguation`.

---

## Phase Pe — Perl extractor

- **Pe1** `engine/src/types.ts` — add `'perl'`
- **Pe2** `engine/src/extraction/grammars.ts` — `perl: 'tree-sitter-perl.wasm'`, extensions `'.pl'`, `'.pm'`, `'.t'`
- **Pe3** `engine/src/extraction/languages/perl.ts` — Perl AST: `subroutine_declaration_statement`. Packages via `package` statement.
- **Pe4** Register in `index.ts`
- **Pe5** Tests
- **Pe6** Update PATCHES.md

Commit: `feat(engine): Perl extractor`.

---

## Phase Z — Release

### Task Z1: Verify the engine still type-checks and tests pass

```bash
cd /Users/wgu/Desktop/CodeGraphAgent/engine && \
npm install --silent && \
npx tsc --noEmit && \
npm test
```

Expected: type-check clean, all engine tests pass (including the new R/Julia/MATLAB/Perl tests).

### Task Z2: Bump engine version

Edit `engine/package.json` — bump `"version"` from current (likely `0.9.7`) to a new value matching our release tag for this plan. Use `0.9.7-bio.1` to express "upstream 0.9.7 + our bio language patches".

### Task Z3: Update CHANGELOG.md

Bump from `v0.1.0-rc1` to `v0.1.0` with a new bullet listing the 4 bio languages added.

### Task Z4: Bump `release_manifest.json`

Update `engine_version` and `base_url`:

```json
{
  "engine_version": "0.2.0",
  "base_url": "https://github.com/Broccolito/CodeGraphAgent/releases/download/engine-v0.2.0/",
  ...
}
```

Set the SHAs back to `"PLACEHOLDER_FILLED_AT_RELEASE"` (Task Z6 fills them after the build).

### Task Z5: Commit and push

```bash
git add -A && git commit -m "release: prep for engine-v0.2.0 + .brxt v0.1.0" && git push
```

### Task Z6: Trigger engine build workflow

```bash
gh workflow run "Build engine bundles" -f release_tag=engine-v0.2.0
gh run watch
```

Wait for completion. Then download SHA256SUMS from the resulting release, update `release_manifest.json`, commit + push.

### Task Z7: Publish v0.1.0 .brxt release

```bash
gh workflow run "Release .brxt" -f release_tag=v0.1.0
gh run watch
gh release view v0.1.0
```

---

## Done — what's working after this plan

- 4 new languages: R, Julia, MATLAB, Perl
- Each with: WASM grammar + LanguageExtractor + vitest tests + PATCHES.md entry
- MATLAB disambiguated from ObjC for `.m` files via content heuristic
- New engine release `engine-v0.2.0` with 6 platform bundles
- New .brxt release `v0.1.0` pinning the new engine

## Risks

| Risk | Mitigation |
| --- | --- |
| MATLAB WASM build fails locally | Fall back to alternative grammar repos or skip MATLAB from v0.1.0 |
| R/Julia AST node names differ from what we wrote | First test failure reveals the exact node kind; update extractor accordingly |
| `.m` disambiguation false-positives | Test fixtures cover both directions; tunable regex |
| Engine type-check fails with our `Language` union additions | Test catches; reorder enum if needed |

## Related documentation

- [CodeGraphAgent BioRouter extension design](extension-design.md) — the design this plan implements, including the rationale for the `.m` content heuristic.
- [CodeGraphAgent foundation plan](foundation-plan.md) — the preceding plan ("Plan 1") that built the repo scaffold, Python shim and release pipeline this plan extends.
- [Extensions and skills guide](../../extensions/extensions-and-skills-guide.md) — how BioRouter installs and enables the resulting `.brxt`.
- [Extension manager](../../extensions/built-in/extension-manager.md) — the built-in MCP server that manages extension lifecycle at runtime.
