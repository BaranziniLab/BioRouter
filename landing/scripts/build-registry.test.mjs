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
// The compiled-in security artifact, named up here because two tests read it
// and one of them runs before the PROTECTED map below is evaluated.
const PROTECTED_PRIVATE_SET = join(REPO, 'crates/biorouter/src/privacy/registry_private.rs');

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
 * A fixture the generator must refuse, the rule whose message proves the
 * refusal came from that rule rather than from a crash, and any extra flags the
 * run needs.
 *
 * The flags matter for exactly one rule. The closed private set is asserted on
 * every REAL run, but a fixture has to opt in with `--assert-private-set` —
 * otherwise every fixture here would have to name its private cards `cdwagent`
 * and `ucsfomopagent`, and the fixtures that exist to exercise *other* rules
 * (`suffixed-download`, `three-affiliations`, `private-no-affiliation`) could
 * not exist at all.
 */
const REJECTED = [
  ['invalid-privacy', /^registry: .*data-privacy must be "private" or "public"/m],
  ['private-no-name', /^registry: .*must declare data-extension-name/m],
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

  // The badge is the no-JS view's ONLY statement of the tier, and the attribute
  // is the generator's. Two sources for one fact drift; publishing while they
  // disagree ships a card that reads "Public" over a private connector.
  ['badge-contradicts-tier', /^registry: .*the privacy badge reads "Public" but the card declares data-privacy="private"/m],
  ['unbadged-card', /^registry: .*carries 0 \[data-privacy-badge\] chips, expected exactly one reading "Public"/m],

  // A catalog of nothing is never a legitimate result — on a real run it
  // rewrites the compiled-in private set to empty.
  ['missing-section', /^registry: .*no element with id="extensions-section"/m],
  ['empty-section', /^registry: .*holds no ext-card/m],
  ['no-download-link', /^registry: .*no \.brxt download link/m],

  // The join key between the published catalog and the installed config entry.
  ['blank-extension-name', /^registry: .*data-extension-name.*only whitespace/m],
  ['punctuated-extension-name', /^registry: .*data-extension-name.*private_agent/m],
  ['duplicate-extension-name', /^registry: .*both declare data-extension-name "twinagent"/m],

  // The private set is a CLOSED LIST of two, both UCSF — not a property a card
  // grants itself by writing data-privacy="private". Checked in both directions
  // and on the affiliation, because all three changes are ones nobody should be
  // able to make as a side effect of editing a card.
  [
    'private-set-third-entry',
    /^registry: .*"thirdprivateagent" is not in the closed private set/m,
    ['--assert-private-set'],
  ],
  [
    'private-set-missing-entry',
    /^registry: the closed private set names "ucsfomopagent", but no card/m,
    ['--assert-private-set'],
  ],
  [
    'private-set-foreign-affiliation',
    /^registry: .*"cdwagent" is private with affiliation \[stanford\], but the closed private set records \[ucsf\]/m,
    ['--assert-private-set'],
  ],
];

for (const [name, rule, args = []] of REJECTED) {
  test(`${name} is refused, by its own rule, writing nothing`, () => {
    const out = outPath();
    const r = run({ input: fixture(name), out, args });
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

  // The badge shares the .ext-tags row with the subject tags, and this row is
  // the tag source. Exact lists, not `includes` — a filter that dropped one tag
  // too many, or published "Private" as a topic, satisfies every other
  // assertion here.
  assert.deepEqual(pub.tags, ['MCP'], 'the Public badge must not be published as a subject tag');
  assert.deepEqual(priv.tags, ['UCSF', 'MCP'], 'the Private badge must not be published as a subject tag');
});

test('a subject tag is not deleted for being spelled like the badge', () => {
  // The first version of the skip filter matched the chip's private/public
  // CLASS. `allTags` also harvests the skills shelf, where "Public" is an
  // ordinary topic, so that filter silently deleted legitimate tags from the
  // published catalog — with no fixture and no assertion on emitted tags, in a
  // generator whose whole job is to be trusted about what it publishes.
  const out = outPath();
  const r = run({ input: fixture('subject-tag-named-public'), out });
  assert.equal(r.code, 0, r.both);
  const reg = JSON.parse(readFileSync(out, 'utf8'));
  assert.deepEqual(reg.extensions[0].tags, ['MCP', 'Public Data', 'Public']);
  assert.equal(reg.extensions[0].privacy, 'public');
});

test('a public card is not refused for describing patients honestly', () => {
  // The false failure the deleted description-keyword heuristic produced. It
  // refused any un-annotated card whose description matched "patient", "ehr",
  // "phi", "clinical record", "medical record" or "de-identified clinical" —
  // inferring a security property from marketing prose. A public literature or
  // imaging tool that says a true thing about patients is not a private
  // extension, and refusing it teaches authors to write around a word list.
  //
  // Without this test, reinstating that heuristic breaks nothing.
  const out = outPath();
  const r = run({ input: fixture('public-description-mentions-patient'), out });
  assert.equal(r.code, 0, r.both);
  const reg = JSON.parse(readFileSync(out, 'utf8'));
  assert.equal(reg.extensions.length, 1);
  assert.equal(reg.extensions[0].privacy, 'public');
  assert.match(reg.extensions[0].description, /patient population/);
});

test('the closed private set is exactly the two UCSF extensions the operator named', () => {
  // The assertion inside the generator is checked against fixtures above; this
  // is the other half — that the list it holds is still the right list, read
  // off the committed security artifact rather than off the generator's own
  // constant. A run that widened both together would pass every fixture here.
  const rust = readFileSync(PROTECTED_PRIVATE_SET, 'utf8');
  assert.match(rust, /PRIVATE_EXTENSIONS: &\[&str\] =\s*&\["cdwagent", "ucsfomopagent"\];/);
  assert.match(
    rust,
    /EXTENSION_AFFILIATIONS: &\[\(&str, &\[&str\]\)\] =\s*&\[\("cdwagent", &\["ucsf"\]\), \("ucsfomopagent", &\["ucsf"\]\)\];/
  );
});

test('--assert-private-set is refused on a real run, which always asserts it', () => {
  // The flag is a fixture affordance, exactly like --emit-rust. Accepting it on
  // a real run would suggest the real run needs asking. Wrapped in
  // protectingArtifacts because a REGRESSION here is a real run: the guard is
  // what stops this test from rewriting the three published files.
  protectingArtifacts(() => {
    const r = run({ args: ['--assert-private-set'] });
    assert.equal(r.code, 1, r.both);
    assert.match(r.stderr, /--assert-private-set is for fixture runs/);
  });
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
  const rust = readFileSync(rs, 'utf8');
  assert.match(rust, /&\["unaffiliatedagent"\];/);
  // ...and NO row in the affiliation table. A row with an empty list would be
  // read by `affiliation_for` as "permits nothing", turning the correct default
  // (unconstrained) into a permanent cross-institutional warning.
  assert.match(rust, /EXTENSION_AFFILIATIONS: &\[\(&str, &\[&str\]\)\] = &\[\];/);
});

test('the compiled-in snapshot carries affiliation and the institution names', () => {
  // DR-26 / Task 47. There is no network path to the registry from Rust, so the
  // compiled snapshot IS the registry there: an affiliation the generator does
  // not emit here is an affiliation the daemon and the CLI can never enforce.
  // The institution map has to come too, because DR-26 requires the warning to
  // NAME both institutions, and the display name lives only in this map.
  const out = outPath();
  const rs = rustPath();
  const r = run({ input: fixture('happy'), out, args: ['--emit-rust', rs] });
  assert.equal(r.code, 0, r.both);

  const rust = readFileSync(rs, 'utf8');
  assert.match(rust, /PRIVATE_EXTENSIONS: &\[&str\] = &\["privatefixtureagent"\];/);
  assert.match(
    rust,
    /EXTENSION_AFFILIATIONS: &\[\(&str, &\[&str\]\)\] =\s*&\[\("privatefixtureagent", &\["ucsf"\]\)\];/
  );
  assert.match(rust, /INSTITUTIONS: &\[\(&str, &str\)\] = &\[\("ucsf", "UCSF"\)\];/);

  // Keyed on the extension NAME, exactly like the private set beside it — the
  // download-derived id is a different string (`suffixed-download` proves it),
  // and an affiliation keyed on the wrong one silently never matches.
  assert.equal(
    rust.includes('"publicfixtureagent"'),
    false,
    'a public extension has no affiliation to declare and must not appear'
  );
});

/**
 * The repo's own rustfmt, or whatever is on PATH — `null` when neither answers.
 *
 * Not a hard dependency: this suite runs in the frontend CI job, where the Rust
 * toolchain is not installed. The assertion it guards is the *corroborating*
 * one; the golden shape beside it always runs.
 */
function findRustfmt() {
  for (const candidate of [join(REPO, 'bin', 'rustfmt'), 'rustfmt']) {
    const probe = spawnSync(candidate, ['--version'], { encoding: 'utf8' });
    if (!probe.error && probe.status === 0) return candidate;
  }
  return null;
}

test('a third affiliated extension wraps the way rustfmt wraps, not the way 100 columns would', () => {
  // The bug this exists to prevent, measured: rustfmt wraps a slice literal by
  // `array_width` (60 columns BETWEEN the brackets), not by the 100-column
  // `max_width`. Two affiliated rows fit either way, so every fixture here
  // passed while the generator emitted, for three rows, the hanging form
  //
  //     pub const EXTENSION_AFFILIATIONS: &[(&str, &[&str])] =
  //         &[("alphawideagent", &["ucsf"]), ("betawideagent", ...)];
  //
  // which `cargo fmt --check` rejects and `--check` below demands — two gates
  // in `just check-everything` with no state satisfying both.
  const out = outPath();
  const rs = rustPath();
  const r = run({ input: fixture('three-affiliations'), out, args: ['--emit-rust', rs] });
  assert.equal(r.code, 0, r.both);

  const rust = readFileSync(rs, 'utf8');
  assert.ok(
    rust.includes(
      'pub const EXTENSION_AFFILIATIONS: &[(&str, &[&str])] = &[\n' +
        '    ("alphawideagent", &["ucsf"]),\n' +
        '    ("betawideagent", &["ucsf"]),\n' +
        '    ("gammawideagent", &["ucsf"]),\n' +
        '];'
    ),
    `three rows must go one per line, not onto one indented line:\n${rust}`
  );

  // ...and that is not merely this generator agreeing with itself. Strip the
  // `#[rustfmt::skip]` attributes — which exist so a miss here can never wedge
  // the build — and rustfmt must leave the result alone.
  const rustfmt = findRustfmt();
  if (rustfmt === null) return;
  const unskipped = join(scratch, `unskipped-${tmpSeq++}.rs`);
  writeFileSync(
    unskipped,
    rust
      .split('\n')
      .filter((line) => line.trim() !== '#[rustfmt::skip]')
      .join('\n')
  );
  const fmt = spawnSync(rustfmt, ['--edition', '2021', '--check', unskipped], {
    encoding: 'utf8',
  });
  assert.equal(
    fmt.status,
    0,
    `rustfmt would rewrite the generated file:\n${fmt.stdout}${fmt.stderr}`
  );
});

test('the emitted consts are exempt from rustfmt, so the two gates cannot deadlock', () => {
  // The structural half of the fix above. Predicting rustfmt is best-effort —
  // its handling of a single very wide element follows no rule this generator
  // implements — so rustfmt is removed as a second authority over the layout of
  // a file whose own header forbids hand-editing it.
  const out = outPath();
  const rs = rustPath();
  const r = run({ input: fixture('three-affiliations'), out, args: ['--emit-rust', rs] });
  assert.equal(r.code, 0, r.both);

  const rust = readFileSync(rs, 'utf8');
  assert.equal(
    (rust.match(/^#\[rustfmt::skip\]$/gm) ?? []).length,
    3,
    `all three consts must carry the attribute:\n${rust}`
  );
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
  'the compiled-in private set': PROTECTED_PRIVATE_SET,
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
