import { render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ThemeProvider } from '../../contexts/ThemeContext';
import { GENERATED_THEMES, THEME_FAMILY_IDS } from '../../styles/themes.generated';
import DocumentPreview from './DocumentPreview';
import type { ArtifactDocumentFormat, ArtifactFilePreview } from './artifactTypes';

const renderDocx = vi.fn(async (_data: ArrayBuffer, container: HTMLElement) => {
  const wrapper = document.createElement('div');
  wrapper.className = 'docx-wrapper';
  const page = document.createElement('section');
  page.className = 'docx';
  page.textContent = 'Genome report';
  page.getBoundingClientRect = vi.fn(() => ({
    bottom: 800,
    height: 800,
    left: 0,
    right: 640,
    top: 0,
    width: 640,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  }));
  wrapper.append(page);
  container.append(wrapper);
});
const cancelPdfRender = vi.fn();
const renderPdfPage = vi.fn(() => ({ promise: Promise.resolve(), cancel: cancelPdfRender }));
const cleanupPdfPage = vi.fn();
const getPdfPage = vi.fn(async () => ({
  getViewport: ({ scale }: { scale: number }) => ({ width: 612 * scale, height: 792 * scale }),
  render: renderPdfPage,
  cleanup: cleanupPdfPage,
}));
const destroyPdf = vi.fn(async () => undefined);
// Typed parameter, not `()`, so the asset-configuration test can read
// `mock.calls[0][0]` — with no declared argument the tuple is empty and
// inspecting the options object is a type error.
const getPdfDocument = vi.fn((_options: Record<string, unknown>) => ({
  promise: Promise.resolve({ numPages: 2, getPage: getPdfPage, destroy: destroyPdf }),
  destroy: destroyPdf,
}));
const renderWorkbook = vi.fn(async () => [
  '<html><head></head><body><script>window.bad = true</script><table><tr><td style="color: rgba(255,255,255,0); background-color: rgba(23,59,87,0)">Gene</td></tr></table></body></html>',
]);
const destroyPresentation = vi.fn();
const renderPresentation = vi.fn(async (_data: ArrayBuffer, container: HTMLElement) => {
  container.textContent = 'PowerPoint slide';
  return { destroy: destroyPresentation };
});

vi.mock('docx-preview', () => ({ renderAsync: renderDocx }));
// The non-legacy entry point, and `workerPort` rather than `workerSrc` — both
// deliberate. jsdom has no Worker, so the component's `new Worker(...)` is
// stubbed in the setup below.
vi.mock('pdfjs-dist', () => ({
  getDocument: getPdfDocument,
  GlobalWorkerOptions: { workerPort: null },
}));
vi.mock('xlsx-preview', () => ({ xlsx2Html: renderWorkbook }));
vi.mock('@aiden0z/pptx-renderer', () => ({
  PptxViewer: { open: renderPresentation },
  RECOMMENDED_ZIP_LIMITS: {},
}));

// jsdom implements no Worker. `PdfPreview` constructs one for pdf.js's
// `workerPort`, so without this every PDF test throws before it renders.
class StubWorker {
  terminate() {}
  postMessage() {}
  addEventListener() {}
  removeEventListener() {}
}
vi.stubGlobal('Worker', StubWorker);

function documentFile(format: ArtifactDocumentFormat) {
  return {
    kind: 'document',
    format,
    title: `preview.${format}`,
    path: `/work/preview.${format}`,
    mimeType:
      format === 'pdf' ? 'application/pdf' : 'application/vnd.openxmlformats-officedocument',
    data: new Uint8Array([1, 2, 3]).buffer,
    size: 3,
    found: true,
  } satisfies Extract<ArtifactFilePreview, { kind: 'document' }>;
}

describe('DocumentPreview', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal(
      'ResizeObserver',
      class {
        constructor(
          private readonly callback: (
            entries: Array<{ contentRect: { width: number } }>,
            observer: unknown
          ) => void
        ) {}

        observe() {
          this.callback([{ contentRect: { width: 480 } }], this);
        }

        disconnect() {}
        unobserve() {}
      }
    );
  });

  it('renders PDF pages without the embedded Chromium controls', async () => {
    render(<DocumentPreview file={documentFile('pdf')} resolvedTheme="light" isResizing={false} />);

    const preview = await screen.findByLabelText('preview.pdf PDF preview');
    expect(preview.tagName).toBe('DIV');
    expect(screen.queryByRole('iframe')).toBeNull();
    expect(await screen.findByLabelText('Page 1 of 2')).toBeVisible();
    expect(screen.getByLabelText('Page 2 of 2')).toBeVisible();
    await waitFor(() => expect(getPdfPage).toHaveBeenCalledTimes(2));
  });

  // The regression this guards is not cosmetic and does not announce itself.
  // pdf.js 6 loads its JPEG 2000 and JBIG2 decoders from `wasmUrl`; without it
  // a scanned or medical PDF renders with its images silently missing — no
  // error, no placeholder. The panel shipped that way. `cMapUrl` is the same
  // shape of failure for CJK glyphs.
  it('gives pdf.js its offline decoders, cmaps, fonts and colour profiles', async () => {
    render(<DocumentPreview file={documentFile('pdf')} resolvedTheme="light" isResizing={false} />);
    await screen.findByLabelText('preview.pdf PDF preview');

    expect(getPdfDocument).toHaveBeenCalledTimes(1);
    const options = getPdfDocument.mock.calls[0][0];

    for (const key of ['wasmUrl', 'cMapUrl', 'standardFontDataUrl', 'iccUrl']) {
      const value = options[key];
      expect(value, `${key} must be configured`).toEqual(expect.any(String));
      // pdf.js throws "Invalid factory url … must include trailing slash"
      // rather than degrading, so this is a hard failure at load, not a nicety.
      expect(String(value).endsWith('/'), `${key} must end in a trailing slash`).toBe(true);
    }
    expect(options.cMapPacked).toBe(true);
  });

  it('renders Word documents as pages fitted to the preview width', async () => {
    render(
      <DocumentPreview file={documentFile('docx')} resolvedTheme="light" isResizing={false} />
    );

    const page = await screen.findByText('Genome report');
    expect(page).toBeInTheDocument();
    await waitFor(() => expect(page).toHaveAttribute('data-preview-scale', '0.7125'));
    expect(renderDocx).toHaveBeenCalledOnce();
  });

  it('renders workbook sheets in a script-free frame', async () => {
    render(<DocumentPreview file={documentFile('xlsx')} resolvedTheme="dark" isResizing={false} />);

    const frame = await screen.findByLabelText('preview.xlsx, sheet 1');
    await waitFor(() => expect(frame.getAttribute('srcdoc')).toContain('Gene'));
    expect(frame.getAttribute('srcdoc')).not.toContain('<script');
    expect(frame.getAttribute('srcdoc')).toContain('color: rgb(255, 255, 255)');
    expect(frame.getAttribute('srcdoc')).toContain('background-color: rgb(23, 59, 87)');
    expect(frame).toHaveAttribute('sandbox', '');
  });

  it('renders PowerPoint slides and disposes the viewer on unmount', async () => {
    const view = render(
      <DocumentPreview file={documentFile('pptx')} resolvedTheme="light" isResizing={false} />
    );

    expect(await screen.findByText('PowerPoint slide')).toBeInTheDocument();
    expect(renderPresentation).toHaveBeenCalledOnce();
    view.unmount();
    expect(destroyPresentation).toHaveBeenCalledOnce();
  });
});

describe('workbook sheet theming', () => {
  afterEach(() => localStorage.clear());

  async function srcdocFor(family: string, resolvedTheme: 'light' | 'dark') {
    localStorage.setItem('theme_family', family);
    const view = render(
      <ThemeProvider>
        <DocumentPreview
          file={documentFile('xlsx')}
          resolvedTheme={resolvedTheme}
          isResizing={false}
        />
      </ThemeProvider>
    );
    const frame = await screen.findByLabelText('preview.xlsx, sheet 1');
    await waitFor(() => expect(frame.getAttribute('srcdoc')).toContain('Gene'));
    const srcdoc = frame.getAttribute('srcdoc') ?? '';
    view.unmount();
    return srcdoc;
  }

  // The sheet document is sandboxed under `default-src 'none'`, so it cannot
  // load the app stylesheet and must inline literal colours. They used to be a
  // single hardcoded light/dark pair, which pinned every family to Parchment.
  it.each(THEME_FAMILY_IDS)('paints the %s ground, ink and table fill', async (family) => {
    for (const mode of ['light', 'dark'] as const) {
      const { background, foreground, card } = GENERATED_THEMES[family][mode].surface;
      const srcdoc = await srcdocFor(family, mode);
      expect(srcdoc).toContain(`color: ${foreground}`);
      expect(srcdoc).toContain(`background: ${background}`);
      expect(srcdoc).toContain(`table { background: ${card}; }`);
    }
  });

  // Without this, re-hardcoding a colour would leave every per-family
  // assertion above still passing for whichever family the hardcode matched.
  it('gives each family a distinct dark sheet', async () => {
    const rendered: string[] = [];
    for (const family of THEME_FAMILY_IDS) rendered.push(await srcdocFor(family, 'dark'));
    expect(new Set(rendered).size).toBe(THEME_FAMILY_IDS.length);
  });

  // Theming must not have loosened the sandbox that makes this safe to render.
  it('keeps the strict CSP and script stripping', async () => {
    const srcdoc = await srcdocFor('alma-mater', 'dark');
    expect(srcdoc).toContain("default-src 'none'");
    expect(srcdoc).not.toContain('<script');
  });
});
