// Runs only synthetic loopback fixtures in a fresh Electron userData directory.
// Usage (from ui/desktop): node scripts/check-managed-app-preview.mjs
import { build } from 'esbuild';
import electronPath from 'electron';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const output = await mkdtemp(path.join(tmpdir(), 'biorouter-managed-preview-probe.'));
const entry = path.join(output, 'probe.cjs');
await build({
  entryPoints: [path.join(root, 'scripts/managed-app-preview-probe.ts')],
  outfile: entry,
  bundle: true,
  platform: 'node',
  format: 'cjs',
  external: ['electron'],
  logLevel: 'warning',
});
console.log(`Managed preview probe evidence: ${output}`);
const child = spawn(electronPath, [entry], {
  cwd: output,
  stdio: 'inherit',
  env: {
    PATH: process.env.PATH ?? '',
    HOME: process.env.HOME ?? tmpdir(),
    TMPDIR: process.env.TMPDIR ?? tmpdir(),
    BIOROUTER_MANAGED_PREVIEW_PROBE_DIR: output,
  },
});
const timeout = setTimeout(() => child.kill('SIGTERM'), 60_000);
child.once('error', (error) => {
  clearTimeout(timeout);
  console.error(error.message);
  process.exitCode = 1;
});
child.once('exit', (code) => {
  clearTimeout(timeout);
  process.exitCode = code ?? 1;
});
