import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import log from './logger';

/**
 * Ensures Windows shims are available in %LOCALAPPDATA%\Biorouter\bin
 * This allows the bundled executables to be found via PATH regardless of where Biorouter is installed
 */
export async function ensureWinShims(): Promise<void> {
  if (process.platform !== 'win32') return;

  const srcDir = path.join(process.resourcesPath, 'bin');
  const localAppData = process.env.LOCALAPPDATA ?? path.join(os.homedir(), 'AppData', 'Local');
  const tgtDir = path.join(localAppData, 'Biorouter', 'bin');

  try {
    await fs.promises.mkdir(tgtDir, { recursive: true });

    // Copy command-line tools, NOT biorouterd.exe (which should always be used locally)
    const shims = ['uvx.exe', 'uv.exe', 'npx.cmd', 'install-node.cmd'];

    await Promise.all(
      shims.map(async (shim) => {
        const src = path.join(srcDir, shim);
        const dst = path.join(tgtDir, shim);
        try {
          await fs.promises.access(src);
          await fs.promises.copyFile(src, dst); // overwrites with newer build
          log.info(`Copied Windows shim: ${shim} to ${dst}`);
        } catch (e) {
          log.error(`Failed to copy shim ${shim}`, e);
        }
      })
    );

    // Prepend uv/npm shims to PATH — these take priority so bundled tools are always found.
    // This does NOT modify the user's permanent system PATH.
    const currentPath = process.env.PATH ?? '';
    if (!currentPath.toLowerCase().includes(tgtDir.toLowerCase())) {
      process.env.PATH = `${tgtDir}${path.delimiter}${currentPath}`;
      log.info(`Added ${tgtDir} to PATH for Biorouter processes only`);
    } else {
      const pathParts = currentPath.split(path.delimiter);
      const binDirIndex = pathParts.findIndex((p) => p.toLowerCase() === tgtDir.toLowerCase());
      if (binDirIndex > 0) {
        pathParts.splice(binDirIndex, 1);
        process.env.PATH = `${tgtDir}${path.delimiter}${pathParts.join(path.delimiter)}`;
        log.info(`Moved ${tgtDir} to beginning of PATH for Biorouter processes only`);
      }
    }

    // Bundle portable git as a fallback for users without git installed.
    // Copied to Biorouter\git\ (separate from Biorouter\bin\) and appended to PATH
    // AFTER all standard locations so system git always wins if present.
    await ensureBundledGit(srcDir, localAppData);
  } catch (error) {
    log.error('Failed to ensure Windows shims:', error);
  }
}

async function ensureBundledGit(srcBinDir: string, localAppData: string): Promise<void> {
  const srcGitDir = path.join(srcBinDir, 'git');
  const dstGitDir = path.join(localAppData, 'Biorouter', 'git');
  const gitExe = path.join(dstGitDir, 'cmd', 'git.exe');

  try {
    await fs.promises.access(srcGitDir);
  } catch {
    // Not bundled in this build (download-mingit.js may have been skipped)
    return;
  }

  // Only copy once per install; Biorouter updates overwrite by deleting and re-copying.
  //
  // ⚠ **Asynchronous, and that is issue #88 rather than a style preference.**
  // This ran as `fs.cpSync(...)`: a synchronous recursive copy of the bundled
  // MinGit tree, which is ~120 MB across thousands of files, with Defender
  // scanning every write on a Windows first run. `ensureWinShims()` is awaited
  // from `appMain()` BEFORE `createNewWindow`, so the main thread was parked
  // for seconds to tens of seconds with no window on screen at all - the same
  // report that produced #88, on the worst part of the path, and neither the
  // watchdog nor `startupBlocking.test.ts` could see it. That test only banned
  // synchronous CHILD-PROCESS calls, because #88 was about probes; a bulk
  // filesystem copy blocks the loop just as hard.
  let installed = true;
  try {
    await fs.promises.access(gitExe);
  } catch {
    installed = false;
  }
  if (!installed) {
    log.info('Installing bundled git fallback...');
    await fs.promises.cp(srcGitDir, dstGitDir, { recursive: true, force: true });
    log.info(`Bundled git installed to ${dstGitDir}`);
  }

  // Append to PATH as last-resort fallback — system git (Program Files\Git\bin) takes priority.
  const gitCmdDir = path.join(dstGitDir, 'cmd');
  const currentPath = process.env.PATH ?? '';
  if (!currentPath.toLowerCase().includes(gitCmdDir.toLowerCase())) {
    process.env.PATH = `${currentPath}${path.delimiter}${gitCmdDir}`;
    log.info(`Added bundled git fallback to PATH: ${gitCmdDir}`);
  }
}
