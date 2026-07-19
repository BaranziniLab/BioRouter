import type { EmbeddedResource, RawResource, ResourceContents } from '../../api';
import type { ThemeFamily } from '../../contexts/ThemeContext';
import { GENERATED_THEMES } from '../../styles/themes.generated';
import type { ArtifactSource } from './artifactTypes';

const TEXT_EXTENSIONS = new Set([
  'bash',
  'c',
  'cc',
  'conf',
  'cpp',
  'cs',
  'css',
  'csv',
  'go',
  'h',
  'hpp',
  'html',
  'java',
  'js',
  'json',
  'jsx',
  'log',
  'md',
  'py',
  'qmd',
  'r',
  'rmd',
  'rs',
  'sh',
  'sql',
  'toml',
  'ts',
  'tsv',
  'tsx',
  'txt',
  'xml',
  'yaml',
  'yml',
]);

const IMAGE_EXTENSIONS = new Set(['gif', 'jpeg', 'jpg', 'png', 'svg', 'webp']);
const HTML_EXTENSIONS = new Set(['htm', 'html']);
const DOCUMENT_EXTENSIONS = new Set(['docx', 'ipynb', 'pdf', 'pptx', 'xlsx']);
const MAX_BROWSER_URL_BYTES = 8 * 1024;
const GENERIC_UI_TITLE_PARTS = new Set([
  'chart',
  'diagram',
  'graph',
  'interactive',
  'preview',
  'visualization',
]);

export function basenameFromPath(value: string): string {
  const clean = value.split(/[?#]/)[0];
  const parts = clean.split(/[\\/]/).filter(Boolean);
  const base = parts[parts.length - 1] || clean || 'Artifact';
  // A real filename can contain a literal `%` that is not percent-encoding
  // (e.g. "results 100%.csv") — decodeURIComponent throws URIError: "URI
  // malformed" on those. This runs during chat render
  // (collectArtifactsFromMessages), so an unguarded throw crashes the whole app
  // into the "Honk!" error boundary. Fall back to the raw basename instead.
  try {
    return decodeURIComponent(base);
  } catch {
    return base;
  }
}

export function extensionFromPath(value: string): string {
  const name = basenameFromPath(value);
  const dot = name.lastIndexOf('.');
  return dot > -1 ? name.slice(dot + 1).toLowerCase() : '';
}

// Extension -> Prism language. Anything missing falls through to the extension
// itself (Prism knows `r`, `sql`, `go`, `json`, …), then to plain text.
const PRISM_LANGUAGES: Record<string, string> = {
  bash: 'bash',
  cc: 'cpp',
  cs: 'csharp',
  conf: 'ini',
  h: 'c',
  hpp: 'cpp',
  htm: 'html',
  js: 'javascript',
  jsonl: 'json',
  jsx: 'jsx',
  log: 'text',
  markdown: 'markdown',
  md: 'markdown',
  py: 'python',
  // R Markdown / Quarto are markdown with fenced R chunks.
  qmd: 'markdown',
  rmd: 'markdown',
  rs: 'rust',
  sh: 'bash',
  toml: 'toml',
  ts: 'typescript',
  tsx: 'tsx',
  txt: 'text',
  yml: 'yaml',
};

export function languageFromPath(value: string, mimeType?: string): string {
  const ext = extensionFromPath(value);
  const mapped = PRISM_LANGUAGES[ext];
  if (mapped) return mapped;
  if (ext) return ext;
  if (mimeType?.includes('json')) return 'json';
  if (mimeType?.includes('html')) return 'html';
  if (mimeType?.includes('xml')) return 'xml';
  return 'text';
}

// Names that title-casing gets wrong: acronyms ("Csv") and camel-cased brands
// ("Typescript"). Keyed by the Prism language, or by extension where it differs.
const LANGUAGE_LABELS: Record<string, string> = {
  bash: 'Shell',
  cpp: 'C++',
  csharp: 'C#',
  css: 'CSS',
  csv: 'CSV',
  html: 'HTML',
  ini: 'INI',
  javascript: 'JavaScript',
  json: 'JSON',
  jsx: 'JSX',
  markdown: 'Markdown',
  sql: 'SQL',
  text: 'Text',
  toml: 'TOML',
  tsv: 'TSV',
  tsx: 'TSX',
  typescript: 'TypeScript',
  xml: 'XML',
  yaml: 'YAML',
};

// Human label for the language chip above a code preview.
export function languageLabel(value: string, mimeType?: string): string {
  const ext = extensionFromPath(value);
  if (ext === 'r') return 'R';
  if (ext === 'rmd') return 'R Markdown';
  if (ext === 'qmd') return 'Quarto';
  const language = languageFromPath(value, mimeType);
  return LANGUAGE_LABELS[language] ?? language.charAt(0).toUpperCase() + language.slice(1);
}

// Three label tokens for the whole artifact panel — the "one status strip"
// typography (design.md §3.2 / D-31), shared by every preview (code, tables,
// markdown, git, notebook) so the strip reads in one voice instead of each
// preview inventing its own sub-header.
//
// D-31 draws the line these three sit on: MONO FOR DATA, SANS FOR CHROME. The
// panel had all of its strip text in mono, which made the preview's chrome read
// in a different voice from the chat beside it. Mono is not decoration here; it
// is a claim that the glyphs matter (you will read this character by character,
// or the digits must not jitter). A word like "Modified" makes no such claim.

/** 11px caps SANS — the language chip and legend labels. Chrome: names a thing. */
export const STRIP_LABEL_CLASS =
  'text-[11px] font-semibold uppercase tracking-[0.09em] text-text-subtle';
/**
 * 11px MONO — paths, git refs, and tabular counts.
 *
 * These earn it, which is exactly why D-31 left them alone: a path is read
 * character by character (l vs 1 vs I), and a count carrying `tabular-nums`
 * wants stable digit widths so it does not jitter as it changes.
 */
export const STRIP_IDENT_CLASS = 'font-mono text-[11px] text-text-muted';
/** 11px SANS — everything the strip states in prose (statuses, notes). */
export const STRIP_META_CLASS = 'text-[11px] text-text-muted';

/** Splits a path so the strip can dim the directory and keep the filename legible. */
export function splitPathForStrip(path: string) {
  const name = basenameFromPath(path);
  return { directory: path.slice(0, path.length - name.length), name };
}

function titleCaseWords(value: string): string {
  return value
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/[-_]+/g, ' ')
    .split(/\s+/)
    .filter(Boolean)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ');
}

/**
 * The resolved colours a CSP-sandboxed preview must inline, for the family and
 * mode the app is actually in.
 *
 * `NotebookPreview` and `DocumentPreview` build their own HTML document and hand
 * it to a `sandbox=""` iframe under `default-src 'none'`. That document cannot
 * load the app stylesheet, so `var(--text-default)` resolves to nothing inside it
 * and the page would paint unstyled. Inlining literal hexes is therefore not a
 * shortcut, it is the only option — but they must be THIS family's hexes. They
 * used to be hand-picked light/dark pairs (`#242321` on dark), which belonged to
 * no family at all: Parchment's dark ground is `#16120c`, Alma Mater's is navy
 * `#08213f`, Roche Limit's is `#1b1b19`. Every family got the same wrong grey.
 *
 * Falls back to Parchment for an unknown family (a `theme_family` left in
 * localStorage by a build that had one we since removed), exactly as
 * `BioRouterMark` does.
 */
export function sandboxedSurface(family: ThemeFamily, theme: 'light' | 'dark') {
  return (GENERATED_THEMES[family] ?? GENERATED_THEMES.parchment)[theme].surface;
}

// Inject the desktop app's resolved theme as `window.__BR_VIZ_HOST_THEME__` so an
// Auto Visualiser figure/report rendered in a `srcdoc` iframe — which has no query
// string — follows the Biorouter app theme instead of the OS `prefers-color-scheme`.
// This is what keeps the side-panel preview identical to the expanded/opened view
// (both then resolve the same theme). The script must run before the figure's own
// runtime (`{{COMMON}}`), which sits right after `<head>`; a baked tool theme
// (`window.__BR_VIZ_THEME__`) still wins over this host default.
//
// Deliberately carries the light/dark mode ONLY, not the theme family: the figure
// on the other side is generated by the Rust backend
// (`crates/biorouter-mcp/src/autovisualiser/templates/_common.js`) and resolves
// light vs dark with a fixed palette. Injecting a family it cannot read would be
// decoration. See the note at `openArtifactInWindow` in main.ts.
export function withHostTheme(html: string, theme: 'light' | 'dark'): string {
  const tag = `<script>window.__BR_VIZ_HOST_THEME__=${JSON.stringify(theme)};</script>`;
  const marker = '<head>';
  const idx = html.indexOf(marker);
  if (idx === -1) return tag + html;
  return html.slice(0, idx + marker.length) + tag + html.slice(idx + marker.length);
}

export function titleFromResourceUri(uri?: string): string | null {
  if (!uri || uri.length > MAX_BROWSER_URL_BYTES || !uri.startsWith('ui://')) return null;
  let parts: string[];
  try {
    const parsed = new URL(uri);
    parts = [parsed.hostname, ...parsed.pathname.split('/')]
      .map((part) => part.trim())
      .filter(Boolean);
  } catch {
    parts = uri
      .replace(/^ui:\/\//, '')
      .split(/[/?#]/)
      .map((part) => part.trim())
      .filter(Boolean);
  }
  if (parts.length === 0) return null;
  if (parts.some((part) => /\.[a-z0-9]+$/i.test(part))) return null;

  const specific = parts.filter((part) => !GENERIC_UI_TITLE_PARTS.has(part.toLowerCase()));
  if (specific.length > 0) {
    const suffix = [...parts]
      .reverse()
      .find((part) => GENERIC_UI_TITLE_PARTS.has(part.toLowerCase()));
    return [...specific, ...(suffix ? [suffix] : [])].map(titleCaseWords).join(' ');
  }

  const lowerParts = parts.map((part) => part.toLowerCase());
  if (lowerParts.includes('interactive') && lowerParts.includes('chart'))
    return 'Interactive Chart';
  return parts.map(titleCaseWords).join(' ');
}

const MARKDOWN_EXTENSIONS = new Set(['markdown', 'md', 'qmd', 'rmd']);
const DELIMITED_EXTENSIONS = new Set(['csv', 'tsv']);

export function isMarkdownPath(value: string): boolean {
  return MARKDOWN_EXTENSIONS.has(extensionFromPath(value));
}

export function isDelimitedPath(value: string): boolean {
  return DELIMITED_EXTENSIONS.has(extensionFromPath(value));
}

// Parse a CSV/TSV table, honouring quoted fields (which may contain the
// delimiter, escaped quotes, and newlines). Rows are capped by the caller.
export function parseDelimitedTable(text: string, delimiter: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = '';
  let inQuotes = false;

  for (let i = 0; i < text.length; i++) {
    const char = text[i];

    if (inQuotes) {
      if (char === '"') {
        if (text[i + 1] === '"') {
          field += '"';
          i++;
        } else {
          inQuotes = false;
        }
      } else {
        field += char;
      }
      continue;
    }

    if (char === '"' && field === '') {
      inQuotes = true;
    } else if (char === delimiter) {
      row.push(field);
      field = '';
    } else if (char === '\n' || char === '\r') {
      if (char === '\r' && text[i + 1] === '\n') i++;
      row.push(field);
      rows.push(row);
      row = [];
      field = '';
    } else {
      field += char;
    }
  }

  if (field !== '' || row.length > 0) {
    row.push(field);
    rows.push(row);
  }
  return rows.filter((r) => r.length > 1 || r[0] !== '');
}

export function looksLikePreviewableFile(value: string): boolean {
  const href = value.trim();
  if (!href || /^(https?|mailto|tel):/i.test(href)) return false;
  const localPath = /^file:\/\//i.test(href) ? pathFromArtifactHref(href) : href.split(/[?#]/)[0];
  const trimmedPath = localPath.replace(/[/\\]+$/, '');
  const basename = basenameFromPath(trimmedPath).toLowerCase();
  if (
    !trimmedPath ||
    /^[/\\]+$/.test(localPath) ||
    /^(?:~|\.{1,2}|[a-z]:)$/i.test(trimmedPath) ||
    ['.git', '.hg', '.svn', '.ds_store'].includes(basename)
  ) {
    return false;
  }
  if (/^file:\/\//i.test(href)) return true;
  if (
    localPath.startsWith('/') ||
    localPath.startsWith('~/') ||
    localPath.startsWith('./') ||
    localPath.startsWith('../') ||
    /^[a-z]:[\\/]/i.test(localPath) ||
    localPath.startsWith('\\\\')
  ) {
    return true;
  }
  const ext = extensionFromPath(localPath);
  return (
    TEXT_EXTENSIONS.has(ext) ||
    IMAGE_EXTENSIONS.has(ext) ||
    HTML_EXTENSIONS.has(ext) ||
    DOCUMENT_EXTENSIONS.has(ext)
  );
}

export function pathFromArtifactHref(href: string): string {
  if (/^file:\/\//i.test(href)) {
    try {
      return decodeURIComponent(new URL(href).pathname);
    } catch {
      return href.replace(/^file:\/\//i, '');
    }
  }
  return href;
}

const MAX_EMBEDDED_HTML_BYTES = 16 * 1024 * 1024;
const MAX_ENCODED_HTML_BYTES = Math.ceil((MAX_EMBEDDED_HTML_BYTES * 4) / 3) + 4;
const MAX_URI_LIST_BYTES = 64 * 1024;
const MAX_ARTIFACT_TITLE_CHARS = 256;

function sanitizeArtifactTitle(title: string, fallback = 'Artifact'): string {
  const sanitized = title
    .replace(/[\p{Cc}\p{Cf}]/gu, '')
    .trim()
    .slice(0, MAX_ARTIFACT_TITLE_CHARS);
  if (sanitized) return sanitized;
  return (
    fallback
      .replace(/[\p{Cc}\p{Cf}]/gu, '')
      .trim()
      .slice(0, MAX_ARTIFACT_TITLE_CHARS) || 'Artifact'
  );
}

function isHtmlMime(mimeType?: string): boolean {
  const essence = mimeType?.split(';', 1)[0]?.trim().toLowerCase();
  return essence === 'text/html' || essence === 'application/xhtml+xml';
}

function isUriListMime(mimeType?: string): boolean {
  return mimeType?.split(';', 1)[0]?.trim().toLowerCase() === 'text/uri-list';
}

function externalBrowserUrl(uri?: string): string | null {
  if (!uri || uri.length > MAX_BROWSER_URL_BYTES) return null;
  try {
    const url = new URL(uri);
    const safeProtocol = url.protocol === 'http:' || url.protocol === 'https:';
    return safeProtocol && !url.username && !url.password ? url.toString() : null;
  } catch {
    return null;
  }
}

function decodeResourceText(
  resource: { blob?: string; text?: string },
  maxBytes: number,
  maxEncodedBytes = Math.ceil((maxBytes * 4) / 3) + 4
): string | null {
  if (typeof resource.blob === 'string') {
    if (resource.blob.length > maxEncodedBytes) return null;
    try {
      const bin = atob(resource.blob);
      if (bin.length > maxBytes) return null;
      const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
      return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    } catch {
      return null;
    }
  }
  if (typeof resource.text === 'string') {
    if (resource.text.length > maxBytes) return null;
    return new TextEncoder().encode(resource.text).byteLength <= maxBytes ? resource.text : null;
  }
  return null;
}

export function decodeResourceHtml(resource: { blob?: string; text?: string }): string | null {
  return decodeResourceText(resource, MAX_EMBEDDED_HTML_BYTES, MAX_ENCODED_HTML_BYTES);
}

function browserUrlFromUriList(resource: { blob?: string; text?: string }): string | null {
  const list = decodeResourceText(resource, MAX_URI_LIST_BYTES);
  if (list === null) return null;
  for (const line of list.split(/\r?\n/)) {
    const candidate = line.trim();
    if (!candidate || candidate.startsWith('#')) continue;
    const url = externalBrowserUrl(candidate);
    if (url) return url;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Files the agent creates
//
// A `ui://` resource announces itself, but a file the agent writes with the
// developer extension leaves no trace in the panel unless the assistant happens
// to mention its path in prose. These helpers read the tool call itself, so a
// written report, R script, CSV or generated image opens in the preview the same
// way a visualization does.
// ---------------------------------------------------------------------------

// Tools whose arguments name a file the agent is creating or rewriting.
const FILE_WRITING_TOOLS = new Set([
  'create_file',
  'edit_file',
  'multi_edit',
  'notebook_edit',
  'str_replace_editor',
  'text_editor',
  'write_file',
]);

// `text_editor`-style commands that leave a file changed on disk. `view` and
// `undo_edit` do not produce something new to look at.
const MUTATING_EDITOR_COMMANDS = new Set(['create', 'diff', 'insert', 'str_replace', 'write']);

const PATH_ARGUMENT_KEYS = [
  'path',
  'file_path',
  'filePath',
  'filename',
  'file_name',
  'target_file',
  'absolute_path',
];

// Shell redirections and the conventional output flags. Deliberately narrow:
// scanning all of stdout for path-like text turns an `ls` into a dozen bogus
// artifacts, and the panel auto-opens whatever arrived last.
const SHELL_OUTPUT_RE =
  /(?:>>?|(?:^|\s)(?:--outfile|--output|--out|-out|-o)[=\s]+)\s*(?:"([^"]+)"|'([^']+)'|([^\s;|&>]+))/g;

// The tool name as the extension exposes it, e.g. `developer__text_editor`.
export function baseToolName(toolName: string): string {
  const delimiter = toolName.lastIndexOf('__');
  return delimiter === -1 ? toolName : toolName.slice(delimiter + 2);
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

// `C:\dir`, `C:/dir` or a `\\server\share` UNC path. Without this, a Windows
// absolute path looks relative and gets glued onto the working directory.
const WINDOWS_ABSOLUTE_RE = /^(?:[a-zA-Z]:[\\/]|\\\\)/;

// Resolve a possibly-relative path against the session's working directory, so
// `results/plot.png` from a shell command becomes something the viewer can read.
export function resolveArtifactPath(rawPath: string, workingDir?: string): string | null {
  const path = pathFromArtifactHref(rawPath.trim().replace(/^["']|["']$/g, ''));
  if (!path) return null;
  if (path.startsWith('/') || path.startsWith('~') || WINDOWS_ABSOLUTE_RE.test(path)) return path;
  if (!workingDir) return null;
  const cleaned = path.replace(/^\.[\\/]/, '');
  if (/^\.\.[\\/]/.test(cleaned) || cleaned === '.' || cleaned === '..') return null;
  const separator = WINDOWS_ABSOLUTE_RE.test(workingDir) ? '\\' : '/';
  return `${workingDir.replace(/[\\/]+$/, '')}${separator}${cleaned}`;
}

function isPreviewableArtifactPath(path: string): boolean {
  const ext = extensionFromPath(path);
  return TEXT_EXTENSIONS.has(ext) || IMAGE_EXTENSIONS.has(ext) || HTML_EXTENSIONS.has(ext);
}

// File paths a tool call is about to create, read off its arguments.
//
// Returns absolute paths only — a relative one without a `workingDir` to anchor
// it would resolve against the wrong directory in the viewer.
export function fileArtifactPathsFromToolCall(
  toolName: string,
  args: unknown,
  workingDir?: string
): string[] {
  const argRecord = asRecord(args);
  if (!argRecord) return [];
  const name = baseToolName(toolName);

  if (name === 'shell' || name === 'bash') {
    const command = argRecord.command;
    if (typeof command !== 'string') return [];
    const paths: string[] = [];
    for (const match of command.matchAll(SHELL_OUTPUT_RE)) {
      const candidate = match[1] ?? match[2] ?? match[3];
      if (!candidate) continue;
      const resolved = resolveArtifactPath(candidate, workingDir);
      if (resolved && isPreviewableArtifactPath(resolved)) paths.push(resolved);
    }
    return paths;
  }

  if (!FILE_WRITING_TOOLS.has(name)) return [];

  // `text_editor` multiplexes read and write behind a `command` argument, so a
  // `view` must not open a preview of a file the agent only looked at.
  const command = argRecord.command;
  if (typeof command === 'string' && !MUTATING_EDITOR_COMMANDS.has(command)) return [];

  for (const key of PATH_ARGUMENT_KEYS) {
    const value = argRecord[key];
    if (typeof value !== 'string' || !value.trim()) continue;
    const resolved = resolveArtifactPath(value, workingDir);
    return resolved ? [resolved] : [];
  }
  return [];
}

export function artifactSourceFromResource(
  content: EmbeddedResource & { type: 'resource' },
  fallbackTitle = 'Artifact'
): ArtifactSource | null {
  const resource = content.resource as ResourceContents & {
    uri?: string;
    mimeType?: string;
    blob?: string;
    text?: string;
    _meta?: Record<string, unknown>;
  };
  if (resource.uri && resource.uri.length > MAX_BROWSER_URL_BYTES) return null;
  const title = sanitizeArtifactTitle(
    titleFromResourceUri(resource.uri) ??
      (resource.uri ? basenameFromPath(resource.uri) : fallbackTitle),
    fallbackTitle
  );
  const prefSize = resource._meta?.['mcpui.dev/ui-preferred-frame-size'] as
    | [string, string]
    | undefined;
  const pxOf = (v?: string): number | undefined => {
    if (!v) return undefined;
    const n = parseInt(v, 10);
    return Number.isFinite(n) && /px$/.test(v) ? n : undefined;
  };
  const preferredWidth = pxOf(prefSize?.[0]);
  const preferredHeight = pxOf(prefSize?.[1]);

  const resourceUrl = externalBrowserUrl(resource.uri);
  if (resourceUrl) {
    return { kind: 'externalUrl', title, url: resourceUrl };
  }

  if (isHtmlMime(resource.mimeType)) {
    const html = decodeResourceHtml(resource);
    if (html === null) return null;
    return {
      kind: 'html',
      title,
      html,
      preferredWidth,
      preferredHeight,
    };
  }

  if (isUriListMime(resource.mimeType)) {
    const url = browserUrlFromUriList(resource);
    return url ? { kind: 'externalUrl', title, url } : null;
  }

  return { kind: 'mcpResource', title, resource, preferredWidth, preferredHeight };
}

export function artifactSourceFromResourceLink(resource: RawResource): ArtifactSource | null {
  const url = externalBrowserUrl(resource.uri);
  if (!url) return null;
  return {
    kind: 'externalUrl',
    title: sanitizeArtifactTitle(
      resource.title?.trim() || resource.name?.trim() || basenameFromPath(resource.uri)
    ),
    url,
  };
}
