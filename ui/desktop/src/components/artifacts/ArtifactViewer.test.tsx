import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ThemeProvider } from '../../contexts/ThemeContext';
import ArtifactViewer from './ArtifactViewer';

function installElectronMock() {
  Object.defineProperty(window, 'electron', {
    configurable: true,
    value: {
      prepareArtifactHtml: vi.fn(async ({ html }: { html: string }) => ({ html })),
      readArtifactFile: vi.fn(async () => ({
        kind: 'text',
        title: 'analysis.sql',
        path: '/tmp/analysis.sql',
        mimeType: 'application/sql',
        text: 'select * from genes;',
        size: 20,
        found: true,
      })),
      openArtifactWindow: vi.fn(),
      openExternal: vi.fn(),
      broadcastThemeChange: vi.fn(),
      on: vi.fn().mockReturnValue(() => undefined),
    },
  });

  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    value: vi.fn().mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    }),
  });
}

describe('ArtifactViewer', () => {
  it('renders HTML artifacts in a read-only side viewer frame', async () => {
    installElectronMock();

    render(
      <ThemeProvider>
        <ArtifactViewer
          artifact={{
            kind: 'html',
            title: 'visualization.html',
            html: '<!doctype html><html><body><h1>Plot</h1></body></html>',
          }}
          onClose={vi.fn()}
          onOpenArtifact={vi.fn()}
        />
      </ThemeProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('artifact-viewer')).toBeInTheDocument();
      expect(screen.getByTitle('visualization.html')).toHaveAttribute('sandbox');
    });
  });

  it('loads generated text files with syntax-highlighted preview', async () => {
    installElectronMock();

    render(
      <ThemeProvider>
        <ArtifactViewer
          artifact={{ kind: 'file', title: 'analysis.sql', path: '/tmp/analysis.sql' }}
          onClose={vi.fn()}
          onOpenArtifact={vi.fn()}
        />
      </ThemeProvider>
    );

    await waitFor(() => {
      expect(window.electron.readArtifactFile).toHaveBeenCalledWith('/tmp/analysis.sql');
      expect(screen.getByText(/select/)).toBeInTheDocument();
    });
  });

  it('reports visualization render errors from the preview frame once', async () => {
    installElectronMock();
    const onRenderError = vi.fn();

    render(
      <ThemeProvider>
        <ArtifactViewer
          artifact={{
            kind: 'html',
            title: 'chart',
            html: '<!doctype html><html><body><h1>Plot</h1></body></html>',
          }}
          onClose={vi.fn()}
          onOpenArtifact={vi.fn()}
          onRenderError={onRenderError}
        />
      </ThemeProvider>
    );

    await waitFor(() => expect(screen.getByTestId('artifact-viewer')).toBeInTheDocument());

    const message = new MessageEvent('message', {
      data: {
        type: 'biorouter-viz-render-error',
        payload: {
          message: 'This visualization could not be rendered.',
          detail: 'ReferenceError: Chart is not defined',
          href: 'file:///tmp/chart.html',
        },
      },
    });
    window.dispatchEvent(message);
    window.dispatchEvent(message);

    expect(onRenderError).toHaveBeenCalledTimes(1);
    expect(onRenderError).toHaveBeenCalledWith({
      artifactTitle: 'chart',
      message: 'This visualization could not be rendered.',
      detail: 'ReferenceError: Chart is not defined',
      href: 'file:///tmp/chart.html',
    });
  });
});
