import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

// Default: small source/VM checks. --browser: one browser, serial synthetic fixtures.
// --only=heatmap selects one renderer; AUTOVIS_SCREENSHOT_DIR is an existing output directory.
const templates = new URL(
  '../../../crates/biorouter-mcp/src/autovisualiser/templates/',
  import.meta.url
);
const names = [
  'network',
  'kaplan_meier',
  'dendrogram',
  'heatmap',
  'calendar',
  'forest',
  'choropleth',
];
const sources = new Map(
  await Promise.all(
    names.map(async (name) => [
      name,
      await readFile(new URL(`${name}_template.html`, templates), 'utf8'),
    ])
  )
);
const common = await readFile(new URL('_common.js', templates), 'utf8');
const results = [];
function check(name, run) {
  try {
    run();
    results.push({ name, passed: true });
    console.log(`PASS ${name}`);
  } catch (error) {
    results.push({ name, passed: false });
    console.error(`FAIL ${name}: ${error.message.split('\n')[0]}`);
  }
}
function runtime(theme) {
  const window = {
    __BR_VIZ_THEME__: theme,
    location: { search: '' },
    addEventListener() {},
    matchMedia: () => ({ matches: theme === 'dark' }),
  };
  const document = {
    documentElement: { style: { setProperty() {} } },
    readyState: 'loading',
    addEventListener() {},
  };
  vm.runInNewContext(common, { window, document, URLSearchParams, Intl, console });
  return window.BioRouterViz;
}
function luminance(color) {
  const rgb = color.startsWith('#')
    ? color
        .slice(1)
        .match(/../g)
        .map((v) => parseInt(v, 16))
    : color
        .match(/[\d.]+/g)
        .slice(0, 3)
        .map(Number);
  return rgb
    .map((v) => {
      const channel = v / 255;
      return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
    })
    .reduce((total, channel, index) => total + channel * [0.2126, 0.7152, 0.0722][index], 0);
}
for (const [name, source] of sources) {
  check(`${name}: template scripts parse`, () => {
    for (const [, script] of source.matchAll(/<script>([\s\S]*?)<\/script>/g))
      new vm.Script(script.replaceAll(/\{\{[A-Z_]+\}\}/g, '{}'));
  });
  check(`${name}: readable bounded data alternative`, () => {
    assert.match(source, /BioRouterViz\.renderFigureData/);
    assert.match(source, /role="region"/);
    assert.match(source, /tabindex="0"/);
  });
}
for (const theme of ['light', 'dark']) {
  check(`${theme}: sequential color has monotonic luminance`, () => {
    const viz = runtime(theme);
    const values = Array.from({ length: 17 }, (_, index) => luminance(viz.sequential(index / 16)));
    const deltas = values.slice(1).map((value, index) => value - values[index]);
    assert.ok(
      deltas.every((value) => value >= -0.001) || deltas.every((value) => value <= 0.001),
      JSON.stringify(values)
    );
    assert.ok(Math.abs(values.at(-1) - values[0]) >= 0.15, 'Endpoints must remain distinguishable');
  });
}
check('network: supplied labels do not enter tooltip HTML', () => {
  assert.doesNotMatch(sources.get('network'), /\.html\([^;]*d\.label/);
});
check('heatmap: supplied axis labels do not enter tooltip HTML', () => {
  assert.doesNotMatch(sources.get('heatmap'), /\.html\([^;]*\+ yl/);
});
check('choropleth: names and legend titles do not enter HTML', () => {
  assert.doesNotMatch(
    sources.get('choropleth'),
    /innerHTML[^;]*(?:featureName|legendTitle)|bindPopup\([^;]*featureName/
  );
});
check('KM preserves explicit group colors and step-after curves', () => {
  assert.match(sources.get('kaplan_meier'), /grp\.color \|\| BioRouterViz\.palette/);
  assert.match(sources.get('kaplan_meier'), /curveStepAfter/);
  assert.match(sources.get('kaplan_meier'), /p\.censored/);
});
check('forest preserves log scale, confidence bounds and reference line', () => {
  assert.match(sources.get('forest'), /d3\.scaleLog/);
  assert.match(sources.get('forest'), /x\(r\.lower\)/);
  assert.match(sources.get('forest'), /x\(r\.upper\)/);
  assert.match(sources.get('forest'), /x\(refLine\)/);
});
function forestMarkerSizes(rows) {
  const source = sources.get('forest');
  const helper = source.match(/function markerWeight\(row\) \{[\s\S]*?\n            \}/)?.[0] || '';
  const maxWeight = source.match(/const maxW = [^\n]+/)[0];
  const markerSize = source.match(/const sz = [^\n]+/)[0];
  return vm.runInNewContext(
    `${helper}\n${maxWeight}\nrows.map((r) => { ${markerSize} return sz; });`,
    {
      rows,
      d3: { max: (values, get) => Math.max(...values.map(get)) },
    }
  );
}
check('forest: observed zero uses a minimum marker, omitted weight defaults to one', () => {
  const sizes = forestMarkerSizes([{ weight: 0 }, {}, { weight: 1 }]);
  assert.equal(sizes[0], 5);
  assert.equal(sizes[1], sizes[2]);
  assert.ok(sizes[0] < sizes[1]);
});
check('forest: negative and non-finite weights reject rather than draw NaN', () => {
  for (const weight of [-1, NaN, Infinity])
    assert.throws(() => forestMarkerSizes([{ weight }, { weight: 1 }]));
});
check('quantitative legend labels distinguish close values, negatives and constant zero', () => {
  const viz = runtime('light');
  const values = [-1.00002, -1.00001, 0, 1.00001, 1.00002];
  const labels = viz.formatScaleValues(values);
  assert.equal(new Set(labels).size, values.length);
  assert.deepEqual(Array.from(labels, Number), values);
  assert.deepEqual(Array.from(viz.formatScaleValues([0])), ['0']);
});
check('calendar rejects excessive date spans before drawing cells', () => {
  assert.match(sources.get('calendar'), /dayCount > 3660/);
});
check('choropleth distinguishes observed zero from missing and non-numeric values', () => {
  const body = sources
    .get('choropleth')
    .match(/function featureValue\(f\) \{([\s\S]*?)\n            \}/)[1];
  const featureValue = vm.runInNewContext(`(function(f) {${body}})`, {
    valueProp: 'score',
    idProp: 'id',
    values: { mapped: 3 },
  });
  for (const [value, expected] of [
    [0, 0],
    ['0', 0],
    [-2, -2],
    [null, null],
    ['', null],
    [' ', null],
    [false, null],
    ['invalid', null],
  ]) {
    assert.equal(featureValue({ properties: { score: value } }), expected);
  }
  assert.equal(featureValue({ properties: { id: 'mapped' } }), 3);
});

// Execute the actual calendar draw script with a tiny D3 surface; native Date uses
// each child process's TZ. This checks data placement, not a rewritten algorithm.
const calendarProbe = String.raw`
  import vm from 'node:vm';
  const { script, data } = JSON.parse(process.argv[1]);
  const marks = []; let tooltip = ''; let tableRows = [];
  function selection(tag = '', isTooltip = false) {
    const mark = tag === 'rect' ? { attrs: {}, handlers: {} } : null;
    if (mark) marks.push(mark);
    const node = {
      append: (kind) => selection(kind),
      attr(key, value) { if (mark) mark.attrs[key] = value; return node; },
      style() { return node; }, text(value) { if (isTooltip) tooltip = value; return node; },
      html(value) { tooltip = value; return node; },
      on(event, callback) { if (mark) mark.handlers[event] = callback; return node; }
    }; return node;
  }
  const d3 = { select: (selector) => selection('', selector === '#tooltip'), min: (items, fn) => Math.min(...items.map(fn)), max: (items, fn) => Math.max(...items.map(fn)) };
  let error;
  try { vm.runInNewContext(script.replace('{{CALENDAR_DATA}}', JSON.stringify(data)), {
    Date, d3, document: { getElementById: () => ({}) },
    BioRouterViz: { autoResize() {}, applyPageTheme() {}, applyScientificStyles() {}, renderFigureData(_table, _headers, rows) { tableRows = Array.from(rows); }, renderFigureLegend() {}, formatScaleValues: (values) => values.map(String), reportSize() {}, guard: (fn) => fn(), sequential: (value) => String(value), dark: false }
  }); } catch (caught) { error = caught.message; }
  if (error) { console.log(JSON.stringify({ error, marks: marks.length })); process.exit(0); }
  console.log(JSON.stringify(marks.map((mark) => {
    mark.handlers.mouseover({ pageX: 0, pageY: 0 });
    const date = tooltip.match(/\d{4}-\d{2}-\d{2}/)?.[0];
    return { date, tableValue: tableRows.find((row) => row[0] === date)?.[1], text: tooltip, fill: mark.attrs.fill, x: mark.attrs.x, y: mark.attrs.y };
  })));
`;
const calendarData = {
  values: [
    { date: '2026-03-07', value: 0 },
    { date: '2026-03-08', value: 5 },
    { date: '2026-03-09', value: 10 },
  ],
};
const calendarScript = [...sources.get('calendar').matchAll(/<script>([\s\S]*?)<\/script>/g)].at(
  -1
)[1];
function probeCalendar(data, timezone = 'UTC') {
  return JSON.parse(
    execFileSync(
      process.execPath,
      [
        '--input-type=module',
        '-e',
        calendarProbe,
        JSON.stringify({ script: calendarScript, data }),
      ],
      { env: { TZ: timezone }, encoding: 'utf8', timeout: 2000 }
    )
  );
}
check('calendar: sparse excessive range rejects before allocating day cells', () => {
  const result = probeCalendar({
    values: [
      { date: '2020-01-01', value: 0 },
      { date: '2030-01-08', value: 1 },
    ],
  });
  assert.match(result.error, /3,660 days/);
  assert.equal(result.marks, 0);
});
check('calendar: leap day, missing day and observed zero remain distinct', () => {
  const marks = probeCalendar(
    {
      values: [
        { date: '2024-02-28', value: 0 },
        { date: '2024-03-01', value: 0 },
      ],
    },
    'America/Los_Angeles'
  );
  assert.deepEqual(
    marks.map((mark) => mark.date),
    ['2024-02-28', '2024-02-29', '2024-03-01']
  );
  assert.equal(marks[0].text, '2024-02-28\n0');
  assert.equal(marks[1].text, '2024-02-29\nNo data');
  assert.notEqual(marks[0].fill, marks[1].fill);
  assert.equal(marks[0].fill, marks[2].fill);
});
check('calendar: autumn daylight-saving boundary preserves every day', () => {
  const marks = probeCalendar(
    {
      values: [
        { date: '2026-10-31', value: -1 },
        { date: '2026-11-02', value: 1 },
      ],
    },
    'America/Los_Angeles'
  );
  assert.deepEqual(
    marks.map((mark) => mark.date),
    ['2026-10-31', '2026-11-01', '2026-11-02']
  );
  assert.equal(new Set(marks.map((mark) => `${mark.x},${mark.y}`)).size, 3);
});
check('calendar: latest duplicate value drives cell, table and quantitative scale', () => {
  const marks = probeCalendar({
    values: [
      { date: '2026-01-01', value: 0 },
      { date: '2026-01-02', value: 999 },
      { date: '2026-01-02', value: 2 },
    ],
  });
  assert.equal(marks.length, 2);
  assert.equal(marks[1].tableValue, 2);
  assert.equal(marks[1].text, '2026-01-02\n2');
  assert.equal(marks[0].fill, '0');
  assert.equal(marks[1].fill, '1');
});
for (const timezone of ['UTC', 'America/Los_Angeles', 'Asia/Tokyo']) {
  check(`calendar: dates and day cells survive ${timezone}`, () => {
    const marks = probeCalendar(calendarData, timezone);
    assert.deepEqual(
      marks.map((mark) => mark.date),
      calendarData.values.map((day) => day.date),
      JSON.stringify({ timezone, marks })
    );
    assert.equal(new Set(marks.map((mark) => `${mark.x},${mark.y}`)).size, 3);
  });
}

const longLabel =
  'Δοκιμή 東京👩🏽‍🔬 — prolonged follow-up <img src="https://invalid.test/synthetic" onerror="alert(1)">';
function region(id, value, x) {
  return {
    type: 'Feature',
    properties: { id, name: id === 'low' ? longLabel : id, score: value },
    geometry: {
      type: 'Polygon',
      coordinates: [
        [
          [x, 0],
          [x + 1, 0],
          [x + 1, 1],
          [x, 1],
          [x, 0],
        ],
      ],
    },
  };
}
const fixtures = {
  network: {
    token: 'NETWORK_DATA',
    assets: ['d3.min.js'],
    rows: 4,
    data: {
      title: 'Synthetic directed relationships',
      directed: true,
      nodes: [
        { id: 'a', label: longLabel, group: 'Input', value: 1 },
        { id: 'b', label: 'Process', group: 'Process', value: 4 },
        { id: 'c', label: 'Output', group: 'Output', value: 9 },
        { id: 'd', label: 'Isolated', group: 'Input', value: 1 },
      ],
      links: [
        { source: 'a', target: 'b', value: 1 },
        { source: 'b', target: 'c', value: 4 },
      ],
    },
  },
  kaplan_meier: {
    token: 'KM_DATA',
    assets: ['d3.min.js'],
    rows: 6,
    data: {
      title: 'Synthetic survival probabilities',
      xAxisLabel: 'Follow-up (months)',
      yAxisLabel: 'Survival probability',
      groups: [
        {
          label: longLabel,
          color: '#556677',
          points: [
            { time: 12, survival: 0.5 },
            { time: 0, survival: 1 },
            { time: 6, survival: 0.8, censored: true },
          ],
        },
        {
          label: 'Comparison',
          points: [
            { time: 0, survival: 1 },
            { time: 6, survival: 0.9 },
            { time: 12, survival: 0.7, censored: true },
          ],
        },
      ],
    },
  },
  dendrogram: {
    token: 'DENDROGRAM_DATA',
    assets: ['d3.min.js'],
    rows: 4,
    data: {
      name: 'Synthetic hierarchy — not branch distances',
      children: [
        {
          name: 'Group A',
          children: [
            { name: longLabel, value: 2 },
            { name: 'Second', value: 7 },
          ],
        },
        {
          name: 'Group B',
          children: [
            { name: 'Third', value: 1 },
            { name: 'Fourth', value: 10 },
          ],
        },
      ],
    },
  },
  heatmap: {
    token: 'HEATMAP_DATA',
    assets: ['d3.min.js'],
    rows: 6,
    data: {
      title: 'Synthetic abundance (units)',
      xAxisLabel: 'Sample',
      yAxisLabel: 'Feature',
      xLabels: [longLabel, 'Sample B', 'Sample C'],
      yLabels: ['Feature A', 'Feature B'],
      values: [
        [0, 5, 10],
        [1, 3, 8],
      ],
    },
  },
  calendar: {
    token: 'CALENDAR_DATA',
    assets: ['d3.min.js'],
    rows: 3,
    data: { title: longLabel, ...calendarData },
  },
  forest: {
    token: 'FOREST_DATA',
    assets: ['d3.min.js'],
    rows: 3,
    data: {
      title: 'Synthetic effect estimates',
      logScale: true,
      referenceLine: 1,
      xAxisLabel: 'Hazard ratio (95% CI)',
      rows: [
        { label: longLabel, estimate: 0.5, lower: 0.25, upper: 0.75, weight: 1 },
        { label: 'Study B', estimate: 1, lower: 0.5, upper: 1.5, weight: 4 },
        { label: 'Study C', estimate: 2, lower: 1, upper: 3, weight: 9 },
      ],
    },
  },
  choropleth: {
    token: 'CHOROPLETH_DATA',
    assets: ['leaflet.min.css', 'leaflet.min.js'],
    rows: 4,
    data: {
      title: 'Synthetic regional rates',
      legendTitle: 'Rate (per 1,000)',
      valueProperty: 'score',
      nameProperty: 'name',
      geojson: {
        type: 'FeatureCollection',
        features: [
          region('low', 0, 0),
          region('middle', 5, 2),
          region('high', 10, 4),
          region('missing', null, 6),
        ],
      },
    },
  },
};

if (process.argv.includes('--browser')) {
  const { chromium } = await import('@playwright/test');
  const assets = Object.fromEntries(
    await Promise.all(
      ['d3.min.js', 'leaflet.min.css', 'leaflet.min.js'].map(async (name) => [
        name,
        await readFile(new URL(`assets/${name}`, templates), 'utf8'),
      ])
    )
  );
  const only = process.argv.find((argument) => argument.startsWith('--only='))?.slice(7);
  assert.ok(!only || names.includes(only), `Unknown renderer ${only}`);
  const browser = await chromium.launch({ headless: true });
  try {
    for (const [name, fixture] of Object.entries(fixtures)) {
      if (only && name !== only) continue;
      for (const theme of ['light', 'dark']) {
        const page = await browser.newPage({
          viewport: { width: 320, height: 1000 },
          colorScheme: theme,
          timezoneId: 'UTC',
        });
        const errors = [];
        page.on('pageerror', (error) => errors.push(error.message));
        page.on('dialog', async (dialog) => {
          errors.push('Untrusted label executed a dialog');
          await dialog.dismiss();
        });
        await page.route('**/*', async (route) => {
          if (/https:\/\/[^/]*tile\.openstreetmap\.org\//.test(route.request().url())) {
            await route.fulfill({
              contentType: 'image/png',
              body: Buffer.from(
                'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/l9sAAAAASUVORK5CYII=',
                'base64'
              ),
            });
          } else {
            errors.push(
              `Blocked synthetic external request: ${new URL(route.request().url()).hostname}`
            );
            await route.abort();
          }
        });
        try {
          const instrumentation =
            name === 'choropleth'
              ? '<script>L.Map.addInitHook(function () { window.__scientificMap = this; });</script>'
              : '';
          const html = sources
            .get(name)
            .replace(
              '{{ASSETS}}',
              () =>
                fixture.assets
                  .map((asset) =>
                    asset.endsWith('.css')
                      ? `<style>${assets[asset]}</style>`
                      : `<script>${assets[asset]}</script>`
                  )
                  .join('\n') + instrumentation
            )
            .replace('{{COMMON}}', () => common)
            .replace(`{{${fixture.token}}}`, () =>
              JSON.stringify(fixture.data).replaceAll('<', '\\u003c')
            );
          await page.setContent(html, { waitUntil: 'load' });
          await page.waitForTimeout(name === 'network' ? 600 : 200);
          await page.locator('.figure-data > summary').click();
          for (const width of [320, 1200, 480]) {
            await page.setViewportSize({ width, height: 1000 });
            await page.waitForTimeout(150);
            const metrics = await page.evaluate(() => {
              const table = document.querySelector('#figureData');
              const visibleLabels = [...document.querySelectorAll('svg text')]
                .filter((node) => node.textContent && getComputedStyle(node).display !== 'none')
                .map((node) => ({
                  text: node.textContent,
                  size: parseFloat(getComputedStyle(node).fontSize),
                  box: node.getBoundingClientRect().toJSON(),
                }));
              return {
                scrollWidth: document.documentElement.scrollWidth,
                theme: window.BioRouterViz.theme,
                labels: visibleLabels,
                rows: table?.querySelectorAll('tbody tr').length ?? 0,
                tableText: table?.textContent ?? '',
                tableFont: table ? parseFloat(getComputedStyle(table).fontSize) : 0,
                alerts: [...document.querySelectorAll('[role="alert"]')].map(
                  (node) => node.textContent
                ),
                injected: document.querySelectorAll('img[src*="invalid.test"]').length,
              };
            });
            check(`${name}/${theme}/${width}: layout and readable data`, () => {
              assert.deepEqual(errors, []);
              assert.deepEqual(metrics.alerts, []);
              assert.equal(metrics.injected, 0);
              assert.equal(metrics.theme, theme);
              assert.ok(metrics.scrollWidth <= width + 1);
              assert.ok(
                metrics.rows >= fixture.rows && metrics.rows <= 500,
                JSON.stringify({ rows: metrics.rows, expectedAtLeast: fixture.rows })
              );
              assert.ok(metrics.tableFont >= 14);
              if (name !== 'calendar')
                assert.ok(
                  metrics.tableText.includes(longLabel),
                  'Full label remains in the data alternative'
                );
              assert.ok(
                metrics.labels.every((label) => label.size >= 14),
                'Visible SVG labels must be readable'
              );
              for (let index = 0; index < metrics.labels.length; index++) {
                const box = metrics.labels[index].box;
                for (const previous of metrics.labels.slice(0, index))
                  assert.ok(
                    !(
                      box.left < previous.box.right &&
                      box.right > previous.box.left &&
                      box.top < previous.box.bottom &&
                      box.bottom > previous.box.top
                    ),
                    'Visible SVG labels overlap'
                  );
              }
            });
            if (process.env.AUTOVIS_SCREENSHOT_DIR)
              await page.screenshot({
                path: `${process.env.AUTOVIS_SCREENSHOT_DIR}/scientific-${name}-${theme}-${width}.png`,
                fullPage: true,
              });
          }
          const data = await page.evaluate((kind) => {
            const attrs = (node, fields) =>
              Object.fromEntries(fields.map((field) => [field, Number(node.getAttribute(field))]));
            if (kind === 'network')
              return {
                nodes: [...document.querySelectorAll('#graph circle')].map((node) => ({
                  id: node.__data__.id,
                  group: node.__data__.group,
                  radius: Number(node.getAttribute('r')),
                })),
                links: [...document.querySelectorAll('#graph line')].map((node) => ({
                  source: node.__data__.source.id,
                  target: node.__data__.target.id,
                  arrow: node.getAttribute('marker-end'),
                  tipClearance:
                    Math.hypot(
                      Number(node.getAttribute('x2')) - node.__data__.target.x,
                      Number(node.getAttribute('y2')) - node.__data__.target.y
                    ) +
                    (((Number(document.querySelector('#arrow').getAttribute('refX')) - 10) *
                      Number(document.querySelector('#arrow').getAttribute('markerWidth'))) /
                      10) *
                      (document.querySelector('#arrow').getAttribute('markerUnits') ===
                      'userSpaceOnUse'
                        ? 1
                        : Number(node.getAttribute('stroke-width'))) -
                    Number(
                      [...document.querySelectorAll('#graph circle')]
                        .find((circle) => circle.__data__.id === node.__data__.target.id)
                        .getAttribute('r')
                    ),
                })),
              };
            if (kind === 'kaplan_meier')
              return {
                curves: [...document.querySelectorAll('#chart path')]
                  .filter((node) => Array.isArray(node.__data__))
                  .map((node) => ({
                    points: node.__data__,
                    stroke: node.getAttribute('stroke'),
                    path: node.getAttribute('d'),
                  })),
                censorMarks: document.querySelectorAll('#chart > g > line').length,
              };
            if (kind === 'dendrogram')
              return {
                nodes: [...document.querySelectorAll('#chart > g > g')].map((node) => ({
                  name: node.__data__.data.name,
                  leaf: !node.__data__.children,
                })),
                links: document.querySelectorAll('#chart path.link').length,
              };
            if (kind === 'heatmap')
              return [...document.querySelectorAll('#chart > g:first-of-type > rect')].map(
                (node) => ({ ...attrs(node, ['x', 'y']), fill: node.getAttribute('fill') })
              );
            if (kind === 'calendar')
              return [...document.querySelectorAll('#chart > g > rect')].map((node) =>
                attrs(node, ['x', 'y'])
              );
            if (kind === 'forest')
              return {
                markers: [...document.querySelectorAll('#chart > g > rect')].map((node) =>
                  attrs(node, ['x', 'width'])
                ),
                intervals: [
                  ...document.querySelectorAll('#chart > g > line:not([stroke-dasharray])'),
                ]
                  .filter((node) => node.getAttribute('y1') === node.getAttribute('y2'))
                  .map((node) => attrs(node, ['x1', 'x2'])),
                reference: Number(
                  document.querySelector('#chart > g > line[stroke-dasharray]')?.getAttribute('x1')
                ),
              };
            if (kind === 'choropleth') {
              const regions = [];
              window.__scientificMap.eachLayer((layer) => {
                if (layer.feature)
                  regions.push({
                    id: layer.feature.properties.id,
                    fill: layer.options.fillColor,
                    inBounds: window.__scientificMap.getBounds().contains(layer.getBounds()),
                  });
              });
              return regions;
            }
          }, name);
          check(`${name}/${theme}: scientific data invariants`, () => {
            if (name === 'network') {
              assert.deepEqual(data.nodes.map((node) => node.id).sort(), ['a', 'b', 'c', 'd']);
              assert.deepEqual(
                data.links.map((link) => [link.source, link.target]),
                [
                  ['a', 'b'],
                  ['b', 'c'],
                ]
              );
              assert.ok(data.links.every((link) => link.arrow));
              assert.ok(
                data.links.every((link) => link.tipClearance >= 1),
                `Arrowheads buried inside target nodes: ${JSON.stringify(data.links)}`
              );
              assert.ok(
                data.nodes[0].radius < data.nodes[1].radius &&
                  data.nodes[1].radius < data.nodes[2].radius
              );
            }
            if (name === 'kaplan_meier') {
              assert.equal(data.curves.length, 2);
              assert.equal(data.curves[0].stroke, '#556677');
              assert.deepEqual(
                data.curves[0].points.map((point) => [point.time, point.survival]),
                [
                  [0, 1],
                  [6, 0.8],
                  [12, 0.5],
                ]
              );
              assert.equal(data.censorMarks, 2);
            }
            if (name === 'dendrogram') {
              assert.equal(data.nodes.length, 7);
              assert.equal(data.nodes.filter((node) => node.leaf).length, 4);
              assert.equal(data.links, 6);
            }
            if (name === 'heatmap') {
              assert.equal(data.length, 6);
              assert.equal(new Set(data.map((cell) => cell.x)).size, 3);
              assert.equal(new Set(data.map((cell) => cell.y)).size, 2);
              const viz = runtime(theme);
              assert.deepEqual(
                data.map((cell) => cell.fill),
                fixture.data.values.flat().map((value) => viz.sequential(value / 10))
              );
            }
            if (name === 'calendar') {
              assert.equal(data.length, 3);
              assert.equal(new Set(data.map((cell) => `${cell.x},${cell.y}`)).size, 3);
            }
            if (name === 'forest') {
              const centers = data.markers.map((marker) => marker.x + marker.width / 2);
              assert.equal(centers.length, 3);
              assert.ok(Math.abs(centers[1] - data.reference) < 0.01);
              assert.ok(Math.abs(centers[1] - centers[0] - (centers[2] - centers[1])) < 0.01);
              assert.ok(
                data.markers[0].width < data.markers[1].width &&
                  data.markers[1].width < data.markers[2].width
              );
              assert.equal(data.intervals.length, 3);
              const x = (value) => data.reference + (centers[2] - centers[1]) * Math.log2(value);
              fixture.data.rows.forEach((row, index) => {
                assert.ok(Math.abs(data.intervals[index].x1 - x(row.lower)) < 0.01);
                assert.ok(Math.abs(data.intervals[index].x2 - x(row.upper)) < 0.01);
              });
            }
            if (name === 'choropleth') {
              assert.equal(data.length, 4);
              assert.ok(
                data.every((region) => region.inBounds),
                'Automatic map bounds include every region'
              );
              const fills = Object.fromEntries(data.map((region) => [region.id, region.fill]));
              const viz = runtime(theme);
              assert.equal(fills.low, viz.sequential(0));
              assert.equal(fills.middle, viz.sequential(0.5));
              assert.equal(fills.high, viz.sequential(1));
              assert.notEqual(fills.missing, fills.low);
            }
          });
          const hover = {
            network: '#graph circle',
            heatmap: '#chart > g:first-of-type > rect',
            calendar: '#chart > g > rect',
            choropleth: '.leaflet-interactive',
          }[name];
          if (hover) {
            await page.setViewportSize({ width: 320, height: 1000 });
            await page.waitForTimeout(250);
            await page.locator(hover).first().hover({ force: true, timeout: 3000 });
            await page.waitForTimeout(150);
            const state = await page.evaluate(() => ({
              injected: document.querySelectorAll('img[src*="invalid.test"]').length,
              text: document.querySelector('.tooltip, .info-box')?.textContent ?? '',
            }));
            check(`${name}/${theme}: hover renders labels as text`, () => {
              assert.deepEqual(errors, []);
              assert.equal(state.injected, 0);
              assert.ok(state.text.length > 0);
              if (name !== 'calendar')
                assert.ok(state.text.includes(longLabel), JSON.stringify(state));
            });
            if (name === 'choropleth') {
              await page.locator(hover).first().click();
              const popup = await page.locator('.leaflet-popup-content').textContent();
              check(
                `${name}/${theme}: popup preserves literal labels without HTML injection`,
                () => {
                  assert.ok(popup.includes(longLabel));
                  assert.deepEqual(errors, []);
                }
              );
            }
          }
          const region = page.getByRole('region').filter({ has: page.locator('#figureData') });
          if (await region.count()) {
            await region.focus();
            await page.keyboard.press('ArrowRight');
            await page.waitForTimeout(120);
            const scroll = await region.evaluate((node) => ({
              left: node.scrollLeft,
              overflowing: node.scrollWidth > node.clientWidth + 1,
            }));
            check(`${name}/${theme}: keyboard data scrolling`, () => {
              if (scroll.overflowing) assert.ok(scroll.left > 0);
            });
          } else
            check(`${name}/${theme}: keyboard data scrolling`, () =>
              assert.fail('No accessible figure-data region')
            );
          if (name === 'network') {
            await page.setViewportSize({ width: 1200, height: 1000 });
            await page.waitForTimeout(250);
            const node = page.locator('#graph circle').first();
            const box = await node.boundingBox();
            const graph = await page.locator('#graph').boundingBox();
            await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
            await page.mouse.down();
            await page.mouse.move(graph.x + 2, graph.y + 2, { steps: 8 });
            await page.mouse.up();
            await page.setViewportSize({ width: 320, height: 1000 });
            await page.waitForTimeout(300);
            const bounds = await page.evaluate(() => {
              const svg = document.querySelector('#graph');
              const frame = svg.getBoundingClientRect();
              return {
                labels: [...svg.querySelectorAll('text')]
                  .filter((node) => node.textContent)
                  .every((node) => {
                    const box = node.getBoundingClientRect();
                    return (
                      box.left >= frame.left &&
                      box.right <= frame.right &&
                      box.top >= frame.top &&
                      box.bottom <= frame.bottom
                    );
                  }),
                nodes: [...svg.querySelectorAll('circle')].every((node) => {
                  const x = Number(node.getAttribute('cx')),
                    y = Number(node.getAttribute('cy')),
                    radius = Number(node.getAttribute('r'));
                  return (
                    x >= radius &&
                    y >= radius &&
                    x + radius <= frame.width &&
                    y + radius <= frame.height
                  );
                }),
              };
            });
            check(
              `${name}/${theme}: drag and resize keep nodes and visible labels inside figure`,
              () => assert.deepEqual(bounds, { labels: true, nodes: true })
            );
          }
          if (name === 'forest' || name === 'heatmap') {
            const edgeData =
              name === 'forest'
                ? {
                    title: 'Close effects and zero weights',
                    xAxisLabel: 'Effect estimate',
                    rows: [
                      {
                        label: longLabel,
                        estimate: 1.00001,
                        lower: 1.000005,
                        upper: 1.000015,
                        weight: 0,
                      },
                      {
                        label: 'Omitted weight',
                        estimate: 1.00002,
                        lower: 1.000015,
                        upper: 1.000025,
                      },
                      {
                        label: 'Unit weight',
                        estimate: 1.00003,
                        lower: 1.000025,
                        upper: 1.000035,
                        weight: 1,
                      },
                    ],
                  }
                : {
                    title: 'Close quantitative values',
                    xLabels: ['A', 'B'],
                    yLabels: [longLabel],
                    values: [[1.00001, 1.00002]],
                  };
            await page.goto('about:blank');
            await page.setContent(
              html.replace(JSON.stringify(fixture.data).replaceAll('<', '\\u003c'), () =>
                JSON.stringify(edgeData).replaceAll('<', '\\u003c')
              ),
              { waitUntil: 'load' }
            );
            await page.locator('.figure-data > summary').click();
            const edge = await page.evaluate(() => ({
              text: [...document.querySelectorAll('#chart text')]
                .map((node) => node.textContent)
                .join(' '),
              table: document.querySelector('#figureData').textContent,
              sizes: [...document.querySelectorAll('#chart > g > rect')].map((node) =>
                Number(node.getAttribute('width'))
              ),
              fills: [...document.querySelectorAll('#chart > g:first-of-type > rect')].map((node) =>
                node.getAttribute('fill')
              ),
              invalid: [...document.querySelectorAll('#chart *')].some((node) =>
                [...node.attributes].some((attr) => /NaN|Infinity/.test(attr.value))
              ),
              alerts: [...document.querySelectorAll('[role="alert"]')].map(
                (node) => node.textContent
              ),
              boxes: [...document.querySelectorAll('#chart text')]
                .filter((node) => node.textContent)
                .map((node) => node.getBoundingClientRect().toJSON()),
            }));
            check(
              `${name}/${theme}: close values and zero weights preserve scientific meaning`,
              () => {
                assert.deepEqual(errors, []);
                assert.deepEqual(edge.alerts, []);
                assert.equal(edge.invalid, false);
                assert.ok(
                  edge.text.includes('1.00001') && edge.text.includes('1.00002'),
                  edge.text
                );
                assert.ok(edge.table.includes('1.00001') && edge.table.includes('1.00002'));
                if (name === 'forest') assert.deepEqual(edge.sizes, [5, 13, 13]);
                else assert.notEqual(edge.fills[0], edge.fills[1]);
                edge.boxes.forEach((box, index) => {
                  for (const other of edge.boxes.slice(0, index))
                    assert.ok(
                      !(
                        box.left < other.right &&
                        box.right > other.left &&
                        box.top < other.bottom &&
                        box.bottom > other.top
                      ),
                      'Close-value labels must not overlap'
                    );
                });
              }
            );
            if (process.env.AUTOVIS_SCREENSHOT_DIR)
              await page.screenshot({
                path: `${process.env.AUTOVIS_SCREENSHOT_DIR}/scientific-${name}-${theme}-close-values.png`,
                fullPage: true,
              });
          }
        } catch (error) {
          check(`${name}/${theme}: fixture executes`, () => {
            throw error;
          });
        } finally {
          await page.close();
        }
      }
    }
  } finally {
    await browser.close();
  }
}
const failed = results.filter((result) => !result.passed).length;
console.log(`Scientific renderer checks: ${results.length - failed} passed; ${failed} failed.`);
if (failed) process.exitCode = 1;
