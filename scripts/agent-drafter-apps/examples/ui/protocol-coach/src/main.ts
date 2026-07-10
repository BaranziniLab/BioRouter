import { createApp } from "./sdk";

// The agent owns the coaching surface: it focus-highlights the current step,
// docks the reagent checklist, notifies progress, and renders parameters into
// @region:notes. This shell only tracks which step we're on and hands the agent
// the step's element id + title so it never has to guess where we are.
const br = createApp({ autoChat: false });

const stepNodes = document.querySelectorAll("[data-br-step]");
const total = stepNodes.length;
let idx = 0; // 0 = not started, 1..total = current step
let running = false;

const coachBtn = document.getElementById("coach") as HTMLButtonElement;
const backBtn = document.getElementById("back") as HTMLButtonElement;
const nextBtn = document.getElementById("next") as HTMLButtonElement;
const resetBtn = document.getElementById("reset") as HTMLButtonElement;
const stepline = document.getElementById("stepline") as HTMLElement;

function stepEl(n: number): HTMLElement {
  return stepNodes[n - 1] as HTMLElement;
}
function title(n: number): string {
  const el = stepEl(n);
  return el && el.dataset.title ? el.dataset.title : "step " + n;
}
function elemId(n: number): string {
  const el = stepEl(n);
  return el ? el.id : "";
}

function sync(): void {
  stepline.textContent = idx === 0 ? "Not started" : "Step " + idx + " of " + total;
  coachBtn.textContent = idx === 0 ? "Coach me" : "Restart";
  coachBtn.disabled = running;
  backBtn.disabled = running || idx <= 1;
  nextBtn.disabled = running || idx === 0;
  nextBtn.textContent = idx >= total ? "Finish ✓" : "Next step ▸";
  resetBtn.disabled = running || idx === 0;
}

// Every button routes a short, explicit instruction through the agent so the
// step-by-step progress (timeline + streamed note) shows in #out.
async function drive(prompt: string): Promise<void> {
  running = true;
  sync();
  try {
    await br.run(prompt, "#out");
  } finally {
    running = false;
    sync();
  }
}

function coach(): void {
  idx = 1;
  drive(
    "Begin coaching me through this transformation protocol (" + total + " steps). " +
      "Set up the reagents checklist now. We are on step 1 of " + total + ": \"" +
      title(1) + "\" (element #" + elemId(1) + ")."
  );
}

function goToStep(to: number): void {
  idx = to;
  drive(
    "Move to step " + idx + " of " + total + ": \"" + title(idx) +
      "\" (element #" + elemId(idx) + ")."
  );
}

function finish(): void {
  drive(
    "That was the final step (" + total + " of " + total + "). Wrap up: clear the " +
      "highlight and give me a one-line completion note."
  );
}

function reset(): void {
  idx = 0;
  drive(
    "Reset the walkthrough: clear all highlights, remove the reagents panel, and " +
      "empty the notes region. Wait for me to start again."
  );
}

coachBtn.addEventListener("click", coach);
backBtn.addEventListener("click", () => {
  if (idx > 1) goToStep(idx - 1);
});
nextBtn.addEventListener("click", () => {
  if (idx >= total) finish();
  else goToStep(idx + 1);
});
resetBtn.addEventListener("click", reset);

sync();
