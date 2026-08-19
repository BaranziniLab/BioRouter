import { defineConfig } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import path from 'node:path';

// Serves the Knowledge-section harness. Everything it needs is in this
// directory (fixtures are checked in), so there is no environment variable and
// no running biorouterd.
//
//   npx vite --config .knowledge-harness/vite.config.mts --port 5200
export default defineConfig({
  root: path.resolve(__dirname),
  publicDir: false,
  plugins: [tailwindcss()],
  server: { fs: { allow: [path.resolve(__dirname, '..')] } },
});
