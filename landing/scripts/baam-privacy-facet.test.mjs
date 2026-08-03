// Browser-level regression suite for the BAAM shelf's privacy badges + facet.
//
//     node --test landing/scripts/baam-privacy-facet.test.mjs
//
// Why a real browser and not jsdom. Two of the three things this file asserts
// are LAYOUT facts, and jsdom has no layout — every getBoundingClientRect() it
// returns is zeros, so a clipped tag row and a perfect one are indistinguishable
// there. `.ext-tags` is `max-height: 22px; overflow: hidden`, which means an
// extra chip does not push the row taller, it silently deletes the last tag from
// view. That failure is invisible in a diff and invisible in a screenshot of any
// card that happened to have two tags. Only a browser that lays the row out can
// see it.
//
// Playwright is not a landing/ dependency — it is resolved out of
// ui/desktop/node_modules, and the browser is whatever this machine already has
// (headless shell, the bundled chromium, or a system Chrome). If none launches
// the tests FAIL rather than skip: a silent skip is how a layout gate becomes a
// decoration. That is also why this file is deliberately NOT wired into
// `just check-registry` / the deploy workflow, both of which name their scripts
// explicitly and run on a bare checkout with no npm install.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createReadStream, existsSync, statSync } from 'node:fs';
import { dirname, extname, join, normalize, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPTS = dirname(fileURLToPath(import.meta.url));
const LANDING = resolve(SCRIPTS, '..');
const REPO = resolve(LANDING, '..');
const PLAYWRIGHT = join(REPO, 'ui/desktop/node_modules/playwright/index.mjs');

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
};

/** Serve landing/ over loopback so `fetch('registry.json')` resolves. */
function serveLanding() {
  const server = createServer((req, res) => {
    const path = decodeURIComponent((req.url || '/').split('?')[0]);
    const file = join(LANDING, normalize(path).replace(/^(\.\.[/\\])+/, ''));
    if (!file.startsWith(LANDING) || !existsSync(file) || !statSync(file).isFile()) {
      res.writeHead(404).end('not found');
      return;
    }
    res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' });
    createReadStream(file).pipe(res);
  });
  return new Promise((ok) => server.listen(0, '127.0.0.1', () => ok(server)));
}

/**
 * Whatever chromium this machine can actually start. Playwright's default
 * headless mode wants a `chrome-headless-shell` download that a repo which only
 * uses Playwright for Electron E2E may never have fetched.
 */
async function launchChromium(chromium) {
  const attempts = [{}, { channel: 'chromium' }, { channel: 'chrome' }];
  const failures = [];
  for (const opts of attempts) {
    try {
      return await chromium.launch(opts);
    } catch (err) {
      failures.push(`${JSON.stringify(opts)}: ${String(err).split('\n')[0]}`);
    }
  }
  throw new Error(
    'no chromium could be launched, so the layout assertions in this file cannot run:\n  ' +
      failures.join('\n  ') +
      '\nInstall one with: cd ui/desktop && npx playwright install chromium'
  );
}

if (!existsSync(PLAYWRIGHT)) {
  test('playwright is available', () => {
    assert.fail(
      `${PLAYWRIGHT} is missing — run \`cd ui/desktop && npm install\`. ` +
        'These are layout assertions; jsdom cannot stand in for them.'
    );
  });
} else {
  const { chromium } = await import(PLAYWRIGHT);
  const server = await serveLanding();
  const BASE = `http://127.0.0.1:${server.address().port}`;
  const browser = await launchChromium(chromium);

  test.after(async () => {
    await browser.close();
    server.close();
  });

  /** A page with the registry-rendered shelf already in the DOM. */
  async function shelfPage() {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    await page.goto(`${BASE}/baam.html`);
    // renderExtensions() empties the static grid once the registry lands.
    await page.waitForSelector('#ext-featured .ext-card');
    return page;
  }

  test('the privacy facet filters the shelf down to the two private extensions', async () => {
    const page = await shelfPage();
    await page.click('#ext-chips .fchip[data-facet="privacy"][data-match="private"]');
    const visible = await page.$$eval('#extensions-section .ext-card:visible', (els) =>
      els.map((e) => e.dataset.extensionName)
    );
    assert.deepEqual(visible.sort(), ['cdwagent', 'ucsfomopagent']);
    await page.close();
  });

  test('a card with three real tags still shows all three', async () => {
    const page = await shelfPage();
    const chips = await page.$$eval(
      '.ext-card[data-extension-name="cdwagent"] .ext-tags > span',
      (els) => els.map((e) => e.textContent)
    );
    assert.equal(chips[0], 'Private');
    assert.ok(chips.length <= 3, `expected at most 3 chips, got ${chips.length}: ${chips}`);

    // The clip test: every laid-out chip must sit inside the 22px row. A chip
    // that wrapped is not "a bit cramped", it is invisible — and the row's
    // `overflow: hidden` means no scrollbar, no ellipsis, nothing to notice.
    const clipped = await page.$$eval('#extensions-section .ext-card .ext-tags > span', (els) =>
      els
        .filter((e) => e.getBoundingClientRect().height > 0)
        .filter((e) => e.getBoundingClientRect().bottom > e.parentElement.getBoundingClientRect().bottom + 0.5)
        .map((e) => `${e.closest('.ext-card').querySelector('h3').textContent}: ${e.textContent}`)
    );
    assert.deepEqual(clipped, [], 'these chips fell outside the clipped .ext-tags row');

    // …and the row is kept inside its 22px by dropping OVERFLOW, never the
    // badge. Without this the previous assertion is satisfiable by hiding
    // everything, privacy label included.
    const badgeless = await page.$$eval('#extensions-section .ext-card:visible', (els) =>
      els
        .map((e) => e.querySelector('.ext-tags > span'))
        .filter((s) => !s || s.getBoundingClientRect().height === 0)
        .map((s) => (s ? s.textContent : '<no chips at all>'))
    );
    assert.deepEqual(badgeless, [], 'the privacy badge must survive the tag-row trim');
    await page.close();
  });

  test('the no-JS view is badged too', async () => {
    const ctx = await browser.newContext({ javaScriptEnabled: false });
    const page = await ctx.newPage();
    await page.goto(`${BASE}/baam.html`);
    assert.equal(await page.locator('.ext-card[data-privacy="private"] .tag.private').count(), 2);
    await ctx.close();
  });
}
