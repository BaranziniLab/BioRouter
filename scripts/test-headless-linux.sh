#!/usr/bin/env bash
# Smoke-test a headless Biorouter Ubuntu deployment.
set -euo pipefail

REMOTE="${1:?usage: test-headless-linux.sh <user@host> <ssh-key> [--live]}"
SSH_KEY="${2:?usage: test-headless-linux.sh <user@host> <ssh-key> [--live]}"
LIVE="${3:-}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

log() { printf '\033[1;36m[headless-test]\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31m[headless-test] FAIL: %s\033[0m\n' "$*" >&2; exit 1; }

SSH=(ssh -i "$SSH_KEY" -o BatchMode=yes -o StrictHostKeyChecking=accept-new "$REMOTE")

run_remote() {
  "${SSH[@]}" "$@"
}

require_local() {
  command -v "$1" >/dev/null 2>&1 || fail "missing local dependency: $1"
}

require_local curl
require_local node

run_browser_check() {
  local url="$1"
  log "checking browser UI with Chromium"
  (
    cd "$ROOT/ui/desktop"
    HEADLESS_URL="$url" node --input-type=module <<'NODE'
import { chromium } from '@playwright/test';

const url = process.env.HEADLESS_URL;
async function launchBrowser() {
  try {
    return await chromium.launch({ headless: true });
  } catch (error) {
    if (!String(error).includes("Executable doesn't exist")) {
      throw error;
    }
    return chromium.launch({ channel: 'chrome', headless: true });
  }
}

const browser = await launchBrowser();
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
const consoleFindings = [];
const pageErrors = [];
const failedResponses = [];
let createdSkillDir = null;
const skillZipPath = process.env.HEADLESS_SKILL_ZIP;
const brxtFilePath = process.env.HEADLESS_BRXT_FILE;
const brxtName = process.env.HEADLESS_BRXT_NAME;

page.on('console', (message) => {
  if (['error', 'warning', 'warn'].includes(message.type())) {
    consoleFindings.push(`${message.type()}: ${message.text()}`);
  }
});
page.on('pageerror', (error) => {
  pageErrors.push(error.message);
});
page.on('response', (response) => {
  if (response.status() >= 400) {
    failedResponses.push(`${response.status()} ${response.url()}`);
  }
});

try {
  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30000 });
  await page.waitForFunction(() => (document.body?.innerText || '').trim().length > 50, {
    timeout: 30000,
  });

  const home = await page.evaluate(() => ({
    title: document.title,
    href: location.href,
    text: (document.body?.innerText || '').replace(/\s+/g, ' ').slice(0, 1200),
    hasElectron: Boolean(window.electron?.getBiorouterdHostPort),
    hasAppConfig: Boolean(window.appConfig?.get),
    secretInUrl: location.href.includes('secret='),
    hasFrameworkOverlay: Boolean(
      document.body?.innerText?.includes('Failed to compile') ||
        document.body?.innerText?.includes('Internal server error') ||
        document.querySelector('[data-vite-error-overlay]')
    ),
  }));

  if (!home.title.toLowerCase().includes('biorouter')) {
    throw new Error(`unexpected page title: ${home.title}`);
  }
  if (!home.text.includes('Biorouter')) {
    throw new Error('home page did not render Biorouter content');
  }
  if (!home.hasElectron || !home.hasAppConfig) {
    throw new Error(
      `browser bridge missing: electron=${home.hasElectron} appConfig=${home.hasAppConfig}`
    );
  }
  if (home.secretInUrl) {
    throw new Error('secret remained visible in browser URL');
  }
  if (home.hasFrameworkOverlay) {
    throw new Error('framework error overlay detected');
  }

  const chooserPromise = page.evaluate(() => window.electron.directoryChooser());
  page.once('dialog', (dialog) => dialog.accept('/home/ubuntu'));
  const chooser = await chooserPromise;
  if (chooser?.canceled || chooser?.filePaths?.[0] !== '/home/ubuntu') {
    throw new Error(`headless directory chooser failed: ${JSON.stringify(chooser)}`);
  }

  const settingsBridge = await page.evaluate(async () => {
    const original = ((await window.electron.getSettings()) || {});
    const token = `headless-settings-${Date.now()}`;
    const saved = await window.electron.saveSettings({
      ...original,
      headlessSmokeProbe: token,
    });
    const reread = ((await window.electron.getSettings()) || {});
    await window.electron.saveSettings(original);
    return {
      saved,
      rereadToken: reread.headlessSmokeProbe,
      restored: true,
    };
  });
  if (!settingsBridge.saved || !String(settingsBridge.rereadToken).startsWith('headless-settings-')) {
    throw new Error(`headless settings bridge failed: ${JSON.stringify(settingsBridge)}`);
  }

  const saveDialogPromise = page.evaluate(() =>
    window.electron.showSaveDialog({ defaultPath: '/home/ubuntu/headless-smoke.yml' })
  );
  page.once('dialog', (dialog) => dialog.accept('/home/ubuntu/headless-smoke.yml'));
  const saveDialog = await saveDialogPromise;
  if (saveDialog?.canceled || saveDialog?.filePath !== '/home/ubuntu/headless-smoke.yml') {
    throw new Error(`headless save dialog failed: ${JSON.stringify(saveDialog)}`);
  }

  const smokeSkillName = `codex-headless-smoke-${Date.now()}`;
  createdSkillDir = `/home/ubuntu/.config/biorouter/skills/${smokeSkillName}`;
  const skillContent = `---\nname: ${smokeSkillName}\ndescription: Headless filesystem smoke skill.\n---\n\n# ${smokeSkillName}\n`;
  const fsBridge = await page.evaluate(
    async ({ skillDir, skillContent }) => {
      const skillFile = `${skillDir}/SKILL.md`;
      const ensured = await window.electron.ensureDirectory(skillDir);
      const written = await window.electron.writeFile(skillFile, skillContent);
      const files = await window.electron.listFiles(skillDir, '.md');
      const read = await window.electron.readFile(skillFile);
      return {
        ensured,
        written,
        files,
        readFound: read?.found,
        readHasContent: Boolean(read?.file?.includes('Headless filesystem smoke skill')),
      };
    },
    { skillDir: createdSkillDir, skillContent }
  );
  if (
    !fsBridge.ensured ||
    !fsBridge.written ||
    !fsBridge.readFound ||
    !fsBridge.readHasContent ||
    !fsBridge.files.includes('SKILL.md')
  ) {
    throw new Error(`headless filesystem bridge failed: ${JSON.stringify(fsBridge)}`);
  }

  const archiveBridge = await page.evaluate(
    async ({ skillZipPath, brxtFilePath, brxtName }) => {
      const extracted = await window.electron.extractSkillZip(skillZipPath);
      const untrustedDownload = await window.electron.downloadRegistryAsset(
        'https://example.com/not-trusted.zip'
      );
      const validated = await window.electron.validateBrxtBundle(brxtFilePath);
      const installed = await window.electron.installBrxtBundle(brxtFilePath, brxtName);
      const uninstalled = await window.electron.uninstallBrxtExtension(brxtName);
      return {
        extracted,
        untrustedDownload,
        validated,
        installed,
        uninstalled,
      };
    },
    { skillZipPath, brxtFilePath, brxtName }
  );
  if (
    archiveBridge.extracted?.error ||
    archiveBridge.extracted?.slug !== 'headless-smoke-skill' ||
    !archiveBridge.extracted?.files?.some?.(([relPath]) => relPath === 'SKILL.md')
  ) {
    throw new Error(`headless skill ZIP bridge failed: ${JSON.stringify(archiveBridge)}`);
  }
  if (!archiveBridge.untrustedDownload?.error) {
    throw new Error(`headless registry trust guard failed: ${JSON.stringify(archiveBridge)}`);
  }
  if (
    archiveBridge.validated?.error ||
    archiveBridge.validated?.manifest?.name !== brxtName ||
    archiveBridge.installed?.success !== true ||
    !archiveBridge.installed?.installDir?.includes(brxtName) ||
    archiveBridge.uninstalled?.success !== true
  ) {
    throw new Error(`headless BRXT bridge failed: ${JSON.stringify(archiveBridge)}`);
  }

  await page.getByRole('button', { name: 'Settings' }).click();
  await page.waitForFunction(
    () => (document.body?.innerText || '').toLowerCase().includes('current model'),
    { timeout: 30000 }
  );

  const settings = await page.evaluate(() => ({
    title: document.title,
    href: location.href,
    text: (document.body?.innerText || '').replace(/\s+/g, ' ').slice(0, 1200),
    secretInUrl: location.href.includes('secret='),
  }));

  if (!settings.title.toLowerCase().includes('settings')) {
    throw new Error(`settings title did not update: ${settings.title}`);
  }
  const settingsText = settings.text.toLowerCase();
  if (!settingsText.includes('current model') || !settingsText.includes('configure providers')) {
    throw new Error('settings page did not render model/provider controls');
  }
  if (settings.secretInUrl) {
    throw new Error('secret appeared in settings URL');
  }

  const modelModalStart = Date.now();
  await page.getByRole('button', { name: /switch models/i }).click();
  await page.waitForFunction(
    () => (document.body?.innerText || '').includes('Select a provider and model'),
    { timeout: 5000 }
  );
  const modelModalOpenMs = Date.now() - modelModalStart;
  if (modelModalOpenMs > 5000) {
    throw new Error(`switch model modal opened too slowly: ${modelModalOpenMs}ms`);
  }
  await page.getByRole('button', { name: 'Cancel' }).click();

  await page.getByRole('button', { name: 'Skills' }).click();
  await page.waitForFunction(
    (smokeSkillName) => {
      const text = (document.body?.innerText || '').toLowerCase();
      return (
        text.includes(smokeSkillName.toLowerCase()) ||
        text.includes('about-biorouter') ||
        text.includes('develop-biorouter')
      );
    },
    smokeSkillName,
    { timeout: 30000 }
  );

  const skills = await page.evaluate(() => ({
    title: document.title,
    href: location.href,
    text: (document.body?.innerText || '').replace(/\s+/g, ' ').slice(0, 1200),
  }));

  if (skills.text.includes('No skills found')) {
    throw new Error('skills page rendered empty');
  }
  if (!skills.text.includes(smokeSkillName)) {
    throw new Error('skills page did not render the temporary remote smoke skill');
  }

  await page.getByRole('button', { name: 'Knowledge' }).click();
  await page.waitForFunction(
    () => {
      const text = (document.body?.innerText || '').toLowerCase();
      return text.includes('knowledge') && text.includes('brainstorm') && text.includes('pages');
    },
    { timeout: 30000 }
  );

  const knowledge = await page.evaluate(() => ({
    title: document.title,
    href: location.href,
    text: (document.body?.innerText || '').replace(/\s+/g, ' ').slice(0, 1600),
  }));

  const knowledgeText = knowledge.text.toLowerCase();
  if (knowledgeText.includes('no knowledge base in focus')) {
    throw new Error('knowledge graph rendered without the active knowledge base');
  }
  if (!knowledgeText.includes('brainstorm') || !knowledgeText.includes('pages')) {
    throw new Error(`knowledge page did not render active graph summary: ${knowledge.text}`);
  }

  const relevantFailedResponses = failedResponses.filter(
    (line) =>
      !line.endsWith('/favicon.ico') &&
      !line.endsWith('/apple-touch-icon.png') &&
      !line.includes('/api/config/pricing')
  );
  const relevantConsoleFindings = consoleFindings.filter(
    (line) => !line.includes('Failed to load resource') || relevantFailedResponses.length > 0
  );
  if (
    relevantConsoleFindings.length > 0 ||
    pageErrors.length > 0 ||
    relevantFailedResponses.length > 0
  ) {
    throw new Error(
      `browser console/page errors found:\n${[
        ...relevantConsoleFindings,
        ...pageErrors,
        ...relevantFailedResponses,
      ].join('\n')}`
    );
  }

  console.log(`BROWSER_TITLE=${settings.title}`);
  console.log(`BROWSER_URL=${knowledge.href}`);
  console.log(`MODEL_MODAL_OPEN_MS=${modelModalOpenMs}`);
  console.log('BROWSER_FS_BRIDGE_OK=true');
  console.log('BROWSER_SETTINGS_BRIDGE_OK=true');
  console.log('BROWSER_ARCHIVE_BRIDGE_OK=true');
  console.log('BROWSER_KNOWLEDGE_OK=true');
  console.log('BROWSER_UI_OK=true');
} finally {
  if (createdSkillDir) {
    await page
      .evaluate((skillDir) => window.electron?.deleteDirectory?.(skillDir), createdSkillDir)
      .catch(() => {});
  }
  await browser.close();
}
NODE
  )
}

log "collecting remote health report"
REPORT="$("${SSH[@]}" 'bash -s' <<'REMOTE'
set -euo pipefail
secret="$(sudo sed -n 's/^BIOROUTER_SERVER__SECRET_KEY=//p' /etc/biorouter-headless/env | tail -n1)"
api=http://127.0.0.1:8080/api
headless=http://127.0.0.1:8080/headless
url="$(/usr/local/bin/biorouter-headless-url)"
. /etc/os-release

headless_health="$(curl -fsS "$headless/health")"
skill_dirs="$(curl -fsS "$headless/fs/list-dirs?path=%7E%2F.config%2Fbiorouter%2Fskills")"
providers_json="$(curl -fsS -H "X-Secret-Key: $secret" "$api/config/providers")"
extensions_json="$(curl -fsS -H "X-Secret-Key: $secret" "$api/config/extensions")"
sessions_json="$(curl -fsS -H "X-Secret-Key: $secret" "$api/sessions")"
apps_json="$(curl -fsS -H "X-Secret-Key: $secret" "$api/apps")"

model_count() {
  local provider="$1"
  curl -fsS -H "X-Secret-Key: $secret" --max-time 25 "$api/config/providers/$provider/models" \
    | jq 'if type == "array" then length else -1 end'
}

printf 'URL=%s\n' "$url"
printf 'OS_ID=%s\n' "$ID"
printf 'OS_VERSION=%s\n' "$VERSION_ID"
printf 'SERVICE_BIOROUTER=%s\n' "$(systemctl is-active biorouter-headless.service)"
printf 'SERVICE_XVFB=%s\n' "$(systemctl is-active biorouter-xvfb.service)"
printf 'HEADLESS_STATUS=%s\n' "$(jq -r '.status' <<<"$headless_health")"
printf 'HEADLESS_SKILL_DIR_COUNT=%s\n' "$(jq '.dirs | length' <<<"$skill_dirs")"
printf 'API_STATUS=%s\n' "$(curl -fsS -H "X-Secret-Key: $secret" "$api/status")"
printf 'BIOROUTER_VERSION=%s\n' "$(/opt/biorouter-headless/bin/biorouter --version | tr -d "\n")"
printf 'BIOROUTERD_VERSION=%s\n' "$(/opt/biorouter-headless/bin/biorouterd --version | tr -d "\n")"
printf 'HEADLESS_VERSION=%s\n' "$(/opt/biorouter-headless/bin/biorouter-headless --version | tr -d "\n")"
printf 'PROVIDER_COUNT=%s\n' "$(jq 'length' <<<"$providers_json")"
printf 'SESSION_COUNT=%s\n' "$(jq 'if type == "array" then length else .sessions|length end' <<<"$sessions_json")"
printf 'APP_COUNT=%s\n' "$(jq 'if type == "array" then length else .apps|length end' <<<"$apps_json")"
for provider in openai openrouter anthropic versa_azure llamacpp google; do
  jq -r --arg provider "$provider" '.[] | select(.name == $provider) | "PROVIDER_" + ($provider | ascii_upcase) + "_CONFIGURED=" + (.is_configured|tostring)' <<<"$providers_json"
done
for provider in openai openrouter anthropic llamacpp; do
  printf 'MODELS_%s=%s\n' "$(printf '%s' "$provider" | tr '[:lower:]' '[:upper:]')" "$(model_count "$provider")"
done
jq -r '
  [.extensions[]? | select((.config.type // .type) == "stdio") | {
    name:(.config.name // .name),
    enabled,
    dir:((.config.args // .args)[2])
  }]
  | @base64
' <<<"$extensions_json" | sed 's/^/STDIO_EXTENSIONS_B64=/'
REMOTE
)"

printf '%s\n' "$REPORT"

value() {
  printf '%s\n' "$REPORT" | awk -F= -v key="$1" '$1 == key {print substr($0, length(key) + 2); exit}'
}

URL="$(value URL)"
[ -n "$URL" ] || fail "remote did not emit a URL"
[[ "$URL" == http://* ]] || [[ "$URL" == https://* ]] || fail "invalid emitted URL: $URL"

case "$(value OS_ID):$(value OS_VERSION)" in
  ubuntu:22.04|ubuntu:24.04) ;;
  *) fail "remote OS is not Ubuntu 22.04/24.04: $(value OS_ID) $(value OS_VERSION)" ;;
esac

for service_key in SERVICE_BIOROUTER SERVICE_XVFB; do
  [ "$(value "$service_key")" = "active" ] || fail "$service_key is not active"
done

[ "$(value HEADLESS_STATUS)" = "ok" ] || fail "headless API status is not ok"
[ "$(value HEADLESS_SKILL_DIR_COUNT)" -ge 1 ] || fail "headless skill directory listing is empty"
[ "$(value API_STATUS)" = "ok" ] || fail "API status is not ok"
[ "$(value PROVIDER_COUNT)" -ge 20 ] || fail "provider count is unexpectedly low"
[ "$(value SESSION_COUNT)" -ge 1 ] || fail "sessions are not visible"
[ "$(value APP_COUNT)" -ge 1 ] || fail "apps are not visible"

for provider in OPENAI OPENROUTER ANTHROPIC VERSA_AZURE LLAMACPP; do
  [ "$(value "PROVIDER_${provider}_CONFIGURED")" = "true" ] || fail "$provider is not configured"
done

if [ "$(value PROVIDER_GOOGLE_CONFIGURED)" != "true" ]; then
  log "WARN: Google provider is not configured"
fi

for provider in OPENAI OPENROUTER ANTHROPIC LLAMACPP; do
  [ "$(value "MODELS_${provider}")" -gt 0 ] || fail "$provider model catalog is empty"
done

extensions_b64="$(value STDIO_EXTENSIONS_B64)"
if [ -n "$extensions_b64" ]; then
  extensions_json="$(printf '%s' "$extensions_b64" | base64 --decode)"
  if printf '%s' "$extensions_json" | grep -q '/Users/'; then
    fail "stdio extension paths still contain macOS /Users paths"
  fi
fi

log "creating remote archive fixtures"
FIXTURES="$("${SSH[@]}" 'python3 - <<'"'"'PY'"'"'
import json
import time
import zipfile

stamp = str(int(time.time() * 1000))
skill_zip = f"/tmp/headless-skill-smoke-{stamp}.zip"
brxt_name = f"headless-smoke-extension-{stamp}"
brxt_file = f"/tmp/{brxt_name}.brxt"

with zipfile.ZipFile(skill_zip, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr(
        "headless-smoke-skill/SKILL.md",
        "---\nname: Headless Smoke Skill\ndescription: Headless archive bridge smoke skill.\n---\n\n# Headless Smoke Skill\n",
    )
    z.writestr("headless-smoke-skill/README.md", "# Headless Smoke Skill\n")

manifest = {
    "name": brxt_name,
    "display_name": "Headless Smoke Extension",
    "description": "Temporary headless BRXT smoke extension.",
    "version": "0.1.0",
    "entry_point": "headless-smoke",
    "repository": "https://example.com/headless-smoke",
    "env_vars": [],
}
pyproject = f"""[build-system]
requires = ["setuptools>=61"]
build-backend = "setuptools.build_meta"

[project]
name = "{brxt_name}"
version = "0.1.0"
requires-python = ">=3.10"
dependencies = []

[project.scripts]
headless-smoke = "headless_smoke:main"

[tool.setuptools.packages.find]
where = ["src"]
"""
with zipfile.ZipFile(brxt_file, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("manifest.json", json.dumps(manifest))
    z.writestr("README.md", "# Headless Smoke Extension\n")
    z.writestr("pyproject.toml", pyproject)
    z.writestr("src/headless_smoke/__init__.py", "def main():\n    return 0\n")

print(f"HEADLESS_SKILL_ZIP={skill_zip}")
print(f"HEADLESS_BRXT_FILE={brxt_file}")
print(f"HEADLESS_BRXT_NAME={brxt_name}")
PY
')"

fixture_value() {
  printf '%s\n' "$FIXTURES" | awk -F= -v key="$1" '$1 == key {print substr($0, length(key) + 2); exit}'
}

HEADLESS_SKILL_ZIP="$(fixture_value HEADLESS_SKILL_ZIP)"
HEADLESS_BRXT_FILE="$(fixture_value HEADLESS_BRXT_FILE)"
HEADLESS_BRXT_NAME="$(fixture_value HEADLESS_BRXT_NAME)"
[ -n "$HEADLESS_SKILL_ZIP" ] || fail "remote skill ZIP fixture was not created"
[ -n "$HEADLESS_BRXT_FILE" ] || fail "remote BRXT fixture was not created"
[ -n "$HEADLESS_BRXT_NAME" ] || fail "remote BRXT fixture name was not created"
export HEADLESS_SKILL_ZIP HEADLESS_BRXT_FILE HEADLESS_BRXT_NAME

log "checking public URL and headless API proxy from this Mac"
curl -fsS "$URL" >/dev/null || fail "browser URL did not respond"
curl -fsS "${URL%/}/api/status" | grep -qx ok || fail "public /api/status did not return ok"
curl -fsS "${URL%/}/headless/health" | jq -e '.status == "ok"' >/dev/null || fail "public /headless/health did not return ok"
run_browser_check "$URL"

if [ "$LIVE" = "--live" ]; then
  log "running live low-cost provider completions"
  run_remote 'set -euo pipefail
    probe() {
      provider=$1
      model=$2
      token=$3
      out=$(timeout 120 env BIOROUTER_DISABLE_KEYRING=true DISPLAY=:99 /opt/biorouter-headless/bin/biorouter run --no-session --max-turns 2 --quiet --provider "$provider" --model "$model" -t "Reply with exactly this token and nothing else: $token" 2>&1)
      printf "%s\n" "$out" | grep -q "$token"
      printf "LIVE_%s_OK\n" "$provider"
    }
    probe openai gpt-4o-mini headless-live-openai-ok
    probe openrouter openai/gpt-4o-mini headless-live-openrouter-ok
  '
fi

log "headless smoke test passed for $URL"
