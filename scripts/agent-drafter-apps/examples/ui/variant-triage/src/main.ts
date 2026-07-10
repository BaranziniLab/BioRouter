import { createApp } from "./sdk";

// Agentic app: the agent does the ACMG classification and renders the triage
// table straight into @region:results (ui_render / ui_highlight / ui_state).
// This module only gathers the pasted variants, starts the run, and mirrors the
// counts the agent publishes. autoChat is off — there is no chat box, just the
// paste-and-run surface plus the agent-owned results region.
const br = createApp({ autoChat: false });

const variants = document.getElementById("variants") as HTMLTextAreaElement;
const triage = document.getElementById("triage") as HTMLButtonElement;
const example = document.getElementById("example") as HTMLButtonElement;
const tally = document.getElementById("tally") as HTMLElement;

const SAMPLE = [
  "BRCA1 c.68_69delAG",
  "TP53 p.R175H",
  "CFTR F508del",
  "MTHFR c.665C>T",
  "APOE e4",
].join("\n");

function run(): void {
  const text = variants.value.trim();
  if (!text) {
    variants.focus();
    return;
  }
  triage.disabled = true;
  triage.textContent = "Triaging…";
  const prompt =
    "Triage these variants and render the ACMG table into @region:results:\n" + text;
  // br.run mounts the step timeline + streams the agent's summary into #out.
  br.run(prompt, "#out").finally(() => {
    triage.disabled = false;
    triage.textContent = "Triage variants";
  });
}

triage.addEventListener("click", run);
example.addEventListener("click", () => {
  variants.value = SAMPLE;
  variants.focus();
});

// The agent publishes tier counts with ui_state; keep them in the header so the
// headline survives after the run's streamed prose is done.
br.ui.onState((state) => {
  const path = Number(state.pathogenic || 0);
  const vus = Number(state.vus || 0);
  const benign = Number(state.benign || 0);
  tally.textContent = path + " pathogenic · " + vus + " VUS · " + benign + " benign";
});
