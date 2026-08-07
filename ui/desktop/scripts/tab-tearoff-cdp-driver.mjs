// Phase 0 driver: instrument both BioRouter windows over CDP, then let a REAL
// OS-level drag (posted by the compiled Swift `drag` binary, not by CDP) run
// between them, and read back what each window saw.
//
// The split matters: CDP is used only to INSTRUMENT and to READ. It never
// generates the input under test. If it did, the measurement would be circular.
import { chromium } from 'playwright';
import { readFileSync } from 'node:fs';

const HARNESS = readFileSync(process.argv[2], 'utf8');
const cmd = process.argv[3] ?? 'setup';

const browser = await chromium.connectOverCDP('http://127.0.0.1:9333');
const ctx = browser.contexts()[0];
const pages = ctx.pages();

const info = [];
for (const p of pages) {
  const d = await p.evaluate(() => ({
    hash: location.hash,
    sx: window.screenX, sy: window.screenY,
    iw: window.innerWidth, ih: window.innerHeight,
    tabs: Array.from(document.querySelectorAll('[data-tab-id]')).map((el) => {
      const r = el.getBoundingClientRect();
      return { id: el.getAttribute('data-tab-id'), x: r.x, y: r.y, w: r.width, h: r.height,
               label: (el.textContent || '').trim().slice(0, 24) };
    }),
    strip: (() => { const s = document.querySelector('.br-tabstrip');
      if (!s) return null; const r = s.getBoundingClientRect();
      return { x: r.x, y: r.y, w: r.width, h: r.height }; })(),
    role: window.__tearoff?.role ?? null,
  }));
  info.push(d);
}

if (cmd === 'setup') {
  // Inject into every window. Each claims A or B via localStorage on first run.
  for (const p of pages) await p.evaluate(HARNESS);
  const after = [];
  for (const p of pages) after.push(await p.evaluate(() => window.__tearoff.role));
  console.log(JSON.stringify({ roles: after, windows: info }, null, 2));
} else if (cmd === 'report') {
  const out = [];
  for (const p of pages) {
    out.push(await p.evaluate(() => {
      const r = window.__tearoff;
      if (!r) return { role: null, error: 'harness missing' };
      const log = r.log;
      const downs = log.filter((x) => x.type === 'pointerdown');
      const moves = log.filter((x) => x.type === 'pointermove' && x.buttons > 0);
      const ups = log.filter((x) => x.type === 'pointerup');
      return {
        role: r.role,
        total: log.length,
        downs: downs.length,
        moves: moves.length,
        movesOutside: moves.filter((x) => !x.inside).length,
        ups: ups.length,
        upsOutside: ups.filter((x) => !x.inside).length,
        lostCapture: log.filter((x) => x.type === 'lostpointercapture').length,
        cancels: log.filter((x) => x.type === 'pointercancel').length,
        firstDownTarget: downs[0]?.target ?? null,
        // The extreme screen coords prove how far outside the frame events kept coming.
        moveScreenXRange: moves.length ? [Math.min(...moves.map((m) => m.screenX)), Math.max(...moves.map((m) => m.screenX))] : null,
        lastUp: ups.length ? { screenX: ups.at(-1).screenX, screenY: ups.at(-1).screenY, inside: ups.at(-1).inside } : null,
      };
    }));
  }
  console.log(JSON.stringify(out, null, 2));
} else if (cmd === 'reset') {
  for (const p of pages) await p.evaluate(() => window.__tearoff?.reset());
  console.log('reset');
} else if (cmd === 'where') {
  console.log(JSON.stringify(info, null, 2));
}

await browser.close();
