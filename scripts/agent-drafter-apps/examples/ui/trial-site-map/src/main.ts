import { createApp } from "./sdk";

// Custom UI: a clickable region map. Each pick is handed to the agent, which is
// the authority on what the map shows — it renders the breakdown into
// @region:detail and outlines the chosen cell. The app only forwards clicks and
// mirrors the agent's highlight back onto the map's selection state.
const br = createApp({ autoChat: false });

const cells = Array.from(document.querySelectorAll<HTMLElement>(".br-region"));

// The agent's ui_highlight is the source of truth for "which region is shown":
// when it outlines a cell, mark that cell active and clear the others.
br.ui.onCommand((cmd) => {
  if (cmd.cmd !== "highlight" || !cmd.target) return;
  for (const c of cells) c.classList.toggle("is-active", "#" + c.id === cmd.target);
});

function pick(cell: HTMLElement): void {
  const slug = cell.dataset.region || "";
  const label = cell.dataset.label || (cell.textContent || "").trim() || slug;
  if (!slug) return;

  // Optimistic feedback before the agent answers; onCommand reconciles it.
  for (const c of cells) c.classList.toggle("is-active", c === cell);
  const empty = document.getElementById("detail-empty");
  if (empty) empty.remove();

  br.run(
    `The user clicked the "${label}" region (slug ${slug}) on the trial site map. ` +
      `Its map cell selector is "#${cell.id}". Render the trial-count breakdown for ` +
      `this region into @region:detail with ui_render — stat cards, a phase table, and ` +
      `a bar chart — then outline "#${cell.id}" with ui_highlight. One sentence of context.`,
    "#out"
  ).catch(() => {
    /* br.run renders its own failure message into #out */
  });
}

for (const cell of cells) {
  cell.addEventListener("click", () => pick(cell));
  cell.addEventListener("keydown", (e) => {
    // role="button" cells aren't native buttons — activate on Enter/Space.
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      pick(cell);
    }
  });
}
