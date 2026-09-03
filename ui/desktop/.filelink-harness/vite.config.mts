import { defineConfig } from 'vite';
import tailwindcss from '@tailwindcss/vite';
import path from 'node:path';
export default defineConfig({
  root: path.resolve(__dirname),
  publicDir: false,
  plugins: [tailwindcss()],
});
