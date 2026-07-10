import { createApp } from "./sdk";

// The agent owns the two regions: it draws the network into @region:overview with
// ui_graph and the evidence table into @region:evidence with ui_render, then walks
// through them with ui_highlight. This shell only collects a target and streams
// the spoken walkthrough into #out.
const br = createApp({ autoChat: false });

const input = document.getElementById("gene") as HTMLInputElement;
const btn = document.getElementById("trace") as HTMLButtonElement;
const examples = document.getElementById("examples") as HTMLDivElement;
const activity = document.getElementById("trace-status") as HTMLDivElement;

// Reflect what the agent is doing to the page, so the shell feels alive while the
// regions fill in. ui_graph/ui_render arrive as "render" frames; ui_highlight
// carries the callout note the agent is narrating.
br.ui.onCommand((cmd) => {
  const target = cmd.target || "";
  if (cmd.cmd === "render" && target.indexOf("overview") >= 0) {
    activity.textContent = "Mapping the interaction network…";
  } else if (cmd.cmd === "render" && target.indexOf("evidence") >= 0) {
    activity.textContent = "Compiling the evidence table…";
  } else if (cmd.cmd === "highlight") {
    activity.textContent = cmd.mode === "clear" ? "" : cmd.note || "Walking through the network…";
  }
});

async function run(target: string) {
  const query = target.trim();
  if (!query) return;
  btn.disabled = true;
  activity.textContent = "Looking up interactions…";
  const prompt =
    `Map the molecular interaction network for "${query}". Draw it into ` +
    `@region:overview with ui_graph, render an edge-by-edge evidence table into ` +
    `@region:evidence, then walk me through the pivotal edges with ui_highlight and short notes.`;
  try {
    await br.run(prompt, "#out");
  } finally {
    btn.disabled = false;
    activity.textContent = "";
  }
}

btn.addEventListener("click", () => run(input.value));
input.addEventListener("keydown", (e) => {
  if (e.key === "Enter") run(input.value);
});
examples.addEventListener("click", (e) => {
  const chip = (e.target as HTMLElement).closest<HTMLElement>("[data-gene]");
  if (!chip) return;
  input.value = chip.dataset.gene || chip.textContent || "";
  run(input.value);
});
