import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ThemeProvider } from '../../contexts/ThemeContext';
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

    expect(screen.getByText('Jupyter notebook')).toBeInTheDocument();
    expect(screen.getByText(/2 cells · Python 3/)).toBeInTheDocument();
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
