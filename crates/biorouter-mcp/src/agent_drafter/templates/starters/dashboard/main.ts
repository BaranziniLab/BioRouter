import { createApp } from "./sdk";

/**
 * Archetype: DASHBOARD — a KPI grid bound to shared state + a refresh action.
 *
 * Manifest surface (seeded by create_app; keep these in sync if you edit):
 *   actions:  set_metric({ key, value, delta? }) — agent updates one KPI tile
 *   signals:  refresh_requested
 *   state:    /metrics/{cohorts,samples,alerts} bound into the KPI grid
 */
const br = createApp({ autoChat: false });

const refresh = document.getElementById("refresh") as HTMLButtonElement;

// User asks for fresh numbers: notify the agent (signal) and run a typed turn.
refresh.addEventListener("click", async () => {
  br.signals.emit("refresh_requested", { at: Date.now() });
  await br.call("refresh_metrics", {});
});

// Agent verb: write one KPI into shared state; the bound grid re-renders the
// single changed tile in place (no full re-render).
br.actions.register("set_metric", async (args) => {
  const m = (args ?? {}) as { key?: string; value?: unknown; delta?: unknown };
  if (!m.key) return { ok: false };
  br.state.set("/metrics/" + m.key, {
    value: m.value ?? "—",
    delta: m.delta ?? "",
  });
  return { ok: true, key: m.key };
});
