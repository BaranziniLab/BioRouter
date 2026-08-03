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
import { existsSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
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
const rustPath = () => join(scratch, `private-${tmpSeq++}.rs`);

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

  // The join key between the published catalog and the installed config entry.
  ['blank-extension-name', /^registry: .*data-extension-name.*only whitespace/m],
  ['punctuated-extension-name', /^registry: .*data-extension-name.*private_agent/m],
  ['duplicate-extension-name', /^registry: .*both declare data-extension-name "twinagent"/m],
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

test('the compiled private set is keyed on the name, not on the download filename', () => {
  // The case the extension_name field exists for. Every private entry in the
  // real catalog happens to have id == extension_name, so a generator that
  // emitted the id into the Rust const passes every other test here.
  const out = outPath();
  const rs = rustPath();
  const r = run({ input: fixture('suffixed-download'), out, args: ['--emit-rust', rs] });
  assert.equal(r.code, 0, r.both);

  const reg = JSON.parse(readFileSync(out, 'utf8'));
  assert.equal(reg.extensions[0].id, 'suffixagent-1.2.3');
  assert.equal(reg.extensions[0].extension_name, 'suffixagent');

  const rust = readFileSync(rs, 'utf8');
  assert.match(rust, /PRIVATE_EXTENSIONS: &\[&str\] = &\["suffixagent"\];/);
  assert.equal(
    rust.includes('1.2.3'),
    false,
    'the download-derived id must not reach the compiled-in private set'
  );
});

test('a private extension with no affiliation is unconstrained, not rejected', () => {
  // happy.html's only private card is affiliated, so without this a generator
  // that REQUIRED affiliation on every private entry passes the whole suite.
  const out = outPath();
  const rs = rustPath();
  const r = run({ input: fixture('private-no-affiliation'), out, args: ['--emit-rust', rs] });
  assert.equal(r.code, 0, r.both);

  const reg = JSON.parse(readFileSync(out, 'utf8'));
  assert.equal(reg.extensions[0].privacy, 'private');
  assert.equal(
    'affiliation' in reg.extensions[0],
    false,
    'absent means unconstrained and must stay absent, never [] '
  );
  assert.match(readFileSync(rs, 'utf8'), /&\["unaffiliatedagent"\];/);
});

test('--input without --out is refused rather than defaulted', () => {
  const r = run({ input: fixture('happy') });
  assert.equal(r.code, 1, r.both);
  assert.match(r.stderr, /--input requires an --out/);
});

// ---- The three published artifacts are unwritable by a fixture run ---------
// The guard compared raw path STRINGS, so any second spelling of a protected
// file slipped past it while resolving to the same inode. Each test below
// snapshots the real file and restores it in a `finally`, so a regression makes
// the test red without also rewriting the tracked catalog.
const PROTECTED = {
  'the published registry': join(LANDING, 'registry.json'),
  'the desktop fallback snapshot': join(
    REPO,
    'ui/desktop/src/components/baam/registry.fallback.json'
  ),
  'the compiled-in private set': join(
    REPO,
    'crates/biorouter/src/privacy/registry_private.rs'
  ),
};

/** Run `body`, and put every protected artifact back byte-for-byte afterwards. */
function protectingArtifacts(body) {
  const before = Object.values(PROTECTED).map((p) => [p, readFileSync(p)]);
  try {
    body();
  } finally {
    for (const [p, bytes] of before) writeFileSync(p, bytes);
  }
}

test('a relative --out that aliases the real registry is refused', () => {
  protectingArtifacts(() => {
    const real = PROTECTED['the published registry'];
    const before = readFileSync(real);
    // Textually different from the absolute default, same file.
    const r = run({ input: fixture('happy'), out: 'landing/registry.json', cwd: REPO });
    assert.equal(r.code, 1, r.both);
    assert.match(r.stderr, /--input requires an --out/);
    assert.deepEqual(readFileSync(real), before, 'the real catalog must not be rewritten');
  });
});

test('a symlink to the real registry is refused', () => {
  protectingArtifacts(() => {
    const real = PROTECTED['the published registry'];
    const before = readFileSync(real);
    const alias = join(scratch, `alias-${tmpSeq++}.json`);
    symlinkSync(real, alias);
    const r = run({ input: fixture('happy'), out: alias });
    assert.equal(r.code, 1, r.both);
    assert.match(r.stderr, /--input requires an --out/);
    assert.deepEqual(readFileSync(real), before, 'the real catalog must not be rewritten');
  });
});

for (const [what, path] of Object.entries(PROTECTED)) {
  if (path === PROTECTED['the published registry']) continue;
  test(`--out pointing at ${what} is refused`, () => {
    protectingArtifacts(() => {
      const before = readFileSync(path);
      const r = run({ input: fixture('happy'), out: path });
      assert.equal(r.code, 1, r.both);
      assert.deepEqual(readFileSync(path), before);
    });
  });
}

test('--emit-rust pointing at the compiled-in private set is refused', () => {
  protectingArtifacts(() => {
    const rs = PROTECTED['the compiled-in private set'];
    const before = readFileSync(rs);
    const r = run({ input: fixture('happy'), out: outPath(), args: ['--emit-rust', rs] });
    assert.equal(r.code, 1, r.both);
    assert.deepEqual(readFileSync(rs), before);
  });
});

test('a flag with no value is refused by name, not by a stack trace', () => {
  const r = run({ input: fixture('happy'), args: ['--out'] });
  assert.equal(r.code, 1, r.both);
  assert.match(r.stderr, /^registry: --out needs a value/m);
});

// ---- Drift between the three outputs is detectable ------------------------
// They are written by one command from one source, which is not the same as
// being in sync: an interrupted run, a hand edit or a rebase that took one side
// of a conflict all leave a published catalog the app does not enforce. Nothing
// could tell before `--check`.
test('--check passes on the committed tree', () => {
  const r = run({ args: ['--check'] });
  assert.equal(r.code, 0, r.both);
  assert.match(r.stdout, /all three outputs are current/);
});

for (const [what, path] of Object.entries(PROTECTED)) {
  test(`--check notices ${what} drifting`, () => {
    protectingArtifacts(() => {
      writeFileSync(path, readFileSync(path, 'utf8') + '\n// drifted\n');
      const r = run({ args: ['--check'] });
      assert.equal(r.code, 1, r.both);
      assert.match(r.stderr, new RegExp(path.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
    });
  });
}

test('--check refuses to pretend a fixture is the real input', () => {
  const r = run({ input: fixture('happy'), out: outPath(), args: ['--check'] });
  assert.equal(r.code, 1, r.both);
  assert.match(r.stderr, /--check/);
});

test('a held generation lock is refused by name', () => {
  // Two generators interleaving is how the three outputs end up describing
  // different worlds. The message has to name the file, because a crash is the
  // one way a stale lock can outlive its run.
  const lock = join(LANDING, '.registry-build.lock');
  writeFileSync(lock, 'held by the test suite\n');
  try {
    const r = run({ args: [] });
    assert.equal(r.code, 1, r.both);
    assert.match(r.stderr, /\.registry-build\.lock/);
  } finally {
    rmSync(lock, { force: true });
  }
});
