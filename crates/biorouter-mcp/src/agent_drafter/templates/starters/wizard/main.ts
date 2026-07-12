import { createApp } from "./sdk";

/**
 * Archetype: WIZARD — a staged form that writes shared state, then submits.
 *
 * Manifest surface (seeded by create_app; keep these in sync if you edit):
 *   actions:  go_to_step({ step }) — agent moves the wizard to a stage
 *   signals:  step_changed, submitted
 *   state:    /step (integer), /form { name, goal }
 */
const br = createApp({ autoChat: false });

const nameInput = document.getElementById("name") as HTMLInputElement;
const goalInput = document.getElementById("goal") as HTMLInputElement;
const next = document.getElementById("next") as HTMLButtonElement;
const back = document.getElementById("back") as HTMLButtonElement;
const submit = document.getElementById("submit") as HTMLButtonElement;

function show(step: number) {
  const s = step === 2 ? 2 : 1;
  br.state.set("/step", s);
  (document.getElementById("stage-1") as HTMLElement).hidden = s !== 1;
  (document.getElementById("stage-2") as HTMLElement).hidden = s !== 2;
  br.signals.emit("step_changed", { step: s });
}

// Fields flow into the shared form doc, so the review + agent stay in sync.
nameInput.addEventListener("input", () => br.state.set("/form/name", nameInput.value));
goalInput.addEventListener("input", () => br.state.set("/form/goal", goalInput.value));
next.addEventListener("click", () => show(2));
back.addEventListener("click", () => show(1));

// Submit → a typed turn carrying the collected form.
submit.addEventListener("click", async () => {
  const name = nameInput.value.trim();
  if (!name) return; // RUN GUARD: don't submit an empty form.
  br.signals.emit("submitted", { name });
  await br.call("submit", { name, goal: goalInput.value });
});

// Agent verb: jump the wizard to a stage (e.g. after validating an answer).
br.actions.register("go_to_step", async (args) => {
  const s = (args ?? {}) as { step?: number };
  show(s.step === 2 ? 2 : 1);
  return { step: br.state.get("/step") };
});
