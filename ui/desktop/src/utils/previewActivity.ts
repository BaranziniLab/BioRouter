/** Fixed, value-free activity tracking. A document stays dirty until explicitly refreshed. */
export const PREVIEW_ACTIVITY_INSTALL = `(() => {
  const key = Symbol.for('biorouter.preview.activity.v1');
  if (window[key]) return;
  const activity = { dirty: false };
  const markDirty = () => {
    activity.dirty = true;
    if (window.parent !== window) window.parent.postMessage({ type: 'biorouter-artifact-dirty' }, '*');
  };
  const frameFocus = () => setTimeout(() => {
    if (document.activeElement?.tagName === 'IFRAME') markDirty();
  }, 0);
  document.addEventListener('input', markDirty, true);
  document.addEventListener('change', markDirty, true);
  window.addEventListener('blur', frameFocus);
  activity.dispose = () => {
    document.removeEventListener('input', markDirty, true);
    document.removeEventListener('change', markDirty, true);
    window.removeEventListener('blur', frameFocus);
    delete window[key];
  };
  window[key] = activity;
})()`;

/** SDK private fields are read only as activity counts; no prompt, result or form values leave the page. */
export const PREVIEW_ACTIVITY_IDLE = `(() => {
  const activity = window[Symbol.for('biorouter.preview.activity.v1')];
  if (!activity || activity.dirty) return false;
  let active = document.activeElement;
  while (active?.shadowRoot?.activeElement) active = active.shadowRoot.activeElement;
  if (document.querySelector('[aria-busy="true"]') ||
      (active && (active.matches('input, textarea, select, iframe, [role="textbox"], [contenteditable]:not([contenteditable="false"])') || active.isContentEditable))) return false;
  const sdk = window.BioRouter || window.Biorouter;
  if (!sdk) return !window.BioRouterAgent;
  if (sdk.ws?.readyState !== WebSocket.OPEN) return false;
  if (!(sdk.pendingCalls instanceof Map) || !(sdk.pendingKb instanceof Map)) return false;
  const maps = ['pendingCalls', 'pendingKb', 'callInFlight', 'callDebounce', 'signalPending', 'runDebounce', 'agentInflight', 'agentActiveCall'];
  if (maps.some(name => sdk[name]?.size > 0)) return false;
  if (['outbox', 'tokensWaiters', 'historyWaiters', 'modelStatusWaiters'].some(name => sdk[name]?.length > 0)) return false;
  return !sdk.activeResolve && !sdk.activeCall && !(sdk.activeRun && !sdk.activeRun.settled);
})()`;

export function withPreviewActivityTracking(html: string): string {
  const script = `<script>${PREVIEW_ACTIVITY_INSTALL}</script>`;
  const head = /<head\b[^>]*>/i.exec(html);
  if (head) {
    const end = head.index + head[0].length;
    return `${html.slice(0, end)}${script}${html.slice(end)}`;
  }
  const root = /<html\b[^>]*>/i.exec(html);
  if (root) {
    const end = root.index + root[0].length;
    return `${html.slice(0, end)}<head>${script}</head>${html.slice(end)}`;
  }
  const doctype = /^\s*<!doctype\b[^>]*>/i.exec(html);
  const end = doctype?.[0].length ?? 0;
  return `${html.slice(0, end)}<head>${script}</head>${html.slice(end)}`;
}
