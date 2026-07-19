import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { ThemeProvider } from '../../contexts/ThemeContext';
import { GENERATED_THEMES, THEME_FAMILY_IDS } from '../../styles/themes.generated';
import NotebookPreview from './NotebookPreview';
import type { ArtifactFilePreview } from './artifactTypes';

function notebookFile(text: string) {
  return {
    kind: 'text',
    title: 'analysis.ipynb',
    path: '/work/analysis.ipynb',
    mimeType: 'application/x-ipynb+json',
    text,
    size: text.length,
    found: true,
  } satisfies Extract<ArtifactFilePreview, { kind: 'text' | 'html' }>;
}

describe('NotebookPreview', () => {
  it('renders markdown, code, execution output, images, and sandboxed HTML', () => {
    const text = JSON.stringify({
      metadata: { kernelspec: { display_name: 'Python 3', language: 'python' } },
      cells: [
        { cell_type: 'markdown', source: ['# Expression analysis\n', 'A notebook summary.'] },
        {
          cell_type: 'code',
          execution_count: 4,
          source: ['print("ready")\n'],
          outputs: [
            { output_type: 'stream', text: ['ready\n'] },
            { output_type: 'display_data', data: { 'image/png': 'aGVsbG8=' } },
            { output_type: 'display_data', data: { 'text/html': '<strong>HTML result</strong>' } },
          ],
        },
      ],
    });

    render(
      <ThemeProvider>
        <NotebookPreview file={notebookFile(text)} resolvedTheme="light" />
      </ThemeProvider>
    );

    // The status strip: shared label + path + cell count + kernel (design spec H).
    expect(screen.getByText('Notebook')).toBeInTheDocument();
    expect(screen.getByText('analysis.ipynb')).toBeInTheDocument();
    expect(screen.getByText('2 cells')).toBeInTheDocument();
    expect(screen.getByText('Python 3')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Expression analysis' })).toBeInTheDocument();
    expect(screen.getByText('ready')).toBeInTheDocument();
    expect(screen.getByAltText('Notebook output')).toHaveAttribute(
      'src',
      'data:image/png;base64,aGVsbG8='
    );
    const htmlOutput = screen.getByTitle('HTML notebook output');
    expect(htmlOutput).toHaveAttribute('sandbox', '');
    expect(htmlOutput.getAttribute('srcdoc')).toContain('<strong>HTML result</strong>');
    expect(htmlOutput.getAttribute('srcdoc')).toContain("default-src 'none'");
  });

  it('shows a readable error for malformed notebook JSON', () => {
    render(
      <ThemeProvider>
        <NotebookPreview file={notebookFile('{broken')} resolvedTheme="light" />
      </ThemeProvider>
    );

    expect(screen.getByText(/could not preview this notebook/i)).toBeInTheDocument();
  });
});

describe('NotebookPreview HTML output theming', () => {
  afterEach(() => localStorage.clear());

  const htmlOutputNotebook = JSON.stringify({
    cells: [
      {
        cell_type: 'code',
        source: ['df.head()\n'],
        outputs: [{ output_type: 'display_data', data: { 'text/html': '<table></table>' } }],
      },
    ],
  });

  function srcdocFor(family: string, resolvedTheme: 'light' | 'dark') {
    localStorage.setItem('theme_family', family);
    const view = render(
      <ThemeProvider>
        <NotebookPreview file={notebookFile(htmlOutputNotebook)} resolvedTheme={resolvedTheme} />
      </ThemeProvider>
    );
    const srcdoc = screen.getByTitle('HTML notebook output').getAttribute('srcdoc') ?? '';
    view.unmount();
    return srcdoc;
  }

  // A notebook's `text/html` output is sandboxed under `default-src 'none'`, so
  // it cannot read the app stylesheet and has to be handed literal hexes. Those
  // hexes used to be one hardcoded light/dark pair, which painted every family
  // Parchment. Assert they come from the ACTIVE family's generated tokens.
  it.each(THEME_FAMILY_IDS)('paints the %s ground and ink in both modes', (family) => {
    for (const mode of ['light', 'dark'] as const) {
      const { background, foreground, border } = GENERATED_THEMES[family][mode].surface;
      const srcdoc = srcdocFor(family, mode);
      expect(srcdoc).toContain(`background:${background}`);
      expect(srcdoc).toContain(`color:${foreground}`);
      expect(srcdoc).toContain(`1px solid ${border}`);
    }
  });

  // The regression guard with teeth: the three families must not agree. If a
  // future edit re-hardcodes a colour, every family renders the same ground and
  // the per-family assertions above would still pass for whichever family that
  // hardcode happened to match.
  it('gives each family a distinct dark ground', () => {
    const grounds = THEME_FAMILY_IDS.map(
      (family) => GENERATED_THEMES[family].dark.surface.background
    );
    expect(new Set(grounds).size).toBe(THEME_FAMILY_IDS.length);

    const rendered = THEME_FAMILY_IDS.map((family) => srcdocFor(family, 'dark'));
    expect(new Set(rendered).size).toBe(THEME_FAMILY_IDS.length);
  });

  it('keeps the strict sandbox and CSP while theming', () => {
    const frame = (() => {
      localStorage.setItem('theme_family', 'roche-limit');
      render(
        <ThemeProvider>
          <NotebookPreview file={notebookFile(htmlOutputNotebook)} resolvedTheme="dark" />
        </ThemeProvider>
      );
      return screen.getByTitle('HTML notebook output');
    })();

    expect(frame).toHaveAttribute('sandbox', '');
    const srcdoc = frame.getAttribute('srcdoc') ?? '';
    expect(srcdoc).toContain("default-src 'none'");
    expect(srcdoc).toContain("style-src 'unsafe-inline'");
    expect(srcdoc).not.toContain('script-src');
  });
});
