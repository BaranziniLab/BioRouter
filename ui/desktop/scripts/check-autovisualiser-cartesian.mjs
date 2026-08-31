import assert from 'node:assert/strict';
import { mkdir, readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import vm from 'node:vm';

const templates = new URL(
  '../../../crates/biorouter-mcp/src/autovisualiser/templates/',
  import.meta.url
);
const require = createRequire(import.meta.url);
const actualChart = require(new URL('assets/chart.min.js', templates).pathname);
const common = await readFile(new URL('_common.js', templates), 'utf8');
const source = Object.fromEntries(
  await Promise.all(
    ['area', 'bubble', 'volcano', 'manhattan'].map(async (name) => [
      name,
      await readFile(new URL(`${name}_template.html`, templates), 'utf8'),
    ])
  )
);
let passed = 0;
let failed = 0;
function check(name, fn) {
  try {
    fn();
    passed++;
    console.log(`PASS ${name}`);
  } catch (error) {
    failed++;
    console.error(`FAIL ${name}: ${error.message.split('\n')[0]}`);
  }
}
const plain = (value) => JSON.parse(JSON.stringify(value));
check('automatic size reporting never mutates overflow hints during observer delivery', () => {
  let observer;
  const frames = [];
  let reports = 0;
  const window = {
    location: { search: '' },
    matchMedia: () => ({ matches: false }),
    addEventListener() {},
  };
  window.parent = window;
  const context = vm.createContext({
    window,
    URLSearchParams,
    Intl,
    console,
    setTimeout() {},
    document: {
      readyState: 'loading',
      addEventListener() {},
      body: {},
      documentElement: { style: { setProperty() {} } },
      querySelectorAll() {
        reports++;
        return [];
      },
    },
    ResizeObserver: class {
      constructor(callback) {
        observer = callback;
      }
      observe() {}
    },
    requestAnimationFrame(callback) {
      frames.push(callback);
    },
  });
  vm.runInContext(common, context);
  window.BioRouterViz.autoResize();
  observer();
  observer();
  assert.equal(reports, 0);
  assert.equal(frames.length, 1);
  frames.shift()();
  assert.equal(reports, 1);
});
check('chart width observer defers and coalesces layout mutations outside delivery', () => {
  let observer;
  const frames = [];
  const host = { clientWidth: 320 };
  let layouts = 0;
  const window = {
    location: { search: '' },
    matchMedia: () => ({ matches: false }),
    addEventListener() {},
  };
  const context = vm.createContext({
    window,
    URLSearchParams,
    Intl,
    console,
    document: {
      readyState: 'loading',
      addEventListener() {},
      documentElement: { style: { setProperty() {} } },
    },
    ResizeObserver: class {
      constructor(callback) {
        observer = callback;
      }
      observe() {}
    },
    requestAnimationFrame(callback) {
      frames.push(callback);
    },
  });
  vm.runInContext(common, context);
  window.BioRouterViz.observeChartWidth(host, () => layouts++);
  host.clientWidth = 480;
  observer();
  observer();
  assert.equal(layouts, 0);
  assert.equal(frames.length, 1);
  frames.shift()();
  assert.equal(layouts, 1);
  observer();
  assert.equal(frames.length, 0);
});
function render(name, data) {
  const result = { elements: new Map(), rows: [], legend: [] };
  const canvas = {
    save() {},
    restore() {},
    measureText: (text) => ({ width: String(text).length * 8 }),
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
          clientWidth: 640,
          getContext: () => canvas,
          setAttribute() {},
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
  Object.assign(window.BioRouterViz, {
    guard(fn) {
      fn();
    },
    autoResize() {},
    reportSize() {},
    applyPageTheme() {},
    applyScientificStyles() {},
    applyChartDefaults() {},
    renderFigureData(_element, headers, rows, caption) {
      result.rows = plain(Array.from(rows));
      result.headers = plain(headers);
      result.caption = caption;
    },
    renderFigureLegend(_element, items) {
      result.legend = plain(items);
    },
    renderChartLegend(_element, chart) {
      result.legend = plain(chart.data.datasets.map((dataset) => dataset.label));
    },
  });
  context.BioRouterViz = window.BioRouterViz;
  context.Chart = function (_canvas, config) {
    result.config = config;
    this.data = config.data;
    this.options = config.options;
  };
  context.Chart.helpers = actualChart.helpers;
  context.Chart.defaults = actualChart.defaults;
  const script = source[name].match(/<script>\s*(const \w+Data =[\s\S]*?)<\/script>/)?.[1];
  assert.ok(script);
  vm.runInContext(
    script.replace(`{{${name.toUpperCase()}_DATA}}`, () => JSON.stringify(data)),
    context
  );
  return result;
}
for (const name of Object.keys(source)) {
  check(`${name} academic shell and accessible data`, () => {
    assert.match(source[name], /BioRouterViz\.applyScientificStyles\(\)/);
    assert.match(source[name], /BioRouterViz\.renderFigureData/);
    assert.match(source[name], /role="img"/);
    assert.doesNotMatch(source[name], /box-shadow|linear-gradient/);
  });
}
const areaData = {
  labels: ['A', 'B'],
  stacked: true,
  datasets: [
    { label: 'Signed', data: [-5, 5], color: 'red' },
    { label: 'Short hex', data: [1, -1], color: '#abc' },
  ],
};
check('area preserves signed stacking with straight observed segments', () => {
  const result = render('area', areaData);
  assert.deepEqual(plain(result.config.data.datasets[0].data), [-5, 5]);
  assert.equal(result.config.options.scales.y.stacked, true);
  assert.equal(result.config.data.datasets[0].tension, 0);
});
check('area preserves named and short-hex color channels', () => {
  const result = render('area', areaData);
  assert.equal(
    actualChart.helpers.color(result.config.data.datasets[0].backgroundColor).rgb.r,
    255
  );
  assert.equal(
    actualChart.helpers.color(result.config.data.datasets[1].backgroundColor).rgb.r,
    170
  );
  assert.equal(
    actualChart.helpers.color(result.config.data.datasets[1].backgroundColor).rgb.g,
    187
  );
});
check('area data alternative retains every observation and series', () => {
  assert.deepEqual(render('area', areaData).rows, [
    ['Signed', 'A', -5],
    ['Signed', 'B', 5],
    ['Short hex', 'A', 1],
    ['Short hex', 'B', -1],
  ]);
});
const bubbleData = {
  datasets: [
    {
      label: 'Sizes',
      color: 'rgb(1,2,3)',
      data: [
        { x: 1, y: 2, r: 0 },
        { x: 2, y: 3, r: 0.5 },
      ],
    },
  ],
};
check('bubble preserves zero and fractional radii and valid CSS colors', () => {
  const result = render('bubble', bubbleData);
  assert.deepEqual(plain(result.config.data.datasets[0].data.map((point) => point.r)), [0, 0.5]);
  assert.equal(
    actualChart.helpers.color(result.config.data.datasets[0].backgroundColor).valid,
    true
  );
});
check('bubble exact data includes observations with invisible zero radius', () => {
  const result = render('bubble', bubbleData);
  assert.equal(result.rows.length, 2);
  assert.ok(result.rows[0].includes(0));
  assert.match(result.caption, /radius|radii/i);
});
check(
  'volcano preserves exact threshold classification and explains it without color alone',
  () => {
    const result = render('volcano', {
      fcThreshold: 2,
      pThreshold: 3,
      points: [
        { label: 'up', log2fc: 2, negLog10P: 3 },
        { label: 'down', log2fc: -2, negLog10P: 3 },
        { label: 'neutral', log2fc: 1, negLog10P: 3 },
      ],
    });
    const points = result.config.data.datasets[0].data;
    assert.notEqual(points[0]._c, points[1]._c);
    assert.notEqual(points[1]._c, points[2]._c);
    assert.deepEqual(
      result.rows.map((row) => row.at(-1)),
      ['Positive threshold met', 'Negative threshold met', 'Thresholds not met']
    );
    assert.equal(result.legend.length, 3);
  }
);
check('Manhattan natural order preserves positions and chromosome labels', () => {
  const result = render('manhattan', {
    points: ['chr10', '2', 'X', '1', 'MT'].map((chrom) => ({ chrom, pos: 10, negLog10P: 2 })),
  });
  assert.deepEqual(plain(result.config.data.datasets.map((dataset) => dataset.label)), [
    'chr1',
    'chr2',
    'chr10',
    'chrX',
    'chrMT',
  ]);
  assert.equal(result.rows.length, 5);
});
check('Manhattan handles prototype-like chromosome labels as literal data', () => {
  const result = render('manhattan', {
    points: [
      { chrom: '__proto__', pos: 10, negLog10P: 2 },
      { chrom: 'constructor', pos: 20, negLog10P: 3 },
    ],
  });
  assert.equal(result.config.data.datasets.length, 2);
  assert.equal(result.rows.length, 2);
});
if (process.argv.includes('--browser')) {
  const { chromium } = await import('@playwright/test');
  const output = process.env.BIOROUTER_FIGURE_OUTPUT || '/tmp/biorouter-cartesian-figures';
  await mkdir(output, { recursive: true });
  const asset = await readFile(new URL('assets/chart.min.js', templates), 'utf8');
  const label =
    'Synthetic observation 東京 Δοκιμή <img src=x onerror=alert(1)> with an extended label';
  const fixtures = {
    area: {
      ...areaData,
      title: 'Signed synthetic observations',
      xAxisLabel: 'Observation period and sampling conditions (days)',
      yAxisLabel: 'Change from baseline (units)',
      labels: [label, 'Comparison'],
      datasets: areaData.datasets.map((dataset, i) => ({
        ...dataset,
        label: i ? dataset.label : label,
      })),
    },
    bubble: {
      title: 'Coordinates and supplied bubble radii',
      xAxisLabel: 'Measurement under synthetic experimental conditions (units)',
      yAxisLabel: 'Observed response (units)',
      datasets: [
        {
          label,
          color: '#abc',
          data: [
            { x: 1, y: 2, r: 0, label: 'Zero' },
            { x: 2, y: 3, r: 10, label },
            { x: 3, y: 1, r: 0.5, label: 'Fractional' },
          ],
        },
      ],
    },
    volcano: {
      title: 'Synthetic threshold classifications',
      fcThreshold: 2,
      pThreshold: 3,
      points: [
        { label, log2fc: 2, negLog10P: 3 },
        { label: 'Negative', log2fc: -2, negLog10P: 3 },
        { label: 'Other', log2fc: 1, negLog10P: 2 },
      ],
    },
    manhattan: {
      title: 'Synthetic chromosome observations',
      significanceLine: 7.301,
      points: ['chr10', '2', 'X', '1', 'MT', '__proto__'].map((chrom, i) => ({
        chrom,
        pos: 10 + i,
        negLog10P: 2 + i,
        label,
      })),
    },
  };
  const browser = await chromium.launch({ headless: true });
  try {
    for (const [name, fixture] of Object.entries(fixtures)) {
      for (const theme of ['light', 'dark']) {
        const page = await browser.newPage({
          viewport: { width: 320, height: 1000 },
          colorScheme: theme,
        });
        page.setDefaultTimeout(4000);
        const errors = [];
        page.on('pageerror', (error) => errors.push(error.message));
        page.on('dialog', async (dialog) => {
          errors.push('Unexpected dialog');
          await dialog.dismiss();
        });
        await page.route('**/*', async (route) => {
          errors.push('Unexpected network request');
          await route.abort();
        });
        const html = source[name]
          .replace('{{ASSETS}}', () => `<script>${asset}</script>`)
          .replace('{{COMMON}}', () => common)
          .replace(`{{${name.toUpperCase()}_DATA}}`, () =>
            JSON.stringify(fixture).replaceAll('<', '\\u003c')
          );
        await page.setContent(html, { waitUntil: 'load' });
        await page.waitForFunction(() => document.querySelector('#figureData tbody tr'));
        await page.locator('summary').click();
        for (const width of [320, 1200, 480]) {
          await page.setViewportSize({ width, height: 1000 });
          await page.waitForTimeout(180);
          const metrics = await page.evaluate(() => {
            const chart = Chart.getChart(document.getElementById('chart'));
            return {
              overflow: document.documentElement.scrollWidth > innerWidth + 1,
              title: parseFloat(getComputedStyle(document.querySelector('h1')).fontSize),
              plotWidth: chart.chartArea.width,
              plotHeight: chart.chartArea.height,
              tickFonts: Object.values(chart.scales).map(
                (scale) => scale._resolveTickFontOptions(0).size
              ),
              axisTitlesFit: Object.values(chart.scales)
                .filter((scale) => scale.options.title.display)
                .every((scale) => {
                  chart.ctx.save();
                  chart.ctx.font = '14px ' + Chart.defaults.font.family;
                  const fits = [scale.options.title.text]
                    .flat()
                    .every(
                      (line) =>
                        chart.ctx.measureText(line).width <=
                        (scale.isHorizontal() ? scale.width : scale.height)
                    );
                  chart.ctx.restore();
                  return fits;
                }),
              rows: document.querySelectorAll('#figureData tbody tr').length,
              images: document.querySelectorAll('img').length,
              aria: document.getElementById('chart').getAttribute('role'),
              alerts: [...document.querySelectorAll('[role="alert"]')].map(
                (element) => element.textContent
              ),
            };
          });
          check(`${name} ${theme} ${width}px layout`, () => {
            assert.deepEqual(errors, []);
            assert.equal(metrics.overflow, false);
            assert.ok(metrics.title >= 18 && metrics.title <= 22);
            assert.ok(metrics.plotWidth > 80 && metrics.plotHeight > 120);
            assert.ok(metrics.tickFonts.every((size) => size >= 14));
            assert.ok(metrics.axisTitlesFit, 'axis titles fit');
            assert.equal(metrics.rows, { area: 4, bubble: 3, volcano: 3, manhattan: 6 }[name]);
            assert.equal(metrics.images, 0);
            assert.equal(metrics.aria, 'img');
            assert.deepEqual(metrics.alerts, [], 'no visible rendering error cards');
          });
          await page.screenshot({
            path: `${output}/${name}-${theme}-${width}.png`,
            fullPage: true,
          });
        }
        if (name === 'area' || name === 'bubble') {
          const button = page.locator('#legend button').first();
          await button.focus();
          await page.keyboard.press('Space');
          const hidden = await page.evaluate(() =>
            Chart.getChart(document.getElementById('chart')).isDatasetVisible(0)
          );
          const pressed = await button.getAttribute('aria-pressed');
          await page.keyboard.press('Enter');
          const restored = await page.evaluate(() =>
            Chart.getChart(document.getElementById('chart')).isDatasetVisible(0)
          );
          check(`${name} ${theme} keyboard legend toggle`, () => {
            assert.equal(hidden, false);
            assert.equal(pressed, 'false');
            assert.equal(restored, true);
          });
        }
        await page.setViewportSize({ width: 320, height: 1000 });
        await page.waitForTimeout(180);
        const table = page.locator('.table-scroll');
        await table.focus();
        await page.keyboard.press('ArrowRight');
        await page.waitForTimeout(100);
        const tableScroll = await table.evaluate((element) => element.scrollLeft);
        check(`${name} ${theme} keyboard data scrolling`, () => assert.ok(tableScroll > 0));
        await page.locator('.figure-scroll').scrollIntoViewIfNeeded();
        const hover = await page.evaluate((name) => {
          const canvas = document.getElementById('chart');
          const chart = Chart.getChart(canvas);
          const point = chart.getDatasetMeta(0).data[name === 'bubble' ? 1 : 0].getCenterPoint();
          const scroll = document.querySelector('.figure-scroll');
          scroll.scrollLeft = Math.max(0, point.x - 130);
          const rect = canvas.getBoundingClientRect();
          return { x: rect.left + point.x, y: rect.top + point.y };
        }, name);
        await page.mouse.move(hover.x, hover.y);
        await page.waitForTimeout(100);
        const tooltip = await page.evaluate(() => {
          const canvas = document.getElementById('chart');
          const chart = Chart.getChart(canvas);
          const t = chart.tooltip;
          const rect = canvas.getBoundingClientRect();
          return {
            opacity: t.opacity,
            left: rect.left + t.x,
            right: rect.left + t.x + t.width,
            top: rect.top + t.y,
            bottom: rect.top + t.y + t.height,
            viewportWidth: innerWidth,
            viewportHeight: innerHeight,
          };
        });
        check(`${name} ${theme} narrow tooltip visible`, () => {
          assert.equal(tooltip.opacity, 1);
          assert.ok(
            tooltip.left >= -1 && tooltip.right <= tooltip.viewportWidth + 1,
            'tooltip fits visible viewport'
          );
          assert.ok(tooltip.top >= -1 && tooltip.bottom <= tooltip.viewportHeight + 1);
        });
        await page.close();
      }
    }
  } finally {
    await browser.close();
  }
}
console.log(`${passed} passed; ${failed} failed`);
process.exitCode = failed ? 1 : 0;
