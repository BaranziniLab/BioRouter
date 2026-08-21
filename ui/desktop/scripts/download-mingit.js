/**
 * download-mingit.js
 *
 * Downloads MinGit (minimal Git for Windows) into src/platform/windows/bin/git/
 * so it can be bundled as a fallback for Windows users who don't have git installed.
 *
 * Run automatically as part of `npm run bundle:windows`.
 * Skipped entirely on non-Windows builds (ELECTRON_PLATFORM != win32).
 * Release builds fail if MinGit cannot be prepared. A developer who is
 * intentionally testing a package without the fallback may opt out with
 * BIOROUTER_ALLOW_MISSING_MINGIT=1.
 */

const https = require('https');
const fs = require('fs');
const path = require('path');
const os = require('os');
const AdmZip = require('adm-zip');

const MINGIT_VERSION = '2.49.0';
const MINGIT_WINDOWS_TAG = `v${MINGIT_VERSION}.windows.1`;
const MINGIT_ZIP_NAME = `MinGit-${MINGIT_VERSION}-64-bit.zip`;
const MINGIT_URL = `https://github.com/git-for-windows/git/releases/download/${MINGIT_WINDOWS_TAG}/${MINGIT_ZIP_NAME}`;
const MINGIT_VERSION_FILE = 'mingit-version.txt';
const ALLOW_MISSING_MINGIT_ENV = 'BIOROUTER_ALLOW_MISSING_MINGIT';

const DEST_DIR = path.join(__dirname, '..', 'src', 'platform', 'windows', 'bin', 'git');
const ZIP_PATH = path.join(os.tmpdir(), MINGIT_ZIP_NAME);

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);

    const get = (url) => {
      https
        .get(url, (res) => {
          if (res.statusCode === 301 || res.statusCode === 302) {
            get(res.headers.location);
            return;
          }
          if (res.statusCode !== 200) {
            file.close();
            reject(new Error(`HTTP ${res.statusCode} downloading ${url}`));
            return;
          }
          const total = parseInt(res.headers['content-length'] || '0', 10);
          let downloaded = 0;
          res.on('data', (chunk) => {
            downloaded += chunk.length;
            if (total > 0) {
              const pct = Math.round((downloaded / total) * 100);
              process.stdout.write(
                `\r  ${pct}% (${Math.round(downloaded / 1024 / 1024)}MB / ${Math.round(total / 1024 / 1024)}MB)`
              );
            }
          });
          res.pipe(file);
          file.on('finish', () => {
            file.close();
            process.stdout.write('\n');
            resolve();
          });
          file.on('error', reject);
        })
        .on('error', reject);
    };

    get(url);
  });
}

function extract(zipPath, destDir) {
  fs.mkdirSync(destDir, { recursive: true });
  new AdmZip(zipPath).extractAllTo(destDir, true);
}

function missingMinGitAllowed(env = process.env) {
  return /^(1|true|on|yes)$/i.test((env[ALLOW_MISSING_MINGIT_ENV] || '').trim());
}

function handleMinGitFailure(error, env = process.env) {
  if (!missingMinGitAllowed(env)) throw error;
  console.warn(`Failed to download/extract MinGit: ${error.message}`);
  console.warn(`Continuing because ${ALLOW_MISSING_MINGIT_ENV} explicitly allows it.`);
}

async function main() {
  const targetPlatform = process.env.ELECTRON_PLATFORM || process.platform;

  if (targetPlatform !== 'win32') {
    console.log('Skipping MinGit download (not a Windows build)');
    return;
  }

  const bundledVersionPath = path.join(DEST_DIR, MINGIT_VERSION_FILE);
  const bundledVersion = fs.existsSync(bundledVersionPath)
    ? fs.readFileSync(bundledVersionPath, 'utf8').trim()
    : null;
  if (bundledVersion === MINGIT_VERSION && fs.existsSync(path.join(DEST_DIR, 'cmd', 'git.exe'))) {
    console.log('MinGit already present, skipping download');
    return;
  }

  console.log(`Downloading MinGit ${MINGIT_VERSION} (~57MB)...`);
  console.log(`  Source: ${MINGIT_URL}`);

  try {
    await download(MINGIT_URL, ZIP_PATH);
    const stagingDir = `${DEST_DIR}.staging`;
    fs.rmSync(stagingDir, { recursive: true, force: true });
    console.log(`Extracting to ${stagingDir} ...`);
    extract(ZIP_PATH, stagingDir);
    if (!fs.existsSync(path.join(stagingDir, 'cmd', 'git.exe'))) {
      throw new Error('downloaded MinGit archive is missing cmd/git.exe');
    }
    fs.writeFileSync(path.join(stagingDir, MINGIT_VERSION_FILE), `${MINGIT_VERSION}\n`);
    fs.rmSync(DEST_DIR, { recursive: true, force: true });
    fs.renameSync(stagingDir, DEST_DIR);
    console.log('MinGit ready.');
  } catch (err) {
    handleMinGitFailure(err);
  } finally {
    fs.rmSync(ZIP_PATH, { force: true });
  }
}

if (require.main === module) {
  main().catch((err) => {
    console.error(`MinGit preparation failed: ${err.message}`);
    process.exitCode = 1;
  });
}

module.exports = { extract, handleMinGitFailure, missingMinGitAllowed };
