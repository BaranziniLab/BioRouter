#!/usr/bin/env node
/*
 * Agent Drafter — in-app PREVIEW runtime test (Playwright).
 *
 * Exercises the "bridge" transport + auto-resize that power the in-app preview
 * (the `ui://` resource rendered in Biorouter's sandboxed MCP-UI iframe), which
 * the existing acp-ws loop test never touches. It builds the *real* assembled
 * preview HTML (the actual templates/agent.js + theme.css, mirroring
 * render.rs::assemble) and drives it inside a faithful emulation of the host:
 *
 *   - the host listens for `{type:"ui-size-change", payload:{width,height}}` and
 *     resizes the iframe (exactly like @mcp-ui/client autoResizeIframe);
 *   - any other message is treated as a UI action and answered with
 *     `{type:"ui-message-response", messageId, payload:{response}}` (exactly like
 *     the @mcp-ui renderer dispatching onUIAction). A "prompt" action is the one
 *     Biorouter's MCPUIResourceRenderer routes into chat.
 *
 * Two cases, asserting the behaviors the code review flagged:
 *   1. STATIC, tall content  -> the iframe must auto-grow (host gets ui-size-change).
 *   2. AGENTIC, send a chat message -> the prompt must reach the host as a
 *      "prompt" action AND the typing indicator must clear (no infinite hang).
 *
 * Usage: node scripts/agent-drafter/preview-runtime-test.mjs
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
// Resolve playwright from ui/desktop/node_modules (not this script's dir).
const _pw = await import(
  pathToFileURL(join(HERE, '../../ui/desktop/node_modules/playwright/index.js')).href
);
const chromium = _pw.chromium || (_pw.default && _pw.default.chromium);
const TPL = join(HERE, '../../crates/biorouter-mcp/src/agent_drafter/templates');
const AGENT_JS = readFileSync(join(TPL, 'agent.js'), 'utf8');
const THEME_CSS = readFileSync(join(TPL, 'theme.css'), 'utf8');
const RESIZE_JS = readFileSync(join(TPL, 'resize.js'), 'utf8');

const log = (...a) => console.log('[preview-test]', ...a);
let failures = 0;
function check(name, cond, detail) {
  if (cond) log(`PASS — ${name}`);
  else { failures++; console.error(`[preview-test] FAIL — ${name}${detail ? ': ' + detail : ''}`); }
}

// --- mirror render.rs::assemble (bridge transport, no endpoint) --------------
function injectBefore(html, needle, insert, appendIfMissing) {
  const i = html.toLowerCase().indexOf(needle.toLowerCase());
  if (i !== -1) return html.slice(0, i) + insert + html.slice(i);
  return appendIfMissing ? html + insert : insert + html;
}
function agentConfigScript(agent) {
  const cfg = {
    transport: 'bridge',
    endpoint: null,
    systemPrompt: agent.systemPrompt || '',
    greeting: agent.greeting ?? null,
    tools: agent.tools || [],
  };
  const json = JSON.stringify(cfg).replace(/</g, '\\u003c');
  return `<script>window.BIOROUTER_AGENT_CONFIG = ${json};</script>\n`;
}
function assemblePreview(entryHtml, { agentic, agent } = {}) {
  const style = `<style id="biorouter-theme">\n${THEME_CSS}\n</style>\n`;
  let html = injectBefore(entryHtml, '</head>', style, false);
  // Auto-resize reporter for EVERY artifact (mirrors render.rs::assemble).
  html = injectBefore(html, '</body>', `<script>\n${RESIZE_JS}\n</script>\n`, true);
  if (agentic) {
    let block = agentConfigScript(agent || {});
    if (!html.toLowerCase().includes('data-br-chat')) {
      block += '<div class="br-container"><div class="br-card" data-br-chat style="margin-top:24px"></div></div>\n';
    }
    block += `<script>\n${AGENT_JS}\n</script>\n`;
    html = injectBefore(html, '</body>', block, true);
  }
  return html;
}

// --- the host page: emulates @mcp-ui renderer + Biorouter action routing -----
const HOST_PAGE = `<!doctype html><html><head><meta charset=utf-8><style>
  html,body{margin:0;height:100%} #frame{width:900px;height:560px;border:1px solid #ccc;display:block}
</style></head><body>
<iframe id="frame" sandbox="allow-scripts"></iframe>
<script>
  window.__sizes = [];      // every ui-size-change height the host received
  window.__actions = [];    // every UI action (prompt/tool/...) the host received
  const frame = document.getElementById('frame');
  window.addEventListener('message', (ev) => {
    if (ev.source !== frame.contentWindow) return;
    const d = ev.data;
    if (!d || typeof d !== 'object') return;
    if (d.type === 'ui-size-change') {
      const h = d.payload && d.payload.height;
      if (h) { frame.style.height = h + 'px'; window.__sizes.push(h); }
      return;
    }
    // Any non-size message == a UI action; @mcp-ui replies with ui-message-response
    // keyed by messageId after running onUIAction. Biorouter routes "prompt" to chat.
    window.__actions.push(d);
    const ok = d.type === 'prompt';
    frame.contentWindow.postMessage({
      type: 'ui-message-response',
      messageId: d.messageId,
      payload: { response: ok
        ? { status: 'success', message: 'Prompt sent to chat successfully' }
        : { status: 'error', error: { code: 'UNKNOWN_ACTION', message: 'Unknown action type' } } },
    }, '*');
  });
  window.__load = (html) => { frame.srcdoc = html; };
</script></body></html>`;

async function newHost(browser) {
  const page = await browser.newPage();
  await page.setContent(HOST_PAGE);
  return page;
}
// the artifact frame (child of the host page)
function artifactFrame(page) {
  // child frames of the host; the srcdoc artifact is the only one
  return page.frames().find((f) => f !== page.mainFrame());
}

async function main() {
  const launchOpts = process.env.PW_EXEC
    ? { executablePath: process.env.PW_EXEC, headless: true }
    : { channel: 'chrome', headless: true }; // use system Google Chrome
  const browser = await chromium.launch(launchOpts);
  try {
    // ---- Case 1: STATIC tall artifact must auto-grow ------------------------
    {
      const tall = `<!doctype html><html><head><title>Tall</title></head><body>
        <main class="br-container"><h1>Tall static artifact</h1>
        <div style="height:1800px;background:linear-gradient(#fafafa,#eee)">spacer</div>
        <p id="end">END</p></main></body></html>`;
      const html = assemblePreview(tall, { agentic: false });
      const page = await newHost(browser);
      await page.evaluate((h) => window.__load(h), html);
      await page.waitForTimeout(1200);
      const sizes = await page.evaluate(() => window.__sizes);
      const maxH = sizes.length ? Math.max(...sizes) : 0;
      log(`static: host received ${sizes.length} ui-size-change msgs, maxHeight=${maxH}`);
      check('static artifact reports its height (auto-resize works)',
        sizes.length > 0 && maxH > 1000,
        `got ${sizes.length} msgs, maxHeight=${maxH} (tall content ~1900px should be reported)`);
      await page.close();
    }

    // ---- Case 2: AGENTIC chat prompt must route + not hang ------------------
    {
      const html = assemblePreview('<!doctype html><html><head></head><body></body></html>',
        { agentic: true, agent: { systemPrompt: 'You are helpful.', greeting: 'Hi!', tools: [] } });
      const page = await newHost(browser);
      page.on('console', (m) => { const t = m.text(); if (/sandbox|form|block/i.test(t)) log('  [console] ' + t); });
      await page.evaluate((h) => window.__load(h), html);
      await page.waitForTimeout(400);
      const af = artifactFrame(page);
      check('agentic artifact mounts a chat input', !!(af && await af.$('.br-chat__form .br-input')),
        'no chat input found');
      // type + click Send (button is type=button; form submit is sandbox-blocked)
      await af.fill('.br-chat__form .br-input', 'What is 2+2?');
      await af.click('.br-chat__form button');
      await page.waitForTimeout(1500);

      const actions = await page.evaluate(() => window.__actions);
      const promptActions = actions.filter((a) => a.type === 'prompt');
      log(`agentic: host received ${actions.length} action msgs; prompt actions: ${promptActions.length}; raw types: ${JSON.stringify(actions.map(a=>a.type ?? a.method ?? '(none)'))}`);
      check('chat prompt reaches the host as a routable "prompt" action',
        promptActions.length === 1 && promptActions[0].payload && promptActions[0].payload.prompt === 'What is 2+2?',
        `prompt actions=${promptActions.length}`);

      // typing indicator must have cleared (promise settled), not spin forever
      const typingDots = await af.$$('.br-typing');
      const userMsgs = await af.$$('.br-msg--user');
      check('user message is rendered', userMsgs.length === 1, `userMsgs=${userMsgs.length}`);
      check('typing indicator clears (no infinite hang)', typingDots.length === 0,
        `${typingDots.length} typing indicators still present after 1.5s`);
      await page.close();
    }
  } finally {
    await browser.close();
  }
  log(failures === 0 ? 'ALL PASS' : `${failures} FAILURE(S)`);
  process.exit(failures === 0 ? 0 : 1);
}
main().catch((e) => { console.error('[preview-test] ERROR', e); process.exit(2); });
