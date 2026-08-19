// Browser harness for the Knowledge section.
//
// Mounts the REAL Knowledge surfaces — KnowledgeView, KBSelectorMenu,
// KBManagerDialog, the ingest rail, the change-log drawer and the graph panel —
// against checked-in fixture JSON, with no Electron and no running biorouterd.
// Only the two boundaries are faked: the Electron IPC bridge and `fetch`.
//
//   cd ui/desktop && npx vite --config .knowledge-harness/vite.config.mts --port 5200
//
// Why it exists: jsdom has no layout, does not run Tailwind, does not evaluate
// `:has()` or `:focus-visible`, and has no canvas. Three defects in this section
// were invisible to `npm run test:run` and visible in ten seconds here — a
// highlight parked on the wrong row, a graph canvas that cached its ink at
// mount, and a focus surface that an explicit `bg-*` utility silently beat.
import { StrictMode, useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { MemoryRouter } from 'react-router-dom';
import { client } from '../src/api/client.gen';
import { ThemeProvider, useTheme, THEME_FAMILIES } from '../src/contexts/ThemeContext';
import { ConfigProvider } from '../src/components/ConfigContext';
import { ModelAndProviderProvider } from '../src/components/ModelAndProviderContext';
import { KnowledgeProvider } from '../src/components/knowledge/KnowledgeContext';
import KnowledgeView from '../src/components/knowledge/KnowledgeView';
import { KBSelectorTrigger } from '../src/components/knowledge/KBSelector/KBSelectorTrigger';
import { KBManagerDialog } from '../src/components/knowledge/KBSelector/KBManagerDialog';
import { SourcesRail } from '../src/components/knowledge/SourcesRail';
import { ChangeLogDrawer } from '../src/components/knowledge/changelog/ChangeLogDrawer';
import { KnowledgeGraphPanel } from '../src/components/knowledge/graph/KnowledgeGraphPanel';
import type { Graph, HistoryEntry, KbListEntry } from '../src/api/types.gen';
import './harness.css';

import basesFixture from './fixtures/bases.json';
import graphFixture from './fixtures/graph.json';
import graphBioOkfFixture from './fixtures/graph-biookf.json';
import historyFixture from './fixtures/history.json';
import pagesFixture from './fixtures/pages.json';
import providersFixture from './fixtures/providers.json';

const BASES = basesFixture as KbListEntry[];
const GRAPH = graphFixture as Graph;
/**
 * The TYPED graph, for the base whose manifest says `biookf`.
 *
 * Hand-written, and this note is the honesty clause: there is no
 * `knowledge_graph_fixture_dump` binary in this tree, so the file is built to
 * exercise every RENDERING channel — all seven families, both credibility ring
 * regimes, negated / synthesized / symmetric / parallel edges, external nodes, a
 * retraction, statuses and quantitative slots — rather than to reproduce a real
 * base's topology. When the dump lands, replace it rather than adding to it.
 */
const GRAPH_BIOOKF = graphBioOkfFixture as Graph;
const graphFor = (id: string): Graph => (id === 'multiple-sclerosis' ? GRAPH_BIOOKF : GRAPH);
const HISTORY = historyFixture as HistoryEntry[];
const PAGES = pagesFixture as Record<string, string>;

// ─── the two boundaries ──────────────────────────────────────────────────────

/**
 * How long a `/knowledge/bases` read takes to come back.
 *
 * Not decoration. The KB picker's highlight defect only exists while the base
 * list is in flight — the picker opens over its two static action rows, and a
 * highlight seeded then and merely *kept* leaves Enter opening the manager. An
 * instant fixture read cannot reproduce it, which is exactly why a harness that
 * answers everything synchronously would have certified the bug as absent.
 */
let baseListDelayMs = 0;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** Session state the harness owns, so switching the primary base actually sticks. */
const selection = {
  primary_kb: 'multiple-sclerosis' as string | null,
  hidden_kbs: [] as string[],
};

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });

type Route = {
  method: string;
  match: RegExp;
  handler: (m: RegExpMatchArray, url: URL, init: RequestInit) => Promise<Response> | Response;
};

const ROUTES: Route[] = [
  {
    method: 'GET',
    match: /^\/knowledge\/bases$/,
    handler: async () => {
      if (baseListDelayMs > 0) await sleep(baseListDelayMs);
      return json(BASES);
    },
  },
  {
    method: 'GET',
    match: /^\/knowledge\/active$/,
    handler: () =>
      json({
        primary_kb: selection.primary_kb,
        active_kb: selection.primary_kb,
        hidden_kbs: selection.hidden_kbs,
        kb_ids: BASES.map((b) => b.id).filter((id) => !selection.hidden_kbs.includes(id)),
      }),
  },
  {
    method: 'POST',
    match: /^\/knowledge\/active$/,
    handler: async (_m, _url, init) => {
      const body = JSON.parse(String(init.body ?? '{}')) as {
        primary_kb?: string | null;
        clear_primary?: boolean;
        hidden_kbs?: string[];
      };
      if (Array.isArray(body.hidden_kbs)) selection.hidden_kbs = body.hidden_kbs;
      if (body.clear_primary) selection.primary_kb = null;
      else if (typeof body.primary_kb === 'string') selection.primary_kb = body.primary_kb;
      // The daemon repairs "the primary is a member of the set" itself.
      if (selection.primary_kb && selection.hidden_kbs.includes(selection.primary_kb)) {
        selection.primary_kb =
          BASES.map((b) => b.id)
            .filter((id) => !selection.hidden_kbs.includes(id))
            .sort()[0] ?? null;
      }
      return json({
        primary_kb: selection.primary_kb,
        active_kb: selection.primary_kb,
        hidden_kbs: selection.hidden_kbs,
        kb_ids: BASES.map((b) => b.id).filter((id) => !selection.hidden_kbs.includes(id)),
      });
    },
  },
  {
    method: 'GET',
    match: /^\/knowledge\/bases\/([^/]+)\/graph$/,
    handler: (m) => json(graphFor(m[1])),
  },
  { method: 'GET', match: /^\/knowledge\/bases\/[^/]+\/history$/, handler: () => json(HISTORY) },
  {
    method: 'GET',
    match: /^\/knowledge\/bases\/[^/]+\/page$/,
    handler: (_m, url) => {
      const id = url.searchParams.get('page') ?? url.searchParams.get('id') ?? '';
      const body = PAGES[id] ?? `# ${id}\n\nNo fixture body for this page.`;
      return json({ body, content: body, path: `knowledge/${id}.md` });
    },
  },
  {
    method: 'GET',
    match: /^\/knowledge\/bases\/([^/]+)\/tier$/,
    handler: (m) => {
      const base = BASES.find((b) => b.id === m[1]);
      return json({
        id: m[1],
        tier: base?.tier ?? 'public',
        page_count: graphFor(m[1]).nodes.length,
        raw_source_count: graphFor(m[1]).nodes.filter((n) => n.kind === 'source').length,
        reason: base?.tier === 'private' ? 'privatized_by_user' : null,
        changed_at: base?.tier === 'private' ? '2026-07-02T12:00:00Z' : null,
      });
    },
  },
  {
    method: 'GET',
    match: /^\/knowledge\/bases\/([^/]+)\/location$/,
    handler: (m) => json({ path: `~/.config/biorouter/knowledge/${m[1]}` }),
  },
  {
    method: 'POST',
    match: /^\/knowledge\/bases\/[^/]+\/restore$/,
    handler: () => json({ ok: true }),
  },
  {
    method: 'POST',
    match: /^\/knowledge\/bases\/[^/]+\/preview$/,
    handler: () => json({ nodes: GRAPH.nodes.slice(0, 8).map((n) => n.id) }),
  },
  { method: 'POST', match: /^\/knowledge\/check-model$/, handler: () => json({ ok: true }) },
  { method: 'GET', match: /^\/config$/, handler: () => json({ config: {} }) },
  { method: 'POST', match: /^\/config\/read$/, handler: () => json(null) },
  { method: 'GET', match: /^\/config\/providers$/, handler: () => json(providersFixture) },
  // Both verbs: `apiGetExtensions` is generated as a POST in this build.
  {
    method: 'GET',
    match: /^\/config\/extensions$/,
    handler: () => json({ extensions: [], warnings: [] }),
  },
  {
    method: 'POST',
    match: /^\/config\/extensions$/,
    handler: () => json({ extensions: [], warnings: [] }),
  },
  {
    method: 'GET',
    match: /^\/config\/providers\/[^/]+\/models$/,
    handler: () => json({ models: [] }),
  },
];

const realFetch = window.fetch.bind(window);

window.fetch = (async (input: RequestInfo | URL, init: RequestInit = {}) => {
  const raw = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
  const url = new URL(raw, window.location.origin);
  const method = (init.method ?? (input instanceof Request ? input.method : 'GET')).toUpperCase();

  for (const route of ROUTES) {
    if (route.method === method && route.match.test(url.pathname)) {
      const m = url.pathname.match(route.match)!;
      return route.handler(m, url, init);
    }
  }
  // Anything the harness does not know about is a fixture gap, not a network
  // call — say so loudly rather than letting the dev server answer with the
  // harness's own index.html and a JSON parse error three frames later.
  if (url.origin === window.location.origin && !/\.(tsx?|css|json|js|map)$/.test(url.pathname)) {
    // eslint-disable-next-line no-console
    console.warn(`[knowledge-harness] unstubbed ${method} ${url.pathname}`);
    return json({ error: `unstubbed ${method} ${url.pathname}` }, 501);
  }
  return realFetch(input as RequestInfo, init);
}) as typeof window.fetch;

Object.defineProperty(window, 'electron', {
  configurable: true,
  value: {
    getSecretKey: async () => 'harness',
    getBiorouterdHostPort: async () => window.location.origin,
    openDirectoryInExplorer: async () => undefined,
    openExternal: async () => undefined,
    broadcastThemeChange: () => undefined,
    logInfo: (m: string) => console.info('[electron.logInfo]', m),
    on: () => () => undefined,
    off: () => undefined,
    getConfig: () => ({}),
  },
});

Object.defineProperty(window, 'appConfig', {
  configurable: true,
  value: { get: (key: string) => (key === 'BIOROUTER_API_HOST' ? window.location.origin : '') },
});

client.setConfig({ baseUrl: window.location.origin });

// ─── the shell ───────────────────────────────────────────────────────────────

type SurfaceId = 'view' | 'picker' | 'manager' | 'rail' | 'changelog' | 'graph';

const SURFACES: { id: SurfaceId; label: string }[] = [
  { id: 'view', label: 'Knowledge view' },
  { id: 'picker', label: 'KB picker (open)' },
  { id: 'manager', label: 'KB manager dialog' },
  { id: 'rail', label: 'Sources rail (ingest)' },
  { id: 'changelog', label: 'Change log drawer' },
  { id: 'graph', label: 'Graph panel' },
];

function Surface({ id }: { id: SurfaceId }) {
  const [pickerOpen, setPickerOpen] = useState(true);
  const [managerOpen, setManagerOpen] = useState(true);
  const [changeLogOpen, setChangeLogOpen] = useState(true);

  switch (id) {
    case 'view':
      return <KnowledgeView />;
    case 'picker':
      return (
        <div className="h-full bg-background-canvas p-8">
          <div className="w-64">
            <KBSelectorTrigger
              open={pickerOpen}
              onOpenChange={setPickerOpen}
              onManage={() => undefined}
              onCreate={() => undefined}
            />
          </div>
        </div>
      );
    case 'manager':
      return (
        <div className="h-full bg-background-canvas p-8">
          <KBManagerDialog open={managerOpen} onOpenChange={setManagerOpen} />
        </div>
      );
    case 'rail':
      return (
        <div className="h-full bg-background-canvas p-8">
          <div className="h-full w-[var(--knowledge-rail-sources,360px)] max-w-full">
            <SourcesRail
              className="h-full"
              kb={{ id: BASES[0].id, name: BASES[0].name, tier: BASES[0].tier }}
            />
          </div>
        </div>
      );
    case 'changelog':
      return (
        <div className="h-full bg-background-canvas p-8">
          <ChangeLogDrawer
            open={changeLogOpen}
            onOpenChange={setChangeLogOpen}
            onPreview={() => undefined}
            onRestored={() => undefined}
          />
        </div>
      );
    case 'graph':
      return (
        <div className="h-full bg-background-canvas p-8">
          <KnowledgeGraphPanel
            kbId={BASES[0].id}
            graph={GRAPH_BIOOKF}
            loading={false}
            error={null}
            onRefresh={() => undefined}
            previewSha={null}
            onClearPreview={() => undefined}
          />
        </div>
      );
  }
}

/**
 * The shell writes INLINE STYLES on purpose. Tailwind v4 does not scan
 * dot-directories, so a utility class written here would never be generated —
 * the harness chrome would render unstyled and read as a broken app. Everything
 * inside `Surface` still gets its classes from `src/`, via `@source '../src'`.
 */
function Shell() {
  const { resolvedTheme, setUserThemePreference, themeFamily, setThemeFamily } = useTheme();
  const [surface, setSurface] = useState<SurfaceId>('view');
  const [slowBases, setSlowBases] = useState(false);
  const [nonce, setNonce] = useState(0);

  useEffect(() => {
    baseListDelayMs = slowBases ? 600 : 0;
  }, [slowBases]);

  const rail: React.CSSProperties = {
    width: 232,
    flexShrink: 0,
    borderRight: '1px solid #999',
    padding: 10,
    font: '13px/1.5 ui-sans-serif, system-ui, sans-serif',
    background: resolvedTheme === 'dark' ? '#111' : '#fafafa',
    color: resolvedTheme === 'dark' ? '#eee' : '#111',
    overflowY: 'auto',
  };
  const btn = (on: boolean): React.CSSProperties => ({
    display: 'block',
    width: '100%',
    textAlign: 'left',
    padding: '7px 9px',
    marginBottom: 2,
    fontSize: 13,
    borderRadius: 6,
    border: 0,
    cursor: 'pointer',
    background: on ? (resolvedTheme === 'dark' ? '#333' : '#e3e3e3') : 'transparent',
    color: 'inherit',
  });

  return (
    <div style={{ display: 'flex', height: '100vh', width: '100vw' }}>
      <div style={rail} data-testid="harness-rail">
        <div style={{ fontWeight: 600, marginBottom: 6 }}>Surface</div>
        {SURFACES.map((s) => (
          <button
            key={s.id}
            type="button"
            data-testid={`harness-surface-${s.id}`}
            style={btn(surface === s.id)}
            onClick={() => setSurface(s.id)}
          >
            {s.label}
          </button>
        ))}

        <div style={{ fontWeight: 600, margin: '14px 0 6px' }}>Theme family</div>
        {THEME_FAMILIES.map((family) => (
          <button
            key={family}
            type="button"
            data-testid={`harness-family-${family}`}
            style={btn(themeFamily === family)}
            onClick={() => setThemeFamily(family)}
          >
            {family}
          </button>
        ))}

        <div style={{ fontWeight: 600, margin: '14px 0 6px' }}>Mode</div>
        {(['light', 'dark'] as const).map((mode) => (
          <button
            key={mode}
            type="button"
            data-testid={`harness-mode-${mode}`}
            style={btn(resolvedTheme === mode)}
            onClick={() => setUserThemePreference(mode)}
          >
            {mode}
          </button>
        ))}

        <div style={{ fontWeight: 600, margin: '14px 0 6px' }}>Scenario</div>
        <button
          type="button"
          data-testid="harness-slow-bases"
          style={btn(slowBases)}
          onClick={() => setSlowBases((v) => !v)}
          title="Delay GET /knowledge/bases by 600ms — the window in which the KB picker's highlight is seeded against an empty list"
        >
          slow base list: {slowBases ? 'on' : 'off'}
        </button>
        <button
          type="button"
          data-testid="harness-remount"
          style={btn(false)}
          onClick={() => setNonce((n) => n + 1)}
          title="Remount the surface without reloading the page (keeps the live theme)"
        >
          remount surface
        </button>
      </div>

      <div style={{ flex: 1, minWidth: 0, height: '100%' }} data-testid="harness-host">
        <KnowledgeProvider key={`${surface}-${nonce}`} sessionId="harness-session">
          <Surface id={surface} />
        </KnowledgeProvider>
      </div>
    </div>
  );
}

function Providers({ children }: { children: React.ReactNode }) {
  // ConfigProvider and ModelAndProviderProvider are the real ones — the ingest
  // rail's model picker reads both — and they are satisfied entirely by the
  // `/config/*` routes stubbed above.
  const tree = useMemo(() => children, [children]);
  return (
    <MemoryRouter>
      <ConfigProvider>
        <ModelAndProviderProvider>{tree}</ModelAndProviderProvider>
      </ConfigProvider>
    </MemoryRouter>
  );
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ThemeProvider>
      <Providers>
        <Shell />
      </Providers>
    </ThemeProvider>
  </StrictMode>
);
