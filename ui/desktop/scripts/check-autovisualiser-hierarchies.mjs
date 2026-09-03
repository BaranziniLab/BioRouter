import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

// Tiny executable baseline by default. Browser geometry is opt-in and requires its own lease.
const root = new URL(
  '../../../crates/biorouter-mcp/src/autovisualiser/templates/',
  import.meta.url
);
const names = ['boxplot', 'sunburst', 'wordcloud'];
const sources = Object.fromEntries(
  await Promise.all(
    names.map(async (name) => [
      name,
      await readFile(new URL(`${name}_template.html`, root), 'utf8'),
    ])
  )
);
const d3Source = await readFile(new URL('assets/d3.min.js', root), 'utf8');
const common = await readFile(new URL('_common.js', root), 'utf8');
const d3Context = vm.createContext({ setTimeout, clearTimeout });
vm.runInContext(d3Source, d3Context);
const results = [];
function check(name, run) {
  try {
    run();
    results.push(true);
    console.log(`PASS ${name}`);
  } catch (error) {
    results.push(false);
    console.error(`FAIL ${name}: ${error.message.replaceAll('\n', ' ').slice(0, 500)}`);
  }
}
const label = 'Δοκιμή 東京👩🏽‍🔬 <img src="https://invalid.test/test" onerror="alert(1)">';
const tokens = { boxplot: 'BOXPLOT_DATA', sunburst: 'SUNBURST_DATA', wordcloud: 'WORDCLOUD_DATA' };
function render(name, data) {
  const marks = [],
    html = [],
    tables = [],
    elements = new Map();
  function element(tag = '', datum) {
    return {
      tag,
      datum,
      attrs: {},
      styles: {},
      events: {},
      textContent: '',
      style: {},
      getComputedTextLength() {
        return this.textContent.length * 8;
      },
    };
  }
  function select(nodes = [element()]) {
    let pending;
    const selection = {
      append(tag) {
        const next = nodes.map((node) => element(tag, node.datum));
        marks.push(...next);
        return select(next);
      },
      selectAll() {
        return select([]);
      },
      data(values) {
        pending = values;
        return selection;
      },
      join(tag) {
        const next = pending.map((datum) => element(tag, datum));
        marks.push(...next);
        return select(next);
      },
      attr(key, value) {
        nodes.forEach((node, i) => {
          node.attrs[key] = typeof value === 'function' ? value.call(node, node.datum, i) : value;
        });
        return selection;
      },
      style(key, value) {
        nodes.forEach((node, i) => {
          node.styles[key] = typeof value === 'function' ? value.call(node, node.datum, i) : value;
        });
        return selection;
      },
      text(value) {
        nodes.forEach((node, i) => {
          node.textContent = typeof value === 'function' ? value.call(node, node.datum, i) : value;
        });
        return selection;
      },
      html(value) {
        html.push(value);
        return selection;
      },
      on(key, callback) {
        nodes.forEach((node) => {
          node.events[key] = callback;
        });
        return selection;
      },
      each(callback) {
        nodes.forEach((node, i) => callback.call(node, node.datum, i));
        return selection;
      },
      call() {
        return selection;
      },
      node() {
        return nodes[0];
      },
    };
    return selection;
  }
  const document = {
    getElementById(id) {
      if (!elements.has(id)) elements.set(id, element(id));
      return elements.get(id);
    },
    createElement() {
      return {
        getContext() {
          return {
            font: '',
            measureText(text) {
              return {
                width:
                  String(text).length * (parseFloat(this.font.match(/[\d.]+px/)?.[0]) || 14) * 0.6,
              };
            },
          };
        },
      };
    },
  };
  const viz = {
    autoResize() {},
    guard(fn) {
      fn();
    },
    applyPageTheme() {},
    applyScientificStyles() {},
    reportSize() {},
    palette: ['#416b80', '#a46d48', '#527967'],
    colors: { text: '#222', surface: '#fff', muted: '#666' },
    dark: false,
    fitSvgLabel(node, text) {
      node.textContent = text;
    },
    hideOverlappingSvgLabels() {},
    formatScaleValues: (values) => values.map(String),
    renderFigureData(_table, headers, rows, caption) {
      tables.push({ headers, rows: Array.from(rows), caption });
    },
  };
  const script = [...sources[name].matchAll(/<script>([\s\S]*?)<\/script>/g)].at(-1)[1];
  vm.runInNewContext(script.replace(`{{${tokens[name]}}}`, JSON.stringify(data)), {
    d3: {
      ...d3Context.d3,
      select: (selector) => select([document.getElementById(String(selector).replace(/^#/, ''))]),
    },
    document,
    BioRouterViz: viz,
    getComputedStyle: () => ({ fontFamily: '-apple-system, sans-serif', fontWeight: '600' }),
  });
  return { marks, html, tables, elements };
}
for (const name of names) {
  check(`${name}: academic shared shell and accessible data`, () => {
    assert.match(sources[name], /applyScientificStyles/);
    assert.match(sources[name], /renderFigureData/);
    assert.match(sources[name], /tabindex="0" role="region"/);
  });
  check(`${name}: supplied labels never enter HTML`, () =>
    assert.doesNotMatch(sources[name], /\.html\(/)
  );
}
check('boxplot: mixed empty group keeps identity without invalid geometry', () => {
  const result = render('boxplot', {
    groups: [
      { label: 'Empty', values: [] },
      { label: 'Observed', values: [1, 2, 3] },
    ],
  });
  assert.ok(
    result.marks.every((mark) =>
      Object.values(mark.attrs).every(
        (value) => typeof value !== 'number' || Number.isFinite(value)
      )
    )
  );
  assert.ok(result.marks.some((mark) => mark.textContent === 'No observations'));
  assert.ok(result.tables[0].rows.some((row) => row.includes('Empty') && row.includes(0)));
});
check('boxplot: duplicate labels retain distinct bands', () => {
  const result = render('boxplot', {
    groups: [
      { label: 'Same', values: [1, 2] },
      { label: 'Same', values: [10, 20] },
    ],
  });
  const boxes = result.marks.filter((mark) => mark.tag === 'rect');
  assert.equal(new Set(boxes.map((mark) => mark.attrs.x)).size, 2);
});
check('boxplot: close statistics stay distinguishable in hover text', () => {
  const result = render('boxplot', { groups: [{ label, values: [1.00001, 1.00002] }] });
  const box = result.marks.find((mark) => mark.tag === 'rect');
  box.events.mouseover({ pageX: 0, pageY: 0 });
  assert.equal(result.html.length, 0);
  const tooltip = result.elements.get('tooltip').textContent;
  const values = result.tables[0].rows[0].slice(4, 7);
  assert.equal(new Set(values).size, 3);
  for (const value of values) assert.ok(tooltip.includes(String(value)), tooltip);
});
check('boxplot: quartiles and Tukey whiskers retain the established formula', () => {
  const result = render('boxplot', { groups: [{ label: 'Observed', values: [1, 2, 3, 4, 100] }] });
  const row = result.tables[0]?.rows[0];
  assert.ok(row);
  for (const value of [1, 2, 3, 4, 100])
    assert.ok(row.includes(value) || row.includes(String(value)), JSON.stringify(row));
});
check('sunburst: deep hierarchy never receives negative opacity', () => {
  let data = { name: 'Leaf', value: 1 };
  for (let i = 0; i < 10; i++) data = { name: `Level ${i}`, children: [data] };
  const result = render('sunburst', data);
  assert.ok(
    result.marks
      .filter((mark) => mark.tag === 'path')
      .every((mark) => (mark.attrs['fill-opacity'] ?? 1) >= 0.5)
  );
});
check('sunburst: zero total has an explicit compact state and retained values', () => {
  const result = render('sunburst', { name: 'Zero total', children: [{ name: label, value: 0 }] });
  assert.ok(
    [...result.elements.values()].some((node) =>
      String(node.textContent).includes('All values are zero')
    )
  );
  assert.ok(result.tables[0].rows.some((row) => row.includes(label)));
});
check('sunburst: own and additive aggregate values are independently available', () => {
  const result = render('sunburst', {
    name: 'Root',
    value: 2,
    children: [
      { name: label, value: 3 },
      { name: 'Other', value: 1 },
    ],
  });
  const rows = result.tables[0]?.rows;
  assert.ok(rows?.some((row) => row.includes('Root') && row.includes(2) && row.includes(6)));
});
check('sunburst: absent values are not described as observed zeros', () => {
  const result = render('sunburst', { name: 'No supplied values' });
  assert.match(result.elements.get('layoutNote').textContent, /No values supplied/);
});

check('sunburst: data guidance discloses the bounded table instead of promising every node', () => {
  const result = render('sunburst', {
    name: 'Root',
    children: [{ name: 'Observed', value: 1 }],
  });
  const note = result.elements.get('layoutNote').textContent;
  assert.match(note, /bounded data table/);
  assert.match(note, /caption states any row limit/);
  assert.doesNotMatch(note, /every displayed node/);
});
check('wordcloud: oversized omitted words remain in table with disclosure', () => {
  const oversized = '東京'.repeat(800);
  const result = render('wordcloud', {
    words: [
      { text: oversized, weight: 10 },
      { text: 'Short', weight: 1 },
    ],
  });
  assert.ok(result.tables[0]?.rows.some((row) => row.includes(oversized)));
  assert.ok(
    [...result.elements.values()].some((node) =>
      /1.*(?:omitted|not placed)/i.test(node.textContent)
    )
  );
});
check('wordcloud: zero remains legitimate and smallest font is readable', () => {
  const result = render('wordcloud', {
    words: [
      { text: 'Zero', weight: 0 },
      { text: 'High', weight: 9 },
    ],
  });
  const text = result.marks.filter((mark) => mark.tag === 'text');
  assert.equal(text.length, 2);
  assert.ok(text.every((mark) => parseFloat(mark.styles['font-size']) >= 14));
  assert.ok(result.tables[0]?.rows.some((row) => row.includes('Zero') && row.includes(0)));
});
check('wordcloud: an all-zero input uses minimum visible font size', () => {
  const result = render('wordcloud', { words: [{ text: 'Zero', weight: 0 }] });
  assert.equal(
    parseFloat(result.marks.find((mark) => mark.tag === 'text').styles['font-size']),
    14
  );
});

if (process.argv.includes('--browser')) {
  const { chromium } = await import('@playwright/test');
  const fixtures = {
    boxplot: {
      title: 'Synthetic group distributions',
      yAxisLabel: 'Observed value (units)',
      groups: [
        { label: 'Empty', values: [] },
        { label: 'Repeated', values: [1, 2, 3, 4, 100] },
        { label: 'Repeated', values: [1.00001, 1.00002] },
        { label, values: [0, 0, 0] },
      ],
    },
    sunburst: {
      name: 'Synthetic additive hierarchy',
      value: 2,
      children: [
        { name: label, value: 3 },
        { name: 'Comparison', value: 1 },
      ],
    },
    wordcloud: {
      title: 'Synthetic weighted terms',
      words: [
        { text: 'Research', weight: 9 },
        { text: 'Values', weight: 4 },
        { text: 'Reference', weight: 1 },
        { text: label, weight: 0 },
      ],
    },
  };
  const browser = await chromium.launch({ headless: true });
  try {
    for (const name of names)
      for (const theme of ['light', 'dark']) {
        const page = await browser.newPage({
          viewport: { width: 320, height: 1000 },
          colorScheme: theme,
        });
        page.setDefaultTimeout(3000);
        const errors = [];
        page.on('pageerror', (error) => errors.push(error.message));
        page.on('dialog', async (dialog) => {
          errors.push('Injected dialog');
          await dialog.dismiss();
        });
        await page.route('**/*', async (route) => {
          errors.push('Unexpected network request');
          await route.abort();
        });
        const htmlFor = (data) =>
          sources[name]
            .replace('{{ASSETS}}', () => `<script>${d3Source}</script>`)
            .replace('{{COMMON}}', () => common)
            .replace(`{{${tokens[name]}}}`, () => JSON.stringify(data).replaceAll('<', '\\u003c'));
        try {
          await page.setContent(htmlFor(fixtures[name]), { waitUntil: 'load' });
          await page.locator('.figure-data > summary').click();
          for (const width of [320, 1200, 480]) {
            await page.setViewportSize({ width, height: 1000 });
            await page.waitForTimeout(100);
            const metrics = await page.evaluate(() => {
              const svg = document.querySelector('#chart');
              const boundary = svg.getBoundingClientRect().toJSON();
              const labels = [...svg.querySelectorAll('text')]
                .filter((node) => node.textContent)
                .map((node) => ({
                  text: node.textContent,
                  size: parseFloat(getComputedStyle(node).fontSize),
                  box: node.getBoundingClientRect().toJSON(),
                }));
              return {
                labels,
                boundary,
                scrollWidth: document.documentElement.scrollWidth,
                table: document.querySelector('#figureData').textContent,
                injected: document.querySelectorAll('img').length,
                alerts: [...document.querySelectorAll('[role="alert"]')].map(
                  (node) => node.textContent
                ),
                invalid: [...svg.querySelectorAll('*')].some((node) =>
                  [...node.attributes].some((attr) => /NaN|Infinity/.test(attr.value))
                ),
              };
            });
            check(
              `${name}/${theme}/${width}: readable non-overlapping layout and full literal labels`,
              () => {
                assert.deepEqual(errors, []);
                assert.deepEqual(metrics.alerts, []);
                assert.equal(metrics.injected, 0);
                assert.equal(metrics.invalid, false);
                assert.ok(metrics.scrollWidth <= width + 1);
                assert.ok(metrics.table.includes(label));
                metrics.labels.forEach((current, index) => {
                  assert.ok(current.size >= 14);
                  const box = current.box,
                    boundary = metrics.boundary;
                  assert.ok(
                    box.left >= boundary.left - 1 &&
                      box.right <= boundary.right + 1 &&
                      box.top >= boundary.top - 1 &&
                      box.bottom <= boundary.bottom + 1,
                    current.text
                  );
                  for (const previous of metrics.labels.slice(0, index)) {
                    const other = previous.box;
                    assert.ok(
                      !(
                        box.left < other.right &&
                        box.right > other.left &&
                        box.top < other.bottom &&
                        box.bottom > other.top
                      ),
                      `${current.text} overlaps ${previous.text}`
                    );
                  }
                });
              }
            );
            if (process.env.AUTOVIS_SCREENSHOT_DIR)
              await page.screenshot({
                path: `${process.env.AUTOVIS_SCREENSHOT_DIR}/${name}-${theme}-${width}.png`,
                fullPage: true,
              });
          }
          await page.setViewportSize({ width: 1200, height: 1000 });
          const invariant = await page.evaluate((kind) => {
            const rows = [...document.querySelectorAll('#figureData tbody tr')].map((row) =>
              [...row.cells].map((cell) => cell.textContent)
            );
            if (kind === 'boxplot')
              return {
                rows,
                x: [...document.querySelectorAll('#chart rect')].map((node) =>
                  Number(node.getAttribute('x'))
                ),
              };
            if (kind === 'sunburst')
              return {
                rows,
                sectors: [...document.querySelectorAll('#chart path')].map((node) => ({
                  value: node.__data__.value,
                  angle: node.__data__.x1 - node.__data__.x0,
                  opacity: Number(node.getAttribute('fill-opacity')),
                })),
              };
            return {
              rows,
              fonts: [...document.querySelectorAll('#chart text')].map((node) => ({
                text: node.textContent,
                size: parseFloat(getComputedStyle(node).fontSize),
              })),
            };
          }, name);
          check(`${name}/${theme}: actual rendered data invariants`, () => {
            if (name === 'boxplot') {
              assert.equal(invariant.rows.length, 4);
              assert.equal(invariant.rows[0][2], '0');
              assert.deepEqual(invariant.rows[1].slice(3), ['1', '2', '3', '4', '4', '100']);
              assert.equal(new Set(invariant.x).size, 3);
            } else if (name === 'sunburst') {
              assert.equal(invariant.rows[0][3], '2');
              assert.equal(invariant.rows[0][4], '6');
              assert.equal(invariant.sectors.length, 2);
              invariant.sectors.forEach((sector) => {
                assert.ok(Math.abs(sector.angle - (sector.value / 6) * 2 * Math.PI) < 1e-9);
                assert.ok(sector.opacity >= 0.5);
              });
            } else {
              const sizes = Object.fromEntries(
                invariant.fonts.map((font) => [font.text, font.size])
              );
              assert.ok(
                sizes.Research > sizes.Values &&
                  sizes.Values > sizes.Reference &&
                  sizes.Reference > sizes[label]
              );
              assert.equal(sizes[label], 14);
            }
          });
          const hover =
            name === 'boxplot'
              ? page.locator('#chart rect').last()
              : name === 'sunburst'
                ? page.locator('#chart path').first()
                : page.locator('#chart text').filter({ hasText: label });
          if (name === 'sunburst') {
            const point = await hover.evaluate((node) => {
              const datum = node.__data__;
              const angle = (datum.x0 + datum.x1) / 2;
              const radius = (datum.y0 + datum.y1) / 2;
              const point = new DOMPoint(
                Math.sin(angle) * radius,
                -Math.cos(angle) * radius
              ).matrixTransform(node.getScreenCTM());
              return { x: point.x, y: point.y };
            });
            await page.mouse.move(point.x, point.y);
          } else await hover.hover();
          const tooltip = await page.locator('#tooltip').textContent();
          check(`${name}/${theme}: actual hover preserves literal untrusted label`, () => {
            assert.ok(tooltip.includes(label));
            assert.deepEqual(errors, []);
          });
          await page.setViewportSize({ width: 320, height: 1000 });
          const region = page.getByRole('region', { name: 'Figure data', exact: true });
          await region.focus();
          await page.keyboard.press('ArrowRight');
          await page.waitForTimeout(100);
          const scroll = await region.evaluate((node) => ({
            left: node.scrollLeft,
            overflow: node.scrollWidth > node.clientWidth,
          }));
          check(`${name}/${theme}: keyboard data scrolling`, () => {
            if (scroll.overflow) assert.ok(scroll.left > 0);
          });
          if (name !== 'boxplot') {
            const edgeData =
              name === 'sunburst'
                ? { name: 'All zero', children: [{ name: label, value: 0 }] }
                : {
                    words: [
                      { text: '東京'.repeat(800), weight: 10 },
                      { text: 'Short', weight: 1 },
                    ],
                  };
            await page.goto('about:blank');
            await page.setContent(htmlFor(edgeData), { waitUntil: 'load' });
            await page.locator('.figure-data > summary').click();
            const edge = await page.evaluate(() => ({
              note: document.querySelector('#layoutNote').textContent,
              hidden: getComputedStyle(document.querySelector('#chart')).display === 'none',
              rows: document.querySelectorAll('#figureData tbody tr').length,
              alerts: [...document.querySelectorAll('[role="alert"]')].map(
                (node) => node.textContent
              ),
            }));
            check(`${name}/${theme}: empty or omitted data state stays explicit`, () => {
              assert.deepEqual(edge.alerts, []);
              if (name === 'sunburst') {
                assert.match(edge.note, /All values are zero/);
                assert.equal(edge.hidden, true);
              } else {
                assert.match(edge.note, /1 omitted/);
                assert.equal(edge.rows, 2);
              }
            });
            if (process.env.AUTOVIS_SCREENSHOT_DIR)
              await page.screenshot({
                path: `${process.env.AUTOVIS_SCREENSHOT_DIR}/${name}-${theme}-edge.png`,
                fullPage: true,
              });
            let denseData;
            if (name === 'sunburst') {
              denseData = { name: label, value: 1 };
              for (let depth = 0; depth < 10; depth++)
                denseData = { name: `Level ${depth}`, children: [denseData] };
            } else
              denseData = {
                words: Array.from({ length: 120 }, (_, index) => ({
                  text: `Term ${String(index).padStart(3, '0')}`,
                  weight: index % 10,
                })),
              };
            await page.goto('about:blank');
            await page.setContent(htmlFor(denseData), { waitUntil: 'load' });
            await page.locator('.figure-data > summary').click();
            const dense = await page.evaluate(() => ({
              boxes: [...document.querySelectorAll('#chart text')]
                .filter((node) => node.textContent)
                .map((node) => node.getBoundingClientRect().toJSON()),
              opacities: [...document.querySelectorAll('#chart path')].map((node) =>
                Number(node.getAttribute('fill-opacity'))
              ),
              rows: document.querySelectorAll('#figureData tbody tr').length,
              note: document.querySelector('#layoutNote').textContent,
              alerts: [...document.querySelectorAll('[role="alert"]')].map(
                (node) => node.textContent
              ),
            }));
            check(
              `${name}/${theme}: deep hierarchy or dense terms retain readable bounded alternatives`,
              () => {
                assert.deepEqual(errors, []);
                assert.deepEqual(dense.alerts, []);
                if (name === 'sunburst') {
                  assert.equal(dense.opacities.length, 10);
                  assert.ok(dense.opacities.every((value) => value >= 0.5));
                  assert.equal(dense.rows, 11);
                } else {
                  assert.equal(dense.rows, 120);
                  assert.ok(dense.boxes.length > 0);
                  assert.match(dense.note, /of 120 terms placed/);
                }
                dense.boxes.forEach((box, index) => {
                  for (const other of dense.boxes.slice(0, index))
                    assert.ok(
                      !(
                        box.left < other.right &&
                        box.right > other.left &&
                        box.top < other.bottom &&
                        box.bottom > other.top
                      ),
                      'Dense labels overlap'
                    );
                });
              }
            );
            if (process.env.AUTOVIS_SCREENSHOT_DIR)
              await page.screenshot({
                path: `${process.env.AUTOVIS_SCREENSHOT_DIR}/${name}-${theme}-dense.png`,
                fullPage: true,
              });
          }
        } catch (error) {
          check(`${name}/${theme}: browser fixture executes`, () => {
            throw error;
          });
        } finally {
          await page.close();
        }
      }
  } finally {
    await browser.close();
  }
}

const failed = results.filter((passed) => !passed).length;
console.log(`Hierarchy renderer checks: ${results.length - failed} passed; ${failed} failed.`);
if (failed) process.exitCode = 1;
