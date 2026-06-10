const MIB = 1024 * 1024;

export const MAX_INGEST_FILE_BYTES = 25 * MIB;
export const MAX_INGEST_CSV_BYTES = 8 * MIB;

const UNSUITABLE_EXTENSIONS = new Set([
  'app',
  'bin',
  'dll',
  'dmg',
  'dylib',
  'exe',
  'iso',
  'msi',
  'pkg',
  'so',
]);

const TEXTLIKE_EXTENSIONS = new Set([
  'c',
  'cc',
  'cpp',
  'css',
  'csv',
  'go',
  'graphql',
  'h',
  'hpp',
  'htm',
  'html',
  'java',
  'js',
  'json',
  'jsonl',
  'md',
  'markdown',
  'mjs',
  'py',
  'rb',
  'rs',
  'sh',
  'sql',
  'svg',
  'toml',
  'ts',
  'tsx',
  'txt',
  'xml',
  'yaml',
  'yml',
]);

// Binary formats with first-class converters in the backend (convert/ in
// biorouter-mcp). Keep in sync with `guess_mime()` / the `convert()` dispatch.
const FIRST_CLASS_BINARY_EXTENSIONS = new Set([
  'docx',
  'pdf',
  'pptx',
  'xlsx',
  'xlsm',
  'xls',
  'ods',
]);

// Tabular formats share the tighter CSV cap: their markdown-table expansion
// is much larger than the file itself.
const SPREADSHEET_EXTENSIONS = new Set(['csv', 'xlsx', 'xlsm', 'xls', 'ods']);

export interface FileDropWarning {
  id: string;
  title: string;
  message: string;
  level: 'warning' | 'error';
}

export interface StagedFileCandidate {
  file?: File;
  path?: string;
  label?: string;
}

export interface FileValidationResult {
  accepted: StagedFileCandidate[];
  warnings: FileDropWarning[];
}

export function formatBytes(bytes: number): string {
  if (bytes >= MIB) {
    return `${(bytes / MIB).toFixed(bytes >= 10 * MIB ? 0 : 1)} MB`;
  }
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

function extensionOf(name: string): string {
  const trimmed = name.trim().toLowerCase();
  const dot = trimmed.lastIndexOf('.');
  if (dot < 0 || dot === trimmed.length - 1) {
    return '';
  }
  return trimmed.slice(dot + 1);
}

function warningFor(
  label: string,
  level: 'warning' | 'error',
  title: string,
  message: string
): FileDropWarning {
  return {
    id: `${label}-${level}-${title}`,
    title,
    message,
    level,
  };
}

export function validateDroppedFiles(candidates: StagedFileCandidate[]): FileValidationResult {
  const accepted: StagedFileCandidate[] = [];
  const warnings: FileDropWarning[] = [];

  for (const candidate of candidates) {
    if (!candidate.file) {
      continue;
    }

    const file = candidate.file;
    const label = candidate.label ?? file.name;
    const ext = extensionOf(file.name);

    if (ext === 'brkb') {
      warnings.push(
        warningFor(
          label,
          'error',
          'Use the knowledge base importer for archives',
          `${label} is a full knowledge-base archive. Use "Import from .brkb" in the knowledge base selector instead of staging it for digestion.`
        )
      );
      continue;
    }

    if (UNSUITABLE_EXTENSIONS.has(ext)) {
      warnings.push(
        warningFor(
          label,
          'error',
          'This file is not suitable for knowledge ingestion',
          `${label} looks like an executable or compiled binary, so it was left out of the staging set.`
        )
      );
      continue;
    }

    const sizeLimit = SPREADSHEET_EXTENSIONS.has(ext)
      ? MAX_INGEST_CSV_BYTES
      : MAX_INGEST_FILE_BYTES;
    if (file.size > sizeLimit) {
      warnings.push(
        warningFor(
          label,
          'error',
          'File is too large to digest safely',
          `${label} is ${formatBytes(file.size)}. The current limit is ${formatBytes(sizeLimit)} for this kind of staged file.`
        )
      );
      continue;
    }

    accepted.push(candidate);

    if (ext && !TEXTLIKE_EXTENSIONS.has(ext) && !FIRST_CLASS_BINARY_EXTENSIONS.has(ext)) {
      warnings.push(
        warningFor(
          label,
          'warning',
          'Staging as plain text',
          `${label} is not one of the common first-class ingest formats, so BioRouter will treat it as readable text if possible during digestion.`
        )
      );
    }
  }

  return { accepted, warnings };
}
