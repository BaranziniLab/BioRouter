/**
 * Phase 0 of the tab tear-off spec: measure whether OS-level pointer capture
 * survives the cursor crossing another window of the same app.
 *
 * WHY THIS IS A HUMAN TASK AND NOT AN AGENT ONE
 * ---------------------------------------------
 * The whole feature rests on one assumption: that a `pointerdown` in window A
 * keeps delivering `pointermove` to A after the cursor leaves A's frame, and
 * that A — not B — receives the `pointerup` even when the button is released
 * over another BioRouter window or over empty desktop. That is asserted from
 * the Electron/Chromium spec. It has never been measured here.
 *
 * It CANNOT be measured with CDP `Input.dispatchMouseEvent`: injected events are
 * delivered straight to a renderer's input pipeline and therefore cannot answer
 * a question about OS-level capture. They will report success whether or not the
 * real behaviour holds. jsdom is useless on four counts: no `elementFromPoint`,
 * zeroed `getBoundingClientRect`, no pointer capture, no windows.
 *
 * So this needs a real hand on a real mouse. It takes about a minute.
 *
 * HOW TO RUN
 * ----------
 *  1. Launch BioRouter and open TWO windows, side by side, both showing a chat
 *     tab strip. Do not overlap them yet.
 *  2. Open DevTools in BOTH windows (View > Toggle Developer Tools).
 *  3. Paste this whole file into the console of BOTH windows. Each will print
 *     the name it has given itself — "A" for the first, "B" for the second.
 *  4. In window A: press and HOLD the mouse button on a chat tab, then drag —
 *       (a) across window B's tab strip, pause a beat,
 *       (b) out onto empty desktop, pause a beat,
 *       (c) release the button THERE, over the desktop.
 *  5. Repeat, but this time release over window B's tab strip.
 *  6. Run `__tearoff.report()` in BOTH consoles and paste both outputs back.
 *
 * WHAT THE ANSWER DECIDES
 * -----------------------
 *  - A keeps receiving moves outside its frame, and gets the up  -> the spec's
 *    mechanism (D2) is sound; Phase 3 can wire it as written.
 *  - A stops receiving moves at its own edge                     -> the gesture
 *    cannot be tracked from the renderer at all, and the design moves to
 *    polling `screen.getCursorScreenPoint()` from the main process.
 *  - B receives pointer events during A's drag                   -> the D3
 *    decision (hit-test in main, never in the target) is wrong, and a much
 *    simpler target-side implementation becomes available.
 *
 * Any of those three is a useful result. The bad outcome is not measuring.
 */
(() => {
  const KEY = '__tearoff_phase0_role';
  // Two windows share nothing but localStorage, so the first to load claims A.
  let role = sessionStorage.getItem(KEY);
  if (!role) {
    const taken = localStorage.getItem('__tearoff_phase0_taken');
    role = taken === 'A' ? 'B' : 'A';
    localStorage.setItem('__tearoff_phase0_taken', role);
    sessionStorage.setItem(KEY, role);
  }

  const log = [];
  const record = (type, e) => {
    log.push({
      type,
      t: Math.round(performance.now()),
      screenX: e.screenX,
      screenY: e.screenY,
      clientX: e.clientX,
      clientY: e.clientY,
      // Whether the point is inside this window's own viewport is the crux:
      // a move recorded here with `inside: false` is capture working.
      inside:
        e.clientX >= 0 &&
        e.clientY >= 0 &&
        e.clientX <= window.innerWidth &&
        e.clientY <= window.innerHeight,
      buttons: e.buttons,
      target: e.target && e.target.closest ? describe(e.target) : '(none)',
    });
  };

  const describe = (el) => {
    const tab = el.closest('[data-tab-id], [role="tab"], .br-tab');
    if (tab) return `tab:${tab.getAttribute('data-tab-id') ?? '?'}`;
    return el.tagName ? el.tagName.toLowerCase() : '?';
  };

  // Capture phase, on window, so nothing in the app can stop us seeing it.
  const on = (type) => window.addEventListener(type, (e) => record(type, e), true);
  ['pointerdown', 'pointermove', 'pointerup', 'pointercancel', 'lostpointercapture'].forEach(on);

  window.__tearoff = {
    role,
    log,
    reset() {
      log.length = 0;
      console.log(`[tear-off ${role}] cleared`);
    },
    report() {
      const downs = log.filter((r) => r.type === 'pointerdown');
      const moves = log.filter((r) => r.type === 'pointermove' && r.buttons > 0);
      const ups = log.filter((r) => r.type === 'pointerup');
      const outsideMoves = moves.filter((r) => !r.inside);
      const outsideUps = ups.filter((r) => !r.inside);

      console.log(`\n===== tear-off Phase 0 — window ${role} =====`);
      console.log(`pointerdown here .............. ${downs.length}`);
      console.log(`drag moves seen ............... ${moves.length}`);
      console.log(`  ...of those, OUTSIDE this window: ${outsideMoves.length}   <-- capture`);
      console.log(`pointerup seen ................ ${ups.length}`);
      console.log(`  ...of those, OUTSIDE this window: ${outsideUps.length}   <-- capture`);
      console.log(
        `lostpointercapture ............ ${log.filter((r) => r.type === 'lostpointercapture').length}`
      );

      if (downs.length === 0 && moves.length > 0) {
        console.log(
          `\nVERDICT for ${role}: this window saw drag events for a gesture that STARTED ELSEWHERE.\n` +
            `That contradicts spec decision D3 — and it is good news, because a target-side\n` +
            `hit test would then be possible and much simpler than main-process geometry.`
        );
      } else if (downs.length > 0 && outsideMoves.length > 0 && outsideUps.length > 0) {
        console.log(
          `\nVERDICT for ${role}: capture HOLDS. Moves and the release were both delivered here\n` +
            `while the cursor was outside this window. The spec's mechanism (D2) is sound.`
        );
      } else if (downs.length > 0 && outsideMoves.length === 0) {
        console.log(
          `\nVERDICT for ${role}: capture DOES NOT HOLD — events stop at this window's edge.\n` +
            `The renderer cannot track this gesture. The design must move to polling\n` +
            `screen.getCursorScreenPoint() from the main process.`
        );
      } else if (downs.length > 0 && outsideUps.length === 0) {
        console.log(
          `\nVERDICT for ${role}: moves cross the edge but the RELEASE was lost. A drag can be\n` +
            `previewed but not committed from the renderer; the commit must come from main.`
        );
      } else {
        console.log(`\nVERDICT for ${role}: nothing recorded. Did the drag start in this window?`);
      }
      console.log(`\nRaw trace: __tearoff.log  (${log.length} entries)`);
      return {
        role,
        downs: downs.length,
        moves: moves.length,
        outsideMoves: outsideMoves.length,
        ups: ups.length,
        outsideUps: outsideUps.length,
      };
    },
  };

  console.log(
    `[tear-off Phase 0] listening as window ${role}. Drag a tab out of window A, ` +
      `then run __tearoff.report() in BOTH windows.`
  );
})();
