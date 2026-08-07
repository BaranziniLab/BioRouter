// Regression gate for #74: the floating titlebar controls (sidebar toggle +
// New Window) must never fall inside an Electron draggable region.
//
// WHY THIS EXISTS AND A UNIT TEST DOES NOT REPLACE IT. jsdom has no layout and
// no concept of `-webkit-app-region`, so a synthetic click on the toggle
// always succeeds — `TitlebarControls.test.tsx` asserted exactly that
// throughout the entire lifetime of the bug. Any jsdom-level test of this
// control is structurally incapable of failing on it. The only honest gates
// are this one (real layout, real computed app-regions, read out of a running
// app) and a real OS-level click, below.
//
// THE RULE BEING CHECKED. Chromium collects `-webkit-app-region` rects by
// walking the layout tree in tree order and appending them to a flat list;
// Electron folds that list IN ORDER, unioning `drag` and subtracting `no-drag`
// (shell/browser/ui/drag_util.cc). So a `drag` rect that appears LATER in the
// DOM re-covers a `no-drag` rect subtracted earlier, regardless of z-index.
// For a single point that reduces to: the LAST rect in the list containing it
// decides. `TitlebarControls` is mounted before `SidebarInset` in AppLayout,
// so every drag rect inside the chat route is "later" and can bury it.
//
// Two measured gotchas worth keeping:
//   * Padding is INSIDE the border box — a reserve applied as padding is still
//     inside the draggable rect. That was the bug. Margins are outside it.
//   * `-webkit-app-region: none` specified EXPLICITLY computes to `no-drag`
//     (it subtracts a rect); only an ABSENT declaration is truly absent. You
//     cannot take a rect out of the fold by writing `none`.
//
// USAGE. Start the dev GUI with a CDP port (see
// docs/desktop-ui/launching-the-dev-gui.md), drive it to a chat route with the
// sidebar COLLAPSED — that is the only state where the rects overlap — then:
//
//   node scripts/titlebar-appregion-check.mjs            # defaults to :9333
//   node scripts/titlebar-appregion-check.mjs 9333 --json
//
// Exit code 0 = controls reachable, 1 = buried (the bug), 2 = harness problem.
//
// CONFIRMING WITH REAL INPUT. This script reads computed layout; it does not
// generate input. To confirm end-to-end, post a real click with the sibling
// CGEventPost driver (CDP's Input.dispatchMouseEvent is injected at the
// renderer and bypasses the window server, so it would report success whether
// or not the OS eats the click — a test that cannot fail):
//
//   swiftc -O scripts/tab-drag-driver.swift -o /tmp/drag
//   /tmp/drag <screenX> <screenY> <screenX> <screenY> 2 1   # from == to = a click
//
// where screenX/Y = window.screenX + the control's client centre, both of
// which this script prints. The sidebar must then actually toggle.

import { chromium } from 'playwright';

const port = process.argv.find((a) => /^\d+$/.test(a)) ?? '9333';
const asJson = process.argv.includes('--json');

let browser;
try {
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
} catch (err) {
  console.error(`could not attach to CDP on :${port} — is the dev GUI running?\n${err.message}`);
  process.exit(2);
}

const page = browser.contexts()[0]?.pages()[0];
if (!page) {
  console.error('no page target on that CDP endpoint');
  await browser.close();
  process.exit(2);
}

const report = await page.evaluate(() => {
  const regions = [];
  for (const el of document.querySelectorAll('*')) {
    const cs = getComputedStyle(el);
    const mode = cs.webkitAppRegion || cs.getPropertyValue('-webkit-app-region');
    if (!mode || mode === 'none') continue;
    if (cs.display === 'none' || cs.visibility === 'hidden') continue;
    const r = el.getBoundingClientRect();
    if (!r.width && !r.height) continue;
    regions.push({
      mode,
      label:
        el.getAttribute('data-testid') ||
        (typeof el.className === 'string' && el.className.trim().split(/\s+/)[0]) ||
        el.tagName.toLowerCase(),
      x: Math.round(r.x),
      y: Math.round(r.y),
      right: Math.round(r.right),
      bottom: Math.round(r.bottom),
    });
  }

  // The fold, evaluated at a point: the last containing rect decides.
  const decide = (px, py) => {
    let winner = null;
    regions.forEach((r, i) => {
      if (px >= r.x && px < r.right && py >= r.y && py < r.bottom) winner = { i, ...r };
    });
    return winner;
  };

  const controls = [
    ['titlebar-sidebar-toggle', document.querySelector('[data-testid="titlebar-sidebar-toggle"]')],
    ['titlebar-new-window', document.querySelector('[data-testid="titlebar-new-window"]')],
  ];

  const checked = [];
  for (const [name, el] of controls) {
    if (!el) continue;
    const r = el.getBoundingClientRect();
    const cx = Math.round(r.x + r.width / 2);
    const cy = Math.round(r.y + r.height / 2);
    const winner = decide(cx, cy);
    checked.push({
      name,
      clientCentre: [cx, cy],
      screenCentre: [window.screenX + cx, window.screenY + cy],
      buried: winner ? winner.mode === 'drag' : false,
      decidedBy: winner
        ? `#${winner.i} ${winner.mode} ${winner.label}`
        : 'nothing (default no-drag)',
    });
  }

  return {
    route: location.hash,
    sidebar: document.querySelector('[data-slot="sidebar"]')?.getAttribute('data-state') ?? null,
    windowOrigin: [window.screenX, window.screenY],
    regions,
    checked,
  };
});

await browser.close();

if (asJson) {
  console.log(JSON.stringify(report, null, 2));
} else {
  console.log(
    `route ${report.route}   sidebar ${report.sidebar}   window origin ${report.windowOrigin.join(',')}`
  );
  console.log('\nordered app-region list (tree order = Electron fold order):');
  report.regions.forEach((r, i) =>
    console.log(
      `  ${String(i).padEnd(3)} ${r.mode.padEnd(8)} ${r.label.slice(0, 28).padEnd(28)} x ${r.x}–${r.right}  y ${r.y}–${r.bottom}`
    )
  );
  console.log('');
  for (const c of report.checked) {
    console.log(
      `  ${c.name.padEnd(24)} centre ${String(c.clientCentre).padEnd(10)} screen ${String(c.screenCentre).padEnd(12)} ` +
        `${c.buried ? 'BURIED IN A DRAG REGION' : 'reachable'}  <- ${c.decidedBy}`
    );
  }
}

if (!report.checked.length) {
  console.error('\nno titlebar controls rendered — drive the app to a route that shows them');
  process.exit(2);
}
if (report.sidebar !== 'collapsed') {
  console.error(
    `\nNOTE: sidebar is "${report.sidebar}", not "collapsed". The rects only overlap when it is ` +
      'collapsed, so this run cannot detect the regression. Collapse it and re-run.'
  );
}
const buried = report.checked.filter((c) => c.buried);
if (buried.length) {
  console.error(`\nFAIL (#74): ${buried.map((c) => c.name).join(', ')} inside a drag region.`);
  process.exit(1);
}
console.log('\nOK: every titlebar control resolves to no-drag.');
