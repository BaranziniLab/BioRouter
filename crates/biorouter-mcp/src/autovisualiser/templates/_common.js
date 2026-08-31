/*
 * Shared runtime injected into every Auto Visualiser template via {{COMMON}}.
 *
 * Provides:
 *   - BioRouterViz.theme       : 'light' | 'dark' resolved from the iframe host query / prefers-color-scheme
 *   - BioRouterViz.palette     : categorical colour palette (theme-aware)
 *   - BioRouterViz.reportSize  : posts `ui-size-change` to the parent frame. An enclosing
 *                                dashboard report (dashboard_template.html) listens and grows
 *                                this panel's iframe to fit; displayed standalone in the
 *                                artifact side panel the message is inert, because the panel
 *                                sizes the frame itself and installs no listener.
 *   - BioRouterViz.autoResize  : wires reportSize to load / ResizeObserver / window resize
 *   - BioRouterViz.showError   : renders a friendly error card instead of a blank/broken frame
 *   - BioRouterViz.guard       : runs a draw fn, catching + surfacing any exception
 *
 * Every template should call BioRouterViz.autoResize() and wrap its draw logic in
 * BioRouterViz.guard(...) so a single bad data point degrades gracefully instead of
 * producing an empty visualization (a common cause of "visualization cannot be generated").
 */
(function () {
  function resolveTheme() {
    var valid = function (v) {
      return v === 'dark' || v === 'light' ? v : null;
    };
    // Precedence, highest first:
    //  1. `window.__BR_VIZ_THEME__` — a theme the render tool baked in (the agent
    //     was asked for a specific look), or the report propagating its own theme
    //     down to a panel. A locked choice; beats everything else.
    //  2. `?theme=` — the standalone-window / file:// host passes the app theme here.
    //  3. `window.__BR_VIZ_HOST_THEME__` — the desktop host's app theme, injected by
    //     the in-chat renderer into the srcdoc preview (which has no query string).
    //     This is what keeps the side-panel preview and the expanded view identical.
    //  4. OS `prefers-color-scheme`, then a light default.
    try {
      var forced = valid(window.__BR_VIZ_THEME__);
      if (forced) return forced;
    } catch (e) {
      /* ignore */
    }
    try {
      var t = valid(new URLSearchParams(window.location.search).get('theme'));
      if (t) return t;
    } catch (e) {
      /* ignore */
    }
    try {
      var host = valid(window.__BR_VIZ_HOST_THEME__);
      if (host) return host;
    } catch (e) {
      /* ignore */
    }
    try {
      if (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) {
        return 'dark';
      }
    } catch (e) {
      /* ignore */
    }
    return 'light';
  }

  var theme = resolveTheme();
  var dark = theme === 'dark';

  // Restrained categorical inks; the first pair remains distinct without red/green.
  var palette = dark
    ? ['#8bb4ca', '#d4a17b', '#93b6a5', '#b5a4ca', '#c9b877', '#b7b9b5']
    : ['#416b80', '#a46d48', '#527967', '#78658d', '#8a783c', '#656761'];

  var colors = {
    bg: dark ? '#1b1b19' : '#ffffff',
    surface: dark ? '#1b1b19' : '#ffffff',
    text: dark ? '#f4f0e6' : '#2a2520',
    muted: dark ? '#b2aaa0' : '#635c54',
    grid: dark ? 'rgba(231,226,218,0.12)' : 'rgba(42,37,32,0.09)',
    border: dark ? '#302f2c' : '#e4e4e0',
    tooltipBg: dark ? 'rgba(20,22,28,0.95)' : 'rgba(0,0,0,0.82)',
    tooltipText: '#ffffff',
  };

  // Expose theme colours as CSS custom properties so templates can use var(--bg) etc.
  try {
    var root = document.documentElement;
    root.style.setProperty('--bg', colors.bg);
    root.style.setProperty('--surface', colors.surface);
    root.style.setProperty('--text', colors.text);
    root.style.setProperty('--muted', colors.muted);
    root.style.setProperty('--border', colors.border);
    root.style.setProperty('--grid', colors.grid);
    root.style.setProperty('--tooltip-bg', colors.tooltipBg);
    root.style.setProperty('--tooltip-text', colors.tooltipText);
  } catch (e) {
    /* ignore */
  }

  function reportSize() {
    document.querySelectorAll('.figure-scroll, .table-scroll').forEach(function (container) {
      var hint = container.previousElementSibling;
      if (!hint || !hint.classList.contains('figure-scroll-hint')) {
        hint = document.createElement('p');
        hint.className = 'figure-scroll-hint';
        hint.style.cssText = 'margin:6px 0;color:var(--muted);font-size:12px;';
        hint.textContent = 'Scroll horizontally to view the full figure or table.';
        container.before(hint);
      }
      hint.hidden = container.scrollWidth <= container.clientWidth + 1;
    });
    var h = Math.max(
      document.body ? document.body.scrollHeight : 0,
      document.body ? document.body.offsetHeight : 0,
      document.documentElement.clientHeight,
      document.documentElement.scrollHeight,
      document.documentElement.offsetHeight
    );
    if (window.parent !== window) {
      // Read by an enclosing dashboard report, which sizes this panel's iframe.
      // Nobody listens when the figure stands alone in the artifact panel.
      window.parent.postMessage({ type: 'ui-size-change', payload: { height: h } }, '*');
    }
  }

  function autoResize() {
    setTimeout(reportSize, 80);
    setTimeout(reportSize, 400);
    if (typeof ResizeObserver !== 'undefined') {
      var ro = new ResizeObserver(function () { reportSize(); });
      ro.observe(document.body);
      ro.observe(document.documentElement);
    }
    window.addEventListener('resize', reportSize);
  }

  function normalizeErrorDetail(detail) {
    if (!detail) return '';
    if (detail && detail.stack) return String(detail.stack);
    if (detail && detail.message) return String(detail.message);
    return String(detail);
  }

  function reportRenderError(message, detail) {
    if (window.parent === window) return;
    try {
      window.parent.postMessage({
        type: 'biorouter-viz-render-error',
        payload: {
          message: message || 'This visualization could not be rendered.',
          detail: normalizeErrorDetail(detail),
          title: document.title || 'Visualization',
          href: window.location.href,
        },
      }, '*');
    } catch (e) {
      /* ignore */
    }
  }

  function showError(message, detail) {
    var host = document.querySelector('.viz-root') || document.body;
    var card = document.createElement('div');
    card.setAttribute('role', 'alert');
    card.style.cssText =
      'margin:16px;padding:18px 20px;border-radius:12px;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;' +
      'border:1px solid ' + (dark ? '#5b2a2a' : '#f3c2c2') + ';' +
      'background:' + (dark ? '#2a1f22' : '#fff5f5') + ';color:' + (dark ? '#f3b0b0' : '#9b2c2c') + ';';
    var title = document.createElement('div');
    title.style.cssText = 'font-weight:600;margin-bottom:6px;';
    title.textContent = '⚠ ' + (message || 'This visualization could not be rendered.');
    card.appendChild(title);
    if (detail) {
      var d = document.createElement('div');
      d.style.cssText = 'font-size:12px;opacity:0.85;white-space:pre-wrap;word-break:break-word;';
      d.textContent = String(detail);
      card.appendChild(d);
    }
    host.appendChild(card);
    reportSize();
    reportRenderError(message, detail);
  }

  function guard(fn) {
    try {
      fn();
    } catch (err) {
      console.error('[BioRouterViz] render failed:', err);
      showError('This visualization could not be rendered.', err && err.message ? err.message : err);
    }
  }

  // Blanket safety net: any uncaught error or rejected promise during rendering
  // surfaces as a friendly card instead of a blank/broken frame. Individual
  // templates can still call guard()/showError() for finer-grained handling.
  var errorShown = false;
  function handleGlobalError(detail) {
    if (errorShown) return;
    errorShown = true;
    showError('This visualization could not be rendered.', detail);
  }
  window.addEventListener('error', function (e) {
    handleGlobalError(e && e.message ? e.message : 'Unexpected rendering error.');
  });
  window.addEventListener('unhandledrejection', function (e) {
    var r = e && e.reason;
    handleGlobalError(r && r.message ? r.message : String(r || 'Unexpected rendering error.'));
  });

  // Apply theme-aware defaults to Chart.js (call before constructing charts).
  function applyChartDefaults() {
    if (typeof window.Chart === 'undefined') return;
    var C = window.Chart;
    C.defaults.color = colors.text;
    C.defaults.borderColor = colors.grid;
    C.defaults.font = C.defaults.font || {};
    C.defaults.font.family =
      '-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif';
    C.defaults.font.size = 14;
    C.defaults.font.lineHeight = 1.3;
    C.defaults.animation = false;
    applyChartTooltipDefaults();
  }

  function applyChartTooltipDefaults() {
    if (typeof window.Chart === 'undefined') return;
    var C = window.Chart;
    var tooltipDefaults = C.defaults.plugins && C.defaults.plugins.tooltip;
    if (!tooltipDefaults) return;
    tooltipDefaults.backgroundColor = colors.tooltipBg;
    tooltipDefaults.titleColor = colors.tooltipText;
    tooltipDefaults.bodyColor = colors.tooltipText;
    tooltipDefaults.borderWidth = 0;
    tooltipDefaults.cornerRadius = 4;
    tooltipDefaults.padding = 8;
    tooltipDefaults.caretSize = 0;
    tooltipDefaults.titleFont = {
      family: '-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif',
      size: 14,
      weight: '600',
    };
    tooltipDefaults.bodyFont = {
      family: '-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif',
      size: 14,
      weight: '400',
    };
  }

  // Measure glyphs rather than guessing from string length (CJK and Unicode).
  function wrapLabel(value, maxWidth, measure) {
    var text = String(value == null ? '' : value);
    var glyphs = typeof Intl !== 'undefined' && Intl.Segmenter
      ? Array.from(new Intl.Segmenter(undefined, { granularity: 'grapheme' }).segment(text), function (part) { return part.segment; })
      : Array.from(text);
    var lines = [];
    var line = '';
    glyphs.forEach(function (glyph) {
      if (glyph === '\n') {
        lines.push(line);
        line = '';
      } else if (line && measure(line + glyph) > maxWidth) {
        var space = line.lastIndexOf(' ');
        if (space > line.length / 2 && measure(line.slice(space + 1) + glyph) <= maxWidth) {
          lines.push(line.slice(0, space));
          line = line.slice(space + 1) + glyph;
        } else {
          lines.push(line);
          line = glyph;
        }
      } else {
        line += glyph;
      }
    });
    if (line || !lines.length) lines.push(line);
    return lines;
  }

  function renderFigureData(table, headers, rows, captionText, totalRows) {
    if (totalRows === undefined) totalRows = rows.length;
    table.replaceChildren();
    table.createCaption().textContent = captionText + (totalRows > 500
      ? ' Showing the first 500 of ' + totalRows + ' rows; remaining rows are not displayed.'
      : ' All ' + totalRows + ' rows shown.');
    var head = table.createTHead().insertRow();
    headers.forEach(function (label) {
      var cell = document.createElement('th');
      cell.scope = 'col';
      cell.textContent = label;
      head.appendChild(cell);
    });
    var body = table.createTBody();
    var count = 0;
    for (var values of rows) {
      if (count++ === 500) break;
      var row = body.insertRow();
      values.forEach(function (value) {
        var cell = row.insertCell();
        if (typeof value === 'number') cell.className = 'numeric';
        cell.textContent = String(value == null ? '—' : value);
      });
    }
  }

  function wrapChartTooltip(value, chart) {
    var context = chart.ctx;
    context.save();
    context.font = '14px -apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif';
    var lines = wrapLabel(value, Math.max(40, chart.width - 48), function (text) {
      return context.measureText(text).width;
    });
    context.restore();
    if (lines.length > 4) return lines.slice(0, 3).concat('… See data table.');
    return lines;
  }

  function fitSvgLabel(element, value, maxWidth) {
    var text = String(value == null ? '' : value);
    element.textContent = text;
    if (element.getComputedTextLength() <= maxWidth) return;
    element.textContent = '…';
    if (element.getComputedTextLength() > maxWidth) {
      element.textContent = '';
      return;
    }
    var glyphs = typeof Intl !== 'undefined' && Intl.Segmenter
      ? Array.from(new Intl.Segmenter(undefined, { granularity: 'grapheme' }).segment(text), function (part) { return part.segment; })
      : Array.from(text);
    var low = 0;
    var high = glyphs.length;
    while (low < high) {
      var middle = Math.ceil((low + high) / 2);
      element.textContent = glyphs.slice(0, middle).join('') + '…';
      if (element.getComputedTextLength() <= maxWidth) low = middle;
      else high = middle - 1;
    }
    element.textContent = glyphs.slice(0, low).join('') + '…';
  }

  function hideOverlappingSvgLabels(root, selector) {
    var boundary = root.getBoundingClientRect();
    var placed = [];
    root.querySelectorAll(selector).forEach(function (label) {
      if (!label.textContent) return;
      var box = label.getBoundingClientRect();
      var outside = box.left < boundary.left || box.right > boundary.right || box.top < boundary.top || box.bottom > boundary.bottom;
      var overlaps = placed.some(function (other) {
        return box.left < other.right + 2 && box.right > other.left - 2 && box.top < other.bottom + 2 && box.bottom > other.top - 2;
      });
      if (outside || overlaps) label.textContent = '';
      else placed.push(box);
    });
  }

  // Apply the page background/text once the body exists.
  function applyPageTheme() {
    if (!document.body) return;
    document.body.style.background = colors.bg;
    document.body.style.color = colors.text;
  }

  function installTooltipStyles() {
    if (!document.head || document.querySelector('[data-biorouter-viz-tooltip-styles]')) return;
    var style = document.createElement('style');
    style.setAttribute('data-biorouter-viz-tooltip-styles', '');
    style.textContent =
      '.tooltip{' +
      'box-sizing:border-box!important;' +
      'width:max-content!important;' +
      'max-width:min(20rem,calc(100vw - 16px))!important;' +
      'max-height:calc(100vh - 16px)!important;' +
      'overflow:hidden;' +
      'overflow-wrap:anywhere;' +
      'background:var(--tooltip-bg)!important;' +
      'color:var(--tooltip-text)!important;' +
      'padding:6px 8px!important;' +
      'border-radius:4px!important;' +
      'font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif!important;' +
      'font-size:14px!important;' +
      'font-weight:400;' +
      'line-height:20px;' +
      'text-align:left;' +
      'pointer-events:none!important;' +
      'transition:opacity 120ms ease!important;' +
      'z-index:10!important;' +
      '}';
    document.head.appendChild(style);
  }

  var tooltipFitFrame = null;
  function fitVisibleTooltips() {
    tooltipFitFrame = null;
    document.querySelectorAll('.tooltip').forEach(function (tooltip) {
      if (parseFloat(tooltip.style.opacity || window.getComputedStyle(tooltip).opacity) <= 0) return;

      var bounds = tooltip.getBoundingClientRect();
      var padding = 8;
      var shiftX = 0;
      var shiftY = 0;
      if (bounds.left < padding) shiftX = padding - bounds.left;
      if (bounds.right + shiftX > window.innerWidth - padding) {
        shiftX += window.innerWidth - padding - (bounds.right + shiftX);
      }
      if (bounds.top < padding) shiftY = padding - bounds.top;
      if (bounds.bottom + shiftY > window.innerHeight - padding) {
        shiftY += window.innerHeight - padding - (bounds.bottom + shiftY);
      }

      var left = parseFloat(tooltip.style.left);
      var top = parseFloat(tooltip.style.top);
      if (shiftX && Number.isFinite(left)) tooltip.style.left = left + shiftX + 'px';
      if (shiftY && Number.isFinite(top)) tooltip.style.top = top + shiftY + 'px';
    });
  }

  function queueTooltipFit() {
    if (tooltipFitFrame !== null) return;
    tooltipFitFrame = window.requestAnimationFrame(fitVisibleTooltips);
  }

  function initializeTooltipLayer() {
    installTooltipStyles();
    window.addEventListener('mouseover', queueTooltipFit);
    window.addEventListener('mousemove', queueTooltipFit);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initializeTooltipLayer, { once: true });
  } else {
    initializeTooltipLayer();
  }

  // Map a normalized value [0,1] to a sequential colour (blue→red), theme-aware.
  function sequential(t) {
    t = Math.max(0, Math.min(1, t));
    var r = Math.round(255 * Math.min(1, 0.1 + 1.6 * t));
    var g = Math.round(255 * (0.3 + 0.5 * (1 - Math.abs(t - 0.5) * 2)));
    var b = Math.round(255 * Math.min(1, 0.1 + 1.6 * (1 - t)));
    return 'rgb(' + r + ',' + g + ',' + b + ')';
  }

  window.BioRouterViz = {
    theme: theme,
    dark: dark,
    palette: palette,
    colors: colors,
    reportSize: reportSize,
    autoResize: autoResize,
    showError: showError,
    guard: guard,
    applyChartDefaults: applyChartDefaults,
    applyChartTooltipDefaults: applyChartTooltipDefaults,
    applyPageTheme: applyPageTheme,
    sequential: sequential,
    wrapLabel: wrapLabel,
    renderFigureData: renderFigureData,
    fitSvgLabel: fitSvgLabel,
    hideOverlappingSvgLabels: hideOverlappingSvgLabels,
    wrapChartTooltip: wrapChartTooltip,
  };
})();
