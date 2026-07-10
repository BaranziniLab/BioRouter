import { createApp } from "./sdk";

// The controls only build a prompt; the agent owns the picture — it draws the
// bars into @region:chart with ui_chart. br.run streams its one-line read (and a
// step timeline) into #out and serializes overlapping runs, so dragging the
// slider never garbles the output.
const br = createApp({ autoChat: false });

const gene = document.getElementById("gene") as HTMLInputElement;
const tissue = document.getElementById("tissue") as HTMLSelectElement;
const floor = document.getElementById("floor") as HTMLInputElement;
const floorVal = document.getElementById("floorval") as HTMLElement;

// Slider value IS log10(TPM), so equal pixel steps span equal ratios.
function threshold(): number {
  return Math.pow(10, Number(floor.value));
}

function fmtTpm(tpm: number): string {
  if (tpm >= 100) return tpm.toFixed(0);
  if (tpm >= 1) return tpm.toFixed(1);
  return tpm.toFixed(2);
}

function syncFloorLabel(): void {
  floorVal.textContent = "≥ " + fmtTpm(threshold()) + " TPM";
}

function plot(): void {
  const symbol = gene.value.trim() || "TP53";
  const tpm = fmtTpm(threshold());
  const panel = tissue.options[tissue.selectedIndex].text;
  const prompt =
    `Gene ${symbol}: chart median RNA expression across "${panel}" from GTEx / Human ` +
    `Protein Atlas reference values. Keep only tissues at or above ${tpm} TPM, sort them ` +
    `high to low, and draw a bar chart (TPM by tissue) into @region:chart with ui_chart. ` +
    `Then give one sentence naming the highest-expressing tissue and its value.`;
  br.run(prompt, "#out");
}

// Debounce the continuous slider; fire selects/gene edits immediately.
let timer = 0;
function schedule(): void {
  window.clearTimeout(timer);
  timer = window.setTimeout(plot, 250);
}

floor.addEventListener("input", () => {
  syncFloorLabel();
  schedule();
});
tissue.addEventListener("change", plot);
gene.addEventListener("change", plot);
gene.addEventListener("keydown", (e: KeyboardEvent) => {
  if (e.key === "Enter") plot();
});

syncFloorLabel();
