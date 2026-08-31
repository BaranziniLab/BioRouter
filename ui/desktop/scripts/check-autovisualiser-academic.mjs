import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const root = new URL('../../../', import.meta.url);
const templates = new URL('crates/biorouter-mcp/src/autovisualiser/templates/', root);
const chartTemplate = await readFile(new URL('chart_template.html', templates), 'utf8');
const common = await readFile(new URL('_common.js', templates), 'utf8');
const failures = [];
function check(name, fn) {
  try {
    fn();
    console.log(`PASS ${name}`);
  } catch (error) {
    failures.push(name);
    console.error(
      `FAIL ${name}: ${error.message.split('\n')[0]} (actual=${JSON.stringify(error.actual)}, expected=${JSON.stringify(error.expected)})`
    );
  }
}

check('chart has no decorative banner or generic subtitle', () => {
  assert.doesNotMatch(chartTemplate, /linear-gradient|Interactive data visualization|📊/);
  assert.match(chartTemplate, /id="chartSubtitle" hidden/);
});
check('chart uses shared academic defaults and an accessible data alternative', () => {
  assert.match(chartTemplate, /BioRouterViz\.applyChartDefaults\(\)/);
  assert.match(chartTemplate, /BioRouterViz\.palette/);
  assert.match(chartTemplate, /role="img"/);
  assert.match(chartTemplate, /<table/);
  assert.doesNotMatch(chartTemplate, /rgba\(54, 162, 235|rgba\(255, 99, 132/);
});
check('numeric chart data explains coordinates with axis quantities and units', () => {
  const rows = [];
  const table = {
    createCaption: () => ({}),
    createTHead: () => ({ insertRow: () => ({ appendChild() {} }) }),
    createTBody: () => ({
      insertRow() {
        const cells = [];
        rows.push(cells);
        return {
          insertCell() {
            const cell = {};
            cells.push(cell);
            return cell;
          },
        };
      },
    }),
  };
  const renderTable = chartTemplate.match(/function renderTable\(\) \{[\s\S]*?\n        \}/)?.[0];
  assert.ok(renderTable, 'table renderer is present');
  vm.runInNewContext(`(${renderTable})()`, {
    categorical: false,
    chartData: {
      xAxisLabel: 'Monthly cost (USD/month)',
      yAxisLabel: 'Time saved (minutes/day)',
      datasets: [{ label: 'Option A', data: [{ x: 120, y: 30 }, null] }],
    },
    document: { getElementById: () => table, createElement: () => ({}) },
  });
  assert.equal(
    rows[0][1].textContent,
    'Monthly cost (USD/month): 120; Time saved (minutes/day): 30'
  );
  assert.equal(rows[1][1].textContent, '—');
});
const window = {
  addEventListener() {},
  location: { search: '' },
  matchMedia: () => ({ matches: false }),
};
const document = {
  documentElement: { style: { setProperty() {} } },
  readyState: 'loading',
  addEventListener() {},
};
window.window = window;
const context = vm.createContext({ window, document, URLSearchParams, console, Intl });
vm.runInContext(common, context);
check('shared labels wrap Unicode without losing text', () => {
  const viz = window.BioRouterViz;
  const label = 'Δοκιμή 東京🧬超長標籤';
  const lines = viz.wrapLabel(label, 48, (text) => Array.from(text).length * 12);
  assert.equal(lines.join(''), label);
  assert.ok(lines.length > 1);
  assert.ok(lines.every((line) => Array.from(line).length <= 4));
});
check('shared Chart defaults use readable text and restrained colors', () => {
  window.Chart = {
    defaults: { font: {}, plugins: { tooltip: {} }, scale: { ticks: {}, title: {} } },
  };
  window.BioRouterViz.applyChartDefaults();
  assert.ok(window.Chart.defaults.font.size >= 14);
  assert.ok(window.Chart.defaults.plugins.tooltip.bodyFont.size >= 14);
  assert.equal(window.BioRouterViz.colors.bg, '#ffffff');
  assert.notEqual(window.BioRouterViz.palette[0], '#4f7cff');
});

if (process.argv.includes('--browser')) {
  const { chromium } = await import('@playwright/test');
  const browser = await chromium.launch({ headless: true });
  try {
    const library = await readFile(new URL('assets/chart.min.js', templates), 'utf8');
    const bar = {
      type: 'bar',
      title: 'Fictional clinic — cost of saving time (Δ, 東京)',
      subtitle: 'Synthetic comparison; not clinical or financial evidence',
      xAxisLabel: 'Scheduling option',
      yAxisLabel: 'Monthly cost (USD)',
      labels: [
        'Option A — appointments and follow-up',
        'Option B — Δοκιμή 東京🧬超長標籤',
        'Option C — Téléconsultation',
      ],
      datasets: [{ label: 'Monthly cost (USD)', data: [120, 180, 155] }],
    };
    const fixtures = [
      { name: 'bar', theme: 'light', data: bar },
      {
        name: 'line',
        theme: 'dark',
        data: {
          ...bar,
          type: 'line',
          subtitle: undefined,
          title: 'Synthetic response — Δοκιμή 東京',
          yAxisLabel:
            'Change in synthetic biomarker concentration from the scheduled baseline assessment (mg/L)',
          datasets: [
            {
              label: 'Cohort A — extended follow-up observations 東京🧬',
              data: [120, 180, 155],
              backgroundColor: '#123456',
              borderColor: '#abcdef',
              borderWidth: 3,
              tension: 0.2,
              fill: true,
            },
            { label: 'Cohort B — comparison observations Δοκιμή', data: [130, 170, 145] },
          ],
        },
      },
      {
        name: 'clinic-scatter',
        theme: 'light',
        data: {
          type: 'scatter',
          title: 'Fictional clinic cost versus time saved',
          xAxisLabel: 'Time saved (minutes/day)',
          yAxisLabel: 'Monthly cost (USD)',
          datasets: [
            { label: 'Option A', data: [{ x: 30, y: 120 }] },
            { label: 'Option B', data: [{ x: 40, y: 180 }] },
          ],
        },
      },
      {
        name: 'numeric-line',
        theme: 'light',
        data: {
          type: 'line',
          title: 'Synthetic <\/script><script>window.__escaped=false<\/script>',
          xAxisLabel: 'Follow-up (days)',
          yAxisLabel: 'Response (mg/L)',
          datasets: [
            { label: 'Unavailable cohort', data: [] },
            {
              label: 'Observed cohort',
              data: [
                { x: 2, y: 10 },
                { x: 20, y: 15 },
                { x: 100, y: 17 },
              ],
            },
          ],
        },
      },
    ];
    for (const fixture of fixtures) {
      const { data } = fixture;
      const html = chartTemplate
        .replace('{{ASSETS}}', () => `<script>${library}</script>`)
        .replace('{{COMMON}}', () => common)
        .replace('{{CHART_DATA}}', () => JSON.stringify(data).replaceAll('<', '\\u003c'));
      const page = await browser.newPage({
        viewport: { width: 320, height: 900 },
        colorScheme: fixture.theme,
      });
      const errors = [];
      page.on('pageerror', (error) => errors.push(error.message));
      await page.setContent(html, { waitUntil: 'load' });
      await page.waitForFunction(
        () => window.Chart.getChart(document.querySelector('canvas'))?.chartArea.width > 0
      );
      await page.locator('summary').click();
      for (const width of [320, 1200, 480]) {
        await page.setViewportSize({ width, height: 900 });
        await page.waitForTimeout(120);
        const metrics = await page.evaluate(() => {
          const chart = window.Chart.getChart(document.querySelector('canvas'));
          const scale = chart.scales.x;
          const labels = scale.getLabelItems(chart.chartArea).map((item) => {
            const lines = Array.isArray(item.label) ? item.label : [String(item.label)];
            chart.ctx.font = item.font.string;
            const width = Math.max(...lines.map((line) => chart.ctx.measureText(line).width));
            const x = item.options.translation[0];
            return { left: x - width / 2, right: x + width / 2, rotation: item.options.rotation };
          });
          return {
            width: innerWidth,
            canvasWidth: chart.width,
            scrollWidth: document.documentElement.scrollWidth,
            plotWidth: chart.chartArea.width,
            plotHeight: chart.chartArea.height,
            tickSize: chart.options.scales.x.ticks.font.size,
            titleSize: getComputedStyle(document.querySelector('h1')).fontSize,
            banner: getComputedStyle(document.querySelector('.header')).backgroundImage,
            labels,
            tableRows: document.querySelectorAll('tbody tr').length,
            axisType: scale.type,
            title: document.querySelector('h1').textContent,
            subtitleHidden: document.querySelector('#chartSubtitle').hidden,
            theme: window.BioRouterViz.theme,
            escaped: typeof window.__escaped === 'undefined',
            alerts: [...document.querySelectorAll('[role="alert"]')].map(
              (element) => element.textContent
            ),
            yTitleWidth: (() => {
              chart.ctx.font =
                chart.options.scales.y.title.font.size + 'px ' + window.Chart.defaults.font.family;
              return Math.max(
                ...chart.options.scales.y.title.text.map(
                  (text) => chart.ctx.measureText(text).width
                )
              );
            })(),
            legends: [...document.querySelectorAll('#chartLegend button')].map((button) => {
              const box = button.getBoundingClientRect();
              return { left: box.left, right: box.right, top: box.top, bottom: box.bottom };
            }),
          };
        });
        check(`real Chart.js ${fixture.name}/${fixture.theme} ${width}px layout`, () => {
          assert.deepEqual(errors, []);
          assert.deepEqual(metrics.alerts, [], 'no visible rendering error cards');
          assert.ok(metrics.scrollWidth <= width + 1, JSON.stringify(metrics));
          assert.ok(metrics.plotWidth >= 100 && metrics.plotHeight >= 180, JSON.stringify(metrics));
          assert.ok(metrics.tickSize >= 14);
          assert.equal(metrics.banner, 'none');
          assert.equal(
            metrics.tableRows,
            Math.max(...data.datasets.map((dataset) => dataset.data.length))
          );
          assert.equal(
            metrics.axisType,
            fixture.name === 'clinic-scatter' || fixture.name === 'numeric-line'
              ? 'linear'
              : 'category'
          );
          assert.equal(metrics.theme, fixture.theme);
          assert.equal(metrics.title, data.title);
          assert.equal(metrics.subtitleHidden, !data.subtitle);
          assert.ok(metrics.escaped);
          assert.ok(metrics.yTitleWidth <= metrics.plotHeight, JSON.stringify(metrics));
          for (const legend of metrics.legends) {
            assert.ok(legend.left >= 0 && legend.right <= width + 1, JSON.stringify(metrics));
          }
          for (let i = 0; i < metrics.labels.length; i++) {
            const label = metrics.labels[i];
            assert.ok(label.rotation === 0, 'tick labels must remain horizontal');
            assert.ok(
              label.left >= -1 && label.right <= metrics.canvasWidth + 1,
              JSON.stringify(metrics)
            );
            if (i)
              assert.ok(metrics.labels[i - 1].right + 2 <= label.left, JSON.stringify(metrics));
          }
        });
        if (process.env.AUTOVIS_SCREENSHOT_DIR) {
          await page.screenshot({
            path: `${process.env.AUTOVIS_SCREENSHOT_DIR}/academic-${fixture.name}-${fixture.theme}-${width}.png`,
            fullPage: true,
          });
        }
      }
      if (data.datasets.length > 1) {
        const button = page.locator('#chartLegend button').first();
        await button.click();
        const hidden = {
          pressed: await button.getAttribute('aria-pressed'),
          visible: await page.evaluate(() =>
            window.Chart.getChart(document.querySelector('canvas')).isDatasetVisible(0)
          ),
        };
        check(`${fixture.name} legend hides series`, () => {
          assert.equal(hidden.pressed, 'false');
          assert.equal(hidden.visible, false);
        });
        await button.focus();
        await page.keyboard.press('Enter');
        const restored = {
          pressed: await button.getAttribute('aria-pressed'),
          visible: await page.evaluate(() =>
            window.Chart.getChart(document.querySelector('canvas')).isDatasetVisible(0)
          ),
        };
        check(`${fixture.name} legend keyboard restores series`, () => {
          assert.equal(restored.pressed, 'true');
          assert.equal(restored.visible, true);
        });
      }
      await page.setViewportSize({ width: 320, height: 900 });
      await page.waitForTimeout(100);
      const actual = await page.evaluate(() => {
        const chart = window.Chart.getChart(document.querySelector('canvas'));
        const index = chart.data.datasets.findIndex((dataset) => dataset.data.length);
        const point = chart.getDatasetMeta(index).data[0];
        chart.tooltip.setActiveElements([{ datasetIndex: index, index: 0 }], {
          x: point.x,
          y: point.y,
        });
        chart.update('none');
        return {
          first: chart.data.datasets[0],
          tooltip: chart.tooltip.opacity,
          tooltipFont: chart.tooltip.options.bodyFont.size,
          tooltipBounds: {
            x: chart.tooltip.x,
            y: chart.tooltip.y,
            width: chart.tooltip.width,
            height: chart.tooltip.height,
            chartWidth: chart.width,
            chartHeight: chart.height,
          },
        };
      });
      check(`${fixture.name} tooltip and explicit styles remain functional`, () => {
        assert.equal(actual.tooltip, 1);
        assert.ok(actual.tooltipFont >= 14);
        const bounds = actual.tooltipBounds;
        assert.ok(
          bounds.x >= 0 && bounds.x + bounds.width <= bounds.chartWidth + 1,
          JSON.stringify(bounds)
        );
        assert.ok(
          bounds.y >= 0 && bounds.y + bounds.height <= bounds.chartHeight + 1,
          JSON.stringify(bounds)
        );
        for (const key of ['backgroundColor', 'borderColor', 'borderWidth', 'tension', 'fill']) {
          if (data.datasets[0][key] !== undefined)
            assert.equal(actual.first[key], data.datasets[0][key]);
        }
      });
      await page.close();
    }
  } finally {
    await browser.close();
  }
}

if (failures.length) process.exitCode = 1;
else console.log(`Academic visualization checks passed (${fileURLToPath(templates)}).`);
