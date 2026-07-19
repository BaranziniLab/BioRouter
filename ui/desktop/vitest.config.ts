/// <reference types="vitest" />
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { resolve } from 'node:path';
import { TEST_TIMEOUT_MS, HOOK_TIMEOUT_MS } from './src/test/timeouts';

const cfg = {
  plugins: [react()],
  resolve: {
    alias: {
      '@': resolve(__dirname, './src'),
    },
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    css: true,
    include: ['src/**/*.{test,spec}.{js,jsx,ts,tsx}'],
    // Set here, not passed as a CLI flag, so a local `npx vitest run` and CI
    // agree. vitest's default is 5000ms, which collided exactly with the
    // asyncUtilTimeout set in src/test/setup.ts — see src/test/timeouts.ts for
    // why that combination produced failures that looked like flakiness.
    testTimeout: TEST_TIMEOUT_MS,
    hookTimeout: HOOK_TIMEOUT_MS,
  },
} satisfies Record<string, any>;

export default defineConfig(cfg as any);
