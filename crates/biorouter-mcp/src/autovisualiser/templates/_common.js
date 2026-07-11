/*
 * Shared runtime injected into every Auto Visualiser template via {{COMMON}}.
 *
 * Provides:
 *   - BioRouterViz.theme       : 'light' | 'dark' resolved from the iframe host query / prefers-color-scheme
 *   - BioRouterViz.palette     : categorical colour palette (theme-aware)
 *   - BioRouterViz.reportSize  : posts `ui-size-change` to the MCP-UI host so the iframe auto-resizes
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

  var palette = [
    '#4f7cff', '#ff6b6b', '#16c79a', '#ff9f40', '#9b59b6',
    '#f6c945', '#2ecc71', '#e74c3c', '#3498db', '#e67e22',
    '#1abc9c', '#e84393', '#00b894', '#6c5ce7', '#fab1a0',
  ];

  var colors = {
    bg: dark ? '#1c1f26' : '#f5f7fa',
    surface: dark ? '#262b35' : '#ffffff',
    text: dark ? '#e8eaed' : '#2b2f36',
    muted: dark ? '#9aa0a6' : '#6b7280',
    grid: dark ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.1)',
    border: dark ? '#3a4150' : '#e5e7eb',
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
  } catch (e) {
    /* ignore */
  }

  function reportSize() {
    var h = Math.max(
      document.body ? document.body.scrollHeight : 0,
      document.body ? document.body.offsetHeight : 0,
      document.documentElement.clientHeight,
      document.documentElement.scrollHeight,
      document.documentElement.offsetHeight
    );
    if (window.parent !== window) {
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
  }

  // Apply the page background/text once the body exists.
  function applyPageTheme() {
    if (!document.body) return;
    document.body.style.background = colors.bg;
    document.body.style.color = colors.text;
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
    applyPageTheme: applyPageTheme,
    sequential: sequential,
  };
})();
