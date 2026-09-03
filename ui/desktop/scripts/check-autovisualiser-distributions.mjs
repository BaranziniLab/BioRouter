import assert from 'node:assert/strict';
import { mkdir, readFile } from 'node:fs/promises';
import vm from 'node:vm';

const templates = new URL(
  '../../../crates/biorouter-mcp/src/autovisualiser/templates/',
  import.meta.url
);
const common = await readFile(new URL('_common.js', templates), 'utf8');
const source = Object.fromEntries(
  await Promise.all(
    ['gauge', 'histogram'].map(async (name) => [
      name,
      await readFile(new URL(`${name}_template.html`, templates), 'utf8'),
    ])
  )
);
let passed = 0;
let failed = 0;
function check(name, test) {
  try {
    test();
    passed++;
    console.log(`PASS ${name}`);
  } catch (error) {
    failed++;
    console.error(`FAIL ${name}: ${error.message.split('\n')[0]}`);
  }
}
const plain = (value) => JSON.parse(JSON.stringify(value));

function render(name, data) {
  const result = { drawn: [], rows: [], elements: new Map() };
  const canvas = {
    save() {},
    restore() {},
    measureText(text) {
      return {
        width: String(text).length * (parseFloat(this.font?.match(/[\d.]+px/)?.[0]) || 14) * 0.6,
      };
    },
    fillText(text) {
      result.drawn.push(String(text));
    },
  };
  const document = {
    readyState: 'loading',
    addEventListener() {},
    documentElement: { style: { setProperty() {} } },
    getElementById(id) {
      if (!result.elements.has(id))
        result.elements.set(id, {
          textContent: '',
          style: {},
          clientWidth: 296,
          setAttribute(key, value) {
            this[key] = value;
          },
          getContext() {
            return canvas;
          },
        });
      return result.elements.get(id);
    },
  };
  const window = {
    location: { search: '' },
    matchMedia: () => ({ matches: false }),
    addEventListener() {},
  };
  const context = vm.createContext({ window, document, URLSearchParams, Intl, console });
  vm.runInContext(common, context);
  const viz = window.BioRouterViz;
  Object.assign(viz, {
    autoResize() {},
    guard(fn) {
      fn();
    },
    applyPageTheme() {},
    applyScientificStyles() {},
    applyChartDefaults() {},
    reportSize() {},
    renderFigureData(_table, headers, rows, caption) {
      result.headers = plain(headers);
      result.rows = plain(Array.from(rows));
      result.caption = caption;
    },
  });
  context.BioRouterViz = viz;
  context.Chart = function (_canvas, config) {
    result.config = plain(config);
    const chart = { ctx: canvas, chartArea: { left: 0, right: 296, top: 0, bottom: 270 } };
    config.plugins?.forEach((plugin) => plugin.afterDraw?.(chart));
  };
  const token = name === 'gauge' ? '{{GAUGE_DATA}}' : '{{HISTOGRAM_DATA}}';
  const script = source[name].match(
    /<script>\s*(const (?:gaugeData|histData) =[\s\S]*?)<\/script>/
  )?.[1];
  assert.ok(script, `${name} script exists`);
  vm.runInContext(script.replace(token, JSON.stringify(data)), context);
  return result;
}

for (const [value, fractions] of [
  [150, [1, 0]],
  [-5, [0, 1]],
]) {
  check(`gauge preserves out-of-range measurement ${value}`, () => {
    const result = render('gauge', { value, min: 0, max: 100, label: 'minutes' });
    assert.ok(result.drawn.includes(String(value)), 'center must show the actual measurement');
    assert.deepEqual(result.config.data.datasets[0].data, fractions);
    assert.match(
      result.elements.get('rangeDescription')?.textContent || '',
      /outside|above|below/i
    );
  });
}
check('gauge preserves measurement precision and explicit threshold colors', () => {
  const value = 0.1234567890123;
  const result = render('gauge', {
    value,
    min: 0,
    max: 1,
    thresholds: [{ value: 1, color: '#a05a32' }],
  });
  assert.ok(result.drawn.includes(String(value)));
  assert.equal(result.config.data.datasets[0].backgroundColor[0], '#a05a32');
});
check('histogram distinguishes narrow bins without changing counts', () => {
  const result = render('histogram', { values: [1.00001, 1.00002], bins: 2 });
  assert.equal(new Set(result.config.data.labels).size, 2);
  assert.deepEqual(result.config.data.datasets[0].data, [1, 1]);
});
check('constant histogram describes one observed value, not an invented spread', () => {
  const result = render('histogram', { values: [4, 4, 4], bins: 5 });
  assert.deepEqual(result.config.data.datasets[0].data, [3]);
  assert.match(result.elements.get('binDescription')?.textContent || '', /same value|equal/i);
});
check('histogram preserves edge counts and explicit color', () => {
  const result = render('histogram', { values: [0, 1, 2, 3, 4], bins: 2, color: '#a05a32' });
  assert.deepEqual(result.config.data.datasets[0].data, [2, 3]);
  assert.equal(result.config.data.datasets[0].backgroundColor, '#a05a32');
  assert.deepEqual(
    result.rows.map((row) => row.slice(1, 4)),
    [
      [0, 2, 2],
      [2, 4, 3],
    ]
  );
});
check('gauge represents a finite range whose span exceeds the number limit', () => {
  const result = render('gauge', { value: 0, min: -1e308, max: 1e308 });
  assert.deepEqual(result.config.data.datasets[0].data, [0.5, 0.5]);
});
check('histogram bins extreme finite values without losing observations', () => {
  const result = render('histogram', { values: [-1e308, 0, 1e308], bins: 2 });
  assert.deepEqual(result.config.data.datasets[0].data, [1, 2]);
  assert.ok(result.rows.every((row) => row.every((value) => Number.isFinite(value))));
});
check('histogram avoids zero-width bins for adjacent floating-point values', () => {
  const result = render('histogram', { values: [1, 1 + Number.EPSILON], bins: 200 });
  assert.ok(result.rows.every((row) => row[2] > row[1]));
  assert.equal(
    result.config.data.datasets[0].data.reduce((sum, count) => sum + count, 0),
    2
  );
  assert.match(result.elements.get('binDescription')?.textContent || '', /precision/i);
});
for (const name of ['gauge', 'histogram']) {
  check(`${name} uses the shared academic shell and accessible data`, () => {
    assert.match(source[name], /BioRouterViz\.applyScientificStyles\(\)/);
    assert.match(source[name], /BioRouterViz\.renderFigureData/);
    assert.match(source[name], /role="img"/);
    assert.match(source[name], /tabindex="0" role="region"/);
    assert.doesNotMatch(source[name], /box-shadow|linear-gradient/);
  });
}
if (process.argv.includes('--browser')) {
  const { chromium } = await import('@playwright/test');
  const output = process.env.BIOROUTER_FIGURE_OUTPUT || '/tmp/biorouter-distributions-figures';
  await mkdir(output, { recursive: true });
  const chart = await readFile(new URL('assets/chart.min.js', templates), 'utf8');
  const longLabel =
    'Synthetic measurement 東京 Δοκιμή with a deliberately extended descriptive quantity (minutes/day)';
  const fixtures = {
    gauge: {
      title: 'Observed measurement outside the configured range',
      value: 150,
      min: 0,
      max: 100,
      label: longLabel,
    },
    histogram: {
      title: 'Distribution of synthetic observations',
      values: [0, 1, 2, 3, 4],
      bins: 2,
      xAxisLabel: longLabel,
      yAxisLabel: 'Number of observations',
      color: '#a05a32',
    },
  };
  const browser = await chromium.launch({ headless: true });
  try {
    for (const [name, data] of Object.entries(fixtures)) {
      for (const theme of ['light', 'dark']) {
        const page = await browser.newPage({
          viewport: { width: 320, height: 1000 },
          colorScheme: theme,
        });
        const errors = [];
        page.on('pageerror', (error) => errors.push(error.message));
        page.on('dialog', async (dialog) => {
          errors.push('Unexpected script dialog');
          await dialog.dismiss();
        });
        await page.route('**/*', async (route) => {
          errors.push('Unexpected network request');
          await route.abort();
        });
        async function load(input) {
          await page.goto('about:blank');
          const html = source[name]
            .replace(
              '{{ASSETS}}',
              () => `<script>${chart}</script><script>
              window.figureTextDraws = [];
              const originalFillText = CanvasRenderingContext2D.prototype.fillText;
              CanvasRenderingContext2D.prototype.fillText = function(text, x, y, ...rest) {
                window.figureTextDraws.push({ text: String(text), x, y, font: this.font, width: this.measureText(text).width });
                return originalFillText.call(this, text, x, y, ...rest);
              };
            </script>`
            )
            .replace('{{COMMON}}', () => common)
            .replace(name === 'gauge' ? '{{GAUGE_DATA}}' : '{{HISTOGRAM_DATA}}', () =>
              JSON.stringify(input).replaceAll('<', '\\u003c')
            );
          await page.setContent(html, { waitUntil: 'load' });
          await page.waitForFunction(
            () => document.querySelectorAll('#figureData tbody tr').length > 0
          );
          await page.locator('summary').click();
          await page.waitForTimeout(200);
        }
        await load(data);
        for (const width of [320, 1200, 480]) {
          await page.setViewportSize({ width, height: 1000 });
          await page.waitForTimeout(200);
          const metrics = await page.evaluate(() => {
            const canvas = document.getElementById('chart');
            const chart = Chart.getChart(canvas);
            const title = document.querySelector('h1');
            const fonts = Object.values(chart.scales).map(
              (scale) => scale._resolveTickFontOptions(0).size
            );
            const axisTitles = Object.values(chart.scales)
              .filter((scale) => scale.options.title.display)
              .map((scale) => {
                chart.ctx.save();
                chart.ctx.font =
                  (scale.options.title.font?.size || Chart.defaults.font.size) +
                  'px ' +
                  Chart.defaults.font.family;
                const lines = Array.isArray(scale.options.title.text)
                  ? scale.options.title.text
                  : [scale.options.title.text];
                const fits = lines.every(
                  (line) =>
                    chart.ctx.measureText(line).width <=
                    (scale.isHorizontal() ? scale.width : scale.height)
                );
                chart.ctx.restore();
                return fits;
              });
            return {
              pageFits: document.documentElement.scrollWidth <= innerWidth + 1,
              canvasFits: canvas.getBoundingClientRect().right <= innerWidth + 1,
              titleSize: parseFloat(getComputedStyle(title).fontSize),
              fonts,
              axisTitles,
              plotWidth: chart.chartArea.width,
              plotHeight: chart.chartArea.height,
              rows: document.querySelectorAll('#figureData tbody tr').length,
              imageRole: canvas.getAttribute('role'),
              alerts: [...document.querySelectorAll('[role="alert"]')].map(
                (element) => element.textContent
              ),
              gradients: [...document.querySelectorAll('*')].some((element) =>
                getComputedStyle(element).backgroundImage.includes('gradient')
              ),
              values: chart.data.datasets[0].data,
            };
          });
          check(`${name} ${theme} ${width}px academic layout`, () => {
            assert.deepEqual(errors, []);
            assert.ok(metrics.pageFits && metrics.canvasFits, 'no page overflow');
            assert.ok(metrics.titleSize >= 18 && metrics.titleSize <= 22);
            assert.ok(
              metrics.fonts.every((size) => size >= 14),
              'readable tick fonts'
            );
            assert.ok(
              metrics.axisTitles.every(Boolean),
              'axis quantity and units must fit without clipping'
            );
            assert.ok(metrics.plotWidth > 80 && metrics.plotHeight > 90, 'plot remains usable');
            assert.equal(metrics.imageRole, 'img');
            assert.deepEqual(metrics.alerts, [], 'no visible rendering error cards');
            assert.equal(metrics.gradients, false);
            assert.deepEqual(metrics.values, name === 'gauge' ? [1, 0] : [2, 3]);
            assert.equal(metrics.rows, name === 'gauge' ? 4 : 2);
          });
          await page.screenshot({
            path: `${output}/${name}-${theme}-${width}.png`,
            fullPage: true,
          });
          if (width === 320) {
            const table = page.locator('.table-scroll');
            await table.focus();
            await page.keyboard.press('End');
            await page.keyboard.press('ArrowRight');
            await page.waitForTimeout(120);
            const scrolled = await table.evaluate((element) => ({
              scroll: element.scrollLeft,
              focused: document.activeElement === element,
            }));
            check(`${name} ${theme} accessible horizontal table`, () => {
              assert.ok(scrolled.scroll > 0 && scrolled.focused);
            });
            if (name === 'histogram') {
              await page.locator('canvas').scrollIntoViewIfNeeded();
              const point = await page.evaluate(() => {
                const canvas = document.getElementById('chart');
                const point = Chart.getChart(canvas).getDatasetMeta(0).data[0].getCenterPoint();
                const rect = canvas.getBoundingClientRect();
                return { x: rect.left + point.x, y: rect.top + point.y };
              });
              await page.mouse.move(point.x, point.y);
              await page.waitForTimeout(80);
              const tooltip = await page.evaluate(() => {
                const chart = Chart.getChart(document.getElementById('chart'));
                const t = chart.tooltip;
                return {
                  opacity: t.opacity,
                  x: t.x,
                  y: t.y,
                  width: t.width,
                  height: t.height,
                  chartWidth: chart.width,
                  chartHeight: chart.height,
                };
              });
              check(`histogram ${theme} tooltip fits`, () => {
                assert.equal(tooltip.opacity, 1);
                assert.ok(tooltip.x >= -1 && tooltip.x + tooltip.width <= tooltip.chartWidth + 1);
                assert.ok(tooltip.y >= -1 && tooltip.y + tooltip.height <= tooltip.chartHeight + 1);
              });
            }
          }
        }
        await page.setViewportSize({ width: 320, height: 1000 });
        const edgeCases =
          name === 'gauge'
            ? [
                { value: -5, min: 0, max: 100 },
                { value: 0.1234567890123, min: 0, max: 1 },
                { value: 0, min: -1e308, max: 1e308 },
              ]
            : [
                { values: [1.00001, 1.00002], bins: 2 },
                { values: [4, 4, 4], bins: 5 },
                { values: [-1e308, 0, 1e308], bins: 2 },
                { values: [1, 1 + Number.EPSILON], bins: 200 },
              ];
        for (const [index, edge] of edgeCases.entries()) {
          await load({ ...edge, title: 'Literal label <img src=x onerror=alert(1)> 東京' });
          const observed = await page.evaluate(() => {
            const chart = Chart.getChart(document.getElementById('chart'));
            return {
              labels: chart.data.labels,
              values: chart.data.datasets[0].data,
              table: document.querySelector('#figureData').textContent,
              images: document.querySelectorAll('img').length,
              drawn: window.figureTextDraws,
              width: chart.width,
            };
          });
          check(`${name} ${theme} edge fixture ${index + 1}`, () => {
            assert.deepEqual(errors, []);
            assert.equal(observed.images, 0);
            if (name === 'histogram') {
              assert.deepEqual(observed.values, [[1, 1], [3], [1, 2], [2]][index]);
              assert.equal(new Set(observed.labels).size, observed.labels.length);
            } else {
              assert.ok(
                observed.table.includes(String(edge.value)),
                'exact measurement remains available'
              );
              const measurement = observed.drawn.findLast(
                (item) => item.text === String(edge.value)
              );
              assert.ok(measurement, 'actual value is drawn, not only placed in the table');
              assert.ok(
                measurement.x - measurement.width / 2 >= 0 &&
                  measurement.x + measurement.width / 2 <= observed.width
              );
              assert.ok(parseFloat(measurement.font.match(/[\d.]+px/)[0]) >= 14);
              if (index === 2) assert.deepEqual(observed.values, [0.5, 0.5]);
            }
          });
          await page.screenshot({
            path: `${output}/${name}-${theme}-edge-${index + 1}.png`,
            fullPage: true,
          });
        }
        await page.close();
      }
    }
  } finally {
    await browser.close();
  }
}
console.log(`${passed} passed; ${failed} failed`);
process.exitCode = failed ? 1 : 0;
