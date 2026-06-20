// Dev rebuild of the Electron main process for the Playwright debug harness.
// Mirrors scripts/build-main.js but injects the forge-provided vite constants
// (MAIN_WINDOW_VITE_DEV_SERVER_URL / _VITE_NAME) that a standalone vite build
// otherwise leaves undefined — without them main.js throws on load.
import { build } from 'vite';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, '..');
const devUrl = process.env.MAIN_WINDOW_VITE_DEV_SERVER_URL || 'http://localhost:5173';

await build({
  configFile: resolve(root, 'vite.main.config.mts'),
  define: {
    MAIN_WINDOW_VITE_DEV_SERVER_URL: JSON.stringify(devUrl),
    MAIN_WINDOW_VITE_NAME: JSON.stringify('main_window'),
  },
  build: {
    outDir: resolve(root, '.vite/build'),
    emptyOutDir: false,
    ssr: true,
    rollupOptions: {
      input: resolve(root, 'src/main.ts'),
      output: { format: 'cjs', entryFileNames: 'main.js' },
    },
  },
});
console.log('main.js rebuilt with dev defines');
