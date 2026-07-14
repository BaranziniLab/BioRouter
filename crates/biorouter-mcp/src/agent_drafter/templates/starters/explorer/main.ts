import { createApp } from "./sdk";

/**
 * Archetype: EXPLORER — a network the agent renders + an inspector + search.
 *
 * Manifest surface (seeded by create_app; keep these in sync if you edit):
 *   actions:  focus_node({ id, label?, type? }) — agent centers + selects a node
 *   signals:  node_selected  (the network's built-in selection)
 *             search_submitted
 *   state:    /query (string), /selection { id, label, type }
 *
 * The graph is the built-in `network` catalog kind: the agent renders it into
 * @region:graph with ui_render and drives selection with focus_node.
 */
const br = createApp({ autoChat: false });

const search = document.getElementById("q") as HTMLInputElement;
const go = document.getElementById("search") as HTMLButtonElement;

// User search → a typed turn, plus a signal the agent can subscribe to.
async function runSearch() {
  const query = search.value.trim();
  if (!query) return; // RUN GUARD: never fire an empty turn.
  br.state.set("/query", query);
  br.signals.emit("search_submitted", { query });
  await br.call("explore", { query });
}
go.addEventListener("click", runSearch);
search.addEventListener("keydown", (e) => {
  if (e.key === "Enter") runSearch();
});

// Agent verb: reflect a node selection into shared state so the inspector's
// data-br-bind fields update in place.
br.actions.register("focus_node", async (args) => {
  const a = (args ?? {}) as { id?: string; label?: string; type?: string };
  br.state.set("/selection", {
    id: a.id ?? "",
    label: a.label ?? "",
    type: a.type ?? "",
  });
  return { focused: a.id ?? null };
});
