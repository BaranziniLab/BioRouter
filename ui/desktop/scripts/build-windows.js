const { spawnSync } = require('child_process');
const { resolve } = require('path');

const appRoot = resolve(__dirname, '..');
const npmCli = process.env.npm_execpath;
const env = {
  ...process.env,
  ELECTRON_PLATFORM: 'win32',
  ELECTRON_ARCH: 'x64',
};

if (!npmCli) {
  throw new Error('npm_execpath is unavailable; run this build through npm run bundle:windows');
}

const steps = [
  ['build Electron main process', process.execPath, ['scripts/build-main.js']],
  ['download MinGit', process.execPath, ['scripts/download-mingit.js']],
  ['prepare Windows binaries', process.execPath, ['scripts/prepare-platform-binaries.js']],
  [
    'make Windows package',
    process.execPath,
    [npmCli, 'run', 'make', '--', '--platform=win32', '--arch=x64'],
  ],
];

for (const [label, command, args] of steps) {
  console.log(`\n[windows-release] ${label}`);
  const result = spawnSync(command, args, {
    cwd: appRoot,
    env,
    stdio: 'inherit',
  });

  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}
