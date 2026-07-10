import { createApp } from "./sdk";

// The agent gathers weight / renal function / drug through ui_ask, so the app
// itself only kicks off the consult and offers a few common drugs to seed it.
const br = createApp({ autoChat: false });

const startBtn = document.getElementById("start") as HTMLButtonElement;
const resetBtn = document.getElementById("reset") as HTMLButtonElement;
const chips = document.getElementById("drug-chips") as HTMLDivElement;

let seededDrug = "";

function selectChip(active: HTMLElement | null): void {
  chips.querySelectorAll(".br-chip").forEach((c) => c.classList.remove("is-active"));
  if (active) active.classList.add("is-active");
}

chips.addEventListener("click", (e) => {
  const chip = (e.target as HTMLElement).closest<HTMLElement>(".br-chip");
  if (!chip) return;
  const drug = chip.dataset.drug || "";
  // Clicking the active chip again clears the pre-selection.
  if (seededDrug === drug) {
    seededDrug = "";
    selectChip(null);
  } else {
    seededDrug = drug;
    selectChip(chip);
  }
});

async function runConsult(): Promise<void> {
  const opener = seededDrug
    ? `Begin a renal dosing consult. The clinician pre-selected ${seededDrug} — pre-fill the drug field and have them confirm it.`
    : "Begin a renal dosing consult.";
  startBtn.disabled = true;
  try {
    // br.run mounts a step timeline in #out and streams the agent's short
    // summary there; the dose tiles and cautions land in the page regions.
    await br.run(opener, "#out");
  } finally {
    startBtn.disabled = false;
  }
}

startBtn.addEventListener("click", runConsult);

resetBtn.addEventListener("click", () => {
  seededDrug = "";
  selectChip(null);
  document.getElementById("out")!.innerHTML = "";
  // Restore the empty-region hints the agent will overwrite next run.
  document.querySelectorAll<HTMLElement>("[data-br-region]").forEach((r, i) => {
    r.innerHTML =
      i === 0
        ? '<p class="br-text br-text--muted">The adjusted regimen will appear here after the consult.</p>'
        : "";
  });
});

// After the agent renders the regimen, bring it into view — the ui_ask modal
// that collected weight / renal function / drug has just closed over it.
br.ui.onCommand((cmd) => {
  if (cmd.cmd === "render" && cmd.target === "@region:recommendation") {
    document
      .querySelector<HTMLElement>('[data-br-region="recommendation"]')
      ?.scrollIntoView({ behavior: "smooth", block: "center" });
  }
});
