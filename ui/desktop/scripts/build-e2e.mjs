import { createRequire } from 'node:module';
import { rm } from 'node:fs/promises';
import { URL } from 'node:url';
import { build } from 'vite';

const require = createRequire(import.meta.url);
const forgeConfig = require('../forge.config.ts');
const ViteConfigGenerator = require('@electron-forge/plugin-vite/dist/ViteConfig').default;

const vitePlugin = forgeConfig.plugins.find(
  (plugin) => plugin?.name === '@electron-forge/plugin-vite'
);
if (!vitePlugin?.config) {
  throw new Error('Electron Forge Vite plugin configuration was not found');
}

const projectDir = new URL('..', import.meta.url).pathname;
const generator = new ViteConfigGenerator(vitePlugin.config, projectDir, true);
await rm(new URL('../.vite', import.meta.url), { recursive: true, force: true });
const configs = await Promise.all([generator.getBuildConfigs(), generator.getRendererConfig()]);

await Promise.all(
  configs.flat().map((config) => build({ configFile: false, logLevel: 'error', ...config }))
);
