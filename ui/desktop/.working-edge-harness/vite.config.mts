import { defineConfig } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import path from 'node:path';

// Serves the composer working-edge probes against the REAL main.css, so the
// authored CSS, the compiled tokens and every theme family are the app's own.
export default defineConfig({
  root: path.resolve(__dirname),
  publicDir: false,
  plugins: [tailwindcss()],
  server: { fs: { allow: [path.resolve(__dirname, '..')] } },
});
