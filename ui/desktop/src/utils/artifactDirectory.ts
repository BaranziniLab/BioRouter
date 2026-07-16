import fs from 'node:fs/promises';
import path from 'node:path';

export type DirectoryArtifactEntry = {
  name: string;
  path: string;
  relativePath: string;
  parentPath: string;
  isDirectory: boolean;
  size?: number;
};

const MAX_DIRECTORY_TREE_ENTRIES = 10_000;
const MAX_DIRECTORY_TREE_DEPTH = 32;

export async function readArtifactDirectoryTree(rootPath: string) {
  const entries: DirectoryArtifactEntry[] = [];

  const visit = async (directoryPath: string, parentPath: string, depth: number) => {
    if (entries.length >= MAX_DIRECTORY_TREE_ENTRIES || depth > MAX_DIRECTORY_TREE_DEPTH) return;

    let children;
    try {
      children = await fs.readdir(directoryPath, { withFileTypes: true });
    } catch {
      return;
    }

    children.sort((left, right) => {
      if (left.isDirectory() !== right.isDirectory()) return left.isDirectory() ? -1 : 1;
      return left.name.localeCompare(right.name);
    });

    for (const child of children) {
      if (entries.length >= MAX_DIRECTORY_TREE_ENTRIES) return;
      if (child.name.startsWith('.') || (!child.isDirectory() && !child.isFile())) continue;

      const relativePath = parentPath ? `${parentPath}/${child.name}` : child.name;
      const entryPath = path.join(directoryPath, child.name);
      const entry: DirectoryArtifactEntry = {
        name: child.name,
        path: entryPath,
        relativePath,
        parentPath,
        isDirectory: child.isDirectory(),
      };
      if (child.isFile()) {
        try {
          entry.size = (await fs.stat(entryPath)).size;
        } catch {
          entry.size = undefined;
        }
      }
      entries.push(entry);

      if (child.isDirectory()) await visit(entryPath, relativePath, depth + 1);
    }
  };

  await visit(rootPath, '', 0);
  return entries;
}
