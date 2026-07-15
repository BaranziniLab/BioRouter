// Export + verify every app in one process (robust; reads ids from round.sh).
// Usage: node export-all.mjs <base>
import { writeFile, mkdir } from 'node:fs/promises';
import { readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { dirname, join } from 'node:path';

const base = process.argv[2] || 'http://127.0.0.1:3000';
const ESB = '/Users/wanjun/Desktop/biorouter/ui/desktop/node_modules/.bin/esbuild';
const rs = readFileSync(
  '/Users/wanjun/Desktop/biorouter-apps-wt/scripts/agent-drafter-apps/round.sh',
  'utf8'
);
const ids = [...rs.matchAll(/^"([a-z0-9-]+)\|/gm)].map((m) => m[1]);

let pass = 0;
const fails = [];
for (const id of ids) {
  try {
    const resp = await fetch(`${base}/apps/${id}/export`);
    if (!resp.ok) throw new Error('http ' + resp.status);
    const files = (await resp.json()).files || {};
    const OUT = `/tmp/exports/${id}`;
    for (const [p, c] of Object.entries(files)) {
      const f = join(OUT, p);
      await mkdir(dirname(f), { recursive: true });
      await writeFile(f, c);
    }
    const need = ['index.html', 'src/main.ts', 'src/sdk.ts', 'package.json', 'serve.mjs', 'run.sh', 'run.command', 'dist/app.js'];
    const missing = need.filter((n) => !(n in files));
    if (missing.length) throw new Error('missing ' + missing.join(','));
    const idx = files['index.html'];
    if (!idx.includes(`ws://127.0.0.1:3000/apps/${id}/agent`)) throw new Error('bad endpoint');
    if (!idx.includes('biorouter-theme') || !idx.includes('dist/app.js')) throw new Error('bad index');
    if ((files['dist/app.js'] || '').length < 500) throw new Error('tiny bundle');
    execFileSync(ESB, [join(OUT, 'src/main.ts'), '--bundle', '--format=iife', '--target=es2018', '--outfile=' + join(OUT, 'dist/app.js'), '--log-level=error']);
    pass++;
  } catch (e) {
    fails.push({ id, err: String(e.message || e) });
  }
}
console.log(`EXPORT: ${pass}/${ids.length} ok`);
for (const f of fails) console.log(`  FAIL ${f.id}: ${f.err}`);
