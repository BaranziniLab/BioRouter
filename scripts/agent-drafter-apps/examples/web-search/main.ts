import { createApp } from "./sdk";

const br = createApp({ autoChat: false });

const q = document.getElementById("q") as HTMLInputElement;
const go = document.getElementById("go") as HTMLButtonElement;
const status = document.getElementById("status") as HTMLDivElement;
const results = document.getElementById("results") as HTMLDivElement;

let buffer = "";

br.on("tool", (e) => {
  if (e.type === "tool") status.textContent = `⚙ ${e.name} (${e.status})…`;
});
br.on("message", (e) => {
  if (e.type !== "message") return;
  status.textContent = "";
  buffer += e.delta;
  results.style.display = "block";
  results.innerHTML = br.renderMarkdown(buffer);
});
br.on("error", () => {
  status.textContent = "⚠ Could not reach the backend.";
});

async function run() {
  const query = q.value.trim();
  if (!query) return;
  buffer = "";
  results.style.display = "none";
  status.textContent = "Searching…";
  go.disabled = true;
  try {
    await br.prompt(
      `Search the web for: "${query}". Summarize the most relevant, up-to-date findings ` +
        `as concise markdown with a short list of sources (title + URL) at the end.`
    );
  } catch {
    status.textContent = "⚠ Search failed.";
  } finally {
    go.disabled = false;
  }
}

go.addEventListener("click", run);
q.addEventListener("keydown", (e) => {
  if (e.key === "Enter") run();
});
