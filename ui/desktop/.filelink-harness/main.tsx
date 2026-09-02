// Browser harness for existence-aware file links.
//
// Mounts the REAL `MarkdownContent` with the REAL stylesheet, and stubs only
// the Electron bridge — the one thing a browser cannot have. It exists because
// the defect the user reported is a COLOUR, and jsdom has no layout engine and
// never runs Tailwind: a component test can assert the class and still not know
// whether the class resolves to anything. This page can be measured.
//
//   npx vite --config .filelink-harness/vite.config.mts --port 5299
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import MarkdownContent from '../src/components/MarkdownContent';
import { ThemeProvider } from '../src/contexts/ThemeContext';
import './harness.css';

// The bridge answers for exactly one path and denies the rest, so the page holds
// a live link and a dead one side by side — which is the comparison the user
// was making when they reported it.
const PRESENT = '/work/analysis.py';
(window as unknown as { electron: unknown }).electron = {
  // `checkFilePaths` is the one that matters. The rest are the bridge surface
  // `ThemeProvider` reaches for on mount — stubbed as no-ops so the harness is
  // testing the file links and not the absence of a theme channel.
  checkFilePaths: async (requests: { path: string }[]) =>
    requests.map((r) => ({ exists: r.path === PRESENT, isDirectory: false })),
  on: () => () => {},
  off: () => {},
  broadcastThemeChange: () => {},
};

// Three ways a path reaches the transcript, because the user saw a `/tmp`
// folder rendered as a link and a bare directory path in prose is not
// linkified at all — so the reported case must arrive as inline code or as a
// markdown link, and those are the forms that have to be checked.
const BODY = [
  `Prose, live: ${PRESENT}`,
  '',
  'Prose, dead: /work/never-written.py',
  '',
  `Inline code, live: \`${PRESENT}\``,
  '',
  'Inline code, dead: `/tmp/scratch-1234`',
  '',
  `Markdown link, live: [the script](${PRESENT})`,
  '',
  'Markdown link, dead: [the report](/tmp/gone/report.md)',
].join('\n');

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ThemeProvider>
      <div className="p-8 bg-background-default text-text-default">
        <MarkdownContent content={BODY} onOpenArtifact={() => {}} workingDir="/work" />
      </div>
    </ThemeProvider>
  </StrictMode>
);
