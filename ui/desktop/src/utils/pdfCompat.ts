export function createPdfWorker(): Worker {
  return new Worker(new URL('./pdfWorker.ts', import.meta.url), { type: 'module' });
}
