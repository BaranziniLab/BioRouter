# Chat / preview lag gate

`chat-perf-probe.mjs` is the repeatable lag check for the tabbed, splittable chat
area and the artifact/preview panel. Lag is an acceptance criterion, not a
nice-to-have, so it gets a probe you can re-run rather than a number in a report
nobody can reproduce.

## Run it

Needs a standalone vite renderer on `:5173` (see `.claude/skills/debug-app` —
the app enforces a single-instance lock, so you cannot run `electron-forge
start` alongside a Playwright-driven instance):

```bash
tmux new-session -d -s vite -x 220 -y 50
tmux send-keys -t vite 'source bin/activate-hermit; cd ui/desktop && \
  npx vite --port 5173 --strictPort --config vite.renderer.config.mts' Enter
until curl -sf -o /dev/null http://localhost:5173/; do sleep 1; done

cd ui/desktop
node scripts/perf/chat-perf-probe.mjs              # 1-group and 4-group scenarios
node scripts/perf/chat-perf-probe.mjs --groups 1   # just the baseline
node scripts/perf/chat-perf-probe.mjs --json /tmp/perf.json
```

Exit code is non-zero when a budget is blown, so it can gate CI or a review.

## What it measures

| Metric | How |
|---|---|
| Typing latency | keydown → painted frame (double-rAF), real CDP key events, in the **focused** composer |
| Typing latency at N groups | same, with 4 `BaseChat`s mounted — the regression this branch's architecture could introduce |
| Long tasks | `PerformanceObserver('longtask')`, >50ms, during the typing burst |
| Transcript scroll | frame deltas + dropped frames over a 25-step wheel scroll |
| Messages in DOM | a count — this is how you see whether virtualization exists |

## Measured baseline (what to expect)

Dev renderer, quiet machine (8 cpus, load ~1.3/cpu), 2026-07-17:

| Scenario | p50 | p95 | max | long tasks >50ms | msgs in DOM |
|---|---|---|---|---|---|
| 1 group | 25.7ms | 33.5ms | 34.8ms | **0** | 355 |
| 4 groups | 35.4ms | 38.7ms | 47.4ms | **0** | 1211 |

**4-group vs 1-group typing p95 ratio: 1.16x** (reproduced at 1.39x on a second
run). Mounting 4 chats does **not** materially slow typing in the focused one.

For contrast, the same probe on the *same build* at load average 93 read
**p95 70ms with 41 long tasks**. That is the machine, not the app — which is
exactly why the load guard exists.

## Budget (`PERF_BUDGET` in the script)

| Budget | Value | Why |
|---|---|---|
| `typingP95Ms` | 60 | ~1.5x the measured 4-group p95. Loose on purpose: the probe runs the **dev** bundle, so a tight absolute would fail a healthy app and train people to ignore the gate |
| `typingMaxMs` | 120 | a single stall past this is felt as a hitch |
| `typingP95RatioVs1Group` | 1.6x | **the load-bearing one** — mounting 4 chats must not materially slow the focused one |
| `longTasksDuringTyping` | 2 | measured 0 on a quiet machine, so >2 is a real finding |

The two figures that carry real signal are the **ratio** and the **long tasks**:
both are architecture-specific and near-immune to the dev-build overhead. The
absolutes are a coarse "something is badly wrong" net.

## Why the probe asserts so loudly

An earlier perf probe in this project reported the N-mounted-`BaseChat` split as
"affordable" **while it was measuring an empty page**. Every number was real and
none of them meant anything. A perf number from an unverified DOM is worse than
no number, because it banks a false pass.

So the probe refuses to measure until it has proved the thing under test is on
screen, and it prints the proof next to every result:

- `verifyMount()` — groups mounted, composers present, a **non-zero transcript**,
  non-trivial body text, and a real on-screen box per group. Throws with the
  observed counts rather than emitting a number.
- `measureTyping()` — asserts the keystrokes actually **landed** in the textarea
  and that each produced a paint sample. A perfect latency over zero typed
  characters is just another void result.
- A single-instance-lock canary — zero windows means the lock, not a finding.
- A machine-load guard — see below.

It seeds a deterministic N-group layout via the `localStorage` blob
`chatGroupsStorage` persists (`biorouter.chatgroups.v1:<windowId>`), rather than
driving a fragile tab-drag gesture, and it drives to `/pair` explicitly because
`ChatGroupsProvider` is mounted **only** on that route (App.tsx) — on `/` the
seed is never even read and you get zero groups.

## Two traps that will fool you

**1. Machine load.** Latency measures a shared machine, not just the app. This
probe was first run on a box at load average 93–209 (cargo builds, other agents,
Box file-sync), where every millisecond reported was contention noise. The probe
asserts `loadavg/cpu <= 1.0` up front and prints load next to every result.
Override with `PERF_MAX_LOAD_PER_CPU=…` only if you accept the numbers are noise.

**2. The dev build is not the app.** The renderer under `:5173` is a **dev**
React build. A CPU profile of a typing burst against it is dominated by
dev-only work — `jsxDEV`, `logComponentRender`, `addObjectDiffToProperties`,
`validatePropertiesInDevelopment`, owner-stack `createTask`/`getTaskName`
(measured: ~646ms of ~1650ms of a 40-keystroke burst). None of that exists in
the packaged app. **Absolute numbers from this probe are dev-inflated**; treat
them as a *relative* before/after signal on identical builds, and do not quote
them as user-felt latency.

To measure a production bundle, build the renderer and serve it on another port,
then redirect at the Playwright layer (`page.route`) — not via Electron's
`webRequest`, which allows only one `onBeforeRequest` listener per session and
`main.js` already registers one, so yours is silently replaced and you measure
dev while calling it prod:

```bash
npx vite build --config vite.renderer.config.mts
npx vite preview --config vite.renderer.config.mts --port 5174 --strictPort
```

Always assert the bundle you got (no `/@vite/client`, hashed `/assets/*.js`)
before trusting a "production" number.

## Housekeeping

Playwright's `app.close()` SIGKILLs Electron, so `main.ts`'s cleanup never runs
and the `biorouterd` it spawned is reparented to init, keeps its port, and
eventually starves new launches. The probe reaps **its own** orphans (backends
whose parent is gone) on startup. If launches hang with no window, check for
orphaned `biorouterd` and orphaned Playwright-Electron processes first — a dead
app is the lock or an orphan, not a perf finding.
