import AdmZip from 'adm-zip';
import fs from 'node:fs';
import path from 'node:path';

function safeEntryName(entryName: string): string {
  if (!entryName || entryName.includes('\0')) {
    throw new Error(`Unsafe zip entry path: ${entryName}`);
  }
  if (path.isAbsolute(entryName) || path.win32.isAbsolute(entryName)) {
    throw new Error(`Unsafe zip entry path: ${entryName}`);
  }

  const parts = entryName.split(/[\\/]+/).filter(Boolean);
  if (parts.some((part) => part === '..')) {
    throw new Error(`Unsafe zip entry path: ${entryName}`);
  }
  return parts.join(path.sep);
}

export function safeZipEntryTarget(baseDir: string, entryName: string): string {
  const relativeName = safeEntryName(entryName);
  const base = path.resolve(baseDir);
  const resolved = path.resolve(base, relativeName);
  if (resolved !== base && !resolved.startsWith(base + path.sep)) {
    throw new Error(`Unsafe zip entry path: ${entryName}`);
  }
  return resolved;
}

export function safeExtractZip(zip: AdmZip, destinationDir: string): void {
  const base = path.resolve(destinationDir);
  fs.mkdirSync(base, { recursive: true });

  for (const entry of zip.getEntries()) {
    const target = safeZipEntryTarget(base, entry.entryName);
    if (entry.isDirectory) {
      fs.mkdirSync(target, { recursive: true });
      continue;
    }

    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, entry.getData());
  }
}
