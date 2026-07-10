import { createApp } from "./sdk";

// Custom UI: no auto chat box. The agent drives the interview via ui_* tools and
// we only observe — echoing its shared state bag into the page as the user answers.
const br = createApp({ autoChat: false });

const startBtn = document.getElementById("start") as HTMLButtonElement;
const criteria = document.getElementById("criteria") as HTMLElement;
const countEl = document.getElementById("count") as HTMLElement;
const phaseEl = document.getElementById("phase") as HTMLElement;

// "age_range" → "Age range"; the agent chooses the keys, so we title-case them.
function prettify(key) {
  return key.replace(/[_-]+/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

function valueText(v) {
  if (v === null || v === undefined || v === "") return "—";
  if (typeof v === "object") {
    try {
      return JSON.stringify(v);
    } catch {
      return String(v);
    }
  }
  return String(v);
}

function placeholder() {
  criteria.innerHTML = "";
  const p = document.createElement("div");
  p.className = "br-text br-text--muted";
  p.textContent = "No criteria captured yet — start the interview.";
  criteria.appendChild(p);
}

// The agent mirrors every answer into shared state with ui_state; this repaints
// the live criteria table each time that state changes (DOM build = no escaping).
br.ui.onState((state) => {
  const keys = Object.keys(state);
  countEl.textContent = String(keys.length);
  if (!keys.length) {
    placeholder();
    return;
  }
  const table = document.createElement("table");
  const tbody = document.createElement("tbody");
  for (const k of keys) {
    const tr = document.createElement("tr");
    const th = document.createElement("th");
    th.textContent = prettify(k);
    const td = document.createElement("td");
    td.textContent = valueText(state[k]);
    tr.appendChild(th);
    tr.appendChild(td);
    tbody.appendChild(tr);
  }
  table.appendChild(tbody);
  criteria.innerHTML = "";
  criteria.appendChild(table);
});

// Reflect where the agent is in the flow so the status tile feels alive.
br.ui.onCommand((cmd) => {
  if (cmd.cmd === "ask") phaseEl.textContent = "Awaiting your answer";
  else if (cmd.cmd === "render") phaseEl.textContent = "Assembling cohort";
});

const PROMPT =
  "Begin building a patient cohort. Interview me one topic at a time with " +
  "ui_ask (age range, primary diagnosis, exclusions), mirror each answer into " +
  "shared state with ui_state, then render the finished cohort definition as a " +
  "table into @region:cohort.";

async function start() {
  startBtn.disabled = true;
  startBtn.textContent = "Interview running…";
  phaseEl.textContent = "Starting…";
  try {
    // br.run mounts a visible step timeline into #out while the agent works.
    await br.run(PROMPT, "#out");
    phaseEl.textContent = "Ready";
  } catch {
    phaseEl.textContent = "Failed";
  } finally {
    startBtn.disabled = false;
    startBtn.textContent = "Restart interview";
  }
}

startBtn.addEventListener("click", start);
placeholder();
