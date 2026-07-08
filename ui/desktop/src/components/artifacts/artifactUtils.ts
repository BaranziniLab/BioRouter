import type { EmbeddedResource, ResourceContents } from '../../api';
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
  'r',
  'rs',
  'sh',
  'sql',
  'toml',
  'ts',
  'tsx',
  'txt',
  'xml',
  'yaml',
  'yml',
]);

const IMAGE_EXTENSIONS = new Set(['gif', 'jpeg', 'jpg', 'png', 'svg', 'webp']);
const HTML_EXTENSIONS = new Set(['htm', 'html']);
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
  return decodeURIComponent(parts[parts.length - 1] || clean || 'Artifact');
}

export function extensionFromPath(value: string): string {
  const name = basenameFromPath(value);
  const dot = name.lastIndexOf('.');
  return dot > -1 ? name.slice(dot + 1).toLowerCase() : '';
}

export function languageFromPath(value: string, mimeType?: string): string {
  const ext = extensionFromPath(value);
  if (ext === 'md') return 'markdown';
  if (ext === 'yml') return 'yaml';
  if (ext === 'htm') return 'html';
  if (ext === 'js' || ext === 'jsx') return 'javascript';
  if (ext === 'ts' || ext === 'tsx') return 'typescript';
  if (ext === 'py') return 'python';
  if (ext === 'rs') return 'rust';
  if (ext === 'sh' || ext === 'bash') return 'bash';
  if (ext === 'txt' || ext === 'log') return 'text';
  if (mimeType?.includes('json')) return 'json';
  if (mimeType?.includes('html')) return 'html';
  if (mimeType?.includes('xml')) return 'xml';
  return ext || 'text';
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

export function titleFromResourceUri(uri?: string): string | null {
  if (!uri || !uri.startsWith('ui://')) return null;
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

export function looksLikePreviewableFile(value: string): boolean {
  const href = value.trim();
  if (!href || /^(https?|mailto|tel):/i.test(href)) return false;
  if (href.startsWith('file://')) return true;
  if (
    href.startsWith('/') ||
    href.startsWith('~/') ||
    href.startsWith('./') ||
    href.startsWith('../')
  ) {
    return true;
  }
  const ext = extensionFromPath(href);
  return TEXT_EXTENSIONS.has(ext) || IMAGE_EXTENSIONS.has(ext) || HTML_EXTENSIONS.has(ext);
}

export function pathFromArtifactHref(href: string): string {
  if (href.startsWith('file://')) {
    try {
      return decodeURIComponent(new URL(href).pathname);
    } catch {
      return href.replace(/^file:\/\//, '');
    }
  }
  return href;
}

export function decodeResourceHtml(resource: { blob?: string; text?: string }): string {
  if (resource.blob) {
    try {
      const bin = atob(resource.blob);
      const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
      return new TextDecoder().decode(bytes);
    } catch {
      return '';
    }
  }
  return resource.text || '';
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
  const title =
    titleFromResourceUri(resource.uri) ??
    (resource.uri ? basenameFromPath(resource.uri) : fallbackTitle);
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

  if (resource.mimeType?.includes('html') || resource.blob || resource.text) {
    return {
      kind: 'html',
      title,
      html: decodeResourceHtml(resource),
      preferredWidth,
      preferredHeight,
    };
  }

  if (resource.uri?.startsWith('http://') || resource.uri?.startsWith('https://')) {
    return { kind: 'externalUrl', title, url: resource.uri };
  }

  return { kind: 'mcpResource', title, resource, preferredWidth, preferredHeight };
}
