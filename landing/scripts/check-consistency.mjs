#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const BIOROUTER = resolve(process.env.BIOROUTER_REPO || join(ROOT, '..', 'BioRouter'));

const read = (path) => readFileSync(path, 'utf8');
const landing = (path) => read(join(ROOT, path));
const source = (path) => read(join(BIOROUTER, path));

const failures = [];
const check = (condition, message) => {
  if (!condition) failures.push(message);
};
const includes = (text, needle, message) => check(text.includes(needle), message || `missing ${needle}`);
const escapeRegExp = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const capture = (text, re, label) => {
  const match = re.exec(text);
  if (!match) throw new Error(`Could not find ${label}`);
  return match[1];
};

const docs = landing('docs.html');
const index = landing('index.html');
const download = landing('download.html');
const mockups = landing('app-mockups.js');
const baam = landing('baam.html');
const registry = JSON.parse(landing('registry.json'));

const cargoVersion = capture(source('Cargo.toml'), /version = "([^"]+)"/, 'Cargo workspace version');
const packageVersion = capture(source('ui/desktop/package.json'), /"version": "([^"]+)"/, 'desktop package version');
check(cargoVersion === packageVersion, `BioRouter version mismatch: Cargo ${cargoVersion}, package ${packageVersion}`);
includes(docs, `biorouter v${cargoVersion}`, 'docs provider defaults should cite the current BioRouter version');
includes(docs, `biorouter v${cargoVersion}`, 'docs extension defaults should cite the current BioRouter version');
includes(index, `v${cargoVersion}`, 'index fallback release version should match BioRouter version');
includes(download, `v${cargoVersion}`, 'download fallback badge should match BioRouter version');
includes(download, `BioRouter-${cargoVersion}-arm64.dmg`, 'download macOS arm64 fallback should match release asset');
includes(download, `BioRouter-win32-x64-${cargoVersion}.zip`, 'download Windows fallback should match release asset');

const providerChecks = [
  ['crates/biorouter/src/providers/versa_bedrock.rs', /VERSA_BEDROCK_DEFAULT_MODEL: &str = "([^"]+)"/, 'Versa Bedrock'],
  ['crates/biorouter/src/providers/openai.rs', /OPEN_AI_DEFAULT_MODEL: &str = "([^"]+)"/, 'OpenAI'],
  ['crates/biorouter/src/providers/anthropic.rs', /ANTHROPIC_DEFAULT_MODEL: &str = "([^"]+)"/, 'Anthropic'],
  ['crates/biorouter/src/providers/azure.rs', /AZURE_DEFAULT_MODEL: &str = "([^"]+)"/, 'Azure OpenAI'],
  ['crates/biorouter/src/providers/google.rs', /GOOGLE_DEFAULT_MODEL: &str = "([^"]+)"/, 'Google Gemini'],
  ['crates/biorouter/src/providers/xai.rs', /XAI_DEFAULT_MODEL: &str = "([^"]+)"/, 'X.AI'],
  ['crates/biorouter/src/providers/openrouter.rs', /OPENROUTER_DEFAULT_MODEL: &str = "([^"]+)"/, 'OpenRouter'],
  ['crates/biorouter/src/providers/llamacpp.rs', /LLAMACPP_DEFAULT_MODEL: &str = "([^"]+)"/, 'Llama Server'],
  ['crates/biorouter/src/providers/ollama.rs', /OLLAMA_DEFAULT_MODEL: &str = "([^"]+)"/, 'Ollama'],
];

for (const [path, re, name] of providerChecks) {
  const value = capture(source(path), re, name);
  includes(docs, value, `docs should include ${name} default ${value}`);
  if (['OpenAI', 'Llama Server', 'Ollama', 'Versa Bedrock'].includes(name)) {
    includes(mockups, value, `mockups should include ${name} default ${value}`);
  }
}

const appSidebar = source('ui/desktop/src/components/BioRouterSidebar/AppSidebar.tsx');
for (const label of ['Home', 'Chat', 'History', 'Workflows', 'Scheduler', 'Extensions', 'Skills', 'Knowledge', 'Apps', 'Settings']) {
  includes(appSidebar, `label: '${label}'`, `BioRouter sidebar source should include ${label}`);
  includes(mockups, `label: '${label}'`, `mockup sidebar should include ${label}`);
}

const bundled = JSON.parse(source('ui/desktop/src/components/settings/extensions/bundled-extensions.json'));
for (const ext of bundled) {
  includes(docs, ext.display_name, `docs should include bundled extension ${ext.display_name}`);
  includes(mockups, ext.display_name, `mockup should include bundled extension ${ext.display_name}`);
  const expected = ext.enabled ? '<strong>On</strong>' : 'Off';
  const rowRe = new RegExp(
    `<tr><td>(?:<strong>)?${escapeRegExp(ext.display_name)}(?:<\\/strong>)?<\\/td><td>[\\s\\S]*?<\\/td><td>(.*?)<\\/td><\\/tr>`
  );
  const docRow = rowRe.exec(docs);
  check(docRow && docRow[1] === expected, `docs default state for ${ext.display_name} should be ${expected}`);
}

includes(docs, '--schedule-id review-weekly', 'scheduler docs should use named schedule ID argument');
includes(docs, '--workflow-source ./review.yaml', 'scheduler docs should show workflow source argument');
includes(docs, "history.replaceState(null, '', '#' + name)", 'docs navigation should keep URL hash in sync');
const baamBeforeScript = baam.split('<script>')[0];
check(registry.extensions.length === (baamBeforeScript.match(/<div[^>]*class="ext-card(?:\s|")/g) || []).length, 'registry extension count should match BAAM fallback cards');

for (const stale of ['v1.85.0', 'v1.80.0', 'claude-opus-4-1', 'claude-sonnet-4-20250514', 'grok-code-fast-1</code>', 'gpt-5.2</code>']) {
  check(!index.includes(stale) && !download.includes(stale) && !docs.includes(stale), `stale token remains: ${stale}`);
}

if (failures.length) {
  console.error(failures.map((failure) => `- ${failure}`).join('\n'));
  process.exit(1);
}

console.log('Landing consistency checks passed.');
