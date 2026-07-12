import { createApp } from "./sdk";

/**
 * Archetype: WORKBENCH — an agent-rendered data table + a bound detail panel.
 *
 * Manifest surface (seeded by create_app; keep these in sync if you edit):
 *   actions:  open_row({ id, title?, body? }) — agent opens a row into detail
 *   signals:  row_selected      (the table's built-in row selection)
 *             filter_changed
 *   state:    /filter (string), /detail { id, title, body }
 *
 * The table is the built-in `table` catalog kind: the agent renders it into
 * @region:rows with ui_render and reacts to row_selected.
 */
const br = createApp({ autoChat: false });

const filter = document.getElementById("filter") as HTMLInputElement;
const apply = document.getElementById("apply") as HTMLButtonElement;

// User applies a filter: persist it, notify the agent, run a typed load turn.
apply.addEventListener("click", async () => {
  const q = filter.value.trim();
  br.state.set("/filter", q);
  br.signals.emit("filter_changed", { filter: q });
  await br.call("load_rows", { filter: q });
});

// Agent verb: surface one row's detail into shared state (bound in the panel).
br.actions.register("open_row", async (args) => {
  const r = (args ?? {}) as { id?: string; title?: string; body?: string };
  br.state.set("/detail", {
    id: r.id ?? "",
    title: r.title ?? "",
    body: r.body ?? "",
  });
  return { opened: r.id ?? null };
});
