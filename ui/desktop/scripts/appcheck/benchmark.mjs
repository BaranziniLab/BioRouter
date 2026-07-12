// Benchmark v2 (apps SDK v2, plan Phase 6). Scores every BioRouter app in the
// local store on the v2 axes, reading ONLY the RAW store files
// (manifest.json + index.html + src/main.ts) — never served/rendered HTML.
// This is the same honesty rule the v1 variety benchmark used: we score what the
// author agent actually wrote, not what the daemon injects at serve time.
//
// Axes (per app + aggregate):
//   (a) bound state paths  — data-br-bind / data-br-bind-attr / data-br-bind-show in index.html
//   (b) declared surface   — actions + signals + components in manifest.surface
//   (c) typed-call usage    — main.ts contains br.call( or actions.register(
//   (d) catalog components   — network/plot/table/kpi/log/component usage in main.ts (or ui_* in prompts)
//   (e) archetype           — classified from index.html structure (no manifest field exists)
//   (f) theme pack           — theme pack declared in manifest / index.html
//
// Store dir: $BIOROUTER_APPS_DIR (default ~/.config/biorouter/agent_drafter).
// Missing/empty store -> clean exit 0 with a "no apps installed" note (CI-safe).
// Any scoring crash -> exit 1.
//
// Usage:
//   node benchmark.mjs           # human table + summary line
//   node benchmark.mjs --json    # machine-readable JSON (per-app + aggregate)
import { readdirSync, readFileSync, statSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';

const JSON_OUT = process.argv.includes('--json');
const STORE =
  process.env.BIOROUTER_APPS_DIR ||
  join(homedir(), '.config', 'biorouter', 'agent_drafter');

// ---- helpers -------------------------------------------------------------

function read(p) {
  try {
    return readFileSync(p, 'utf8');
  } catch {
    return '';
  }
}

function countMatches(text, re) {
  const m = text.match(re);
  return m ? m.length : 0;
}

// Count of declared entries in a surface sub-block (array | object | absent).
function declCount(v) {
  if (Array.isArray(v)) return v.length;
  if (v && typeof v === 'object') return Object.keys(v).length;
  return 0;
}

// Classify the archetype from RAW index.html structure. The manifest has no
// archetype field, so this is heuristic. Structured archetypes win over chat;
// chat wins over "other". Order matters.
function classifyArchetype(html) {
  const h = html;
  const has = (re) => re.test(h);

  const isCanvas = has(/<canvas\b/i) || has(/data-br-region=["']scene["']/i) || has(/data-br-canvas\b/i) || has(/\bbr-scene\b/);
  const isNetwork = has(/data-br-region=["']network["']/i) || has(/\bbr-network\b/) || has(/data-br-graph\b/) || has(/id=["']network["']/i);
  const isKpi = has(/\bbr-kpi\b/) || has(/data-br-region=["']kpi["']/i) || has(/\bbr-kpi-grid\b/) || has(/\bbr-metric\b/) || has(/\bbr-stat\b/);
  const hasTable = has(/<table\b/i) || has(/\bbr-table\b/) || has(/data-br-region=["']table["']/i);
  const hasDetail = has(/data-br-region=["']detail["']/i) || has(/\bbr-detail\b/) || has(/id=["']detail["']/i);
  const isWizard = has(/\bbr-step(per|s)?\b/) || has(/data-br-step\b/) || has(/\bbr-wizard\b/) || has(/data-br-region=["']step/i);
  const isChat = has(/data-br-chat\b/) || has(/\bbr-chat\b/) || has(/data-br-region=["']chat["']/i);

  if (isCanvas) return 'canvas';
  if (isNetwork) return 'explorer';
  if (isKpi) return 'dashboard';
  if (hasTable && hasDetail) return 'workbench';
  if (isWizard) return 'wizard';
  if (isChat) return 'chat';
  return 'other';
}

// Structured (rich, non-chatbot) archetypes named in the plan.
const STRUCTURED = new Set(['explorer', 'dashboard', 'workbench', 'wizard', 'canvas']);

// data-br-bind-attr / data-br-bind-show longest-first so the bare form does not
// double-count the suffixed forms.
const BIND_RE = /data-br-bind-attr|data-br-bind-show|data-br-bind/g;

const TYPED_CALL_RE = /br\.call\s*\(|actions\.register\s*\(/;

// Catalog-component usage beyond chat, detected honestly in main.ts / prompts.
// "log" is only matched in ui_log / br.log( / kind:"log" forms to avoid
// colliding with console.log.
const CATALOG_MAIN_RE =
  /ui_(?:network|plot|table|kpi|log|component|chart|graph|figure)\b|\bbr\.(?:network|plot|table|kpi|log|chart|graph|component|figure)\s*\(|kind:\s*["'](?:network|plot|table|kpi|log|figure|component)["']|components\.register\s*\(/;
const CATALOG_PROMPT_RE = /ui_(?:network|plot|table|kpi|log|component|chart|graph|render|patch|figure)\b/;
const CATALOG_HTML_RE = /data-br-region=["'](?:network|plot|table|kpi|log)["']/i;

function themePack(manifest, html) {
  const candidates = [
    manifest?.theme,
    manifest?.theme_pack,
    manifest?.themePack,
    manifest?.surface?.theme,
    manifest?.capabilities?.ui?.theme,
    manifest?.capabilities?.ui?.theme_pack,
  ];
  for (const c of candidates) {
    if (typeof c === 'string' && c.trim() && c.trim() !== 'default') return c.trim();
    if (c && typeof c === 'object' && Object.keys(c).length) return 'custom';
  }
  const m = html.match(/data-br-theme(?:-pack)?=["']([^"']+)["']/i);
  if (m && m[1] && m[1] !== 'default') return m[1];
  return null;
}

// ---- enumerate apps ------------------------------------------------------

function listApps(dir) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return null; // store dir missing
  }
  return entries
    .filter((e) => e.isDirectory() && !e.name.startsWith('.'))
    .map((e) => e.name)
    .filter((name) => existsSync(join(dir, name, 'manifest.json')))
    .sort();
}

// ---- score one app -------------------------------------------------------

function scoreApp(dir, id) {
  const appDir = join(dir, id);
  let manifest = {};
  try {
    manifest = JSON.parse(read(join(appDir, 'manifest.json')) || '{}');
  } catch {
    manifest = { __parse_error: true };
  }
  const html = read(join(appDir, 'index.html'));
  const mainTsPath = join(appDir, 'src', 'main.ts');
  const hasMainTs = existsSync(mainTsPath);
  const mainTs = hasMainTs ? read(mainTsPath) : '';
  const systemPrompt =
    (manifest?.agent?.system_prompt && String(manifest.agent.system_prompt)) || '';

  const boundPaths = countMatches(html, BIND_RE);
  const surface = manifest?.surface || {};
  const actions = declCount(surface.actions);
  const signals = declCount(surface.signals);
  const components = declCount(surface.components);
  const declaredSurface = actions + signals + components;

  const typedCalls = TYPED_CALL_RE.test(mainTs);
  const catalog =
    CATALOG_MAIN_RE.test(mainTs) ||
    CATALOG_PROMPT_RE.test(systemPrompt) ||
    CATALOG_HTML_RE.test(html);

  const archetype = classifyArchetype(html);
  const theme = themePack(manifest, html);

  return {
    id,
    archetype,
    boundPaths,
    surface: { actions, signals, components, total: declaredSurface },
    typedCalls,
    catalog,
    themePack: theme,
    hasMainTs,
    manifestOk: !manifest.__parse_error,
  };
}

// ---- aggregate -----------------------------------------------------------

function aggregate(apps) {
  const n = apps.length;
  const archDist = {};
  let totalBound = 0;
  let appsWithBinding = 0;
  let totalSurface = 0;
  let actions = 0;
  let signals = 0;
  let components = 0;
  let typed = 0;
  let catalog = 0;
  let themed = 0;
  let chat = 0;
  let structured = 0;

  for (const a of apps) {
    archDist[a.archetype] = (archDist[a.archetype] || 0) + 1;
    totalBound += a.boundPaths;
    if (a.boundPaths > 0) appsWithBinding++;
    totalSurface += a.surface.total;
    actions += a.surface.actions;
    signals += a.surface.signals;
    components += a.surface.components;
    if (a.typedCalls) typed++;
    if (a.catalog) catalog++;
    if (a.themePack) themed++;
    if (a.archetype === 'chat') chat++;
    if (STRUCTURED.has(a.archetype)) structured++;
  }

  const pct = (x) => (n ? +((x / n) * 100).toFixed(1) : 0);
  return {
    apps: n,
    archetypeDistribution: archDist,
    boundStatePaths: {
      total: totalBound,
      avgPerApp: n ? +(totalBound / n).toFixed(2) : 0,
      appsWithAnyBinding: appsWithBinding,
      pctWithBinding: pct(appsWithBinding),
    },
    declaredSurface: { total: totalSurface, actions, signals, components },
    typedCalls: { apps: typed, pct: pct(typed) },
    catalogComponents: { apps: catalog, pct: pct(catalog) },
    themePacks: { apps: themed, pct: pct(themed) },
    nonChat: { apps: n - chat, pct: pct(n - chat) },
    structured: { apps: structured, pct: pct(structured) },
    chat: { apps: chat, pct: pct(chat) },
  };
}

// ---- render --------------------------------------------------------------

function pad(s, w) {
  s = String(s);
  return s.length >= w ? s : s + ' '.repeat(w - s.length);
}

function renderTable(agg, apps) {
  const L = [];
  L.push(`BioRouter apps benchmark v2`);
  L.push(`store: ${STORE}`);
  L.push(`apps scored: ${agg.apps}`);
  L.push('');

  // Archetype distribution
  L.push('Archetype distribution (from raw index.html structure):');
  const order = ['explorer', 'dashboard', 'workbench', 'wizard', 'canvas', 'chat', 'other'];
  const seen = new Set(order);
  const keys = [...order, ...Object.keys(agg.archetypeDistribution).filter((k) => !seen.has(k))];
  for (const k of keys) {
    const c = agg.archetypeDistribution[k] || 0;
    if (c === 0 && !['chat', 'other'].includes(k)) continue;
    const p = agg.apps ? ((c / agg.apps) * 100).toFixed(1) : '0.0';
    const bar = '#'.repeat(Math.round((c / Math.max(1, agg.apps)) * 30));
    L.push(`  ${pad(k, 11)} ${pad(c, 4)} ${pad(p + '%', 7)} ${bar}`);
  }
  L.push('');

  // Aggregate v2 feature usage
  L.push('v2 feature usage (raw-file honest):');
  L.push(`  bound state paths        total=${agg.boundStatePaths.total}  avg/app=${agg.boundStatePaths.avgPerApp}  apps=${agg.boundStatePaths.appsWithAnyBinding} (${agg.boundStatePaths.pctWithBinding}%)`);
  L.push(`  declared surface         total=${agg.declaredSurface.total}  actions=${agg.declaredSurface.actions} signals=${agg.declaredSurface.signals} components=${agg.declaredSurface.components}`);
  L.push(`  typed calls (br.call)    apps=${agg.typedCalls.apps} (${agg.typedCalls.pct}%)`);
  L.push(`  catalog components       apps=${agg.catalogComponents.apps} (${agg.catalogComponents.pct}%)`);
  L.push(`  theme packs              apps=${agg.themePacks.apps} (${agg.themePacks.pct}%)`);
  L.push(`  structured archetypes    apps=${agg.structured.apps} (${agg.structured.pct}%)`);
  L.push('');

  // Per-app rows for apps with any v2 signal.
  const flagged = apps.filter(
    (a) => a.boundPaths > 0 || a.surface.total > 0 || a.typedCalls || a.catalog || a.themePack,
  );
  if (flagged.length) {
    L.push(`Apps using any v2 feature (${flagged.length}/${agg.apps}):`);
    L.push(`  ${pad('id', 30)} ${pad('archetype', 11)} ${pad('bind', 5)} ${pad('surf', 5)} ${pad('typed', 6)} ${pad('catlg', 6)} theme`);
    for (const a of flagged) {
      L.push(
        `  ${pad(a.id, 30)} ${pad(a.archetype, 11)} ${pad(a.boundPaths, 5)} ${pad(a.surface.total, 5)} ${pad(a.typedCalls ? 'yes' : '-', 6)} ${pad(a.catalog ? 'yes' : '-', 6)} ${a.themePack || '-'}`,
      );
    }
  } else {
    L.push('Per-app: no app uses any v2 feature (bound paths / surface / typed calls / catalog / theme pack).');
  }
  L.push('');
  L.push(
    `v2-score: ${agg.nonChat.pct}% non-chat, ${agg.boundStatePaths.avgPerApp} avg bound paths, ${agg.typedCalls.pct}% typed-calls`,
  );
  return L.join('\n');
}

// ---- main ----------------------------------------------------------------

function main() {
  const ids = listApps(STORE);
  if (ids === null || ids.length === 0) {
    const note =
      ids === null
        ? `no apps installed: store dir not found (${STORE})`
        : `no apps installed: store dir is empty (${STORE})`;
    if (JSON_OUT) {
      console.log(
        JSON.stringify(
          {
            store: STORE,
            note,
            apps: [],
            aggregate: aggregate([]),
            summary: 'v2-score: 0% non-chat, 0 avg bound paths, 0% typed-calls',
          },
          null,
          2,
        ),
      );
    } else {
      console.log(`BioRouter apps benchmark v2`);
      console.log(`store: ${STORE}`);
      console.log(note);
      console.log('v2-score: 0% non-chat, 0 avg bound paths, 0% typed-calls');
    }
    process.exit(0);
  }

  const apps = ids.map((id) => scoreApp(STORE, id));
  const agg = aggregate(apps);
  const summary = `v2-score: ${agg.nonChat.pct}% non-chat, ${agg.boundStatePaths.avgPerApp} avg bound paths, ${agg.typedCalls.pct}% typed-calls`;

  if (JSON_OUT) {
    console.log(JSON.stringify({ store: STORE, apps, aggregate: agg, summary }, null, 2));
  } else {
    console.log(renderTable(agg, apps));
  }
  process.exit(0);
}

try {
  main();
} catch (e) {
  console.error('benchmark v2 crashed:', e && e.stack ? e.stack : e);
  process.exit(1);
}
