import { createApp } from "./sdk";

/**
 * Archetype: CANVAS — the avatar archetype. An author-registered draw surface;
 * the AGENT supplies data via actions/state, never code.
 *
 * Manifest surface (seeded by create_app; keep these in sync if you edit):
 *   components: scene { x, y }              — author-drawn canvas
 *   actions:   move_avatar({ direction, steps }), reset_scene()
 *   signals:   avatar_moved
 *   state:     /scene { x, y } drives the rendered avatar
 */
const br = createApp({ autoChat: false });
const GRID = 10;

function clamp(n: number): number {
  return Math.max(0, Math.min(GRID - 1, n));
}
function scene(): { x: number; y: number } {
  return (br.state.get("/scene") ?? { x: 5, y: 5 }) as { x: number; y: number };
}

// Shared draw + move logic, reused by the component, the buttons, and the agent.
function drawInto(el: HTMLElement, x: number, y: number) {
  let canvas = el.querySelector("canvas") as HTMLCanvasElement | null;
  if (!canvas) {
    canvas = document.createElement("canvas");
    canvas.width = 320;
    canvas.height = 320;
    el.appendChild(canvas);
  }
  const g = canvas.getContext("2d");
  if (!g) return;
  const cell = canvas.width / GRID;
  const accent = getComputedStyle(el).getPropertyValue("--br-accent").trim() || "rgb(184,90,50)";
  g.clearRect(0, 0, canvas.width, canvas.height);
  g.fillStyle = accent;
  g.beginPath();
  g.arc((x + 0.5) * cell, (y + 0.5) * cell, cell * 0.35, 0, Math.PI * 2);
  g.fill();
}

function mountScene(el: HTMLElement) {
  const p = scene();
  drawInto(el, p.x, p.y);
  br.state.subscribe("/scene", (v) => {
    const n = (v ?? { x: 0, y: 0 }) as { x?: number; y?: number };
    drawInto(el, n.x ?? p.x, n.y ?? p.y);
  });
}

function move(direction: string, steps: number) {
  const step = Math.max(1, Math.min(20, steps));
  const p = scene();
  const dx = direction === "left" ? -step : direction === "right" ? step : 0;
  const dy = direction === "up" ? -step : direction === "down" ? step : 0;
  const next = { x: clamp(p.x + dx), y: clamp(p.y + dy) };
  br.state.set("/scene", next);
  br.signals.emit("avatar_moved", next);
  return next;
}

// Author-registered catalog component: props/state are agent-controlled input.
br.components.register("scene", {
  mount(el) {
    mountScene(el);
  },
});

// User controls drive the same move() the agent's action uses.
const pad = document.getElementById("pad") as HTMLElement;
pad.addEventListener("click", (e) => {
  const dir = (e.target as HTMLElement).getAttribute("data-dir");
  if (dir) move(dir, 1);
});
document.getElementById("reset")!.addEventListener("click", () => {
  br.state.set("/scene", { x: 5, y: 5 });
  br.signals.emit("avatar_moved", { x: 5, y: 5 });
});

// Agent verbs.
br.actions.register("move_avatar", async (args) => {
  const a = (args ?? {}) as { direction?: string; steps?: number };
  return { position: move(a.direction ?? "up", a.steps ?? 1) };
});
br.actions.register("reset_scene", async () => {
  const home = { x: 5, y: 5 };
  br.state.set("/scene", home);
  br.signals.emit("avatar_moved", home);
  return { position: home };
});

// Seed the scene so the bound coordinates + avatar show on first load.
br.state.set("/scene", { x: 5, y: 5 });
mountScene(document.getElementById("scene") as HTMLElement);
