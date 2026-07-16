import { spawn } from 'node:child_process';
import fs from 'node:fs/promises';
import path from 'node:path';

export type GitArtifactStatus = 'untracked' | 'modified' | 'staged' | 'committed' | 'pushed';

export type GitArtifactEntry = {
  name: string;
  path: string;
  relativePath: string;
  parentPath: string;
  isDirectory: boolean;
  status: GitArtifactStatus;
};

const MAX_GIT_OUTPUT_BYTES = 24 * 1024 * 1024;
const statusPriority: Record<GitArtifactStatus, number> = {
  untracked: 5,
  modified: 4,
  staged: 3,
  committed: 2,
  pushed: 1,
};

async function runGit(rootPath: string, args: string[]) {
  return new Promise<Buffer | null>((resolve) => {
    const child = spawn('git', ['-C', rootPath, ...args], {
      stdio: ['ignore', 'pipe', 'ignore'],
      windowsHide: true,
    });
    const chunks: Buffer[] = [];
    let totalBytes = 0;
    let settled = false;

    child.stdout.on('data', (chunk: Buffer) => {
      totalBytes += chunk.length;
      if (totalBytes > MAX_GIT_OUTPUT_BYTES) {
        child.kill();
        return;
      }
      chunks.push(chunk);
    });
    child.on('error', () => {
      if (!settled) {
        settled = true;
        resolve(null);
      }
    });
    child.on('close', (code) => {
      if (settled) return;
      settled = true;
      resolve(code === 0 && totalBytes <= MAX_GIT_OUTPUT_BYTES ? Buffer.concat(chunks) : null);
    });
  });
}

function safeRelativePath(value: string) {
  const normalized = value.replace(/\\/g, '/').replace(/^\.\//, '');
  if (
    !normalized ||
    normalized.startsWith('/') ||
    normalized === '..' ||
    normalized.startsWith('../')
  ) {
    return null;
  }
  return normalized;
}

export function parseGitArtifactStatus(porcelain: string) {
  const statuses = new Map<string, GitArtifactStatus>();
  const records = porcelain.split('\0');
  for (let index = 0; index < records.length; index += 1) {
    const record = records[index];
    if (record.length < 4) continue;
    const indexStatus = record[0];
    const worktreeStatus = record[1];
    const relativePath = safeRelativePath(record.slice(3));
    if (!relativePath || indexStatus === '!') continue;

    let status: GitArtifactStatus;
    if (indexStatus === '?' && worktreeStatus === '?') status = 'untracked';
    else if (worktreeStatus !== ' ') status = 'modified';
    else if (indexStatus !== ' ') status = 'staged';
    else continue;
    statuses.set(relativePath, status);

    if ('RC'.includes(indexStatus) || 'RC'.includes(worktreeStatus)) index += 1;
  }
  return statuses;
}

export function buildGitArtifactEntries({
  rootPath,
  trackedPaths,
  statuses,
  committedPaths,
}: {
  rootPath: string;
  trackedPaths: string[];
  statuses: Map<string, GitArtifactStatus>;
  committedPaths: Set<string>;
}) {
  const fileStatuses = new Map<string, GitArtifactStatus>();
  for (const candidate of trackedPaths) {
    const relativePath = safeRelativePath(candidate);
    if (!relativePath) continue;
    fileStatuses.set(
      relativePath,
      statuses.get(relativePath) ?? (committedPaths.has(relativePath) ? 'committed' : 'pushed')
    );
  }
  for (const [relativePath, status] of statuses) fileStatuses.set(relativePath, status);

  const directoryStatuses = new Map<string, GitArtifactStatus>();
  for (const [relativePath, status] of fileStatuses) {
    const segments = relativePath.split('/');
    for (let index = 1; index < segments.length; index += 1) {
      const directory = segments.slice(0, index).join('/');
      const current = directoryStatuses.get(directory);
      if (!current || statusPriority[status] > statusPriority[current]) {
        directoryStatuses.set(directory, status);
      }
    }
  }

  const entry = (
    relativePath: string,
    status: GitArtifactStatus,
    isDirectory: boolean
  ): GitArtifactEntry => {
    const segments = relativePath.split('/');
    return {
      name: segments[segments.length - 1] ?? relativePath,
      path: path.join(rootPath, ...segments),
      relativePath,
      parentPath: segments.slice(0, -1).join('/'),
      isDirectory,
      status,
    };
  };

  return [
    ...Array.from(directoryStatuses, ([relativePath, status]) => entry(relativePath, status, true)),
    ...Array.from(fileStatuses, ([relativePath, status]) => entry(relativePath, status, false)),
  ].sort((a, b) => {
    if (a.parentPath !== b.parentPath) return a.parentPath.localeCompare(b.parentPath);
    if (a.isDirectory !== b.isDirectory) return a.isDirectory ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
}

export async function readGitArtifactTree(rootPath: string) {
  try {
    await fs.stat(path.join(rootPath, '.git'));
  } catch {
    return null;
  }

  const [trackedOutput, statusOutput, branchOutput, upstreamOutput] = await Promise.all([
    runGit(rootPath, ['ls-files', '-z', '--cached']),
    runGit(rootPath, ['status', '--porcelain=v1', '-z', '--untracked-files=all']),
    runGit(rootPath, ['branch', '--show-current']),
    runGit(rootPath, ['rev-parse', '--verify', '--quiet', '@{upstream}']),
  ]);
  if (!trackedOutput || !statusOutput || !branchOutput) return null;

  const trackedPaths = trackedOutput.toString('utf8').split('\0').filter(Boolean);
  const statuses = parseGitArtifactStatus(statusOutput.toString('utf8'));
  let committedPaths = new Set<string>();
  if (upstreamOutput) {
    const aheadOutput = await runGit(rootPath, [
      'diff',
      '--name-only',
      '-z',
      '@{upstream}...HEAD',
      '--',
    ]);
    committedPaths = new Set(
      (aheadOutput?.toString('utf8').split('\0').filter(Boolean) ?? []).flatMap((candidate) => {
        const relativePath = safeRelativePath(candidate);
        return relativePath ? [relativePath] : [];
      })
    );
  } else {
    committedPaths = new Set(trackedPaths.filter((candidate) => !statuses.has(candidate)));
  }

  return {
    branch: branchOutput.toString('utf8').trim() || 'detached HEAD',
    entries: buildGitArtifactEntries({ rootPath, trackedPaths, statuses, committedPaths }),
  };
}
