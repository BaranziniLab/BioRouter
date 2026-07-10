import { createApp } from "./sdk";

// autoChat off: this app has its own paste form and result surface, and the
// agent paints the scorecard (ui_render) and the page theme (ui_theme) itself.
const br = createApp({ autoChat: false });

const assay = document.getElementById("assay") as HTMLSelectElement;
const metrics = document.getElementById("metrics") as HTMLTextAreaElement;
const run = document.getElementById("run") as HTMLButtonElement;
const mood = document.getElementById("mood") as HTMLSpanElement;

// Mirror the agent's verdict into the header pill by reading back the accent it
// sets with ui_theme. Hex values are written WITHOUT a leading '#' so the
// build's element-id scanner never mistakes a color literal for a '#id' lookup.
const VERDICTS = [
  ["5bbe5e", "Pass"],
  ["cf6d47", "Marginal"],
  ["e85252", "Fail"],
];
br.ui.onCommand((cmd) => {
  if (cmd.cmd !== "theme") return;
  const accent = cmd.accent || "";
  const hit = VERDICTS.find((v) => accent.indexOf(v[0]) >= 0);
  mood.textContent = hit ? hit[1] : "Reviewing";
});

async function evaluate() {
  const pasted = metrics.value.trim();
  if (!pasted) return;
  mood.textContent = "Reviewing";
  run.disabled = true;
  try {
    await br.run(
      `Assay: ${assay.value}\nQC metrics for one sample:\n${pasted}\n\n` +
        `Score each metric against standard ${assay.value} thresholds, build the ` +
        `scorecard, recolor the app to the verdict, and notify it.`,
      "#out"
    );
  } finally {
    run.disabled = false;
  }
}

run.addEventListener("click", evaluate);
