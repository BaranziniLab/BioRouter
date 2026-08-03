#!/usr/bin/env node
// The "Extension agents in the marketplace" table in `landing/docs.html` is
// hand-written prose: no generator emits it and, until this file existed, no
// check read it. That is the drift nobody would notice — the day BAAM starts
// showing privacy badges, a docs table that says an agent is Public while
// `registry.json` tags it private is a page telling a user their clinical
// connector is safe for a public session. The badge and the table disagree and
// only the badge is enforced.
//
// So the table gains a Privacy column and this file joins it back to
// `registry.json`, which is the single source of truth for the tier (the same
// file `build-registry.mjs` reads the BAAM cards into, and writes the desktop
// snapshot and the compiled-in Rust baseline from). It runs from
// `check-consistency.mjs --check`, i.e. inside `just check-everything` and the
// landing deploy gate, and its own mutant suite is
// `check-docs-privacy.test.mjs`.
//
// **This file reads markup, not a DOM**, and every rule below that looks
// paranoid is one a review found a way through:
//   * comments are stripped first — markup a browser never renders must not
//     satisfy a check about what the page says (a commented-out `<th>Privacy</th>`
//     passed a raw-text search while the rendered table had no such column);
//   * the section and its table must occur exactly once — a second, conflicting
//     copy appended anywhere below is what the reader actually sees, and a
//     first-match reader would grade the wrong one;
//   * rows are matched with attributes allowed — `<tr class="…">` is the same
//     row to a browser and was invisible to a `<tr>`-literal regex;
//   * a row must have exactly as many cells as the header has columns, or the
//     tier is being read out of some other column.
//
// Scope, recorded so a later reviewer does not "fix" an absence:
// `landing/skills.html` and `landing/index.html` list **no extensions at all**
// — skills carry no privacy tier and the home page names none — so they need no
// Privacy column and are deliberately not checked here. `landing/baam.html` is
// the hand-maintained source `build-registry.mjs` generates the registry *from*
// (not the other way round), and it is gated by `check-registry`, so it is not
// this file's business either. Only `docs.html`'s hand-written table is.
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SECTION = 'Extension agents in the marketplace';

// The agents the table is expected to document, each exactly once.
//
// It duplicates the page, which is the point: without it, deleting a row is
// silent. The completeness loop at the bottom catches a missing *private*
// agent, but a public row can vanish — during an edit, a merge, a careless
// rewrite — and every remaining row still agrees with the registry, so nothing
// complains and the page quietly documents five agents. Extra rows are fine and
// need no edit here; **removing** one has to be deliberate enough to say so.
const DOCUMENTED_AGENTS = [
  'SPOKEAgent',
  'UCSFOMOPAgent',
  'CDWAgent',
  'PlaywrightAgent',
  'CodeGraphAgent',
  'BiorOffice',
];

// Mirrors `nameToKey` in check-consistency.mjs and `name_to_key` in
// crates/biorouter/src/privacy/extensions.rs, so a docs row spelled
// "PlaywrightAgent" still joins to the registry's "Playwright Agent".
const nameToKey = (value) => value.replace(/\s+/g, '').toLowerCase();

// Markup a browser never renders must never satisfy a check about what the page
// tells a user. Everything below reads comment-free HTML.
const stripComments = (html) => html.replace(/<!--[\s\S]*?-->/g, '');

const stripTags = (html) =>
  html
    .replace(/<[^>]*>/g, '')
    .replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ')
    .trim();

const cellsOf = (row, tag) =>
  [...row.matchAll(new RegExp(`<${tag}\\b[^>]*>([\\s\\S]*?)</${tag}>`, 'g'))].map((match) => match[1]);

const countOf = (list, value) => list.filter((entry) => entry === value).length;

/**
 * @param {string} rawDocs contents of landing/docs.html
 * @param {object} registry parsed landing/registry.json
 * @returns {string[]} failure messages; empty means the table agrees with the registry
 */
export function checkDocsPrivacy(rawDocs, registry) {
  const failures = [];
  const fail = (message) => failures.push(message);

  const docs = stripComments(rawDocs);

  const sections = docs.split(SECTION).length - 1;
  if (sections === 0) {
    return [`docs.html no longer has a "${SECTION}" section — did it move? Update check-docs-privacy.mjs`];
  }
  if (sections > 1) {
    return [
      `docs.html mentions "${SECTION}" ${sections} times — this check grades the first one, so a ` +
        'second copy would go unread while a reader scrolls to it. Keep exactly one.',
    ];
  }

  // The section runs to the next <h2>: a table further down the page belongs to
  // some other section and must not be mistaken for this one's.
  const sectionAt = docs.indexOf(SECTION);
  const after = docs.slice(sectionAt);
  const nextHeading = after.slice(SECTION.length).search(/<h2\b/);
  const scope = nextHeading === -1 ? after : after.slice(0, SECTION.length + nextHeading);

  const tables = [...scope.matchAll(/<table\b[^>]*>([\s\S]*?)<\/table>/g)];
  if (tables.length === 0) {
    return [`docs.html: no table follows the "${SECTION}" heading`];
  }
  if (tables.length > 1) {
    return [
      `docs.html: the "${SECTION}" section holds ${tables.length} tables — this check grades the ` +
        'first, so the others would be unchecked. Keep exactly one.',
    ];
  }
  const body = tables[0][1];

  const head = /<thead\b[^>]*>([\s\S]*?)<\/thead>/.exec(body);
  if (!head) return [`docs.html: the "${SECTION}" table has no <thead>`];
  const headers = cellsOf(head[1], 'th').map(stripTags);
  for (const column of ['Agent', 'Privacy']) {
    const seen = countOf(headers, column);
    if (seen === 0) {
      fail(`docs.html: the "${SECTION}" table has no ${column} column (headers: ${headers.join(', ')})`);
    } else if (seen > 1) {
      fail(
        `docs.html: the "${SECTION}" table has more than one ${column} column (${seen}) — ` +
          'a reader cannot tell which one governs, and this check would only read the first'
      );
    }
  }
  const privacyColumn = headers.indexOf('Privacy');
  const agentColumn = headers.indexOf('Agent');
  if (failures.length) return failures;

  // The registry is the source of truth. Index it under every spelling it
  // publishes — id, display name, and the manifest name the installed
  // extension reports — because the docs table uses whichever reads best.
  //
  // A spelling two extensions share is not a lookup to resolve by insertion
  // order: `Map.set` would let a later private entry named "SPOKE Agent" take
  // over the public SPOKEAgent's key, and the docs row for it could then be
  // flipped to Private with this check applauding. Refuse the collision instead.
  const byKey = new Map();
  for (const ext of registry.extensions || []) {
    if (ext.privacy !== 'private' && ext.privacy !== 'public') {
      fail(`registry.json: extension ${ext.id || '(unnamed)'} declares no privacy tier`);
      continue;
    }
    for (const spelling of [ext.id, ext.name, ext.extension_name]) {
      if (!spelling) continue;
      const key = nameToKey(spelling);
      const owner = byKey.get(key);
      if (owner && owner !== ext) {
        fail(
          `registry.json: ${ext.id || ext.name} and ${owner.id || owner.name} both normalise to ` +
            `"${key}" — the alias already belongs to another extension, so a docs row naming it ` +
            'would be graded against whichever one this check happened to read first'
        );
        continue;
      }
      if (!owner) byKey.set(key, ext);
    }
  }
  if (byKey.size === 0) {
    return [...failures, 'registry.json publishes no usable extensions to check the docs table against'];
  }

  const rows = [...body.matchAll(/<tr\b[^>]*>([\s\S]*?)<\/tr>/g)]
    .map((match) => cellsOf(match[1], 'td'))
    .filter((cells) => cells.length > 0);
  if (rows.length === 0) {
    return [...failures, `docs.html: the "${SECTION}" table lists no agents`];
  }

  /** @type {Map<string, string>} normalised agent key -> the spelling used */
  const seenAgents = new Map();
  const listed = new Set();
  for (const cells of rows) {
    const agent = stripTags(cells[agentColumn] ?? '');
    if (!agent) {
      fail(`docs.html: a row of the "${SECTION}" table names no agent`);
      continue;
    }
    // A short row does not read an empty tier, it reads the NEXT column's — a
    // row missing its Credentials cell would grade "What it connects" prose as
    // the tier, and a row missing an earlier one would silently shift Privacy.
    if (cells.length !== headers.length) {
      fail(
        `docs.html: the ${agent} row has ${cells.length} cells but the "${SECTION}" table has ` +
          `${headers.length} columns — the Privacy cell is not the one being read`
      );
      continue;
    }
    const key = nameToKey(agent);
    if (seenAgents.has(key)) {
      fail(`docs.html: the "${SECTION}" table lists ${agent} more than once`);
      continue;
    }
    seenAgents.set(key, agent);
    const declared = stripTags(cells[privacyColumn] ?? '');
    if (declared !== 'Private' && declared !== 'Public') {
      fail(`docs.html: ${agent} declares privacy "${declared}" — expected exactly Private or Public`);
      continue;
    }
    // An id the registry does not publish cannot be checked at all, and a docs
    // table naming an extension nobody can install is its own bug.
    const ext = byKey.get(key);
    if (!ext) {
      fail(`docs.html: ${agent} is listed in the "${SECTION}" table but is not in registry.json`);
      continue;
    }
    listed.add(ext.id);
    const expected = ext.privacy === 'private' ? 'Private' : 'Public';
    if (declared !== expected) {
      fail(
        `docs.html: ${agent} is documented as ${declared} but registry.json tags it ` +
          `${ext.privacy} — the docs table and the marketplace badge disagree`
      );
    }
  }

  // A row that quietly disappeared. See DOCUMENTED_AGENTS above for why the
  // list is pinned here rather than derived from the page it describes.
  for (const agent of DOCUMENTED_AGENTS) {
    if (!seenAgents.has(nameToKey(agent))) {
      fail(
        `docs.html: the "${SECTION}" table no longer lists ${agent} — if that removal is ` +
          'intended, drop it from DOCUMENTED_AGENTS in check-docs-privacy.mjs and say why'
      );
    }
  }

  // A private extension the table simply omits is the same drift wearing a
  // different hat: the reader concludes the six listed agents are the whole
  // story and that theirs is not private. The asymmetry is deliberate and the
  // page states it — the table is exhaustive for Private and curated for
  // Public — so this loop is what makes that sentence true.
  for (const ext of registry.extensions || []) {
    if (ext.privacy === 'private' && !listed.has(ext.id)) {
      fail(
        `docs.html: registry.json tags ${ext.name || ext.id} private, but the "${SECTION}" ` +
          `table does not list it — add a row so the Privacy column is complete`
      );
    }
  }

  return failures;
}

// Runnable on its own for a quick answer; `check-consistency.mjs --check` is
// what the Justfile actually gates on.
if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
  const failures = checkDocsPrivacy(
    readFileSync(join(root, 'docs.html'), 'utf8'),
    JSON.parse(readFileSync(join(root, 'registry.json'), 'utf8'))
  );
  if (failures.length) {
    console.error(failures.map((failure) => `- ${failure}`).join('\n'));
    process.exit(1);
  }
  console.log(`docs.html's "${SECTION}" Privacy column agrees with registry.json.`);
}
