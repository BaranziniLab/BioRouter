import { useMemo } from 'react';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { useThemeFamily } from '../../contexts/ThemeContext';
import { CODE_FONT_FAMILY, codeThemesByFamily } from '../../styles/codeTheme';
import { cn } from '../../utils';
import MarkdownContent from '../MarkdownContent';
import type { ArtifactFilePreview } from './artifactTypes';

type NotebookFile = Extract<ArtifactFilePreview, { kind: 'text' | 'html' }>;

type NotebookOutput = {
  output_type?: string;
  text?: string | string[];
  data?: Record<string, unknown>;
  ename?: string;
  evalue?: string;
  traceback?: string[];
};

type NotebookCell = {
  cell_type?: string;
  source?: string | string[];
  execution_count?: number | null;
  outputs?: NotebookOutput[];
};

type Notebook = {
  cells: NotebookCell[];
  metadata?: {
    kernelspec?: { display_name?: string; language?: string };
    language_info?: { name?: string };
  };
};

function joined(value: unknown) {
  if (typeof value === 'string') return value;
  if (Array.isArray(value) && value.every((item) => typeof item === 'string')) {
    return value.join('');
  }
  return '';
}

function safeNotebookHtml(html: string, resolvedTheme: 'light' | 'dark') {
  const foreground = resolvedTheme === 'dark' ? '#e8e5df' : '#282724';
  const background = resolvedTheme === 'dark' ? '#242321' : '#fffdf8';
  return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:"><style>html,body{margin:0;color:${foreground};background:${background};font:13px/1.5 ui-sans-serif,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}body{padding:12px}img,svg{max-width:100%;height:auto}table{border-collapse:collapse}td,th{border:1px solid #aaa6;padding:4px 7px}</style></head><body>${html}</body></html>`;
}

function NotebookOutputView({
  output,
  resolvedTheme,
}: {
  output: NotebookOutput;
  resolvedTheme: 'light' | 'dark';
}) {
  if (output.output_type === 'error') {
    const traceback =
      output.traceback?.join('\n') || [output.ename, output.evalue].filter(Boolean).join(': ');
    return (
      <pre className="overflow-auto whitespace-pre-wrap rounded-md bg-status-danger/10 p-3 font-mono text-xs text-status-danger">
        {traceback || 'Notebook execution failed.'}
      </pre>
    );
  }

  const stream = joined(output.text);
  if (stream) {
    return (
      <pre className="overflow-auto whitespace-pre-wrap px-3 py-2 font-mono text-xs text-text-default">
        {stream}
      </pre>
    );
  }

  const data = output.data ?? {};
  const png = joined(data['image/png']);
  if (png) {
    return (
      <div className="overflow-auto p-3">
        <img src={`data:image/png;base64,${png.replace(/\s/g, '')}`} alt="Notebook output" />
      </div>
    );
  }

  const jpeg = joined(data['image/jpeg']);
  if (jpeg) {
    return (
      <div className="overflow-auto p-3">
        <img src={`data:image/jpeg;base64,${jpeg.replace(/\s/g, '')}`} alt="Notebook output" />
      </div>
    );
  }

  const svg = joined(data['image/svg+xml']);
  if (svg) {
    return (
      <div className="overflow-auto p-3">
        <img
          src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`}
          alt="Notebook output"
        />
      </div>
    );
  }

  const html = joined(data['text/html']);
  if (html) {
    return (
      <iframe
        title="HTML notebook output"
        srcDoc={safeNotebookHtml(html, resolvedTheme)}
        sandbox=""
        className="min-h-40 w-full bg-white"
      />
    );
  }

  const markdown = joined(data['text/markdown']);
  if (markdown) {
    return (
      <div className="px-3 py-2">
        <MarkdownContent content={markdown} />
      </div>
    );
  }

  const plain = joined(data['text/plain']);
  if (plain) {
    return (
      <pre className="overflow-auto whitespace-pre-wrap px-3 py-2 font-mono text-xs text-text-default">
        {plain}
      </pre>
    );
  }

  const json = data['application/json'];
  if (json !== undefined) {
    return (
      <pre className="overflow-auto whitespace-pre-wrap px-3 py-2 font-mono text-xs text-text-default">
        {typeof json === 'string' ? json : JSON.stringify(json, null, 2)}
      </pre>
    );
  }

  return null;
}

export default function NotebookPreview({
  file,
  resolvedTheme,
}: {
  file: NotebookFile;
  resolvedTheme: 'light' | 'dark';
}) {
  const themeFamily = useThemeFamily();
  const parsed = useMemo(() => {
    try {
      const value = JSON.parse(file.text) as Partial<Notebook>;
      if (!Array.isArray(value.cells)) throw new Error('Notebook cells are missing.');
      return { notebook: value as Notebook, error: null };
    } catch (cause) {
      return {
        notebook: null,
        error: cause instanceof Error ? cause.message : 'Could not read this notebook.',
      };
    }
  }, [file.text]);

  if (!parsed.notebook) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-sm text-text-muted">
        Could not preview this notebook: {parsed.error}
      </div>
    );
  }

  const notebook = parsed.notebook;
  const language =
    notebook.metadata?.language_info?.name || notebook.metadata?.kernelspec?.language || 'python';
  const kernel = notebook.metadata?.kernelspec?.display_name;

  return (
    <div className="h-full overflow-auto bg-background-medium">
      <div className="sticky top-0 z-10 flex items-center gap-2 border-b border-border-subtle bg-background-muted/95 px-4 py-2 backdrop-blur">
        <span className="text-xs font-semibold text-text-default">Jupyter notebook</span>
        <span className="text-xs text-text-muted">
          {notebook.cells.length} cell{notebook.cells.length === 1 ? '' : 's'}
          {kernel ? ` · ${kernel}` : ''}
        </span>
      </div>
      <div className="mx-auto max-w-5xl space-y-3 p-4">
        {notebook.cells.map((cell, index) => {
          const source = joined(cell.source);
          const executionLabel = cell.execution_count ?? ' ';
          if (cell.cell_type === 'markdown') {
            return (
              <section
                key={index}
                aria-label={`Markdown cell ${index + 1}`}
                className="rounded-lg border border-border-subtle bg-background-default px-4 py-3"
              >
                <MarkdownContent content={source} />
              </section>
            );
          }

          if (cell.cell_type === 'code') {
            return (
              <section
                key={index}
                aria-label={`Code cell ${index + 1}`}
                className="overflow-hidden rounded-lg border border-border-subtle bg-background-default"
              >
                <div className="flex min-w-0">
                  <div className="w-[72px] shrink-0 px-2 py-3 text-right font-mono text-[11px] text-text-muted">
                    In [{executionLabel}]:
                  </div>
                  <div className="min-w-0 flex-1 overflow-auto border-l border-border-subtle">
                    <SyntaxHighlighter
                      style={codeThemesByFamily[themeFamily][resolvedTheme]}
                      language={language}
                      PreTag="div"
                      customStyle={{ margin: 0, padding: '12px 14px', background: 'transparent' }}
                      codeTagProps={{ style: { fontFamily: CODE_FONT_FAMILY, whiteSpace: 'pre' } }}
                    >
                      {source.replace(/\r?\n$/, '')}
                    </SyntaxHighlighter>
                  </div>
                </div>
                {(cell.outputs?.length ?? 0) > 0 && (
                  <div className="border-t border-border-subtle">
                    {cell.outputs?.map((output, outputIndex) => (
                      <div
                        key={outputIndex}
                        className={cn(outputIndex > 0 && 'border-t border-border-subtle')}
                      >
                        <NotebookOutputView output={output} resolvedTheme={resolvedTheme} />
                      </div>
                    ))}
                  </div>
                )}
              </section>
            );
          }

          return (
            <section
              key={index}
              aria-label={`Raw cell ${index + 1}`}
              className="overflow-auto whitespace-pre-wrap rounded-lg border border-border-subtle bg-background-default p-4 font-mono text-xs"
            >
              {source}
            </section>
          );
        })}
      </div>
    </div>
  );
}
