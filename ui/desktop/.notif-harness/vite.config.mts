import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import path from 'node:path';

// Renders the REAL notification components against the app's real main.css so a
// browser (not jsdom) can confirm the toast/alert layout — chip top-alignment,
// the close-button gutter, and the status tints in both themes.
export default defineConfig({
  root: path.resolve(__dirname),
  publicDir: false,
  plugins: [react(), tailwindcss()],
  server: { fs: { allow: [path.resolve(__dirname, '..')] } },
});
