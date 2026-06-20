/* Agent Drafter — auto-resize reporter.
 *
 * Injected into EVERY artifact (static and agentic). It reports the artifact's
 * content height to the BioRouter host so the preview iframe auto-grows instead
 * of clipping/scrolling. The host (@mcp-ui/client `autoResizeIframe`) listens
 * for `{ type: "ui-size-change", payload: { height } }`.
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
