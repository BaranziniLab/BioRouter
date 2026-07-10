// Browser harness for the artifact side panel.
//
// Mounts the real `ArtifactViewer` against the real backend output (fixtures
// produced by the Rust tools) with only the Electron IPC bridge stubbed, so what
// renders here is what renders in the app. Driven by agent-browser.
//
//   npx vite --config .artifact-harness/vite.config.mts --port 5199
import { StrictMode, useState } from 'react';
import { createRoot } from 'react-dom/client';
import ArtifactViewer from '../src/components/artifacts/ArtifactViewer';
import type { ArtifactSource } from '../src/components/artifacts/artifactTypes';
import { ThemeProvider } from '../src/contexts/ThemeContext';
import './harness.css';

const MIME: Record<string, string> = {
  csv: 'text/csv',
  html: 'text/html',
  md: 'text/markdown',
  png: 'image/png',
  r: 'text/x-r',
  tsv: 'text/tab-separated-values',
};

const extOf = (path: string) => path.split('.').pop()!.toLowerCase();

async function readFixture(path: string) {
  const name = path.split('/').pop()!;
  const response = await fetch(`/fixtures/${name}`);
  if (!response.ok) {
    return { kind: 'error', title: name, path, error: `HTTP ${response.status}`, found: false };
  }
  const ext = extOf(name);
  const mimeType = MIME[ext] ?? 'text/plain';

  if (mimeType.startsWith('image/')) {
    const buffer = await response.arrayBuffer();
    let binary = '';
    new Uint8Array(buffer).forEach((byte) => (binary += String.fromCharCode(byte)));
    return {
      kind: 'image',
      title: name,
      path,
      mimeType,
      dataUrl: `data:${mimeType};base64,${btoa(binary)}`,
      size: buffer.byteLength,
      found: true,
    };
  }

  const text = await response.text();
  return {
    kind: ext === 'html' ? 'html' : 'text',
    title: name,
    path,
    mimeType,
    text,
    size: text.length,
    found: true,
  };
}

// Only the IPC bridge is faked; every component below is the shipping one.
Object.defineProperty(window, 'electron', {
  configurable: true,
  value: {
    prepareArtifactHtml: async ({ html }: { html: string }) => ({ html }),
    readArtifactFile: readFixture,
    openArtifactWindow: async () => undefined,
    openExternal: async () => undefined,
    broadcastThemeChange: () => undefined,
    on: () => () => undefined,
    off: () => undefined,
    getConfig: () => ({}),
  },
});

const ARTIFACTS: { label: string; make: () => Promise<ArtifactSource> }[] = [
  {
    label: 'autovisualiser dashboard',
    make: async () => ({
      kind: 'html',
      title: 'Dashboard Tumour Vs Normal',
      html: await (await fetch('/fixtures/dashboard.html')).text(),
    }),
  },
  {
    label: 'agent drafter app',
    make: async () => ({
      kind: 'html',
      title: 'Agent Drafter Cohort Explorer',
      html: await (await fetch('/fixtures/agent-drafter-card.html')).text(),
    }),
  },
  // One entry per file kind the agent can create, so the whole preview surface can
  // be swept in a real browser (jsdom cannot see the Tailwind/Prism class clash).
  ...[
    'report.md',
    'genes.csv',
    'config.json',
    'pipeline.yaml',
    'sample.xml',
    'analysis.R',
    'summarize.py',
    'query.sql',
    'page.html',
    'theme.css',
    'run.sh',
    'notes.txt',
    'Cargo.toml',
    'lib.rs',
    'app.ts',
    'volcano.png',
  ].map((name) => ({
    label: name,
    make: async (): Promise<ArtifactSource> => ({ kind: 'file', title: name, path: `/w/${name}` }),
  })),
];

function Harness() {
  const [artifact, setArtifact] = useState<ArtifactSource | null>(null);
  const [active, setActive] = useState<string>('');

  const open = async (entry: (typeof ARTIFACTS)[number]) => {
    setArtifact(null);
    setActive(entry.label);
    setArtifact(await entry.make());
  };

  // The shell uses inline styles on purpose: Tailwind v4 does not scan
  // dot-directories, so utility classes written here would never be generated.
  // Everything inside ArtifactViewer still gets its classes from src/.
  return (
    <div style={{ display: 'flex', height: '100vh', width: '100vw' }}>
      <div style={{ width: 220, flexShrink: 0, borderRight: '1px solid #ddd', padding: 8 }}>
        {ARTIFACTS.map((entry) => (
          <button
            key={entry.label}
            type="button"
            data-testid={`open-${entry.label.replace(/\s+/g, '-')}`}
            onClick={() => open(entry)}
            style={{
              display: 'block',
              width: '100%',
              textAlign: 'left',
              padding: '8px 10px',
              marginBottom: 2,
              fontSize: 13,
              borderRadius: 6,
              border: 0,
              cursor: 'pointer',
              background: active === entry.label ? '#eee' : 'transparent',
            }}
          >
            {entry.label}
          </button>
        ))}
      </div>
      <div style={{ flex: 1, minWidth: 0, height: '100%' }} data-testid="panel-host">
        {artifact ? (
          <ArtifactViewer
            artifact={artifact}
            onClose={() => setArtifact(null)}
            onOpenArtifact={setArtifact}
          />
        ) : (
          <div style={{ padding: 24, fontSize: 13 }}>Pick an artifact.</div>
        )}
      </div>
    </div>
  );
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ThemeProvider>
      <Harness />
    </ThemeProvider>
  </StrictMode>
);
