#!/usr/bin/env node
// Generate registry.json from baam.html.
//
// The BAAM page keeps static cards as the no-JS fallback, and registry.json is
// the machine-readable catalog consumed by the page at runtime and by Biorouter.
// This script regenerates that catalog from the fallback cards so both surfaces
// stay in sync.
//
//   node scripts/build-registry.mjs
//
// Re-run this whenever baam.html's cards change.
//
// A real run writes THREE files from one source, so the privacy annotations on
// the cards cannot drift from what the app enforces:
//
//   landing/registry.json                                  — the published catalog
//   ui/desktop/src/components/baam/registry.fallback.json  — the bundled snapshot
//   crates/biorouter/src/privacy/registry_private.rs       — the compiled-in set
//
// Two flags exist so the validations below can be exercised against the
// fixtures in scripts/fixtures/ without touching any of that:
//
//   node scripts/build-registry.mjs --input scripts/fixtures/happy.html \
//                                   --out /tmp/happy-registry.json

import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const REPO = join(ROOT, '..');
const SITE_BASE = 'https://biorouter.ucsf.edu/';

// --- argv, so the validations can be exercised against fixtures -------------
// Both flags are needed. `--input` alone still WRITES to landing/registry.json,
// so a fixture run would overwrite the real 37-extension catalog with the
// fixture's two entries — which is why passing one without the other is an
// error rather than a default.
const argv = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = argv.indexOf(name);
  return i === -1 ? fallback : argv[i + 1];
};
const DEFAULT_INPUT = join(ROOT, 'baam.html');
const DEFAULT_OUTPUT = join(ROOT, 'registry.json');
const INPUT = flag('--input', DEFAULT_INPUT);
const OUTPUT = flag('--out', DEFAULT_OUTPUT);
if (INPUT !== DEFAULT_INPUT && OUTPUT === DEFAULT_OUTPUT) {
  console.error('registry: --input requires --out; refusing to overwrite landing/registry.json');
  process.exit(1);
}
// The two sibling outputs belong to a real run only. A fixture run writes its
// own --out and nothing else, or a two-card fixture would leave the compiled-in
// private set holding two invented names.
const IS_REAL_RUN = OUTPUT === DEFAULT_OUTPUT;

// Collect every violation and report them together, then exit non-zero. One
// `throw` per violation would hide the second and third problems behind the
// first, which is how a publisher ends up fixing this file three times.
const violations = [];
const fail = (msg) => violations.push(msg);

// Institution id → display name. The warning copy for a cross-institutional
// flow renders from this, so an affiliation naming an id that is absent here
// has no name to render with and is a build failure rather than a silent pass.
const INSTITUTIONS = {
  ucsf: 'UCSF',
};

// A card whose own description says it reads clinical data, and which declares
// no tier at all, is a classification nobody made — see the check below.
const CLINICAL_KEYWORDS = [
  'patient',
  'clinical record',
  'ehr',
  'phi',
  'medical record',
  'de-identified clinical',
];

// The key form `name_to_key` reduces an extension name to before it looks up a
// tier (crates/biorouter/src/config/extensions.rs): whitespace stripped,
// lowercased. Emitting the key form into BOTH the JSON and the Rust const is
// what makes the two impossible to disagree about. It is not a heuristic — no
// suffix is stripped and no character is invented; it is the same reduction the
// resolver already applies to whatever the installed config entry is called.
const nameToKey = (s) => s.replace(/\s+/g, '').toLowerCase();

const html = readFileSync(INPUT, 'utf8');

const stripTags = (s) =>
  s
    .replace(/<[^>]+>/g, '')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/\s+/g, ' ')
    .trim();

const slugFromUrl = (url) => {
  const file = url.split('/').pop() || url;
  return file.replace(/\.(zip|brxt)$/i, '');
};

const absolutize = (url) =>
  /^https?:\/\//i.test(url) ? url : SITE_BASE + url.replace(/^\.?\//, '');

// Extract the inner HTML of the first element with the given id (div-level).
function sliceById(id) {
  const open = html.indexOf(`id="${id}"`);
  if (open === -1) return '';
  // Walk forward to the matching close of this <div>.
  const start = html.lastIndexOf('<div', open);
  let depth = 0;
  let i = start;
  const re = /<\/?div\b[^>]*>/g;
  re.lastIndex = start;
  let m;
  while ((m = re.exec(html))) {
    if (m[0].startsWith('</')) depth--;
    else depth++;
    if (depth === 0) {
      i = m.index + m[0].length;
      break;
    }
  }
  return html.slice(start, i);
}

function pickCards(scope, cardClass) {
  // Split on the card boundary; each fragment after the first starts inside a card.
  const cards = [];
  // Match the card class exactly — followed by a space (more classes) or the
  // closing quote — so we don't also match `${cardClass}-header` etc.
  const re = new RegExp(`<div class="${cardClass}(?: [^"]*)?"([^>]*)>`, 'g');
  let m;
  const indices = [];
  while ((m = re.exec(scope))) indices.push({ start: m.index, attrStart: m.index });
  for (let k = 0; k < indices.length; k++) {
    const from = indices[k].start;
    const to = k + 1 < indices.length ? indices[k + 1].start : scope.length;
    cards.push(scope.slice(from, to));
  }
  return cards;
}

const first = (re, s) => {
  const m = re.exec(s);
  return m ? m[1] : '';
};

const allTags = (containerRe, s) => {
  const block = first(containerRe, s);
  if (!block) return [];
  return [...block.matchAll(/<span class="tag[^"]*">([^<]+)<\/span>/g)].map((m) =>
    stripTags(m[1])
  );
};

// ---- Extensions ----------------------------------------------------------
const extScope = sliceById('extensions-section');
const extensions = pickCards(extScope, 'ext-card').map((card) => {
  const name = stripTags(first(/<h3>([\s\S]*?)<\/h3>/, card));
  const org = stripTags(first(/<div class="ext-org">([\s\S]*?)<\/div>/, card));
  const description = stripTags(first(/<p class="ext-desc">([\s\S]*?)<\/p>/, card));
  const github = first(/<a href="([^"]+)"[^>]*class="ext-gh-link"/, card);
  const download = first(/<a href="([^"]+)"[^>]*class="brxt-chip"/, card);
  const tags = allTags(/<div class="ext-tags">([\s\S]*?)<\/div>/, card);
  const license = first(/data-license="([^"]+)"/, card) || 'Apache-2.0';
  const id = slugFromUrl(download);

  // The DEFAULT matters more than the extraction: an un-annotated card is
  // public by construction, so R11(ii)'s fail-open direction is enforced by the
  // tool rather than by reviewer discipline. An annotation that is PRESENT but
  // unparseable is a different thing and must not collapse into the default —
  // hence the `[^"]*`, which lets `data-privacy=""` reach the check below.
  const declaresPrivacy = /data-privacy=/.test(card);
  const privacy = declaresPrivacy ? first(/data-privacy="([^"]*)"/, card) : 'public';
  const extensionName = first(/data-extension-name="([^"]+)"/, card) || '';
  // null = absent = unconstrained, which is the right default: most extensions
  // carry no institutional constraint and must not all become mismatches on the
  // day this ships. An empty list is NOT the same thing — see below.
  const affiliation = /data-affiliation=/.test(card)
    ? first(/data-affiliation="([^"]*)"/, card).split(/\s+/).filter(Boolean)
    : null;

  if (privacy !== 'private' && privacy !== 'public') {
    fail(`${id}: data-privacy must be "private" or "public", got "${privacy}"`);
  }
  if (privacy === 'private' && !extensionName) {
    // No suffix-stripping heuristic. `spokeagent-0.4.1` proves ids and
    // manifest names diverge, and a heuristic in a security path is right
    // until it isn't.
    fail(`${id}: a private extension must declare data-extension-name`);
  }
  // Forces the medcp/msbaseagent revisit AT PUBLISH TIME rather than relying on
  // someone remembering: the private badge is granted by publishing to BAAM.
  if (!declaresPrivacy) {
    // `.some(k => …)` would scope `k` to the callback, so the message could not
    // name the keyword from outside it. `.find` binds the match where the
    // message can see it.
    const hit = CLINICAL_KEYWORDS.find((k) => description.toLowerCase().includes(k));
    if (hit) {
      fail(`${id}: description matches "${hit}" but the card declares no data-privacy`);
    }
  }
  // DR-26 — affiliation is a third axis. HIPAA compliance does not transfer
  // between institutions, so a private connector may name whose agreements
  // cover its data.
  if (affiliation !== null) {
    if (privacy !== 'private') {
      fail(
        `${id}: data-affiliation is meaningless on a ${privacy} extension — ` +
          `affiliation asks under whose agreements, which only arises once data is private`
      );
    }
    if (affiliation.length === 0) {
      fail(
        `${id}: data-affiliation is present but empty — absent means unconstrained, ` +
          `so an empty list would turn a typo into a granted flow`
      );
    }
    for (const inst of affiliation) {
      if (!Object.prototype.hasOwnProperty.call(INSTITUTIONS, inst)) {
        fail(`${id}: data-affiliation names "${inst}", which is not in the institutions map`);
      }
    }
  }

  // org looks like "BaranziniLab · UCSF · v0.4.3"
  const parts = org.split('·').map((p) => p.trim());
  const version = parts.find((p) => /^v?\d/.test(p)) || '';
  const organization = parts.filter((p) => p !== version).join(' · ');
  return {
    id,
    name,
    organization,
    version,
    description,
    tags,
    github,
    download: absolutize(download),
    filename: download.split('/').pop(),
    license,
    privacy,
    ...(extensionName ? { extension_name: nameToKey(extensionName) } : {}),
    ...(affiliation !== null ? { affiliation } : {}),
  };
});

// ---- The compiled-in private set -----------------------------------------
// Rendered as Rust source rather than JSON because there is no network path to
// the registry from the CLI or the daemon: the only fetch lives in Electron's
// `registry:fetch` handler. Without a compiled-in copy, Rust could enforce
// nothing at all.
//
// Written to match what rustfmt would produce, so regenerating never leaves the
// tree failing `cargo fmt --check`: a slice literal stays on one line while it
// fits inside rustfmt's 100-column default, and wraps four-space-indented when
// it does not.
function renderPrivateSet(exts) {
  const keys = exts
    .filter((e) => e.privacy === 'private')
    .map((e) => e.extension_name)
    .sort();
  const decl = 'pub const PRIVATE_EXTENSIONS: &[&str] = &[';
  const inline = `${decl}${keys.map((k) => `"${k}"`).join(', ')}];`;
  const body =
    inline.length <= 100
      ? inline
      : `${decl}\n${keys.map((k) => `    "${k}",`).join('\n')}\n];`;
  return (
    [
      '//! **Generated file — do not edit by hand.**',
      '//!',
      '//! `landing/scripts/build-registry.mjs` writes this in the same run as',
      '//! `landing/registry.json` and the desktop fallback snapshot, from the',
      '//! `data-privacy` / `data-extension-name` annotations on the extension cards',
      '//! in `landing/baam.html`. Regenerate all three with:',
      '//!',
      '//!     node landing/scripts/build-registry.mjs',
      '//!',
      '//! The set has to live in Rust: there is no network path to the registry from',
      '//! here (the only fetch is the Electron `main.ts` `registry:fetch` handler), so',
      '//! without this file the CLI and the daemon can enforce nothing.',
      '//!',
      '//! ⚠ Nothing fails CI when this file and the registry disagree. One command',
      '//! rewrites both from one source, so the only way to drift is to hand-edit this',
      '//! file — which is what the first line asks you not to do.',
      '',
      '/// The BAAM extensions whose cards declare `data-privacy="private"`, and which',
      '/// so must never be admitted to a public session.',
      '///',
      '/// Values are `name_to_key` **keys** — whitespace-stripped and lowercased —',
      '/// which is the form `classify_extension` reduces its argument to before the',
      '/// lookup. That makes the entry match either spelling the registry publishes:',
      '/// the id (`cdwagent`) or the bundle `manifest.name` (`CDWAgent`).',
      body,
    ].join('\n') + '\n'
  );
}

// ---- Skills --------------------------------------------------------------
function parseSkillGrid(id, category) {
  const scope = sliceById(id);
  return pickCards(scope, 'skill-card').map((card) => {
    const name = stripTags(first(/<h3>([\s\S]*?)<\/h3>/, card));
    const type = stripTags(first(/<div class="skill-type">([\s\S]*?)<\/div>/, card));
    const description = stripTags(first(/<p class="skill-desc">([\s\S]*?)<\/p>/, card));
    const download = first(/<a href="([^"]+)"[^>]*class="skill-dl-btn"/, card);
    const tags = allTags(/<div class="skill-tags">([\s\S]*?)<\/div>/, card);
    const license = first(/data-license="([^"]+)"/, card) || 'Apache-2.0';
    const dataTags = first(/<div class="skill-card[^"]*" data-tags="([^"]*)"/, card)
      .split(/\s+/)
      .filter(Boolean);
    return {
      id: slugFromUrl(download),
      name,
      category,
      type,
      description,
      tags,
      keywords: dataTags,
      download: absolutize(download),
      filename: download.split('/').pop(),
      license,
    };
  });
}

const skills = [
  ...parseSkillGrid('core-skill-grid', 'Core'),
  ...parseSkillGrid('dev-skill-grid', 'Developer'),
  ...parseSkillGrid('bio-skill-grid', 'Biomedical'),
];

const registry = {
  version: 2,
  source: 'https://biorouter.ucsf.edu/baam',
  institutions: INSTITUTIONS,
  extensions,
  skills,
};

if (violations.length) {
  for (const v of violations) console.error(`registry: ${v}`);
  console.error(`registry: ${violations.length} validation failure(s); nothing written`);
  process.exit(1);
}

const out = JSON.stringify(registry, null, 2) + '\n';
writeFileSync(OUTPUT, out);

if (IS_REAL_RUN) {
  // The desktop app bundles a snapshot so Browse works offline. It is the same
  // document; copying it here is what keeps "verified in sync, by luck" from
  // being how that stays true.
  writeFileSync(join(REPO, 'ui/desktop/src/components/baam/registry.fallback.json'), out);
  // And the compiled-in private set, which is the only form of this catalog the
  // CLI and the daemon can read at all — there is no network path to the
  // registry from Rust.
  writeFileSync(
    join(REPO, 'crates/biorouter/src/privacy/registry_private.rs'),
    renderPrivateSet(extensions)
  );
}

console.log(
  `registry.json written: ${extensions.length} extensions, ${skills.length} skills ` +
    `(${skills.filter((s) => s.category === 'Core').length} core, ` +
    `${skills.filter((s) => s.category === 'Developer').length} developer, ` +
    `${skills.filter((s) => s.category === 'Biomedical').length} biomedical)`
);
