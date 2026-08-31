import assert from 'node:assert/strict';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import vm from 'node:vm';

// Pure VM checks run by default; browser geometry requires the separate --browser lease.
const root = new URL('../../../crates/biorouter-mcp/src/autovisualiser/', import.meta.url);
const read = (path) => readFile(new URL(path, root), 'utf8');
const mermaidTemplate = await read('templates/mermaid_template.html');
const dashboardTemplate = await read('templates/dashboard_template.html');
const common = await read('templates/_common.js');
const results = [];
async function check(name, run) {
  try {
    await run();
    results.push(true);
    console.log(`PASS ${name}`);
  } catch (error) {
    results.push(false);
    console.error(`FAIL ${name}: ${error.message.replaceAll('\n', ' ').slice(0, 600)}`);
  }
}
const literal = 'Δοκιμή 東京 👩🏽‍🔬 <img src=x onerror=alert(1)> </script>';
const js = (data) =>
  JSON.stringify(data)
    .replaceAll('<', '\\u003c')
    .replaceAll('>', '\\u003e')
    .replaceAll('&', '\\u0026');
const escapeHtml = (text) =>
  text
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
const lastScript = (html) => [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)].at(-1)[1];
async function mermaidVm({ fail = false } = {}) {
  const nodes = new Map();
  const svg = {
    style: {},
    attrs: {},
    viewBox: { baseVal: { width: 1800, height: 400 } },
    getAttribute(name) {
      return name === 'viewBox' ? '0 0 1800 400' : null;
    },
    setAttribute(name, value) {
      this.attrs[name] = value;
    },
  };
  const node = (id) => {
    if (!nodes.has(id))
      nodes.set(id, { textContent: '', style: {}, setAttribute() {}, querySelector: () => svg });
    return nodes.get(id);
  };
  const errors = [];
  let config;
  const context = vm.createContext({
    document: { getElementById: node },
    console,
    setInterval,
    clearInterval,
    BioRouterViz: {
      dark: false,
      colors: { bg: '#fff', text: '#222', muted: '#666' },
      palette: ['#416b80'],
      autoResize() {},
      applyScientificStyles() {},
      hideOverlappingSvgLabels() {},
      reportSize() {},
      guard(fn) {
        fn();
      },
      showError(message) {
        errors.push(message);
      },
    },
    mermaid: {
      initialize(value) {
        config = value;
      },
      render: async () => {
        if (fail) throw new Error('Synthetic invalid source');
        return { svg: '<svg></svg>' };
      },
    },
  });
  vm.runInContext(lastScript(mermaidTemplate).replace('{{MERMAID_CODE}}', js(literal)), context);
  for (let i = 0; i < 8; i++) await Promise.resolve();
  return { config, nodes, svg, errors };
}
await check('Mermaid retains strict security', async () =>
  assert.equal((await mermaidVm()).config.securityLevel, 'strict')
);
await check(
  'Mermaid source alternative preserves literal Unicode and hostile-looking text',
  async () => assert.equal((await mermaidVm()).nodes.get('diagramSource')?.textContent, literal)
);
await check('Mermaid intrinsic viewport preserves readable large-figure dimensions', async () => {
  const { svg } = await mermaidVm();
  assert.equal(svg.style.width, '1800px');
  assert.equal(svg.style.height, '400px');
  assert.match(mermaidTemplate, /tabindex="0" role="region"/);
});
await check('Mermaid render failure leaves source available and shows one error', async () => {
  const result = await mermaidVm({ fail: true });
  assert.equal(result.errors.length, 1);
  assert.equal(result.nodes.get('diagramSource')?.textContent, literal);
});
await check('Mermaid defaults use app font at a legible size', async () => {
  const { config } = await mermaidVm();
  assert.ok(parseFloat(config.themeVariables?.fontSize) >= 16);
  assert.match(config.themeVariables.fontFamily, /system|apple/i);
});
function expandVm(full) {
  const buttons = [],
    classes = new Set(full ? ['span-full'] : []);
  const fig = {
    classList: {
      contains: (name) => classes.has(name),
      toggle(name) {
        if (classes.has(name)) {
          classes.delete(name);
          return false;
        }
        classes.add(name);
        return true;
      },
    },
  };
  const context = vm.createContext({
    document: {
      createElement: () => {
        const button = {
          attrs: {},
          setAttribute(k, v) {
            this.attrs[k] = String(v);
          },
          addEventListener(k, fn) {
            this[k] = fn;
          },
        };
        buttons.push(button);
        return button;
      },
    },
    fig,
    ICON_EXPAND: 'expand',
    ICON_COLLAPSE: 'collapse',
    window: { setTimeout() {} },
    frame: {},
  });
  const start = dashboardTemplate.indexOf("var expandBtn = document.createElement('button');");
  vm.runInContext(
    dashboardTemplate.slice(
      start,
      dashboardTemplate.indexOf('actions.appendChild(expandBtn);', start)
    ),
    context
  );
  return buttons[0];
}
for (const initial of [false, true])
  await check(
    `Dashboard initial ${initial ? 'full' : 'half'} width and keyboard control state agree`,
    () => {
      const button = expandVm(initial);
      assert.equal(button.attrs['aria-expanded'], String(initial));
      assert.equal(button.attrs['aria-label'], button.title);
      button.click();
      assert.equal(button.attrs['aria-expanded'], String(!initial));
      assert.equal(button.attrs['aria-label'], button.title);
    }
  );
await check('Dashboard prose escapes HTML and excludes javascript links', () => {
  const start = dashboardTemplate.indexOf('function escapeHtml(');
  const context = vm.createContext({});
  vm.runInContext(
    dashboardTemplate.slice(start, dashboardTemplate.indexOf('function decodeB64(', start)),
    context
  );
  const host = {};
  context.renderProse(
    host,
    literal + ' [unsafe](javascript:alert(1)) [safe](https://example.test/abc)'
  );
  assert.ok(!host.innerHTML.includes('<img'));
  assert.ok(!host.innerHTML.includes('href="javascript:'));
  assert.ok(host.innerHTML.includes('rel="noopener noreferrer"'));
});
await check('Dashboard narrow contents tracks fit their available width', () =>
  assert.match(dashboardTemplate, /minmax\(min\(100%,\s*260px\),\s*1fr\)/)
);
await check('Dashboard long titles and prose can wrap without decorative shadows', () => {
  assert.match(dashboardTemplate, /overflow-wrap:\s*anywhere/);
  assert.ok(!dashboardTemplate.includes('0 8px 24px'));
});
await check('Dashboard example has real synthetic observations', async () => {
  const source = await read('tools_dashboard.rs');
  const example = JSON.parse(source.split('Example:\n')[1].split('\n\nPanel width')[0]);
  assert.match(example.title, /synthetic/i);
  for (const section of example.sections)
    for (const panel of section.panels) assert.ok(panel.figure.params.data.datasets?.length > 0);
});

const fixtures = {
  raw: 'flowchart LR\n A["Measured input"] --> B["Independent analysis"] --> C["Reported result"]',
  flowchart:
    'flowchart TD\n A["Δοκιμή 東京"] --> B{"Quality checked?"}\n B -->|Yes| C["Retain"]\n B -->|No| D["Review"]',
  gantt:
    'gantt\n title Synthetic schedule\n dateFormat YYYY-MM-DD\n section Analysis\n Measure :a, 2026-01-01, 2d\n Review :b, after a, 3d',
  sequence:
    'sequenceDiagram\n participant A as Researcher\n participant B as Independent reviewer\n A->>B: Submit observations\n B-->>A: Return assessment',
  mindmap:
    'mindmap\n root((Synthetic research))\n  Observations\n   Measured\n  Interpretation\n   Uncertainty',
  timeline:
    'timeline\n title Synthetic history\n 2024 : Observations\n 2025 : Analysis\n 2026 : Review',
  er: 'erDiagram\n PERSON ||--o{ OBSERVATION : contributes\n PERSON {\n string id PK\n }\n OBSERVATION {\n float value\n }',
  state:
    'stateDiagram-v2\n [*] --> Pending\n Pending --> Reviewing\n Reviewing --> Complete\n Complete --> [*]',
  class:
    'classDiagram\n class Observation {\n +float value\n +validate() bool\n }\n class Review\n Review --> Observation',
};
function verifyStateNodes(data, source) {
  const expected = ['br_412d42', 'br_412042', 'br_415f42'];
  assert.deepEqual(data.nodes.map((node) => node.id).sort(), [...expected].sort());
  for (const [i, label] of ['A-B', 'A B', 'A_B'].entries())
    assert.equal(data.nodes.find((node) => node.id === expected[i]).label, label);
  assert.ok(data.markers.includes('root_start') && data.markers.includes('root_end'));
  for (const edge of [
    '[*] --> br_412d42',
    'br_412d42 --> br_412042',
    'br_412042 --> br_415f42',
    'br_415f42 --> [*]',
  ])
    assert.ok(source.includes(edge), `Missing transition ${edge}`);
}
await check(
  'State acceptance rejects collapsed identities rather than accepting repeated labels',
  () => {
    assert.throws(() =>
      verifyStateNodes({ nodes: [{ id: 'A_B', label: 'A_B' }], markers: [] }, 'A_B --> A_B')
    );
  }
);
const only = process.argv
  .find((arg) => arg.startsWith('--only='))
  ?.slice(7)
  .split(',');
const typedHtml = {};
if (process.env.BIOROUTER_MERMAID_FIXTURE_DIR) {
  for (const name of Object.keys(fixtures)) {
    typedHtml[name] = await readFile(
      `${process.env.BIOROUTER_MERMAID_FIXTURE_DIR}/${name}.html`,
      'utf8'
    );
    fixtures[name] = JSON.parse(
      typedHtml[name].match(/const mermaidCode = ("(?:[^"\\]|\\.)*");/)[1]
    );
  }
}
if (process.argv.includes('--browser')) {
  const { chromium } = await import('@playwright/test');
  const asset = await read('templates/assets/mermaid.min.js');
  const output = process.env.BIOROUTER_FIGURE_OUTPUT;
  if (output) await mkdir(output, { recursive: true });
  const mermaidHtml = (source, title = 'Synthetic diagram') =>
    mermaidTemplate
      .replaceAll('{{TITLE}}', () => escapeHtml(title))
      .replace('{{ASSETS}}', () => `<script>${asset}</script>`)
      .replace('{{COMMON}}', () => common)
      .replace('{{MERMAID_CODE}}', () => js(source));
  const browser = await chromium.launch({ headless: true });
  try {
    for (const [name, source] of Object.entries(fixtures).filter(
      ([name]) => !only || only.includes(name)
    ))
      for (const theme of ['light', 'dark']) {
        const page = await browser.newPage({
          viewport: { width: 320, height: 900 },
          colorScheme: theme,
        });
        const errors = [],
          requests = [];
        page.on('pageerror', (error) => errors.push(error.message));
        await page.route('**/*', (route) => {
          requests.push(route.request().url());
          return route.abort();
        });
        await page.setContent(typedHtml[name] || mermaidHtml(source, `${name}: ${literal}`));
        await page
          .waitForSelector('#mermaidTarget svg, [role="alert"]', { timeout: 5000 })
          .catch(async (error) => {
            throw new Error(`${name}: ${error.message}; page errors: ${errors.join('; ')}`);
          });
        for (const width of [320, 480, 1200]) {
          await page.setViewportSize({ width, height: 900 });
          await check(
            `${name} ${theme} ${width}: readable source, bounded figure, no render alerts`,
            async () => {
              assert.deepEqual(errors, []);
              assert.deepEqual(requests, []);
              const metrics = await page.evaluate(() => ({
                alerts: [...document.querySelectorAll('[role="alert"]')].map(
                  (el) => el.textContent
                ),
                source: document.getElementById('diagramSource')?.textContent,
                width: document.documentElement.scrollWidth,
                viewport: innerWidth,
                font: Math.min(
                  ...[...document.querySelectorAll('#mermaidTarget text,#mermaidTarget .nodeLabel')]
                    .filter((el) => el.textContent.trim())
                    .map((el) => parseFloat(getComputedStyle(el).fontSize))
                ),
                small: [...document.querySelectorAll('#mermaidTarget text')]
                  .filter((el) => parseFloat(getComputedStyle(el).fontSize) < 14)
                  .map((el) => [el.getAttribute('class'), el.textContent]),
              }));
              if (output)
                await page.screenshot({
                  path: `${output}/${name}-${theme}-${width}.png`,
                  fullPage: true,
                });
              assert.deepEqual(metrics.alerts, []);
              assert.equal(metrics.source, source);
              assert.ok(metrics.width <= metrics.viewport + 1);
              assert.ok(
                Number.isFinite(metrics.font) && metrics.font >= 14,
                JSON.stringify(metrics)
              );
              assert.equal(await page.locator('#mermaidTarget svg').count(), 1);
              if (name === 'gantt')
                assert.ok(
                  await page.evaluate(() => {
                    const boxes = [...document.querySelectorAll('.grid .tick text')]
                      .filter((el) => el.textContent)
                      .map((el) => el.getBoundingClientRect());
                    return boxes.every((box, i) =>
                      boxes
                        .slice(i + 1)
                        .every(
                          (other) =>
                            box.right <= other.left ||
                            other.right <= box.left ||
                            box.bottom <= other.top ||
                            other.bottom <= box.top
                        )
                    );
                  }),
                  'Date ticks overlap'
                );
            }
          );
        }
        if (typedHtml[name] && !['raw', 'mindmap', 'timeline'].includes(name)) {
          await check(
            `${name} ${theme}: actual typed compiler output preserves three identities and references`,
            async () => {
              const data = await page.evaluate(
                async ({ source, name }) => {
                  if (name === 'state') {
                    const nodes = [...document.querySelectorAll('#mermaidTarget g.node')].map(
                      (node) => ({ id: node.getAttribute('data-id'), label: node.textContent })
                    );
                    return {
                      nodes: nodes.filter((node) => node.id.startsWith('br_')),
                      markers: nodes.map((node) => node.id),
                    };
                  }
                  const diagram = await mermaid.mermaidAPI.getDiagramFromText(source);
                  const keys = (value) =>
                    value instanceof Map ? [...value.keys()] : Object.keys(value);
                  const getters = {
                    flowchart: 'getVertices',
                    sequence: 'getActors',
                    er: 'getEntities',
                    class: 'getClasses',
                    state: 'getStates',
                  };
                  if (name === 'gantt')
                    return {
                      tasks: diagram.db
                        .getTasks()
                        .map((task) => ({
                          id: task.id,
                          start: +task.startTime,
                          end: +task.endTime,
                        })),
                    };
                  return {
                    ids: keys(diagram.db[getters[name]]()).filter((id) => id.startsWith('br_')),
                    visible: document.getElementById('mermaidTarget').textContent,
                  };
                },
                { source, name }
              );
              const ids = ['br_412d42', 'br_412042', 'br_415f42'];
              if (name === 'gantt') {
                assert.equal(data.tasks.length, 4);
                const byId = Object.fromEntries(data.tasks.map((task) => [task.id, task]));
                assert.equal(byId[ids[1]].start, byId[ids[0]].end);
                assert.equal(byId[ids[2]].start, byId[ids[1]].end);
                assert.equal(byId.br_auto_3.start, Math.max(byId[ids[0]].end, byId[ids[2]].end));
              } else if (name === 'state') {
                verifyStateNodes(data, source);
              } else {
                assert.deepEqual(data.ids.sort(), ids.sort());
                for (const label of ['A-B', 'A B', 'A_B'])
                  assert.ok(data.visible.includes(label), `Missing visible ${label}`);
              }
            }
          );
        }
        await page.close();
      }
    if (!only || only.includes('security')) {
      const page = await browser.newPage();
      await page.route('**/*', (route) => route.abort());
      const securityErrors = [],
        dialogs = [];
      page.on('pageerror', (error) => securityErrors.push(error.message));
      page.on('dialog', (dialog) => {
        dialogs.push(dialog.message());
        dialog.dismiss();
      });
      for (const source of [
        'not a diagram <img src=x onerror=alert(1)>',
        '%%{init: {"securityLevel": "loose"}}%%\nflowchart TD\n A["<img src=x onerror=alert(1)>"] --> B\n click A "javascript:alert(1)"',
      ]) {
        await page.goto('about:blank');
        await page.setContent(mermaidHtml(source));
        await page.waitForSelector('#mermaidTarget svg,[role="alert"]', { timeout: 5000 });
        await check(
          'Mermaid malformed or hostile source remains literal and cannot execute',
          async () => {
            assert.equal(await page.locator('#diagramSource').textContent(), source);
            assert.deepEqual(securityErrors, []);
            assert.deepEqual(dialogs, []);
            const unsafe = await page
              .locator('#diagramSource img,[onerror],[onclick],a[href^="javascript:"]')
              .evaluateAll((nodes) => nodes.map((node) => node.outerHTML));
            assert.deepEqual(unsafe, []);
            const alerts = await page.locator('[role="alert"]').count();
            assert.equal(alerts, source.startsWith('not') ? 1 : 0);
          }
        );
      }
      await page.close();
    }
    for (const theme of !only || only.includes('dashboard') ? ['light', 'dark'] : []) {
      const child = mermaidHtml(fixtures.flowchart);
      const chartAsset = await read('templates/assets/chart.min.js');
      const chart = (await read('templates/chart_template.html'))
        .replace('{{COMMON}}', () => common)
        .replace('{{ASSETS}}', () => `<script>${chartAsset}</script>`)
        .replace('{{CHART_DATA}}', () =>
          js({
            type: 'bar',
            title: 'Synthetic measurements',
            labels: ['A', 'B'],
            datasets: [{ label: 'Observation (units)', data: [10, 20] }],
          })
        );
      const data = {
        title: 'Synthetic multi-panel report ' + literal,
        summary: literal,
        sections: [
          {
            title: 'Observed examples',
            panels: [
              {
                index: 0,
                assets: [],
                title: 'Flowchart ' + literal,
                width: 'half',
                notes: literal,
                caption: 'A synthetic workflow.',
              },
              { index: 1, assets: [], title: 'Tall fixed-height figure', height: 220 },
              {
                index: 2,
                assets: [],
                title: 'Measured values',
                caption: 'Synthetic A=10; B=20.',
                width: 'half',
              },
              {
                index: 3,
                assets: [],
                title: 'Expected partial failure',
                error: 'Synthetic invalid input ' + literal,
              },
            ],
          },
        ],
      };
      const tall =
        '<html><head><script>' +
        common +
        '</script><style>body{color:var(--text);background:var(--bg);font:14px/1.4 -apple-system,sans-serif}</style></head><body><div style="height:1900px">Tall synthetic content</div><p id="last-row">Last observation</p></body></html>';
      const store = [child, tall, chart]
        .map(
          (html, index) =>
            `<script type="text/plain" id="autovis-panel-${index}">${Buffer.from(html).toString('base64')}</script>`
        )
        .join('');
      const html = dashboardTemplate
        .replace('{{COMMON}}', () => common)
        .replaceAll('{{TITLE}}', 'Synthetic report')
        .replace('{{ASSET_STORE}}', '')
        .replace('{{PANEL_STORE}}', () => store)
        .replace('{{DASHBOARD_DATA}}', () => js(data));
      const page = await browser.newPage({
        viewport: { width: 320, height: 900 },
        colorScheme: theme,
      });
      const errors = [];
      page.on('pageerror', (error) => errors.push(error.message));
      await page.route('**/*', (route) => route.abort());
      await page.setContent(html);
      await page.locator('iframe').first().scrollIntoViewIfNeeded();
      await page
        .frameLocator('iframe')
        .first()
        .locator('#mermaidTarget svg')
        .waitFor({ timeout: 5000 });
      await check(
        `dashboard ${theme}: hydrated child has no errors and shares report theme`,
        async () => {
          assert.equal(
            await page.frameLocator('iframe').first().locator('[role="alert"]').count(),
            0
          );
          assert.equal(await page.frames()[1].evaluate(() => BioRouterViz.theme), theme);
        }
      );
      const tallFrame = page.locator('iframe').nth(1);
      await tallFrame.scrollIntoViewIfNeeded();
      await page
        .frameLocator('iframe')
        .nth(1)
        .locator('#last-row')
        .waitFor({ state: 'attached', timeout: 3000 });
      await check(
        `dashboard ${theme}: fixed-height long panel is reachable with user scrolling`,
        async () => {
          const box = await tallFrame.boundingBox();
          await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
          await page.mouse.wheel(0, 2100);
          await page.waitForTimeout(100);
          const position = await page.frames()[2].evaluate(() => ({
            y: scrollY,
            bottom: document.getElementById('last-row').getBoundingClientRect().bottom,
            height: innerHeight,
          }));
          assert.ok(
            position.y > 0 && position.bottom <= position.height + 2,
            JSON.stringify(position)
          );
        }
      );
      await check(`dashboard ${theme}: foreign resize messages cannot change a panel`, async () => {
        const before = await page
          .locator('iframe')
          .first()
          .evaluate((frame) => frame.style.height);
        await page.evaluate(() =>
          window.postMessage({ type: 'ui-size-change', payload: { height: 1500 } }, '*')
        );
        await page.waitForTimeout(30);
        assert.equal(
          await page
            .locator('iframe')
            .first()
            .evaluate((frame) => frame.style.height),
          before
        );
      });
      await page.locator('iframe').nth(2).scrollIntoViewIfNeeded();
      await page.frameLocator('iframe').nth(2).locator('#mainChart').waitFor({ timeout: 3000 });
      await check(
        `dashboard ${theme}: mixed chart panel preserves data and renders without alerts`,
        async () => {
          const element = await page.locator('iframe').nth(2).elementHandle();
          const frame = await element.contentFrame();
          const data = await frame.evaluate(() => ({
            alerts: document.querySelectorAll('[role="alert"]').length,
            values: Chart.getChart('mainChart')?.data.datasets[0].data,
          }));
          assert.equal(data.alerts, 0);
          assert.deepEqual(data.values, [10, 20]);
        }
      );
      await page.evaluate(() => scrollTo(0, 0));
      for (const width of [320, 480, 1200]) {
        await page.setViewportSize({ width, height: 900 });
        await check(
          `dashboard ${theme} ${width}: intentional partial failure only, readable narrow report`,
          async () => {
            assert.deepEqual(errors, []);
            assert.equal(await page.locator('[role="alert"]').count(), 1);
            assert.equal(await page.locator('img').count(), 0);
            assert.ok(
              await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)
            );
            const button = page.locator('.icon-btn').first();
            const before = await button.getAttribute('aria-expanded');
            await button.focus();
            await page.keyboard.press('Enter');
            assert.notEqual(await button.getAttribute('aria-expanded'), before);
            assert.equal(
              await button.getAttribute('aria-label'),
              await button.getAttribute('title')
            );
            if (output)
              await page.screenshot({
                path: `${output}/dashboard-${theme}-${width}.png`,
                fullPage: true,
              });
          }
        );
      }
      await page.close();
    }
  } finally {
    await browser.close();
  }
}
const summary = `${results.filter(Boolean).length} passed, ${results.filter((result) => !result).length} failed`;
console.log(summary);
if (process.env.BIOROUTER_FIGURE_OUTPUT)
  await writeFile(`${process.env.BIOROUTER_FIGURE_OUTPUT}/result.txt`, summary + '\n');
process.exitCode = results.every(Boolean) ? 0 : 1;
