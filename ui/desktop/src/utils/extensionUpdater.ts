/**
 * extensionUpdater.ts
 *
 * On startup, scans installed extensions in ~/.config/biorouter/extensions/,
 * reads each manifest.json for version + repository, queries the GitHub
 * releases API for the latest .brxt asset, and auto-updates outdated ones
 * in the background.
 *
 * Skips silently on network errors so offline users are unaffected.
 */

import * as path from 'path';
import * as os from 'os';
import * as fs from 'fs/promises';
import { mkdirSync } from 'fs';
import { spawnSync } from 'child_process';
import { BrowserWindow } from 'electron';
import { compareVersions } from 'compare-versions';
import AdmZip from 'adm-zip';
import { safeExtractZip } from './safeZip';
import log from './logger';
import { SPAWN_ENV } from './dependencyChecker';

// ─── Types ────────────────────────────────────────────────────────────────────

interface ExtensionManifest {
  name: string;
  display_name: string;
  description: string;
  version: string;
  entry_point: string;
  repository: string; // e.g. "https://github.com/owner/repo"
}

interface GitHubRelease {
  tag_name: string;
  assets: Array<{
    name: string;
    browser_download_url: string;
    size: number;
  }>;
}

interface ExtensionUpdateInfo {
  name: string;
  displayName: string;
  currentVersion: string;
  latestVersion: string;
  downloadUrl: string;
  installDir: string;
}

export interface ExtensionUpdateEvent {
  type:
    | 'update-found'
    | 'update-start'
    | 'update-progress'
    | 'update-done'
    | 'update-error'
    | 'all-done';
  ext?: string;
  displayName?: string;
  from?: string;
  to?: string;
  percent?: number;
  error?: string;
  updatedCount?: number;
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

const EXTENSIONS_DIR = path.join(os.homedir(), '.config', 'biorouter', 'extensions');
const GITHUB_API_BASE = 'https://api.github.com/repos';

/** Parse owner/repo from a GitHub URL. Returns null for non-GitHub URLs. */
function parseGitHubRepo(repoUrl: string): { owner: string; repo: string } | null {
  try {
    const url = new URL(repoUrl);
    if (!url.hostname.includes('github.com')) return null;
    const parts = url.pathname.replace(/^\//, '').replace(/\/$/, '').split('/');
    if (parts.length < 2) return null;
    const [owner, repo] = parts;
    if (!owner || !repo) return null;
    return { owner, repo: repo.replace(/\.git$/, '') };
  } catch {
    return null;
  }
}

/** Clean version string: strip leading 'v' and whitespace. */
function cleanVersion(v: string): string {
  return v.trim().replace(/^v/i, '');
}

/** Fetch with a timeout and a User-Agent header (GitHub requires it). */
async function fetchJson<T>(url: string, timeoutMs = 15000): Promise<T> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const res = await fetch(url, {
      signal: controller.signal,
      headers: { 'User-Agent': 'Biorouter-ExtensionUpdater/1.0' },
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}: ${url}`);
    return (await res.json()) as T;
  } finally {
    clearTimeout(timer);
  }
}

/** Download a URL to a file, tracking progress. */
async function downloadFile(
  url: string,
  dest: string,
  onProgress: (percent: number) => void
): Promise<void> {
  const res = await fetch(url, {
    headers: { 'User-Agent': 'Biorouter-ExtensionUpdater/1.0' },
  });
  if (!res.ok) throw new Error(`Download failed: HTTP ${res.status}`);

  const total = Number(res.headers.get('content-length') || 0);
  let received = 0;
  const chunks: Uint8Array[] = [];

  const reader = res.body?.getReader();
  if (!reader) throw new Error('No response body');

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    received += value.length;
    if (total > 0) onProgress(Math.round((received / total) * 100));
  }

  const buf = Buffer.concat(chunks);
  await fs.writeFile(dest, buf);
}

// ─── Core update flow ─────────────────────────────────────────────────────────

async function scanInstalledExtensions(): Promise<
  Array<{ manifest: ExtensionManifest; installDir: string }>
> {
  try {
    const entries = await fs.readdir(EXTENSIONS_DIR, { withFileTypes: true });
    const results: Array<{ manifest: ExtensionManifest; installDir: string }> = [];
    for (const entry of entries) {
      if (!entry.isDirectory()) continue;
      const manifestPath = path.join(EXTENSIONS_DIR, entry.name, 'manifest.json');
      try {
        const raw = await fs.readFile(manifestPath, 'utf8');
        const manifest = JSON.parse(raw) as ExtensionManifest;
        if (manifest.name && manifest.version && manifest.repository) {
          results.push({ manifest, installDir: path.join(EXTENSIONS_DIR, entry.name) });
        }
      } catch {
        // skip malformed manifests
      }
    }
    return results;
  } catch {
    return [];
  }
}

async function checkExtensionUpdate(
  manifest: ExtensionManifest,
  installDir: string
): Promise<ExtensionUpdateInfo | null> {
  const parsed = parseGitHubRepo(manifest.repository);
  if (!parsed) return null;

  const { owner, repo } = parsed;
  const apiUrl = `${GITHUB_API_BASE}/${owner}/${repo}/releases/latest`;

  let release: GitHubRelease;
  try {
    release = await fetchJson<GitHubRelease>(apiUrl);
  } catch (err) {
    log.warn(
      `[ExtensionUpdater] Cannot fetch release for ${manifest.name}:`,
      (err as Error).message
    );
    return null;
  }

  const latestVersion = cleanVersion(release.tag_name);
  const currentVersion = cleanVersion(manifest.version);

  let cmp: number;
  try {
    cmp = compareVersions(latestVersion, currentVersion);
  } catch {
    cmp = 0;
  }
  if (cmp <= 0) return null; // already up-to-date

  // Find a .brxt asset
  const asset = release.assets.find((a) => a.name.endsWith('.brxt'));
  if (!asset) {
    log.info(`[ExtensionUpdater] No .brxt asset in release ${latestVersion} for ${manifest.name}`);
    return null;
  }

  return {
    name: manifest.name,
    displayName: manifest.display_name,
    currentVersion,
    latestVersion,
    downloadUrl: asset.browser_download_url,
    installDir,
  };
}

async function applyUpdate(
  info: ExtensionUpdateInfo,
  send: (e: ExtensionUpdateEvent) => void
): Promise<void> {
  const tmpDir = path.join(os.tmpdir(), `biorouter-ext-update-${info.name}-${Date.now()}`);
  const tmpBrxt = path.join(tmpDir, `${info.name}.brxt`);

  try {
    mkdirSync(tmpDir, { recursive: true });

    send({
      type: 'update-start',
      ext: info.name,
      displayName: info.displayName,
      from: info.currentVersion,
      to: info.latestVersion,
    });

    // Download
    await downloadFile(info.downloadUrl, tmpBrxt, (percent) => {
      send({ type: 'update-progress', ext: info.name, percent });
    });
    send({ type: 'update-progress', ext: info.name, percent: 100 });

    // Extract into install dir (zip-slip guarded: rejects entries escaping installDir)
    const zip = new AdmZip(tmpBrxt);
    safeExtractZip(zip, info.installDir);

    // Re-run uv sync to apply any dependency changes
    const uvResult = spawnSync('uv', ['sync'], {
      cwd: info.installDir,
      encoding: 'utf8',
      timeout: 120_000,
      env: SPAWN_ENV,
    });

    if (uvResult.status !== 0) {
      const detail =
        uvResult.error?.message ||
        uvResult.stderr ||
        uvResult.stdout ||
        `exited with status ${uvResult.status}`;
      throw new Error(`uv sync failed: ${detail}`);
    }

    send({
      type: 'update-done',
      ext: info.name,
      displayName: info.displayName,
      to: info.latestVersion,
    });
    log.info(`[ExtensionUpdater] Updated ${info.name} → ${info.latestVersion}`);
  } catch (err) {
    const msg = (err as Error).message;
    log.error(`[ExtensionUpdater] Failed to update ${info.name}:`, msg);
    send({ type: 'update-error', ext: info.name, error: msg });
  } finally {
    // Cleanup temp dir
    try {
      await fs.rm(tmpDir, { recursive: true, force: true });
    } catch {
      /* best-effort cleanup — ignore if the temp dir is already gone */
    }
  }
}

// ─── Public entry point ───────────────────────────────────────────────────────

export async function runExtensionUpdateCheck(): Promise<void> {
  log.info('[ExtensionUpdater] Starting extension update check...');

  const installed = await scanInstalledExtensions();
  if (installed.length === 0) {
    log.info('[ExtensionUpdater] No installed extensions found.');
    return;
  }

  // Notify all windows about updates via a push event
  const send = (evt: ExtensionUpdateEvent) => {
    BrowserWindow.getAllWindows().forEach((win) => {
      if (!win.isDestroyed()) win.webContents.send('extension-update-event', evt);
    });
  };

  let updatedCount = 0;
  for (const { manifest, installDir } of installed) {
    let updateInfo: ExtensionUpdateInfo | null = null;
    try {
      updateInfo = await checkExtensionUpdate(manifest, installDir);
    } catch (err) {
      // Network/parse error — skip silently (offline fallback)
      log.warn(`[ExtensionUpdater] Skipping ${manifest.name}:`, (err as Error).message);
      continue;
    }

    if (!updateInfo) continue;

    send({
      type: 'update-found',
      ext: updateInfo.name,
      displayName: updateInfo.displayName,
      from: updateInfo.currentVersion,
      to: updateInfo.latestVersion,
    });
    await applyUpdate(updateInfo, send);
    updatedCount++;
  }

  send({ type: 'all-done', updatedCount });
  log.info(`[ExtensionUpdater] Done. ${updatedCount} extension(s) updated.`);
}

/** Called from main.ts after app window is ready. */
export function scheduleExtensionUpdateCheck(): void {
  // Run after 8 seconds — later than dependency check and auto-updater
  setTimeout(() => {
    runExtensionUpdateCheck().catch((err) => log.error('[ExtensionUpdater] Unhandled error:', err));
  }, 8000);
}
