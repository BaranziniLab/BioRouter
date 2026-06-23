// Export one app via biorouterd, write the folder, and verify the exported
// project is complete + the bundle builds. Usage: node export-check.mjs <base> <id>
import { writeFile, mkdir } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { dirname, join } from 'node:path';

const base = process.argv[2], id = process.argv[3];
const OUT = `/tmp/exports/${id}`;
const ESB = '/Users/wanjun/Desktop/BioRouter/ui/desktop/node_modules/.bin/esbuild';
const res = { id, files: 0, hasRun: false, hasDist: false, endpointOk: false, themeOk: false, rebuildOk: false, ok: false, err: '' };
try {
  const r = await fetch(`${base}/apps/${id}/export`);
  if (!r.ok) throw new Error('export http ' + r.status);
  const data = await r.json();
  const files = data.files || {};
  res.files = Object.keys(files).length;
  for (const [p, c] of Object.entries(files)) {
    const full = join(OUT, p);
    await mkdir(dirname(full), { recursive: true });
    await writeFile(full, c);
  }
  const need = ['index.html','src/main.ts','src/sdk.ts','package.json','serve.mjs','run.sh','run.command','dist/app.js'];
  const missing = need.filter((n) => !(n in files));
  if (missing.length) throw new Error('missing: ' + missing.join(','));
  res.hasRun = 'run.command' in files && 'run.sh' in files;
  res.hasDist = 'dist/app.js' in files && files['dist/app.js'].length > 500;
  const idx = files['index.html'];
  res.endpointOk = idx.includes(`ws://127.0.0.1:3000/apps/${id}/agent`);
  res.themeOk = idx.includes('biorouter-theme') && idx.includes('dist/app.js');
  // Re-bundle the exported src to confirm it compiles standalone.
  try { execFileSync(ESB, [join(OUT,'src/main.ts'),'--bundle','--format=iife','--target=es2018','--outfile='+join(OUT,'dist/app.js'),'--log-level=error']); res.rebuildOk = true; }
  catch (e) { res.err = 'esbuild: ' + (e.stderr?.toString()||e.message).slice(0,200); }
  res.ok = res.files>=8 && res.hasRun && res.hasDist && res.endpointOk && res.themeOk && res.rebuildOk;
} catch (e) { res.err = String(e.message||e); }
console.log(JSON.stringify(res));
