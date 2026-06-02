const MIB = 1024 * 1024;

export const MAX_INGEST_FILE_BYTES = 25 * MIB;
export const MAX_INGEST_CSV_BYTES = 8 * MIB;
export const LARGE_DATASET_WARNING_BYTES = 2 * MIB;

const SUPPORTED_EXTENSIONS = new Set(['pdf', 'md', 'markdown', 'html', 'htm', 'docx', 'csv', 'txt']);
const UNSUITABLE_EXTENSIONS = new Set([
  'app',
  'bin',
  'dmg',
  'dll',
  'dylib',
  'exe',
  'iso',
  'msi',
  'pkg',
  'so',
  'zip',
]);

export interface FileDropWarning {
  id: string;
  title: string;
  message: string;
  level: 'warning' | 'error';
}

export interface FileValidationResult {
  accepted: File[];
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

function warningFor(file: File, level: 'warning' | 'error', title: string, message: string): FileDropWarning {
  return {
    id: `${file.name}-${file.size}-${level}-${title}`,
    title,
    message,
    level,
  };
}

export function validateDroppedFiles(files: File[]): FileValidationResult {
  const accepted: File[] = [];
  const warnings: FileDropWarning[] = [];

  for (const file of files) {
    const ext = extensionOf(file.name);

    if (ext === 'brkb') {
      warnings.push(
        warningFor(
          file,
          'error',
          'Use the knowledge base importer for archives',
          `${file.name} is a full knowledge-base archive. Use "Import from .brkb" in the knowledge base selector instead of dropping it into the ingest area.`,
        ),
      );
      continue;
    }

    if (UNSUITABLE_EXTENSIONS.has(ext)) {
      warnings.push(
        warningFor(
          file,
          'error',
          'This file is not suitable for knowledge ingestion',
          `${file.name} looks like an executable or packaged binary. Digest text-like source material instead, and leave installers, binaries, and compiled artifacts out of the curation set.`,
        ),
      );
      continue;
    }

    if (!SUPPORTED_EXTENSIONS.has(ext)) {
      warnings.push(
        warningFor(
          file,
          'error',
          'Unsupported file type',
          `${file.name} is not one of the supported ingest formats. Use PDF, Markdown, HTML, DOCX, CSV, or plain text.`,
        ),
      );
      continue;
    }

    const sizeLimit = ext === 'csv' ? MAX_INGEST_CSV_BYTES : MAX_INGEST_FILE_BYTES;
    if (file.size > sizeLimit) {
      warnings.push(
        warningFor(
          file,
          'error',
          'File is too large to digest safely',
          `${file.name} is ${formatBytes(file.size)}. The current limit is ${formatBytes(sizeLimit)} for ${ext.toUpperCase()} uploads. Prefer a curated excerpt, summary sheet, README, or a smaller representative slice.`,
        ),
      );
      continue;
    }

    accepted.push(file);

    if (ext === 'csv' && file.size >= LARGE_DATASET_WARNING_BYTES) {
      warnings.push(
        warningFor(
          file,
          'warning',
          'Large CSV staged with caution',
          `${file.name} is large enough that raw row-level data may crowd out the actual knowledge signal. During curation, keep codebooks or README context, omit dump-like columns/files, and prefer representative subsets over full exports.`,
        ),
      );
    }
  }

  return { accepted, warnings };
}
