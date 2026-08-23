/* Agent Drafter — auto-resize reporter.
 *
 * Injected into EVERY artifact (static and agentic). It posts the artifact's
 * content height as `{ type: "ui-size-change", payload: { height } }` to the
 * parent frame. An enclosing page that embeds this artifact listens and grows
 * the iframe to fit — an app built with the SDK does (`renderFigure` in
 * sdk.ts), as does an Auto Visualiser dashboard report. When the artifact is
 * displayed standalone in the desktop artifact side panel the message is inert:
 * that panel sizes the frame itself and installs no listener.
 *
 * Exposes `window.__brReportSize()` so the agent runtime (agent.js) can trigger
 * a fresh measurement after it mutates the DOM (e.g. new chat messages) without
 * setting up a second observer.
 */
(function () {
  "use strict";
  if (window.__brReportSize) return; // already installed
  function reportSize() {
    var h = Math.max(
      document.documentElement.scrollHeight,
      document.body ? document.body.scrollHeight : 0
    );
    try {
      window.parent.postMessage({ type: "ui-size-change", payload: { height: h } }, "*");
    } catch (e) { /* not embedded */ }
  }
  window.__brReportSize = reportSize;

  if (typeof ResizeObserver !== "undefined") {
    try {
      var ro = new ResizeObserver(function () { reportSize(); });
      ro.observe(document.documentElement);
    } catch (e) { /* ignore */ }
  }
  window.addEventListener("load", reportSize);
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", function () { setTimeout(reportSize, 60); });
  } else {
    setTimeout(reportSize, 60);
  }
})();
