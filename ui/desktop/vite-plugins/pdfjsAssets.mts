import { cpSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import type { Plugin } from 'vite';

/**
 * Copies pdf.js's runtime assets into `public/pdfjs/` so the PDF preview can
 * load them offline.
 *
 * **This is not an optimisation.** Without them pdf.js degrades silently:
 *
 * - `wasm/` holds the **JPEG 2000 (`openjpeg`) and JBIG2 decoders**, which are
 *   the standard image codecs inside *scanned and medical PDFs*. Missing, those
 *   images simply fail to decode — the page renders with holes and no error.
 * - `cmaps/` is required for CJK and other non-Latin encodings; missing, glyphs
 *   drop out.
 * - `standard_fonts/` backs the Standard-14 fonts when a PDF does not embed
 *   them.
 * - `iccs/` backs colour management for CMYK documents.
 *
 * `quickjs-eval.wasm` is deliberately **excluded** — 469 KB of XFA/AcroForm
 * scripting that a view-only preview must not run.
 *
 * Copying into `public/` rather than emitting through the bundler is what makes
 * this work in both modes: Vite serves `publicDir` in dev and copies it into the
 * build output, so one mechanism covers `npm start` and a packaged app.
 */
export function pdfjsAssets(): Plugin {
  const require = createRequire(import.meta.url);

  return {
    name: 'biorouter-pdfjs-assets',
    buildStart() {
      const pdfjsRoot = dirname(require.resolve('pdfjs-dist/package.json'));
      const target = join(process.cwd(), 'public', 'pdfjs');

      // Rebuild from scratch so an upgrade cannot leave a stale mixture of two
      // pdf.js versions' cmaps behind.
      rmSync(target, { recursive: true, force: true });
      mkdirSync(target, { recursive: true });

      for (const dir of ['cmaps', 'standard_fonts', 'iccs', 'wasm']) {
        const from = join(pdfjsRoot, dir);
        if (!existsSync(from)) continue;
        cpSync(from, join(target, dir), {
          recursive: true,
          filter: (src) => !src.includes('quickjs-eval'),
        });
      }
    },
  };
}
