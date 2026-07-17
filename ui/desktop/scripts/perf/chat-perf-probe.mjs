#!/usr/bin/env node
// Chat/preview lag probe — the repeatable perf gate for the tabbed, splittable
// chat area.
//
// WHY THIS EXISTS, AND WHY IT ASSERTS SO LOUDLY
// --------------------------------------------
// An earlier perf probe in this project reported the N-mounted-BaseChat split
// as "affordable" while it was in fact measuring an EMPTY PAGE. Every number
// was real; none of them meant anything, because the thing under test was not
// on screen. A perf number from an unverified DOM is worse than no number: it
// banks a false pass.
//
// So this probe REFUSES to measure until it has proved the app is really there.
// verifyMount() asserts hard minimums (groups mounted, composer present, real
// transcript messages rendered) and throws — loudly, with the observed counts —
// rather than emit a number it cannot stand behind. Every result block carries
// the mount counts it was measured against, so a reader can always tell a fast
// number from a void one.
//
// USAGE (from ui/desktop, with a standalone vite renderer on :5173 —
//        see .claude/skills/debug-app):
//
//   tmux new-session -d -s vite -x 220 -y 50
//   tmux send-keys -t vite 'cd ui/desktop && npx vite --port 5173 --strictPort \
//     --config vite.renderer.config.mts' Enter
//   until curl -sf -o /dev/null http://localhost:5173/; do sleep 1; done
//
//   node scripts/perf/chat-perf-probe.mjs            # 1 group and 4 groups
//   node scripts/perf/chat-perf-probe.mjs --groups 1 # just the 1-group baseline
//   node scripts/perf/chat-perf-probe.mjs --json out.json
//
// THRESHOLDS: see PERF_BUDGET below and scripts/perf/README.md.
import { _electron as electron } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';

// ---------------------------------------------------------------------------
// Machine load guard.
//
// Latency is a measurement of a SHARED machine, not just of the app. This probe
// was first run on a box at load average 93 (cargo builds + other agents), where
// every millisecond reported was contention noise wearing a perf number's
// clothes — the same class of lie as measuring an empty page. A number is only
// meaningful if the machine was quiet enough to produce it, so we check, print
// the load next to every result, and refuse by default when it is too high.
// ---------------------------------------------------------------------------
const CPUS = os.cpus().length;
const MAX_LOAD_PER_CPU = Number(process.env.PERF_MAX_LOAD_PER_CPU || '1.0');

function loadNow() {
  const [one] = os.loadavg();
  return { load1: +one.toFixed(2), cpus: CPUS, perCpu: +(one / CPUS).toFixed(2) };
}

function assertQuietMachine() {
  const l = loadNow();
  if (l.perCpu > MAX_LOAD_PER_CPU) {
    throw new Error(
      `Machine load ${l.load1} over ${l.cpus} cpus = ${l.perCpu}/cpu, above ${MAX_LOAD_PER_CPU}. ` +
        `Timings taken now would measure CONTENTION, not the app. Wait for the box to go quiet ` +
        `(or set PERF_MAX_LOAD_PER_CPU to override, knowing the numbers are noise).`
    );
  }
  return l;
}

// ---------------------------------------------------------------------------
// Budget. These are the numbers the gate enforces. Rationale in README.md.
// ---------------------------------------------------------------------------
// Calibrated against MEASURED baselines on the dev renderer, on a quiet machine
// (load ~1.3/cpu, 8 cpus), 2026-07-17:
//
//   1 group  (355 msgs in DOM):  p50 25.7ms  p95 33.5ms  max 34.8ms  0 long tasks
//   4 groups (1211 msgs in DOM): p50 35.4ms  p95 38.7ms  max 47.4ms  0 long tasks
//   ratio of 4-group to 1-group p95: 1.16x
//
// The absolute budget is deliberately loose because this probe runs against the
// DEV bundle, whose jsxDEV/owner-stack overhead does not exist in the packaged
// app (see README) — a tight absolute number here would fail a healthy app and
// train people to ignore the gate. The two figures that actually carry signal
// are the RATIO (architecture-specific: does mounting N chats cost the focused
// one?) and LONG TASKS (input blocking), both of which are near-immune to the
// dev overhead. Keep the absolutes as a coarse "something is badly wrong" net.
const PERF_BUDGET = {
  // ~1.5x the measured 4-group p95, so ordinary jitter passes and a real
  // regression (at load 93 this same probe read 70ms) fails.
  typingP95Ms: 60,
  typingMaxMs: 120,
  // THE question this branch's architecture raises: mounting 4 chats must not
  // make typing in the focused one materially worse. Measured 1.16x.
  typingP95RatioVs1Group: 1.6,
  // Long tasks (>50ms) block input. Measured 0 in both scenarios on a quiet
  // machine, so anything above a couple is a real finding.
  longTasksDuringTyping: 2,
};

const args = process.argv.slice(2);
const argOf = (flag, dflt) => {
  const i = args.indexOf(flag);
  return i >= 0 && args[i + 1] ? args[i + 1] : dflt;
};
const jsonOut = argOf('--json', null);
const onlyGroups = argOf('--groups', null);
const KEYSTROKES = Number(argOf('--keystrokes', '40'));

const APP_DIR = process.cwd();
const MAIN_JS = path.join(APP_DIR, '.vite', 'build', 'main.js');

// ---------------------------------------------------------------------------
// Real sessions. A probe against an empty/unknown session measures an empty
// state, which is exactly the void result this file exists to prevent. We read
// the REAL session db and pick the longest transcripts we can find.
// ---------------------------------------------------------------------------
const SESSIONS_DB = path.join(os.homedir(), '.local/share/biorouter/sessions/sessions.db');

function pickRealSessions(count) {
  if (!fs.existsSync(SESSIONS_DB)) {
    throw new Error(
      `No session db at ${SESSIONS_DB}. This probe measures real transcripts; ` +
        `it will not fabricate an empty one. Run the app once to create sessions.`
    );
  }
  // Copy first: the live db is WAL and may be locked by a running biorouterd.
  const copy = path.join(os.tmpdir(), `br-perf-sessions-${process.pid}.db`);
  fs.copyFileSync(SESSIONS_DB, copy);
  const sql =
    'SELECT s.id, COUNT(m.id) AS n FROM sessions s JOIN messages m ON m.session_id=s.id ' +
    `GROUP BY s.id ORDER BY n DESC LIMIT ${count};`;
  const out = execFileSync('sqlite3', [copy, sql], { encoding: 'utf8' });
  fs.rmSync(copy, { force: true });
  const rows = out
    .trim()
    .split('\n')
    .filter(Boolean)
    .map((line) => {
      const [id, n] = line.split('|');
      return { id, messages: Number(n) };
    });
  if (rows.length < count) {
    throw new Error(
      `Need ${count} sessions with messages, found ${rows.length}. Probe would be measuring empty chats.`
    );
  }
  return rows;
}

// ---------------------------------------------------------------------------
// Layout seeding. The chat-groups state persists to localStorage keyed by a
// per-window id in sessionStorage (chatGroupsStorage.ts). Seeding both before
// first paint gives a deterministic N-group mount without driving a fragile
// tab-drag gesture.
// ---------------------------------------------------------------------------
const WINDOW_ID = 'perfprobe';
const STORAGE_KEY = `biorouter.chatgroups.v1:${WINDOW_ID}`;

function buildState(sessions) {
  const n = sessions.length;
  const groups = {};
  sessions.forEach((s, i) => {
    const gid = `grp-${i + 1}`;
    groups[gid] = {
      groupId: gid,
      tabs: [
        {
          tabId: `tab-${i + 1}`,
          sessionId: s.id,
          title: `perf-${i + 1}`,
          userSetName: true,
        },
      ],
      activeTabId: `tab-${i + 1}`,
    };
  });
  const leaf = (i) => ({ kind: 'leaf', groupId: `grp-${i}` });
  let layout;
  if (n === 1) layout = leaf(1);
  else if (n === 2) layout = { kind: 'branch', dir: 'row', children: [leaf(1), leaf(2)], sizes: [0.5, 0.5] };
  else
    // 2x2: a row of two columns. Exercises depth-2 tree rendering, which a flat
    // row would not.
    layout = {
      kind: 'branch',
      dir: 'row',
      children: [
        { kind: 'branch', dir: 'col', children: [leaf(1), leaf(2)], sizes: [0.5, 0.5] },
        { kind: 'branch', dir: 'col', children: [leaf(3), leaf(4)], sizes: [0.5, 0.5] },
      ],
      sizes: [0.5, 0.5],
    };
  return { version: 1, layout, groups, activeGroupId: 'grp-1', seq: n + 1 };
}

// ---------------------------------------------------------------------------
// In-page instrumentation, installed before any app JS runs.
// ---------------------------------------------------------------------------
function initScript(storageKey, windowId, stateJson) {
  return `
    try {
      sessionStorage.setItem('biorouter.chatgroups.windowId', ${JSON.stringify(windowId)});
      localStorage.setItem(${JSON.stringify(storageKey)}, ${JSON.stringify(stateJson)});
    } catch (e) {}

    // --- long tasks -------------------------------------------------------
    window.__perfLongTasks = [];
    try {
      new PerformanceObserver((l) => {
        for (const e of l.getEntries()) {
          window.__perfLongTasks.push({ start: e.startTime, duration: e.duration });
        }
      }).observe({ type: 'longtask', buffered: true });
    } catch (e) {}

    // --- React commit counting -------------------------------------------
    // Installing a DevTools hook stub BEFORE react-dom loads makes React mark
    // roots with ProfileMode, which is what populates fiber.actualDuration. We
    // then walk each commit and record which components actually rendered.
    // This is how we answer "does a keystroke in one pane re-render the others".
    window.__perfCommits = [];
    window.__perfRecording = false;
    (function () {
      let nextId = 1;
      const renderers = new Map();
      function walk(root) {
        const rendered = [];
        const seen = new Set();
        (function visit(fiber) {
          if (!fiber || seen.has(fiber)) return;
          seen.add(fiber);
          // actualDuration > 0 means this fiber did work in THIS commit.
          if (fiber.actualDuration > 0) {
            let name = fiber.type;
            if (typeof name === 'function') name = name.displayName || name.name;
            else if (typeof name === 'object' && name)
              name = name.displayName || (name.render && (name.render.displayName || name.render.name));
            if (typeof name === 'string' && name) {
              rendered.push({ name, dur: fiber.actualDuration });
            }
          }
          visit(fiber.child);
          visit(fiber.sibling);
        })(root.current);
        return rendered;
      }
      window.__REACT_DEVTOOLS_GLOBAL_HOOK__ = {
        renderers,
        supportsFiber: true,
        isDisabled: false,
        checkDCE() {},
        inject(r) { const id = nextId++; renderers.set(id, r); return id; },
        onCommitFiberUnmount() {},
        onPostCommitFiberRoot() {},
        onCommitFiberRoot(id, root) {
          if (!window.__perfRecording) return;
          try {
            window.__perfCommits.push({ t: performance.now(), rendered: walk(root) });
          } catch (e) {}
        },
      };
    })();
  `;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * The app hash-navigates several times during startup (it resolves which
 * session to resume), and each navigation destroys the page's JS execution
 * context. Evaluating mid-flap throws "Execution context was destroyed" — or,
 * worse, could read a half-built DOM. Wait for the URL to hold still before
 * trusting anything we read out of the page.
 */
async function settleNavigation(page, quietMs = 2500, timeoutMs = 45_000) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  let stableSince = Date.now();
  while (Date.now() < deadline) {
    let url;
    try {
      url = page.url();
    } catch {
      url = null;
    }
    if (url !== last) {
      last = url;
      stableSince = Date.now();
    } else if (Date.now() - stableSince >= quietMs) {
      return url;
    }
    await sleep(250);
  }
  return last;
}

/** Retry an evaluate across a navigation that lands mid-call. */
async function evalRetry(page, fn, arg, tries = 5) {
  let lastErr;
  for (let i = 0; i < tries; i++) {
    try {
      return await page.evaluate(fn, arg);
    } catch (e) {
      lastErr = e;
      if (!/Execution context was destroyed|Target closed|navigation/i.test(e.message)) throw e;
      await settleNavigation(page, 1500, 15_000);
    }
  }
  throw lastErr;
}

// ---------------------------------------------------------------------------
// The verification gate. Nothing downstream of here is trusted unless this
// passes, and the counts it returns are printed alongside every number.
// ---------------------------------------------------------------------------
async function verifyMount(page, expectedGroups) {
  const counts = await evalRetry(page, () => {
    const q = (s) => document.querySelectorAll(s).length;
    const groups = document.querySelectorAll('[data-chat-group-id]');
    const perGroup = [...groups].map((g) => ({
      id: g.getAttribute('data-chat-group-id'),
      // A group that is mounted but zero-sized is just as void as one that is
      // absent, so measure the box too.
      w: Math.round(g.getBoundingClientRect().width),
      h: Math.round(g.getBoundingClientRect().height),
      textareas: g.querySelectorAll('textarea').length,
    }));
    return {
      rootChildren: document.getElementById('root')?.children.length ?? 0,
      groups: groups.length,
      perGroup,
      tabs: q('.br-tab'),
      textareas: q('textarea'),
      // The transcript. These are the message wrappers BaseChat renders.
      messages: q('[data-testid="message-container"]'),
      // Any visible text at all — the crudest possible "is this an empty page"
      // canary, and the one the earlier void result would have tripped.
      bodyTextLen: (document.body.innerText || '').trim().length,
    };
  });

  const problems = [];
  if (counts.rootChildren === 0) problems.push('#root is EMPTY — app never mounted');
  if (counts.groups !== expectedGroups)
    problems.push(`expected ${expectedGroups} chat group(s), found ${counts.groups}`);
  if (counts.textareas < expectedGroups)
    problems.push(`expected >=${expectedGroups} composer(s), found ${counts.textareas}`);
  if (counts.messages === 0)
    problems.push('ZERO transcript messages rendered — this is the empty-page void result');
  if (counts.bodyTextLen < 200)
    problems.push(`body text is only ${counts.bodyTextLen} chars — page looks blank`);
  for (const g of counts.perGroup) {
    if (g.w < 50 || g.h < 50)
      problems.push(`group ${g.id} is ${g.w}x${g.h} — mounted but not visible`);
  }

  if (problems.length) {
    console.error('\n*** MOUNT VERIFICATION FAILED — refusing to report numbers ***');
    for (const p of problems) console.error('  - ' + p);
    console.error('  observed: ' + JSON.stringify(counts, null, 2));
    throw new Error('verifyMount failed: ' + problems.join('; '));
  }
  return counts;
}

const pct = (arr, p) => {
  if (!arr.length) return null;
  const s = [...arr].sort((a, b) => a - b);
  return +s[Math.min(s.length - 1, Math.floor((p / 100) * s.length))].toFixed(2);
};
const summarize = (arr) => ({
  n: arr.length,
  p50: pct(arr, 50),
  p95: pct(arr, 95),
  max: arr.length ? +Math.max(...arr).toFixed(2) : null,
  mean: arr.length ? +(arr.reduce((a, b) => a + b, 0) / arr.length).toFixed(2) : null,
});

// ---------------------------------------------------------------------------
// Typing latency: keydown -> painted frame, per keystroke.
//
// Double-rAF is the measurement: the first rAF callback runs before the frame
// React's update will paint in; the second runs after that frame has been
// painted. The delta from the keydown timestamp is therefore keystroke-to-paint.
// ---------------------------------------------------------------------------
async function measureTyping(page, label) {
  const ta = await page.$('[data-active-group="true"] textarea');
  if (!ta) throw new Error(`${label}: no textarea in the ACTIVE group — cannot measure typing`);
  await ta.click();
  await sleep(250);

  await page.evaluate(() => {
    window.__typing = [];
    const el = document.querySelector('[data-active-group="true"] textarea');
    if (!el) throw new Error('textarea vanished');
    window.__typingEl = el;
    window.__typingHandler = () => {
      const t0 = performance.now();
      requestAnimationFrame(() =>
        requestAnimationFrame(() => window.__typing.push(performance.now() - t0))
      );
    };
    el.addEventListener('keydown', window.__typingHandler);
    window.__perfLongTasks.length = 0;
    window.__perfCommits.length = 0;
    window.__perfRecording = true;
  });

  // Real, trusted key events through CDP — not synthetic dispatch, which would
  // bypass the very browser work we are trying to time.
  const text = 'the quick brown fox jumps over the lazy dog again '.slice(0, KEYSTROKES);
  await page.keyboard.type(text, { delay: 55 });
  await sleep(400);

  const res = await page.evaluate(() => {
    window.__perfRecording = false;
    window.__typingEl?.removeEventListener('keydown', window.__typingHandler);
    const commits = window.__perfCommits;
    const tally = {};
    for (const c of commits)
      for (const r of c.rendered) tally[r.name] = (tally[r.name] || 0) + 1;
    return {
      samples: window.__typing,
      typedLen: document.querySelector('[data-active-group="true"] textarea')?.value.length ?? 0,
      longTasks: window.__perfLongTasks.map((t) => +t.duration.toFixed(1)),
      commits: commits.length,
      topRendered: Object.entries(tally)
        .sort((a, b) => b[1] - a[1])
        .slice(0, 14)
        .map(([name, n]) => `${name} x${n}`),
    };
  });

  // Prove the keystrokes actually landed. A perfect latency number over zero
  // typed characters is another flavour of the void result.
  if (res.typedLen < text.length * 0.8) {
    throw new Error(
      `${label}: typed ${text.length} chars but textarea holds ${res.typedLen} — input did not land, numbers void`
    );
  }
  if (res.samples.length < text.length * 0.8) {
    throw new Error(
      `${label}: only ${res.samples.length}/${text.length} keystrokes produced a paint sample — numbers void`
    );
  }

  // Clear the composer so a later scenario starts clean and nothing is sent.
  await page.keyboard.press('Meta+A');
  await page.keyboard.press('Backspace');

  return {
    latency: summarize(res.samples),
    typedChars: res.typedLen,
    longTasks: res.longTasks.filter((d) => d > 50),
    reactCommits: res.commits,
    topRendered: res.topRendered,
  };
}

// ---------------------------------------------------------------------------
// Transcript scroll + virtualization check.
// ---------------------------------------------------------------------------
async function measureScroll(page) {
  const mounted = await page.evaluate(() => {
    const scroller =
      document.querySelector('[data-active-group="true"] [data-testid="scroll-area"]') ||
      document.querySelector('[data-active-group="true"] .overflow-y-auto');
    return {
      messagesInDom: document.querySelectorAll(
        '[data-active-group="true"] [data-testid="message-container"]'
      ).length,
      scrollerFound: !!scroller,
      scrollHeight: scroller?.scrollHeight ?? 0,
    };
  });

  await page.evaluate(() => {
    window.__perfLongTasks.length = 0;
    window.__frames = [];
    let last = performance.now();
    window.__rafOn = true;
    const tick = () => {
      const now = performance.now();
      window.__frames.push(now - last);
      last = now;
      if (window.__rafOn) requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });

  const box = await page.evaluate(() => {
    const g = document.querySelector('[data-active-group="true"]');
    const r = g.getBoundingClientRect();
    return { x: Math.round(r.x + r.width / 2), y: Math.round(r.y + r.height / 2) };
  });
  await page.mouse.move(box.x, box.y);
  for (let i = 0; i < 25; i++) {
    await page.mouse.wheel(0, -400);
    await sleep(32);
  }
  await sleep(300);

  const res = await page.evaluate(() => {
    window.__rafOn = false;
    const frames = window.__frames.slice(2);
    return {
      frames,
      longTasks: window.__perfLongTasks.map((t) => +t.duration.toFixed(1)).filter((d) => d > 50),
    };
  });
  const dropped = res.frames.filter((f) => f > 32).length;
  return {
    ...mounted,
    frameCount: res.frames.length,
    droppedFrames: dropped,
    worstFrameMs: res.frames.length ? +Math.max(...res.frames).toFixed(1) : null,
    longTasks: res.longTasks,
  };
}

// ---------------------------------------------------------------------------
async function runScenario(nGroups, sessions) {
  const state = buildState(sessions.slice(0, nGroups));
  const env = {
    ...process.env,
    ELECTRON_IS_DEV: '1',
    NODE_ENV: 'development',
    BIOROUTER_ALLOWLIST_BYPASS: 'true',
  };
  delete env.ELECTRON_RUN_AS_NODE; // else Playwright drives plain Node, not Electron
  delete env.NODE_OPTIONS;
  delete env.VSCODE_INSPECTOR_OPTIONS;

  // Isolated userData so the single-instance lock can't collide with a dev app.
  const shim = path.join(os.tmpdir(), `br-perf-main-${nGroups}.cjs`);
  fs.writeFileSync(
    shim,
    `const { app } = require('electron');
const path = require('node:path');
const os = require('node:os');
app.setPath('userData', path.join(os.tmpdir(), 'br-perf-userdata-${nGroups}'));
require(${JSON.stringify(MAIN_JS)});`
  );

  const app = await electron.launch({ args: [shim], cwd: APP_DIR, env, timeout: 90_000 });
  try {
    const page = await app.firstWindow();

    // Single-instance-lock canary. The lock is keyed by userData path and we set
    // an isolated one above, so a concurrent dev/agent instance must NOT be able
    // to no-op us — but if that assumption ever breaks, the symptom is a dead or
    // window-less app, and measuring THAT is how a void result gets reported as
    // a real number. Fail loudly instead.
    if (app.windows().length === 0) {
      throw new Error(
        'Electron launched with ZERO windows — almost certainly the single-instance lock ' +
          '(another instance owns this userData dir). Numbers would be void; refusing to measure.'
      );
    }

    await page.addInitScript(initScript(STORAGE_KEY, WINDOW_ID, JSON.stringify(state)));
    // The init script must run before the app's JS. firstWindow() may already
    // have navigated, so reload to guarantee ordering.
    await page.reload({ waitUntil: 'domcontentloaded' }).catch(() => {});
    // The app hash-navigates while it resolves which session to resume; every
    // one of those destroys the execution context. Settle before asserting.
    await settleNavigation(page);
    await page
      .waitForFunction(() => document.getElementById('root')?.children.length > 0, {
        timeout: 40_000,
      })
      .catch(() => {});

    // ChatGroupsProvider is mounted ONLY inside the /pair route (App.tsx), so
    // the seeded layout is not even read anywhere else — on '/' you get the Hub
    // and zero groups. Drive to /pair explicitly; the provider then loads the
    // seed and its OUT effect mirrors the active session back into the URL.
    await evalRetry(page, () => {
      if (!location.hash.startsWith('#/pair')) location.hash = '#/pair';
    });
    await settleNavigation(page, 2000, 25_000);
    // Let the transcripts fetch and lay out.
    await page
      .waitForFunction(
        (n) => document.querySelectorAll('[data-chat-group-id]').length >= n,
        nGroups,
        { timeout: 40_000 }
      )
      .catch(() => {});
    await page
      .waitForFunction(
        () => document.querySelectorAll('[data-testid="message-container"]').length > 0,
        { timeout: 40_000 }
      )
      .catch(() => {});
    await settleNavigation(page, 2000, 20_000);
    await sleep(2500);

    const mount = await verifyMount(page, nGroups);
    console.log(`\n### ${nGroups} group(s) — MOUNT VERIFIED`);
    console.log(
      `    groups=${mount.groups} tabs=${mount.tabs} composers=${mount.textareas} ` +
        `messagesInDom=${mount.messages} bodyText=${mount.bodyTextLen}ch`
    );
    console.log(`    boxes: ${mount.perGroup.map((g) => `${g.id} ${g.w}x${g.h}`).join(', ')}`);

    const typing = await measureTyping(page, `${nGroups}-group`);
    console.log(`    typing keydown->paint: ${JSON.stringify(typing.latency)}`);
    console.log(`    typedChars=${typing.typedChars} longTasks(>50ms)=${JSON.stringify(typing.longTasks)}`);
    console.log(`    reactCommits=${typing.reactCommits}`);
    console.log(`    rendered per keystroke burst: ${typing.topRendered.join(', ')}`);

    const scroll = await measureScroll(page);
    console.log(`    scroll: ${JSON.stringify(scroll)}`);

    const loadAfter = loadNow();
    console.log(`    machine load during run: ${loadAfter.load1} (${loadAfter.perCpu}/cpu)`);

    await page.screenshot({ path: `/tmp/br-shots/perf-${nGroups}g.png` }).catch(() => {});

    return { groups: nGroups, mount, typing, scroll, load: loadAfter };
  } finally {
    await app.close().catch(() => {});
  }
}

// ---------------------------------------------------------------------------
async function main() {
  fs.mkdirSync('/tmp/br-shots', { recursive: true });

  // Reap backends orphaned by earlier probe runs. Playwright's app.close()
  // SIGKILLs Electron, so main.ts's own cleanup never runs and the biorouterd it
  // spawned is reparented to init and keeps holding its port — after a few runs
  // new launches starve and never open a window. Only ever kill OUR orphans:
  // a backend with a live parent belongs to somebody else's app.
  try {
    const pids = execFileSync('pgrep', ['-f', 'ui/desktop/src/bin/biorouterd'], { encoding: 'utf8' })
      .trim().split('\n').filter(Boolean);
    for (const pid of pids) {
      const ppid = execFileSync('ps', ['-p', pid, '-o', 'ppid='], { encoding: 'utf8' }).trim();
      if (ppid === '1') {
        try { process.kill(Number(pid), 9); console.log(`reaped orphaned biorouterd ${pid}`); } catch {}
      }
    }
  } catch {
    // pgrep exits non-zero when nothing matches — that is the good case.
  }

  const load = assertQuietMachine();
  console.log(`machine: load1=${load.load1} over ${load.cpus} cpus (${load.perCpu}/cpu)`);

  const scenarios = onlyGroups ? [Number(onlyGroups)] : [1, 4];
  const sessions = pickRealSessions(4);
  console.log(
    'Real sessions under test: ' + sessions.map((s) => `${s.id}(${s.messages} msgs)`).join(', ')
  );

  const results = [];
  for (const n of scenarios) results.push(await runScenario(n, sessions));

  console.log('\n================ VERDICT ================');
  const fails = [];
  for (const r of results) {
    const p95 = r.typing.latency.p95;
    const mx = r.typing.latency.max;
    if (p95 > PERF_BUDGET.typingP95Ms)
      fails.push(`${r.groups}g typing p95 ${p95}ms > ${PERF_BUDGET.typingP95Ms}ms`);
    if (mx > PERF_BUDGET.typingMaxMs)
      fails.push(`${r.groups}g typing max ${mx}ms > ${PERF_BUDGET.typingMaxMs}ms`);
    if (r.typing.longTasks.length > PERF_BUDGET.longTasksDuringTyping)
      fails.push(
        `${r.groups}g ${r.typing.longTasks.length} long tasks while typing > ${PERF_BUDGET.longTasksDuringTyping}`
      );
    console.log(
      `${r.groups} group(s): typing p50=${r.typing.latency.p50}ms p95=${p95}ms max=${mx}ms | ` +
        `msgsInDom=${r.mount.messages} | commits=${r.typing.reactCommits} | ` +
        `scrollDropped=${r.scroll.droppedFrames}/${r.scroll.frameCount}`
    );
  }
  const one = results.find((r) => r.groups === 1);
  const four = results.find((r) => r.groups === 4);
  if (one && four) {
    const ratio = +(four.typing.latency.p95 / Math.max(one.typing.latency.p95, 0.01)).toFixed(2);
    console.log(`4-group vs 1-group typing p95 ratio: ${ratio}x (budget ${PERF_BUDGET.typingP95RatioVs1Group}x)`);
    if (ratio > PERF_BUDGET.typingP95RatioVs1Group)
      fails.push(`4g/1g typing p95 ratio ${ratio}x > ${PERF_BUDGET.typingP95RatioVs1Group}x`);
  }

  if (jsonOut) fs.writeFileSync(jsonOut, JSON.stringify({ budget: PERF_BUDGET, results }, null, 2));

  if (fails.length) {
    console.log('\nFAIL:');
    for (const f of fails) console.log('  - ' + f);
    process.exitCode = 1;
  } else {
    console.log('\nPASS — all within budget.');
  }
}

main().catch((e) => {
  console.error('\nPROBE ERROR:', e.message);
  process.exitCode = 1;
});
