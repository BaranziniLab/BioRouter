import { createApp } from "./sdk";

// autoChat off: this app has its own shell. br.run() streams the agent's short
// written read-out into #out while its ui_* tools fill the dashboard regions.
const br = createApp({ autoChat: false });

const summary = document.getElementById("summary") as HTMLTextAreaElement;
const build = document.getElementById("build") as HTMLButtonElement;
const example = document.getElementById("example") as HTMLButtonElement;

// A realistic DESeq2 summary so a scientist can try the app with no data of
// their own. Tab-separated; the agent parses either tables or prose.
const EXAMPLE = [
  "DESeq2 results — TNF-treated vs control · 14,204 genes tested · 812 with padj < 0.05",
  "",
  "gene\tlog2FC\tpadj",
  "IL6\t4.10\t2.1e-14",
  "CXCL10\t3.82\t9.4e-13",
  "CCL2\t3.15\t5.0e-11",
  "NFKBIA\t2.44\t1.8e-08",
  "SOD2\t1.97\t4.2e-07",
  "TNFAIP3\t1.85\t6.6e-07",
  "MT1X\t-2.63\t3.1e-09",
  "ALB\t-3.04\t7.7e-10",
  "CYP1A2\t-2.21\t2.9e-08",
  "APOA1\t-1.72\t5.5e-06",
  "HMGCS2\t-1.58\t3.3e-05",
  "FABP1\t-1.44\t8.1e-05",
].join("\n");

function buildDashboard(): void {
  const text = summary.value.trim();
  if (!text) {
    summary.focus();
    return;
  }
  build.disabled = true;
  br.run(
    "Build a differential-expression dashboard from this result summary. Open the " +
      "dashboard layout, fill the metrics and top-hits regions, and tell me in a " +
      "sentence what stood out.\n\n" +
      text,
    "#out"
  ).finally(() => {
    build.disabled = false;
  });
}

build.addEventListener("click", buildDashboard);
example.addEventListener("click", () => {
  summary.value = EXAMPLE;
  summary.focus();
});
// Cmd/Ctrl+Enter submits, like running a notebook cell.
summary.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key === "Enter") buildDashboard();
});

// Bring the dashboard into view the moment the agent starts driving it, so the
// user watches the result fill in rather than staring at the paste box.
let revealed = false;
br.ui.onCommand((cmd) => {
  if (revealed || (cmd.cmd !== "layout" && cmd.cmd !== "render")) return;
  revealed = true;
  const el = document.querySelector('[data-br-region="metrics"]');
  if (el) el.scrollIntoView({ behavior: "smooth", block: "center" });
});
