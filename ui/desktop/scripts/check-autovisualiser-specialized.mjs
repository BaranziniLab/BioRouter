import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
import { execFileSync } from 'node:child_process';

const templates = new URL(
  '../../../crates/biorouter-mcp/src/autovisualiser/templates/',
  import.meta.url
);
const names = ['map', 'chord', 'donut', 'radar', 'sankey', 'treemap'];
const sources = new Map(
  await Promise.all(
    names.map(async (name) => [
      name,
      process.argv.includes('--baseline')
        ? execFileSync(
            'git',
            [
              'show',
              `HEAD:crates/biorouter-mcp/src/autovisualiser/templates/${name}_template.html`,
            ],
            { cwd: new URL('../../../', import.meta.url), encoding: 'utf8' }
          )
        : await readFile(new URL(`${name}_template.html`, templates), 'utf8'),
    ])
  )
);
let failures = 0;
function check(name, run) {
  try {
    run();
    console.log(`PASS ${name}`);
  } catch (error) {
    failures++;
    console.error(`FAIL ${name}: ${error.message.split('\n')[0]}`);
  }
}
for (const [name, source] of sources) {
  check(`${name} academic shell and full data alternative`, () => {
    assert.match(source, /BioRouterViz\.applyPageTheme\(\)/);
    assert.match(source, /BioRouterViz\.palette/);
    assert.match(source, /id="figureData"/);
    assert.match(source, /overflow-wrap: anywhere/);
    assert.doesNotMatch(
      source,
      /linear-gradient|schemeCategory10|schemeTableau10|rgba\(255, 99, 132|font-size: 2\.5em/
    );
  });
  check(`${name} template scripts parse`, () => {
    for (const [, script] of source.matchAll(/<script>([\s\S]*?)<\/script>/g)) {
      new vm.Script(script.replaceAll(/\{\{[A-Z_]+\}\}/g, '{}'));
    }
  });
  check(`${name} table preserves narrow-screen numeric columns`, () => {
    assert.match(source, /min-width: 560px/);
    assert.match(source, /\.numeric \{ white-space: nowrap/);
  });
}
check('zero-valued donut has a compact explicit empty state', () => {
  assert.match(sources.get('donut'), /All values are zero/);
  assert.match(sources.get('donut'), /every\(\(value\) => value === 0\)/);
});
const shared = await readFile(new URL('_common.js', templates), 'utf8');
const windowStub = {
  addEventListener() {},
  location: { search: '' },
  matchMedia: () => ({ matches: false }),
};
const documentStub = {
  documentElement: { style: { setProperty() {} } },
  readyState: 'loading',
  addEventListener() {},
  createElement: () => ({}),
};
vm.runInNewContext(shared, {
  window: windowStub,
  document: documentStub,
  URLSearchParams,
  Intl,
  console,
});
check('shared SVG fitting retains graphemes with logarithmic measurement', () => {
  let measurements = 0;
  const element = {
    textContent: '',
    getComputedTextLength() {
      measurements++;
      return (
        Array.from(
          new Intl.Segmenter(undefined, { granularity: 'grapheme' }).segment(this.textContent)
        ).length * 10
      );
    },
  };
  windowStub.BioRouterViz.fitSvgLabel(element, '東京👩🏽‍🔬é'.repeat(1024), 50);
  assert.equal(element.textContent, '東京👩🏽‍🔬é…');
  assert.ok(measurements <= 16, `Unexpected ${measurements} measurements`);
  windowStub.BioRouterViz.fitSvgLabel(element, '東京', 50);
  assert.equal(element.textContent, '東京');
  windowStub.BioRouterViz.fitSvgLabel(element, '東京', 5);
  assert.equal(element.textContent, '');
});
check('shared SVG collisions hide only overlapping or out-of-bounds labels', () => {
  const labels = [
    [0, 20],
    [10, 30],
    [35, 50],
    [90, 110],
  ].map(([left, right]) => ({
    textContent: 'Label',
    getBoundingClientRect: () => ({ left, right, top: 0, bottom: 10 }),
  }));
  windowStub.BioRouterViz.hideOverlappingSvgLabels(
    {
      getBoundingClientRect: () => ({ left: 0, right: 100, top: 0, bottom: 100 }),
      querySelectorAll: () => labels,
    },
    'text'
  );
  assert.deepEqual(
    labels.map((label) => label.textContent),
    ['Label', '', 'Label', '']
  );
});
check('shared tables cap DOM rows and disclose omitted data without HTML interpolation', () => {
  const body = [];
  const caption = {};
  const table = {
    replaceChildren() {
      body.length = 0;
    },
    createCaption: () => caption,
    createTHead: () => ({ insertRow: () => ({ appendChild() {} }) }),
    createTBody: () => ({
      insertRow() {
        const cells = [];
        body.push(cells);
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
  const label = '東京🧬 <img src=x onerror=alert(1)>';
  windowStub.BioRouterViz.renderFigureData(
    table,
    ['Name', 'Value'],
    Array.from({ length: 501 }, () => [label, null]),
    'Category values.'
  );
  assert.equal(body.length, 500);
  assert.equal(body[0][0].textContent, label);
  assert.equal(body[0][1].textContent, '—');
  assert.match(caption.textContent, /Showing the first 500 of 501 rows/);
  windowStub.BioRouterViz.renderFigureData(table, ['Name'], [[label]], 'Category values.');
  assert.equal(body.length, 1);
  assert.doesNotMatch(caption.textContent, /first 500/);
});
if (process.argv.includes('--browser')) {
  const { chromium } = await import('@playwright/test');
  const common = await readFile(new URL('_common.js', templates), 'utf8');
  const assets = Object.fromEntries(
    await Promise.all(
      [
        'chart.min.js',
        'd3.min.js',
        'd3.sankey.min.js',
        'leaflet.min.js',
        'leaflet.markercluster.min.js',
        'leaflet.min.css',
      ].map(async (name) => [name, await readFile(new URL(`assets/${name}`, templates), 'utf8')])
    )
  );
  const longLabel =
    'Δοκιμή 東京🧬 — prolonged observation and follow-up <img src=x onerror=alert(1)>';
  const fixtures = {
    map: {
      token: 'MAP_DATA',
      assets: ['leaflet.min.css', 'leaflet.min.js', 'leaflet.markercluster.min.js'],
      rows: 2,
      data: {
        title: 'Synthetic mapped observations',
        subtitle: 'Fictional data only',
        clustering: false,
        markers: [
          { lat: 37.7, lng: -122.4, name: longLabel, value: 10, color: '#556677' },
          { lat: 40.7, lng: -74, name: 'Comparison', value: 20, useDefaultIcon: true },
        ],
      },
    },
    chord: {
      token: 'CHORD_DATA',
      assets: ['d3.min.js'],
      rows: 9,
      data: {
        labels: [longLabel, 'Comparison', 'Control'],
        matrix: [
          [0, 10, 5],
          [8, 0, 2],
          [4, 3, 0],
        ],
      },
    },
    donut: {
      token: 'CHARTS_DATA',
      assets: ['chart.min.js'],
      rows: 4,
      data: [
        {
          title: 'Synthetic composition (count)',
          data: [
            { label: longLabel, value: 10 },
            { label: 'Comparison', value: 20 },
          ],
        },
        { title: 'Empty counts', type: 'pie', labels: ['A', 'B'], data: [0, 0] },
      ],
    },
    radar: {
      token: 'RADAR_DATA',
      assets: ['chart.min.js'],
      rows: 6,
      data: {
        labels: [
          longLabel,
          'Repeatability',
          'Completeness',
          'Sensitivity',
          'Precision',
          'Coverage',
        ],
        datasets: [
          { label: longLabel, data: [80, 60, 70, 50, 65, 75] },
          { label: 'Comparison', data: [70, 55, 60, 65, 70, 60] },
        ],
      },
    },
    sankey: {
      token: 'SANKEY_DATA',
      assets: ['d3.min.js', 'd3.sankey.min.js'],
      rows: 3,
      data: {
        nodes: [
          { name: longLabel, category: 'input' },
          { name: 'Process', category: 'process' },
          { name: 'Output', category: 'output' },
          { name: 'Control', category: 'output' },
        ],
        links: [
          { source: longLabel, target: 'Process', value: 10 },
          { source: 'Process', target: 'Output', value: 8 },
          { source: 'Process', target: 'Control', value: 2 },
        ],
      },
    },
    treemap: {
      token: 'TREEMAP_DATA',
      assets: ['d3.min.js'],
      rows: 3,
      data: {
        name: 'Synthetic groups',
        children: [
          {
            name: 'Group A',
            children: [
              { name: longLabel, value: 100 },
              { name: 'Comparison', value: 50 },
            ],
          },
          { name: 'Group B', children: [{ name: 'Tiny value', value: 1 }] },
        ],
      },
    },
  };
  const browser = await chromium.launch({ headless: true });
  try {
    for (const [name, fixture] of Object.entries(fixtures)) {
      const only = process.argv.find((argument) => argument.startsWith('--only='))?.slice(7);
      if (only && name !== only) continue;
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
        await page.route('https://**/*', async (route) => {
          if (/tile\.|arcgisonline/.test(route.request().url())) {
            await route.fulfill({
              contentType: 'image/png',
              body: Buffer.from(
                'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/l9sAAAAASUVORK5CYII=',
                'base64'
              ),
            });
          } else {
            errors.push('Unexpected external request');
            await route.abort();
          }
        });
        const html = sources
          .get(name)
          .replace('{{ASSETS}}', () =>
            fixture.assets
              .map((asset) =>
                asset.endsWith('.css')
                  ? `<style>${assets[asset]}</style>`
                  : `<script>${assets[asset]}</script>`
              )
              .join('\n')
          )
          .replace('{{COMMON}}', () => common)
          .replace(`{{${fixture.token}}}`, () =>
            JSON.stringify(fixture.data).replaceAll('<', '\\u003c')
          );
        await page.setContent(html, { waitUntil: 'load' });
        await page.waitForFunction(
          () => document.querySelectorAll('#figureData tbody tr').length > 0
        );
        await page
          .locator('details')
          .filter({ has: page.locator('#figureData') })
          .locator('summary')
          .click();
        for (const width of [320, 1200, 480]) {
          await page.setViewportSize({ width, height: 1000 });
          await page.waitForTimeout(150);
          const metrics = await page.evaluate(() => {
            const labels = [...document.querySelectorAll('svg text')]
              .filter((node) => node.textContent && getComputedStyle(node).display !== 'none')
              .map((node) => {
                const box = node.getBoundingClientRect();
                return {
                  left: box.left,
                  right: box.right,
                  top: box.top,
                  bottom: box.bottom,
                  size: parseFloat(getComputedStyle(node).fontSize),
                };
              });
            return {
              width: innerWidth,
              scrollWidth: document.documentElement.scrollWidth,
              theme: window.BioRouterViz.theme,
              background: getComputedStyle(document.body).backgroundColor,
              font: parseFloat(getComputedStyle(document.querySelector('#figureData')).fontSize),
              rows: document.querySelectorAll('#figureData tbody tr').length,
              table: document.querySelector('#figureData').textContent,
              injectedImages: document.querySelectorAll(
                '#figureData img, .legend-item img, .chart-title img'
              ).length,
              labels,
              numericCellsReadable: [...document.querySelectorAll('#figureData .numeric')].every(
                (cell) => getComputedStyle(cell).whiteSpace === 'nowrap'
              ),
              numericHeadersReadable: [...document.querySelectorAll('#figureData th')]
                .filter((cell) => ['Value', 'Flow'].includes(cell.textContent))
                .every((cell) => {
                  const range = document.createRange();
                  range.selectNodeContents(cell);
                  return range.getClientRects().length === 1;
                }),
              emptyPlots: [...document.querySelectorAll('.empty-chart')].map((node) => ({
                text: node.textContent,
                height: node.getBoundingClientRect().height,
                canvases: node.querySelectorAll('canvas').length,
              })),
              map:
                typeof mapData === 'undefined'
                  ? null
                  : {
                      center: map.getCenter(),
                      zoom: map.getZoom(),
                      bounds: map.getBounds(),
                      markers: [...document.querySelectorAll('.leaflet-marker-icon')].map(
                        (node) => ({
                          rect: node.getBoundingClientRect().toJSON(),
                          transform: node.style.transform,
                        })
                      ),
                      allVisible: mapData.markers.every((point) =>
                        map.getBounds().contains([point.lat, point.lng])
                      ),
                    },
            };
          });
          check(`${name}/${theme} ${width}px layout`, () => {
            assert.deepEqual(errors, []);
            assert.equal(metrics.theme, theme);
            assert.equal(
              metrics.background,
              theme === 'dark' ? 'rgb(27, 27, 25)' : 'rgb(255, 255, 255)'
            );
            assert.ok(metrics.scrollWidth <= width + 1, JSON.stringify(metrics));
            assert.equal(metrics.rows, fixture.rows);
            assert.ok(metrics.table.includes(longLabel));
            assert.equal(metrics.injectedImages, 0);
            assert.ok(metrics.font >= 14);
            assert.ok(
              metrics.numericCellsReadable && metrics.numericHeadersReadable,
              'Numeric columns remain readable'
            );
            if (name === 'donut') {
              assert.equal(metrics.emptyPlots.length, 1);
              assert.equal(metrics.emptyPlots[0].text, 'All values are zero.');
              assert.ok(metrics.emptyPlots[0].height < 80);
              assert.equal(metrics.emptyPlots[0].canvases, 0);
            }
            if (metrics.map) assert.ok(metrics.map.allVisible, JSON.stringify(metrics.map));
            for (let i = 0; i < metrics.labels.length; i++) {
              const label = metrics.labels[i];
              assert.ok(label.size >= 14);
              for (const previous of metrics.labels.slice(0, i)) {
                assert.ok(
                  !(
                    label.left < previous.right &&
                    label.right > previous.left &&
                    label.top < previous.bottom &&
                    label.bottom > previous.top
                  ),
                  'Visible SVG labels overlap'
                );
              }
            }
          });
          if (process.env.AUTOVIS_SCREENSHOT_DIR)
            await page.screenshot({
              path: `${process.env.AUTOVIS_SCREENSHOT_DIR}/specialized-${name}-${theme}-${width}.png`,
              fullPage: true,
            });
        }
        const tableRegion = page.getByRole('region', { name: 'Figure data' });
        await tableRegion.focus();
        await page.keyboard.press('ArrowRight');
        await page.waitForTimeout(150);
        const tableScroll = await tableRegion.evaluate((node) => node.scrollLeft);
        check(`${name}/${theme} table keyboard scrolling`, () => assert.ok(tableScroll > 0));
        await tableRegion.evaluate((node) => {
          node.scrollLeft = 0;
        });
        if (name === 'donut' || name === 'radar') {
          await page.setViewportSize({ width: 320, height: 1000 });
          await page.waitForTimeout(150);
          const button = page.locator('button.legend-item').first();
          await button.click();
          const hidden = await page.evaluate((kind) => {
            const chart = window.Chart.getChart(document.querySelector('canvas'));
            return kind === 'donut' ? !chart.getDataVisibility(0) : !chart.isDatasetVisible(0);
          }, name);
          const pressed = await button.getAttribute('aria-pressed');
          await button.focus();
          await page.keyboard.press('Enter');
          const restored = await button.getAttribute('aria-pressed');
          check(`${name}/${theme} legend mouse and keyboard`, () => {
            assert.ok(hidden);
            assert.equal(pressed, 'false');
            assert.equal(restored, 'true');
          });
          const tooltip = await page.evaluate(() => {
            const chart = window.Chart.getChart(document.querySelector('canvas'));
            const point = chart.getDatasetMeta(0).data[0];
            chart.tooltip.setActiveElements([{ datasetIndex: 0, index: 0 }], {
              x: point.x,
              y: point.y,
            });
            chart.update('none');
            return {
              opacity: chart.tooltip.opacity,
              font: chart.tooltip.options.bodyFont.size,
              x: chart.tooltip.x,
              y: chart.tooltip.y,
              width: chart.tooltip.width,
              height: chart.tooltip.height,
              chartWidth: chart.width,
              chartHeight: chart.height,
            };
          });
          check(`${name}/${theme} tooltip`, () => {
            assert.equal(tooltip.opacity, 1);
            assert.ok(tooltip.font >= 14);
            assert.ok(
              tooltip.x >= 0 && tooltip.x + tooltip.width <= tooltip.chartWidth + 1,
              JSON.stringify(tooltip)
            );
            assert.ok(
              tooltip.y >= 0 && tooltip.y + tooltip.height <= tooltip.chartHeight + 1,
              JSON.stringify(tooltip)
            );
          });
        } else if (name === 'map') {
          const fit = await page.evaluate(() =>
            mapData.markers.every((point) => map.getBounds().contains([point.lat, point.lng]))
          );
          if (!fit) {
            await page.close();
            continue;
          }
          await page.locator('.leaflet-marker-icon').first().click();
          const popup = await page.locator('.leaflet-popup-content').textContent();
          check(`map/${theme} bounds, popup and layer control`, () => {
            assert.ok(fit);
            assert.ok(popup.includes(longLabel));
          });
          assert.equal(await page.locator('.leaflet-control-layers').count(), 1);
          await page.evaluate(() => {
            mapData.autoFit = false;
            map.setView([10, 20], 7, { animate: false });
          });
          await page.setViewportSize({ width: 320, height: 1000 });
          await page.waitForTimeout(150);
          const explicit = await page.evaluate(() => ({
            zoom: map.getZoom(),
            center: map.getCenter(),
          }));
          check(`map/${theme} explicit view survives resize`, () => {
            assert.equal(explicit.zoom, 7);
            assert.ok(Math.abs(explicit.center.lat - 10) < 0.01);
          });
        } else {
          await page.setViewportSize({ width: 320, height: 1000 });
          await page.waitForTimeout(150);
          const target =
            name === 'chord' ? '.group-arc' : name === 'sankey' ? '.node rect' : '.treemap-rect';
          await page.locator(target).first().hover({ force: true });
          await page.waitForTimeout(150);
          const tooltip = await page.locator('.tooltip').evaluate((node) => ({
            opacity: Number(getComputedStyle(node).opacity),
            text: node.textContent,
            font: parseFloat(getComputedStyle(node).fontSize),
            left: node.getBoundingClientRect().left,
            right: node.getBoundingClientRect().right,
            top: node.getBoundingClientRect().top,
            bottom: node.getBoundingClientRect().bottom,
            viewportHeight: innerHeight,
          }));
          check(`${name}/${theme} tooltip`, () => {
            assert.equal(tooltip.opacity, 1);
            assert.ok(tooltip.text.length > 0);
            assert.ok(tooltip.font >= 14, JSON.stringify(tooltip));
            assert.ok(tooltip.left >= 0 && tooltip.right <= 320, JSON.stringify(tooltip));
            assert.ok(
              tooltip.top >= 0 && tooltip.bottom <= tooltip.viewportHeight,
              JSON.stringify(tooltip)
            );
          });
          if (name === 'treemap') {
            const before = await page.locator('.treemap-rect').count();
            await page.locator('.treemap-rect').first().click();
            const focused = await page.locator('.treemap-rect').count();
            await page.locator('#resetTreemap').click();
            const restored = await page.locator('.treemap-rect').count();
            check(`treemap/${theme} drilldown and reset`, () => {
              assert.ok(focused < before);
              assert.equal(restored, before);
            });
          }
        }
        await page.close();
      }
    }
  } finally {
    await browser.close();
  }
}
if (failures) process.exitCode = 1;
