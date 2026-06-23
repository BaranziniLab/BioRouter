/**
 * App entry point (TypeScript, bundled by esbuild → dist/app.js).
 *
 * `createApp()` reads `window.BIOROUTER_APP_CONFIG`, opens a WebSocket to the
 * BioRouter backend, and (by default) mounts a chat panel into the element with
 * the `data-br-chat` attribute. Replace the body below to drive your own UI:
 *
 *   import { createApp } from "./sdk";
 *   const br = createApp({ autoChat: false });
 *   const out = document.getElementById("out")!;
 *   br.on("message", (e) => { if (e.type === "message") out.innerHTML += e.delta; });
 *   document.getElementById("go")!.addEventListener("click", () =>
 *     br.prompt((document.getElementById("q") as HTMLInputElement).value));
 */
import { createApp } from "./sdk";

createApp();
