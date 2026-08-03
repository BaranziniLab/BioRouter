// Regression suite for build-registry.mjs.
//
//     node --test landing/scripts/build-registry.test.mjs
//
// The fixtures in scripts/fixtures/ were live but inert: they were run by hand
// once and never again, so nothing stopped a later edit from making a rule
// unreachable. This file is what makes them a gate.
//
// Three things every rejection test asserts, because each one has been the
// difference between a gate and a decoration:
//
//   * the exit status is EXACTLY 1, not merely non-zero. A ReferenceError, an
//     ENOENT on a renamed fixture and an unrecognised flag all exit non-zero,
//     and all three are what a half-finished implementation produces.
//   * stderr names the RULE that fired. Without this a crash reads as a pass.
//   * the file the run asked for does not exist afterwards. `--out /dev/null`
//     cannot tell "wrote nothing" from "wrote garbage and then failed".
//
// No npm dependency: node:test ships with the runtime, so this runs anywhere
// the generator does.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPTS = dirname(fileURLToPath(import.meta.url));
const LANDING = resolve(SCRIPTS, '..');
const REPO = resolve(LANDING, '..');
const GENERATOR = join(SCRIPTS, 'build-registry.mjs');
const FIXTURES = join(SCRIPTS, 'fixtures');

let tmpSeq = 0;
const scratch = mkdtempSync(join(tmpdir(), 'registry-gen-'));
process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));

/** Run the generator the way a person would, from the repo root. */
function run({ input, out, args = [], cwd = REPO }) {
  const argv = [GENERATOR];
  if (input !== undefined) argv.push('--input', input);
  if (out !== undefined) argv.push('--out', out);
  argv.push(...args);
  const r = spawnSync(process.execPath, argv, { cwd, encoding: 'utf8' });
  if (r.error) throw r.error;
  return { code: r.status, stdout: r.stdout, stderr: r.stderr, both: r.stdout + r.stderr };
}

const fixture = (name) => join(FIXTURES, `${name}.html`);
const outPath = () => join(scratch, `out-${tmpSeq++}.json`);

/**
 * A fixture the generator must refuse, and the rule whose message proves the
 * refusal came from that rule rather than from a crash.
 */
const REJECTED = [
  ['invalid-privacy', /^registry: .*data-privacy must be "private" or "public"/m],
  ['private-no-name', /^registry: .*must declare data-extension-name/m],
  ['clinical-unannotated', /^registry: .*declares no data-privacy/m],
  ['unknown-institution', /^registry: .*data-affiliation names "atlantis"/m],
  ['public-with-affiliation', /^registry: .*data-affiliation is meaningless/m],
  ['empty-affiliation', /^registry: .*data-affiliation is present but empty/m],

  // Attribute syntax HTML allows and the card-fragment substring search did
  // not. Each of these read as an un-annotated (public) card, which is the
  // fail-open direction.
  ['spaced-attribute', /^registry: .*must declare data-extension-name/m],
  ['single-quoted-attribute', /^registry: .*must declare data-extension-name/m],
  ['boolean-attribute', /^registry: .*data-privacy is present with no value/m],
  ['nested-metadata', /^registry: .*data-privacy.*must be declared on the card element itself/m],

  // A catalog of nothing is never a legitimate result — on a real run it
  // rewrites the compiled-in private set to empty.
  ['missing-section', /^registry: .*no element with id="extensions-section"/m],
  ['empty-section', /^registry: .*holds no ext-card/m],
  ['no-download-link', /^registry: .*no \.brxt download link/m],
];

for (const [name, rule] of REJECTED) {
  test(`${name} is refused, by its own rule, writing nothing`, () => {
    const out = outPath();
    const r = run({ input: fixture(name), out });
    assert.equal(r.code, 1, `expected exit 1, got ${r.code}\n${r.both}`);
    assert.match(r.stderr, rule);
    assert.equal(
      existsSync(out),
      false,
      'a rejected run must leave the file it was asked to write absent'
    );
  });
}

test('the happy fixture parses, so the refusals above are not "zero cards"', () => {
  const out = outPath();
  const r = run({ input: fixture('happy'), out });
  assert.equal(r.code, 0, r.both);
  assert.match(r.stdout, /registry\.json written: 2 extensions, 0 skills/);

  const reg = JSON.parse(readFileSync(out, 'utf8'));
  assert.equal(reg.version, 2);
  assert.deepEqual(reg.institutions, { ucsf: 'UCSF' });
  assert.equal(reg.extensions.length, 2);

  // Positive assertions on the emitted document, not just on the exit status.
  // Every rule below has a wrong implementation that still exits 0.
  const pub = reg.extensions.find((e) => e.id === 'publicfixtureagent');
  assert.equal(pub.privacy, 'public', 'an un-annotated card is public by construction');
  assert.equal('extension_name' in pub, false);
  assert.equal('affiliation' in pub, false, 'absent affiliation must stay absent, not become []');

  const priv = reg.extensions.find((e) => e.id === 'privatefixtureagent');
  assert.equal(priv.privacy, 'private');
  assert.equal(priv.extension_name, 'privatefixtureagent', 'emitted in name_to_key form');
  assert.deepEqual(priv.affiliation, ['ucsf']);
});

test('attribute order does not decide whether a card exists', () => {
  // A card that reads perfectly in a browser must not be invisible to the
  // generator: an invisible private card is an empty compiled-in private set.
  const out = outPath();
  const r = run({ input: fixture('reordered-attributes'), out });
  assert.equal(r.code, 0, r.both);
  const reg = JSON.parse(readFileSync(out, 'utf8'));
  assert.equal(reg.extensions.length, 1, 'the card must be found with class last');
  assert.equal(reg.extensions[0].privacy, 'private');
  assert.equal(reg.extensions[0].extension_name, 'reorderedagent');
});

test('--input without --out is refused rather than defaulted', () => {
  const r = run({ input: fixture('happy') });
  assert.equal(r.code, 1, r.both);
  assert.match(r.stderr, /--input requires --out/);
});
