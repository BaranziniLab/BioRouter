import { defineConfig } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import path from 'node:path';

// Serves the reference-chip harness (issue #65). jsdom computes no layout and
// applies no Tailwind, so the class contract the unit tests assert says nothing
// about whether the chip actually looks right or stays inside its container —
// this repo has a documented case (the Prism/Tailwind `token table` collision)
// where only a real browser caught the bug.
//
//   npx vite --config .reference-chip-harness/vite.config.mts --port 5201
export default defineConfig({
  root: path.resolve(__dirname),
  publicDir: false,
  plugins: [tailwindcss(), react()],
  server: { fs: { allow: [path.resolve(__dirname, '..')] } },
});
