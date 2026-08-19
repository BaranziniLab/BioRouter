# The startup freeze, and main-thread blocking generally

> **What this is.** Why the desktop app froze a few seconds after every launch
> (issue #88), how to tell that class of bug apart from the things it is
> routinely mistaken for, and how to measure it.
> **Status:** Current.
> **Audience:** anyone touching the Electron main process, and anyone handed a
> report that the app "feels stuck" or "hangs at startup".

## The symptom, and why the obvious explanation was wrong

The window appeared and looked healthy for a few seconds. Then macOS showed the
spinning wait cursor, the app stopped repainting and stopped accepting clicks,
and several seconds later it came back. On a slow disk it lasted long enough for
macOS to mark it "Application Not Responding".

The natural reading is a CPU problem — something in the background heating the
machine up and starving the app. It was not. **CPU contention does not stop a
window repainting; it makes it slow.** A window that renders *zero* frames while
the process is alive is a blocked event loop, which is a different fault with a
different fix.

## The mechanism

The Electron main process runs window compositing, IPC and input handling on one
thread. `spawnSync` (and `execSync`, `execFileSync`, a large `readFileSync`, a
synchronous unzip or hash) parks that thread until it returns. Nothing repaints,
no click is delivered, no timer fires. The wait cursor is the OS reporting
exactly this.

Three subsystems armed timers during app-ready, each in its own module, none
aware of the others, and all three landed inside the renderer's first-paint
window:

| Fired | What | The blocking call | Budget |
|---|---|---|---|
| T+2s | auto-updater setup | `writeFileSync` + ~180 synchronous log writes | ~100 ms |
| **T+4s** | **dependency check** | **`spawnSync('biorouter doctor')` — a 141 MB binary** | **15 s**, then 7 more probes at 8 s each |
| T+8s | extension update check | `spawnSync('uv', ['sync'])` | 120 s per extension |

The dominant one was the dependency check. `biorouter doctor` measured **3.45 s
warm** on an M-series Mac; worst case across the fallback path was ~71 s of a
completely frozen window. `cli:status` added three more synchronous probes on
every launch, because the renderer invokes it from a mount effect.

## Measuring it

Two instruments, both in the repo.

**`ui/desktop/scripts/measure-startup-freeze.mjs`** — runs a probe both ways and
counts how many 60 fps ticks the event loop actually serviced:

```bash
node ui/desktop/scripts/measure-startup-freeze.mjs
```

```text
BEFORE  spawnSync          wall   488ms | frames rendered   1/ 30 | longest freeze   488ms
AFTER   execFile async     wall   463ms | frames rendered  28/ 28 | longest freeze    18ms

BEFORE  slow probe (3s)    wall  3011ms | frames rendered   1/188 | longest freeze  3011ms
AFTER   slow probe (3s)    wall  3011ms | frames rendered 179/188 | longest freeze    18ms
```

The freeze scales one-for-one with the probe's duration. That is the signature.

⚠ **The naive version of this instrument reads zero.** Measuring lag *inside* the
timer callback cannot work, because during a `spawnSync` the callback does not run
at all — there is nothing to measure from. The first version of this harness
reported `max stall 0.0ms` for a 1.7 s freeze, alongside `ticks observed 0`, and
the second number was the finding. Record tick *timestamps* and derive the largest
gap afterwards.

**`ui/desktop/src/utils/mainThreadWatchdog.ts`** — ships in the app. One timer,
twice a second, measuring its own lateness; anything over 250 ms is logged as
jank, over a second as a freeze. It exists because #88 shipped and survived: the
only evidence was a user saying the app felt stuck.

Its unit tests need the wall clock driven separately from the timer queue.
Vitest's default fake timers also fake `Date`, advancing both in lockstep, so an
interval always looks punctual and the lateness under test is always zero —
`vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] })` plus a `Date.now`
spy is what reproduces a blocked loop.

## The A/B that closed it

Both runs launched the real dev app in a sandboxed profile.

```text
AFTER (fixed)
  12:24:25.658  Opening URL
  12:24:27.664  Setting up auto-updater             T+2.0s
  12:24:32.556  [DependencyChecker] All present     T+6.9s
  12:24:40.666  [ExtensionUpdater] Starting         T+15.0s
  watchdog stall reports: 0

CONTROL (a spawnSync reintroduced on the dependency-check timer)
  [MainThreadWatchdog] main process blocked for 2539ms (8.6s after start).
```

The control matters as much as the result: a watchdog that never fires might
simply be broken.

## Rules that came out of it

- **No synchronous child process anywhere on the startup path.** Use `runProbe()`
  from `ui/desktop/src/utils/dependencyChecker.ts`, or `child_process.spawn`.
  `startupBlocking.test.ts` fails if `spawnSync`/`execSync`/`execFileSync` returns
  to any startup-path module — verified by reintroducing one.
- **Startup delays live in one file**, `utils/startupSchedule.ts`. Three modules
  each choosing their own delay is how they came to overlap. Raising a delay is
  safe; lowering one puts work back into the paint window.
- **Independent probes run concurrently.** The native fallback ran its probes in
  sequence, so its cost was the *sum* of every timeout.
- **Log a fatal startup error before showing a dialog.** `dialog.showErrorBox` is
  modal and blocks the main thread, so on a headless launch it produced a box
  nobody could see and no log line — indistinguishable from a hang.

## Things this is mistaken for

- **"The updater is eating CPU."** It was a real aggravator — the GitHub fallback
  buffered a whole installer in memory (~600 MB peak for a 200 MB asset) and
  auto-downloaded unprompted from background paths — but it never held the main
  thread. Fixing only that would have left the freeze in place.
- **"The machine is loaded."** Load makes every probe slower, which makes the
  freeze *longer*, so a busy machine amplifies this bug. It does not cause it.
- **"The window is broken."** A blank window during daemon startup is a different
  thing: `createChat` shows the window before it awaits the daemon's readiness
  probe. Under a second in practice, and not this bug.

## Related documentation

- [Launching the dev GUI from a shell without a TTY](launching-the-dev-gui.md)
- [When the app "stops scaling with the window"](window-scaling-regressions.md)
- [Debugging the dev GUI with agent-browser](agent-browser-debugging.md)
