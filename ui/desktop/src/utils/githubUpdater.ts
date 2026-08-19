import { app } from 'electron';
import { compareVersions } from 'compare-versions';
import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';
import log from './logger';
import { safeJsonParse } from './conversionUtils';

interface GitHubRelease {
  tag_name: string;
  name: string;
  published_at: string;
  html_url: string;
  assets: Array<{
    name: string;
    browser_download_url: string;
    size: number;
  }>;
}

interface UpdateCheckResult {
  updateAvailable: boolean;
  latestVersion?: string;
  downloadUrl?: string;
  releaseUrl?: string;
  error?: string;
}

export class GitHubUpdater {
  private readonly owner = 'BaranziniLab';
  private readonly repo = 'biorouter';
  private readonly apiUrl = `https://api.github.com/repos/${this.owner}/${this.repo}/releases/latest`;

  async checkForUpdates(): Promise<UpdateCheckResult> {
    const startTime = Date.now();
    try {
      log.info('=== GitHubUpdater: STARTING UPDATE CHECK ===');
      log.info(`GitHubUpdater: API URL: ${this.apiUrl}`);
      log.info(`GitHubUpdater: Current app version: ${app.getVersion()}`);
      log.info(`GitHubUpdater: Timestamp: ${new Date().toISOString()}`);

      log.info('GitHubUpdater: Initiating fetch request...');
      const controller = new AbortController();
      const timeoutId = setTimeout(() => {
        log.error('GitHubUpdater: Fetch request timed out after 30 seconds');
        controller.abort();
      }, 30000);

      const response = await fetch(this.apiUrl, {
        headers: {
          Accept: 'application/vnd.github.v3+json',
          'User-Agent': `Biorouter-Desktop/${app.getVersion()}`,
        },
        signal: controller.signal,
      });

      clearTimeout(timeoutId);
      const fetchDuration = Date.now() - startTime;
      log.info(
        `GitHubUpdater: GitHub API response status: ${response.status} ${response.statusText} (took ${fetchDuration}ms)`
      );

      if (!response.ok) {
        const errorText = await response.text();
        log.error(`GitHubUpdater: GitHub API error response: ${errorText}`);
        throw new Error(`GitHub API returned ${response.status}: ${response.statusText}`);
      }

      const release: GitHubRelease = await safeJsonParse<GitHubRelease>(
        response,
        'Failed to get GitHub release information'
      );
      log.info(`GitHubUpdater: Found release: ${release.tag_name} (${release.name})`);
      log.info(`GitHubUpdater: Release published at: ${release.published_at}`);
      log.info(`GitHubUpdater: Release assets count: ${release.assets.length}`);

      const latestVersion = release.tag_name.replace(/^v/, ''); // Remove 'v' prefix if present
      const currentVersion = app.getVersion();

      log.info(
        `GitHubUpdater: Current version: ${currentVersion}, Latest version: ${latestVersion}`
      );

      // Compare versions
      const updateAvailable = compareVersions(latestVersion, currentVersion) > 0;
      log.info(`GitHubUpdater: Update available: ${updateAvailable}`);

      if (!updateAvailable) {
        return {
          updateAvailable: false,
          latestVersion,
        };
      }

      // Find the appropriate download URL based on platform
      const platform = process.platform;
      const arch = process.arch;
      let downloadUrl: string | undefined;

      log.info(`GitHubUpdater: Looking for asset for platform: ${platform}, arch: ${arch}`);

      // Real published release asset names (see CLAUDE.md "Release assets"):
      //   Biorouter-{ver}-arm64.dmg, Biorouter-{ver}-x64.dmg,
      //   Biorouter-win32-x64-{ver}.zip, biorouter_{ver}_amd64.deb,
      //   Biorouter-{ver}-1.x86_64.rpm
      const v = latestVersion;
      let candidates: string[];
      if (platform === 'darwin') {
        candidates = arch === 'arm64' ? [`Biorouter-${v}-arm64.dmg`] : [`Biorouter-${v}-x64.dmg`];
      } else if (platform === 'win32') {
        candidates = [`Biorouter-win32-x64-${v}.zip`];
      } else {
        // Linux: prefer .deb, then .rpm.
        candidates = [`biorouter_${v}_amd64.deb`, `Biorouter-${v}-1.x86_64.rpm`];
      }

      log.info(`GitHubUpdater: Candidate assets: ${candidates.join(', ')}`);
      log.info(`GitHubUpdater: Available assets: ${release.assets.map((a) => a.name).join(', ')}`);

      let asset = candidates
        .map((name) => release.assets.find((a) => a.name.toLowerCase() === name.toLowerCase()))
        .find(Boolean);
      // Resilient fallback: match by OS/arch tokens + extension if exact names drift.
      if (!asset) {
        const tokens =
          platform === 'darwin'
            ? [arch === 'arm64' ? 'arm64' : 'x64', '.dmg']
            : platform === 'win32'
              ? ['win32', '.zip']
              : ['.deb'];
        asset = release.assets.find((a) => {
          const n = a.name.toLowerCase();
          return tokens.every((t) => n.includes(t.toLowerCase()));
        });
      }

      if (asset) {
        downloadUrl = asset.browser_download_url;
        log.info(`GitHubUpdater: Found matching asset: ${asset.name} (${asset.size} bytes)`);
      } else {
        log.warn(`GitHubUpdater: No matching asset found for ${platform}/${arch}`);
      }

      if (!downloadUrl) {
        throw new Error(
          `Update Available but no download URL found for platform: ${platform}, arch: ${arch}`
        );
      }

      return {
        updateAvailable: true,
        latestVersion,
        downloadUrl,
        releaseUrl: release.html_url,
      };
    } catch (error) {
      log.error('GitHubUpdater: Error checking for updates:', error);
      log.error('GitHubUpdater: Error details:', {
        message: error instanceof Error ? error.message : 'Unknown error',
        stack: error instanceof Error ? error.stack : 'No stack',
        name: error instanceof Error ? error.name : 'Unknown',
        code:
          error instanceof Error && 'code' in error
            ? (error as Error & { code: unknown }).code
            : undefined,
      });
      return {
        updateAvailable: false,
        error: error instanceof Error ? error.message : 'Unknown error',
      };
    }
  }

  async downloadUpdate(
    downloadUrl: string,
    latestVersion: string,
    onProgress?: (percent: number) => void
  ): Promise<{ success: boolean; downloadPath?: string; extractedPath?: string; error?: string }> {
    const downloadStartTime = Date.now();
    try {
      log.info('=== GitHubUpdater: STARTING DOWNLOAD ===');
      log.info(`GitHubUpdater: Download URL: ${downloadUrl}`);
      log.info(`GitHubUpdater: Version: ${latestVersion}`);
      log.info(`GitHubUpdater: Timestamp: ${new Date().toISOString()}`);

      log.info('GitHubUpdater: Initiating download fetch request...');
      const response = await fetch(downloadUrl);
      const fetchDuration = Date.now() - downloadStartTime;
      log.info(
        `GitHubUpdater: Download response received in ${fetchDuration}ms - Status: ${response.status} ${response.statusText}`
      );

      if (!response.ok) {
        throw new Error(`Download failed: ${response.status} ${response.statusText}`);
      }

      // Get total size from headers
      const contentLength = response.headers.get('content-length');
      const totalSize = contentLength ? parseInt(contentLength, 10) : 0;
      log.info(
        `GitHubUpdater: Content-Length: ${totalSize} bytes (${(totalSize / 1024 / 1024).toFixed(2)} MB)`
      );

      if (!response.body) {
        throw new Error('Response body is null');
      }
      let lastReportedPercent = -1; // Track last reported percentage to throttle updates
      let lastLoggedPercent = -1; // Track for logging at 10% intervals

      // Stream straight to disk.
      //
      // This used to accumulate every chunk in `chunks[]` and then
      // `Buffer.concat(chunks.map(Buffer.from))`. For a ~200 MB installer that is
      // the chunks, plus a full copy of them, plus the concatenated result — ~600 MB
      // of main-process RSS — and the concat itself is a large synchronous memcpy on
      // the main thread. That is the CPU-and-memory spike people notice shortly after
      // launch (#88). Writing through a stream is O(1) memory and never blocks.
      //
      // Written to a `.part` file and renamed on completion, so an interrupted
      // download can never be mistaken for a finished installer.
      const downloadsDir = path.join(os.homedir(), 'Downloads');
      // Preserve the real asset extension (.dmg/.zip/.deb/.rpm) from the URL so
      // the file the user double-clicks is the actual installer.
      const urlName = downloadUrl.split('/').pop() || '';
      const ext = urlName.match(/\.(dmg|zip|deb|rpm)$/i)?.[1] || 'zip';
      const fileName = `Biorouter-${latestVersion}.${ext}`;
      const downloadPath = path.join(downloadsDir, fileName);
      const partPath = `${downloadPath}.part`;

      await fs.mkdir(downloadsDir, { recursive: true });
      log.info(`GitHubUpdater: Streaming to ${partPath}...`);
      const fileHandle = await fs.open(partPath, 'w');
      const writeStream = fileHandle.createWriteStream();

      const reader = response.body.getReader();
      let downloadedSize = 0;
      let lastProgressTime = Date.now();

      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;

          // Respect backpressure: if the disk is slower than the socket, wait for
          // the drain rather than letting Node buffer the difference in memory.
          if (!writeStream.write(value)) {
            await new Promise<void>((resolve, reject) => {
              writeStream.once('drain', resolve);
              writeStream.once('error', reject);
            });
          }
          downloadedSize += value.length;

          // Report progress - only when percentage changes by at least 1%
          if (totalSize > 0 && onProgress) {
            const percent = Math.round((downloadedSize / totalSize) * 100);

            // Only report if percent changed (throttles from hundreds/sec to ~100 total)
            if (percent !== lastReportedPercent) {
              onProgress(percent);
              lastReportedPercent = percent;

              // Log at 10% intervals for debugging
              if (percent % 10 === 0 && percent !== lastLoggedPercent) {
                const elapsed = Date.now() - downloadStartTime;
                const speed = downloadedSize / (elapsed / 1000) / 1024; // KB/s
                log.info(
                  `GitHubUpdater: Download progress ${percent}% (${(downloadedSize / 1024 / 1024).toFixed(2)}/${(totalSize / 1024 / 1024).toFixed(2)} MB) @ ${speed.toFixed(0)} KB/s`
                );
                lastLoggedPercent = percent;
              }
            }
          }

          // Warn if no progress for 30 seconds
          const now = Date.now();
          if (now - lastProgressTime > 30000) {
            log.warn(
              `GitHubUpdater: Download appears slow - no significant progress in 30 seconds (${downloadedSize}/${totalSize} bytes)`
            );
            lastProgressTime = now;
          } else if (value.length > 0) {
            lastProgressTime = now;
          }
        }
      } finally {
        await new Promise<void>((resolve) => writeStream.end(resolve));
        await fileHandle.close().catch(() => {});
      }

      const downloadDuration = Date.now() - downloadStartTime;
      const avgSpeed = downloadedSize / (downloadDuration / 1000) / 1024;
      log.info(
        `GitHubUpdater: Download stream complete - ${downloadedSize} bytes in ${downloadDuration}ms (avg ${avgSpeed.toFixed(0)} KB/s)`
      );

      // A truncated transfer would otherwise land in Downloads looking complete.
      if (totalSize > 0 && downloadedSize !== totalSize) {
        await fs.rm(partPath, { force: true });
        throw new Error(
          `Download truncated: got ${downloadedSize} of ${totalSize} bytes. Check your connection and try again.`
        );
      }

      await fs.rename(partPath, downloadPath);

      const totalDuration = Date.now() - downloadStartTime;
      log.info(`=== GitHubUpdater: DOWNLOAD COMPLETE in ${totalDuration}ms ===`);
      log.info(`GitHubUpdater: File saved to ${downloadPath}`);

      // Return success - user will handle extraction manually
      return { success: true, downloadPath, extractedPath: downloadsDir };
    } catch (error) {
      const duration = Date.now() - downloadStartTime;
      log.error(`=== GitHubUpdater: DOWNLOAD FAILED after ${duration}ms ===`);
      log.error('GitHubUpdater: Error downloading update:', error);
      log.error('GitHubUpdater: Download error details:', {
        message: error instanceof Error ? error.message : 'Unknown error',
        stack: error instanceof Error ? error.stack : 'No stack',
        name: error instanceof Error ? error.name : 'Unknown',
      });
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
      };
    }
  }
}

// Create singleton instance
export const githubUpdater = new GitHubUpdater();
