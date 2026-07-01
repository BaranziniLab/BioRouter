#!/usr/bin/env node
// Generate registry.json from baam.html.
//
// The BAAM page keeps static cards as the no-JS fallback, and registry.json is
// the machine-readable catalog consumed by the page at runtime and by BioRouter.
// This script regenerates that catalog from the fallback cards so both surfaces
// stay in sync.
//
//   node scripts/build-registry.mjs
//
// Re-run this whenever baam.html's cards change.

import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const SITE_BASE = 'https://biorouter.ucsf.edu/';

const html = readFileSync(join(ROOT, 'baam.html'), 'utf8');

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
  // org looks like "BaranziniLab · UCSF · v0.4.3"
  const parts = org.split('·').map((p) => p.trim());
  const version = parts.find((p) => /^v?\d/.test(p)) || '';
  const organization = parts.filter((p) => p !== version).join(' · ');
  return {
    id: slugFromUrl(download),
    name,
    organization,
    version,
    description,
    tags,
    github,
    download: absolutize(download),
    filename: download.split('/').pop(),
    license,
  };
});

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
  version: 1,
  source: 'https://biorouter.ucsf.edu/baam',
  extensions,
  skills,
};

const out = JSON.stringify(registry, null, 2) + '\n';
writeFileSync(join(ROOT, 'registry.json'), out);

console.log(
  `registry.json written: ${extensions.length} extensions, ${skills.length} skills ` +
    `(${skills.filter((s) => s.category === 'Core').length} core, ` +
    `${skills.filter((s) => s.category === 'Developer').length} developer, ` +
    `${skills.filter((s) => s.category === 'Biomedical').length} biomedical)`
);
