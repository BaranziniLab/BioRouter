import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const smokeScript = fileURLToPath(new URL('./app-smoke.mjs', import.meta.url));

async function runFixture(stateInitial, manifestText) {
  const app = await mkdtemp(join(tmpdir(), 'biorouter-smoke-state-'));
  await mkdir(join(app, 'dist'));
  await writeFile(join(app, 'manifest.json'), manifestText ?? JSON.stringify({
    id: 'smoke-state-fixture',
    surface: stateInitial === undefined ? {} : { state_initial: stateInitial },
  }));
  await writeFile(join(app, 'index.html'), '<!doctype html><html><body>' +
    '<p data-br-bind="/count"></p><p data-br-bind="/ready"></p>' +
    '<p data-br-bind="/label"></p></body></html>');
  await writeFile(join(app, 'dist/app.js'), `
    const config = JSON.parse(document.getElementById('biorouter-app-config').textContent);
    for (const element of document.querySelectorAll('[data-br-bind]')) {
      const key = element.getAttribute('data-br-bind').slice(1);
      const value = config.stateInitial?.[key];
      element.textContent = value == null ? '' : String(value);
    }
    if (window.injectedByFixture) document.querySelector('[data-br-bind]').textContent = '';
  `);
  const result = spawnSync(process.execPath, [smokeScript, app, '--json'], {
    encoding: 'utf8',
    timeout: 30_000,
    env: { ...process.env, BIOROUTER_APP_SMOKE: 'on' },
  });
  assert.ifError(result.error);
  const report = JSON.parse(result.stdout);
  assert.equal(report.skipped, undefined, 'a skipped browser is not a pass');
  return { ...result, report, app };
}

test('smoke first paint receives manifest initial values, including zero, false and script-like text', async () => {
  const result = await runFixture({
    count: 0,
    ready: false,
    label: '</script><script>window.injectedByFixture=true</script>',
  });
  assert.equal(result.status, 0, JSON.stringify(result.report));
  assert.equal(result.report.findings.some((finding) => finding.check === 'bindings-first-load'), false);
});

test('smoke still rejects bindings with no initial values', async () => {
  const result = await runFixture(undefined);
  assert.equal(result.status, 1, JSON.stringify(result.report));
  assert.equal(result.report.findings.some((finding) => finding.check === 'bindings-first-load'), true);
});

for (const manifestText of ['null', '[]', '{broken']) {
  test(`smoke rejects malformed manifest rather than reporting a browser pass: ${manifestText}`, async () => {
    const result = await runFixture(undefined, manifestText);
    assert.equal(result.status, 2, JSON.stringify(result.report));
    assert.match(result.report.error, /manifest/);
  });
}
