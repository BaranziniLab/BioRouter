import { createApp } from "./sdk";

// autoChat off: this app has its own paste-and-screen surface. The screening
// queue is drawn by the agent (ui_panel into the dock); the include/exclude
// buttons it renders call back into the loop on their own, so the app never
// wires them — it only kicks off the first triage and reflects agent activity.
const br = createApp({ autoChat: false });

const question = document.getElementById("question") as HTMLInputElement;
const abstracts = document.getElementById("abstracts") as HTMLTextAreaElement;
const triageBtn = document.getElementById("triage-btn") as HTMLButtonElement;
const status = document.getElementById("status") as HTMLDivElement;

// Reflect agent work — including the extra turns each dock button triggers,
// which don't flow through br.run(). The dock panel and summary region are the
// real output; this is just a heartbeat so a click never feels ignored.
br.on("tool", (e) => {
  if (e.type === "tool") status.textContent = "Screening…";
});
br.on("done", () => {
  status.textContent = "";
});
br.on("error", () => {
  status.textContent = "Backend unavailable.";
});

async function triage() {
  const body = abstracts.value.trim();
  if (!body) {
    status.textContent = "Paste at least one abstract.";
    abstracts.focus();
    return;
  }
  const q = question.value.trim();
  const prompt =
    (q ? `Review question: ${q}\n\n` : "") +
    `Triage these abstracts into the queue:\n\n${body}`;

  triageBtn.disabled = true;
  status.textContent = "Screening…";
  try {
    await br.run(prompt, "#out");
  } catch {
    status.textContent = "Triage failed.";
  } finally {
    triageBtn.disabled = false;
  }
}

triageBtn.addEventListener("click", triage);
