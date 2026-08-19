/**
 * Measures how long the Node event loop is stalled by the OLD (spawnSync) and
 * NEW (execFile) dependency probes. In the Electron main process this loop is
 * the one driving the window, so a stall here is a frozen UI, one for one.
 *
 * The monitor records the TIMESTAMP of every tick and derives the largest gap
 * afterwards. Measuring lag inside the callback cannot work here: during a
 * spawnSync the callback never runs at all, so there is nothing to measure from.
 */
import { spawnSync, execFile } from 'child_process';
import { promisify } from 'util';
const execFileAsync = promisify(execFile);

const CLI = process.argv[2] || '/Applications/Biorouter.app/Contents/Resources/bin/biorouter';
const ARGS = ['doctor', '--format', 'json', '--no-update'];
const FRAME = 16; // one 60fps frame

async function measure(label, run) {
  await new Promise((r) => setTimeout(r, 300)); // let the loop settle

  const ticks = [];
  const timer = setInterval(() => ticks.push(Date.now()), FRAME);

  await new Promise((r) => setTimeout(r, 100)); // a few ticks before we start
  const t0 = Date.now();
  await run();
  const t1 = Date.now();
  await new Promise((r) => setTimeout(r, 100)); // and a few after
  clearInterval(timer);

  const during = ticks.filter((t) => t >= t0 && t <= t1);
  let maxGap = 0;
  const spanning = [t0, ...during, t1];
  for (let i = 1; i < spanning.length; i++) {
    maxGap = Math.max(maxGap, spanning[i] - spanning[i - 1]);
  }
  const wall = t1 - t0;
  const expected = Math.max(1, Math.floor(wall / FRAME));

  console.log(
    `${label.padEnd(26)} wall ${String(wall).padStart(5)}ms | ` +
      `frames rendered ${String(during.length).padStart(3)}/${String(expected).padStart(3)} | ` +
      `longest freeze ${String(maxGap).padStart(5)}ms`
  );
  return { wall, frames: during.length, expected, maxGap };
}

console.log(`probe: ${CLI} ${ARGS.join(' ')}`);
console.log('("frames" = 60fps ticks the event loop actually serviced during the probe)');
console.log('');

const before = await measure('BEFORE  spawnSync', async () => {
  spawnSync(CLI, ARGS, { encoding: 'utf8', timeout: 15000 });
});
const after = await measure('AFTER   execFile async', async () => {
  await execFileAsync(CLI, ARGS, {
    encoding: 'utf8',
    timeout: 15000,
    maxBuffer: 8 * 1024 * 1024,
  }).catch(() => {});
});

// A cold start (binary not in page cache), a slow disk, or a probe for a tool
// that is missing and times out, all take seconds rather than milliseconds. The
// warm numbers above are the BEST case; this is what those look like.
console.log('');
const slowBefore = await measure('BEFORE  slow probe (3s)', async () => {
  spawnSync('sh', ['-c', 'sleep 3'], { encoding: 'utf8', timeout: 15000 });
});
const slowAfter = await measure('AFTER   slow probe (3s)', async () => {
  await execFileAsync('sh', ['-c', 'sleep 3'], { encoding: 'utf8', timeout: 15000 }).catch(() => {});
});

console.log('');
console.log('What the user sees while the dependency check runs:');
console.log(
  `  BEFORE: ${before.frames} of ${before.expected} frames — window frozen for ${(before.maxGap / 1000).toFixed(2)}s`
);
console.log(`  AFTER:  ${after.frames} of ${after.expected} frames — longest hitch ${after.maxGap}ms`);
console.log('');
console.log('Same check on a cold or slow machine (3s probe):');
console.log(
  `  BEFORE: ${slowBefore.frames} of ${slowBefore.expected} frames — window frozen for ${(slowBefore.maxGap / 1000).toFixed(2)}s`
);
console.log(
  `  AFTER:  ${slowAfter.frames} of ${slowAfter.expected} frames — longest hitch ${slowAfter.maxGap}ms`
);
