#!/usr/bin/env node
/**
 * BR-71 glass-box harness. Drives a running biorouterd (just debug-server)
 * end-to-end WITHOUT the GUI. Two tiers:
 *
 *  BASELINE (always runs): connects a fake "window" to /ui/workspace, then
 *  exercises the observation plane on an ORDINARY session it creates through
 *  POST /agent/start. Validates: the workspace socket's auth gate in BOTH
 *  directions (a good secret connects, a wrong one is refused — the only
 *  end-to-end check that `check_token`'s /ui/workspace exemption did not just
 *  disable authentication for that path); that a /reply is accepted and its
 *  turn actually closes; observer snapshot-then-live ordering; and the §8.4
 *  resync-cost measurement. It does NOT touch the spawn bridge, so it can never
 *  mask the live tier.
 *
 *  ⚠ The baseline SENDS a `workspace_echo` but does not assert anything about
 *  it, and an earlier revision of this header listed "the echo round trip"
 *  among the validated items. There is no daemon route that reads a stored echo
 *  back, so the send exercises `apply_inbound_frame` without observing it; the
 *  echo's contract is covered by `inbound_frames_reach_the_bridge_by_type` in
 *  `routes/workspace.rs`. Listing it here would make this header the operator's
 *  false evidence that the check ran — the same failure the note below records.
 *
 *  ⚠ The baseline canNOT cover `user_direct` stamping, and an earlier revision
 *  of this comment claimed it did. Stamping fires only for a session whose
 *  `session_type` is SubAgent, and there is no way to create one over HTTP —
 *  `StartAgentRequest` (routes/agent.rs:71-81) has `working_dir`, `workflow`,
 *  `workflow_id`, `workflow_deeplink`, `extension_overrides` and no session
 *  type. Only a real spawn makes one, so both stamping call sites live in the
 *  LIVE tier below. A header that lists a check the script does not perform is
 *  worse than no header: it is the operator's evidence that the check ran.
 *
 *  LIVE (BIOROUTER_HARNESS_LIVE=1 + a configured provider): a parent chat is
 *  asked to spawn a subagent. Validates the Task 33 control-plane chain that
 *  unit tests cannot: open_tab/annotate_tab frames arrive; the child observer
 *  streams; POST /interrupt into the RUNNING child returns 202 (the lease
 *  makes is_turn_active true AND the registered agent drains the queue — the
 *  injected text must appear in the child's observer stream with user_direct
 *  provenance); POST /agent/cancel returns cancelled:true WITH a turn id; a
 *  /reply into the now-idle child is stamped user_direct too (Task 35's OTHER
 *  call site — the tab composer — which nothing else in this plan exercises);
 *  and the parent's final transcript carries "human_intervened":true.
 *
 * Exit codes: 0 = every assertion that ran passed. 1 = an assertion failed.
 *             2 = the harness crashed. 3 = the LIVE tier could not conclude
 *             (the child finished before it could be steered or cancelled).
 *             ⚠ 3 is NOT a pass — Task 40's gate requires 0. Re-run it; if it
 *             recurs, lengthen the child's task rather than relaxing a check.
 */
const BASE = process.env.BIOROUTER_HARNESS_BASE ?? 'http://127.0.0.1:3000';
const SECRET = process.env.BIOROUTER_SERVER__SECRET_KEY ?? 'test';
const LIVE = process.env.BIOROUTER_HARNESS_LIVE === '1';

/** The steer injected into the running child, matched verbatim in its stream. */
const STEER_TEXT = 'Stop at number 3 and summarize.';
/** The tab-composer message, matched verbatim in the child's stored rows. */
const COMPOSER_TEXT = 'typed straight into the subagent tab';

let failures = 0;
let inconclusive = 0;
function assert(name, condition, detail = '') {
  const mark = condition ? '✓' : '✗';
  console.log(`${mark} ${name}${condition ? '' : detail ? ` — ${detail}` : ''}`);
  if (!condition) failures += 1;
}
function skip(name, why) {
  console.log(`- ${name} (skipped: ${why})`);
}
/**
 * A LIVE-tier check that could not run because the world moved (the child
 * finished early). Distinct from `skip`, which is "this tier is off": an
 * inconclusive run must not read as a pass, because these are the assertions
 * Task 40 calls the flagship gate.
 */
function inconclusiveLive(name, why) {
  console.log(`⚠ ${name} — INCONCLUSIVE: ${why}`);
  inconclusive += 1;
}

async function api(path, options = {}) {
  const res = await fetch(`${BASE}${path}`, {
    ...options,
    headers: {
      'X-Secret-Key': SECRET,
      'Content-Type': 'application/json',
      ...(options.headers ?? {}),
    },
  });
  return res;
}

async function json(path, options) {
  const res = await api(path, options);
  return { status: res.status, body: await res.json().catch(() => null) };
}

/**
 * A `/reply` body's `user_message`.
 *
 * ⚠ `metadata` is REQUIRED and camelCased, and the plan's literal omitted it.
 * `Message` (`conversation/message.rs`, `pub struct Message`) is
 * `#[serde(rename_all = "camelCase")]` and its `metadata: MessageMetadata` has
 * no `#[serde(default)]`, so a body without it is rejected by axum's extractor
 * before any handler runs: `422 … missing field \`metadata\``, in ~2 ms. That is
 * indistinguishable from a slow turn at the observer — the stream simply never
 * carries a `Finish`/`Error`, because no turn was ever started — so the omission
 * silently turned the baseline's two lifecycle assertions into a 15 s timeout.
 * `MessageMetadata`'s `user_visible`/`agent_visible` likewise have no defaults
 * and must be sent as `userVisible`/`agentVisible`; snake_case is a 422 too.
 * `id` really is optional (serde fills `Option` with `None`), so it is omitted.
 */
function userMessage(text) {
  return {
    role: 'user',
    created: 0,
    content: [{ type: 'text', text }],
    metadata: { userVisible: true, agentVisible: true },
  };
}

/** The stored conversation rows of a `GET /sessions/{id}` body, in either shape. */
function conversationRows(body) {
  const rows = body?.conversation?.messages ?? body?.conversation ?? [];
  return Array.isArray(rows) ? rows : [];
}

const TIMER = Symbol('observer-wait-elapsed');

/**
 * Open ONE long-lived observer stream and read frames from it.
 *
 * ⚠ Two defects in the plan's `observe()`, both of which make this script — the
 * gate Task 40 blocks on — fail against blameless code.
 *
 * 1. It raced `reader.read()` against a 500 ms timer and ABANDONED the losing
 *    read. Per the Streams standard an abandoned read stays queued and reads are
 *    served FIFO, so the NEXT chunk fulfils the abandoned read and its value is
 *    discarded — a silently dropped frame. `observe_session_events` ticks a
 *    heartbeat every 500 ms (`tokio::time::interval(Duration::from_millis(500))`),
 *    so the two timers were in permanent contention and the drop was not rare.
 *    Fail-safe in direction — a lost frame can only make an assertion fail — but
 *    a built-in flake generator in a pass/fail gate is a defect in the gate.
 *    Here exactly ONE read is outstanding: `pending` is held across iterations
 *    and cleared only once its value has been consumed, so the timer bounds how
 *    long we wait for that same read rather than starting another.
 *
 * 2. It fused "subscribe" and "read until", so the baseline could only start the
 *    stream and post `/reply` concurrently — nothing guaranteed the subscription
 *    existed before the turn published its terminal event, and `Finish`/`Error`
 *    is live-only: it is NOT replayed in the snapshot. Awaiting the response
 *    headers IS that guarantee, because `observe_session_events` calls
 *    `session_events::subscribe` before it builds the response. So opening is
 *    separate and awaited, and `collect` may be called repeatedly on the same
 *    stream — which also means a message published between two collects arrives
 *    as the live `Message` frame the assertions look for, not folded into a new
 *    subscription's snapshot.
 *
 * A read that rejects is returned as `{ error }` rather than thrown: a transport
 * blip must not escape to `main().catch` and exit 2 ("the harness crashed"),
 * which is a different diagnosis from "the stream died".
 */
async function openObserver(sessionId) {
  const res = await api(`/sessions/${sessionId}/events`);
  if (!res.ok || !res.body) {
    await res.text().catch(() => {});
    const error = `HTTP ${res.status}`;
    return { error, collect: async () => ({ frames: [], error }), close: async () => {} };
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  const frames = [];
  let buffer = '';
  let pending = null; // the ONE outstanding reader.read(), never abandoned
  let closed = false;

  const nextChunk = async (waitMs) => {
    if (!pending) pending = reader.read();
    let timer;
    const outcome = await Promise.race([
      pending.then((chunk) => ({ chunk })),
      new Promise((resolve) => {
        timer = setTimeout(() => resolve(TIMER), Math.max(1, waitMs));
      }),
    ]);
    clearTimeout(timer);
    if (outcome === TIMER) return null; // `pending` still owns the read
    pending = null;
    return outcome.chunk;
  };

  const collect = async (until, timeoutMs = 15000) => {
    const deadline = Date.now() + timeoutMs;
    while (!closed && Date.now() < deadline) {
      if (until(frames)) break;
      let chunk;
      try {
        chunk = await nextChunk(Math.min(250, deadline - Date.now()));
      } catch (error) {
        closed = true;
        return { frames, error: `stream read failed: ${error}` };
      }
      if (!chunk) continue;
      if (chunk.done) {
        closed = true;
        break;
      }
      if (!chunk.value) continue;
      buffer += decoder.decode(chunk.value, { stream: true });
      let index;
      while ((index = buffer.indexOf('\n\n')) >= 0) {
        const record = buffer.slice(0, index);
        buffer = buffer.slice(index + 2);
        const data = record.split('\n').find((l) => l.startsWith('data: '));
        if (data) {
          try { frames.push(JSON.parse(data.slice(6))); } catch { /* keepalive */ }
        }
      }
    }
    return { frames };
  };

  const close = async () => {
    closed = true;
    await reader.cancel().catch(() => {});
  };

  return { collect, close };
}

async function main() {
  // ---- fake window on /ui/workspace ---------------------------------------
  const wsUrl = `${BASE.replace(/^http/, 'ws')}/ui/workspace?secret=${encodeURIComponent(
    SECRET
  )}&window_id=harness`;
  const receivedFrames = [];
  const ws = new WebSocket(wsUrl);
  ws.onmessage = (event) => {
    try {
      const frame = JSON.parse(String(event.data));
      receivedFrames.push(frame);
      if (frame.request_id) {
        ws.send(JSON.stringify({
          type: 'workspace_result', request_id: frame.request_id, ok: true, detail: 'harness',
        }));
      }
    } catch { /* ignore */ }
  };
  const opened = await new Promise((resolve) => {
    ws.onopen = () => resolve(true);
    ws.onerror = () => resolve(false);
  });
  // ⚠ NOT `assert(name, true)`. That was the previous form: a literal that could
  // not fail, whose only real failure mode was the harness crashing on the
  // rejected promise above — which prints a stack trace and exit 2, not a `✗`.
  assert('workspace WS connects with query secret', opened && ws.readyState === WebSocket.OPEN);
  if (!opened) {
    console.error('cannot continue without the workspace socket');
    process.exit(1);
  }

  // The negative control, and the ONLY end-to-end check that adding
  // `/ui/workspace` to `check_token`'s exemption list did not simply make the
  // path unauthenticated. `check_workspace_ws_auth`'s unit tests assert the
  // predicate; this asserts the predicate is what the server actually runs.
  const refused = await new Promise((resolve) => {
    const bad = new WebSocket(
      `${BASE.replace(/^http/, 'ws')}/ui/workspace?secret=definitely-not-the-secret&window_id=harness-bad`
    );
    const done = (value) => {
      try { bad.close(); } catch { /* already closed */ }
      resolve(value);
    };
    bad.onopen = () => done(false);   // opened with a wrong secret == the gate is gone
    bad.onerror = () => done(true);
    bad.onclose = () => done(true);
    setTimeout(() => done(false), 5000);
  });
  assert('workspace WS REFUSES a wrong secret', refused);

  ws.send(JSON.stringify({
    type: 'workspace_echo', window_id: 'harness', focused_session: null, layout: [],
  }));

  // ---- BASELINE: observation plane on a directly-driven subagent session --
  const started = await json('/agent/start', {
    method: 'POST', body: JSON.stringify({ working_dir: '/tmp' }),
  });
  assert('POST /agent/start creates a session', started.status === 200 && !!started.body?.id);
  const probeId = started.body.id;

  // Snapshot-then-live ordering: subscribe FIRST — awaited, so the
  // subscription provably exists — then drive one /reply turn (it fails without
  // a provider; the lifecycle bracket is what we assert).
  const probeObserver = await openObserver(probeId);
  assert(
    'observer stream opens for the probe session',
    !probeObserver.error,
    probeObserver.error ?? ''
  );
  const probeReply = await api('/reply', {
    method: 'POST',
    body: JSON.stringify({ session_id: probeId, user_message: userMessage('probe') }),
  });
  // Drain the reply's own SSE body in the background so it is not left dangling.
  // NOT awaited, and NOT cancelled: awaiting parks until the turn ends, and
  // cancelling drops the reply socket — which is precisely what trips the turn's
  // cancellation token (see `cancel_turn`'s doc comment), i.e. it would destroy
  // the closure event the next assertion is waiting for.
  probeReply.text().catch(() => {});
  // A 422/409 here means no turn ever started, and the two assertions below
  // would then fail as a silent 15 s timeout rather than naming the cause.
  assert(
    'POST /reply is accepted (the turn actually starts)',
    probeReply.status === 200,
    `got ${probeReply.status} — the request body was rejected, so no turn ran`
  );
  const { frames: probeFrames, error: probeStreamError } = await probeObserver.collect(
    (frames) =>
      frames.some((f) => f.type === 'UpdateConversation') &&
      frames.some((f) => f.type === 'Finish' || f.type === 'Error'),
  );
  await probeObserver.close();
  assert(
    'observer yields UpdateConversation snapshot first',
    probeFrames[0]?.type === 'UpdateConversation',
    `first frame: ${probeFrames[0]?.type}${probeStreamError ? ` (${probeStreamError})` : ''}`
  );
  assert(
    'observer sees turn closure (Finish/Error)',
    probeFrames.some((f) => f.type === 'Finish' || f.type === 'Error'),
    probeStreamError ?? `${probeFrames.length} frames, none terminal`
  );

  // §8.4 resync-cost measurement: time a fresh observer's first snapshot. The
  // clock stops at the snapshot, before `close()` — the plan's version included
  // the awaited `reader.cancel()` in the number.
  //
  // ⚠ Read it for what it is: the cost of resyncing THIS session, which holds
  // one probe turn. §8.4 asks about resync cost for a real conversation, and
  // this figure will read as "resync is free at any size" if it is quoted
  // without its subject.
  const t0 = Date.now();
  const resyncObserver = await openObserver(probeId);
  const { frames: resyncFrames } = await resyncObserver.collect(
    (frames) => frames.length >= 1, 5000);
  const snapshotMs = Date.now() - t0;
  await resyncObserver.close();
  assert(
    'fresh observer resyncs from storage',
    resyncFrames[0]?.type === 'UpdateConversation',
    resyncObserver.error ?? `first frame: ${resyncFrames[0]?.type}`
  );
  console.log(
    `  (resync snapshot latency: ${snapshotMs} ms for a ${conversationRows(
      (await json(`/sessions/${probeId}`)).body
    ).length}-message session — record BOTH numbers in the PR for §8.4)`
  );

  // ---- LIVE tier ----------------------------------------------------------
  if (!LIVE) {
    skip('spawn announces open_tab + annotate_tab frames', 'set BIOROUTER_HARNESS_LIVE=1');
    skip('interrupt into the RUNNING child returns 202 and appears user_direct', 'live only');
    skip('cancel of the child returns cancelled:true with a turn id', 'live only');
    skip('/reply into a subagent session is stamped user_direct', 'live only');
    skip('parent transcript reports "human_intervened":true', 'live only');
  } else {
    const parent = await json('/agent/start', {
      method: 'POST', body: JSON.stringify({ working_dir: '/tmp' }),
    });
    const parentId = parent.body.id;

    // ⚠ `/agent/start` does NOT attach a provider, and the plan's LIVE tier
    // assumed it did. A session created over HTTP has none: the turn runner
    // calls `restore_provider_from_session` (`workspace/services.rs`,
    // `start_detached_turn`) and `Agent::provider()` returns
    // `Err("Provider not set")` (`agents/agent.rs`), so the parent's turn dies
    // on its first step, no subagent is ever spawned, and every LIVE assertion
    // below fails with no hint as to why. The GUI does not hit this because it
    // follows `/agent/start` with `/agent/update_provider` — so the harness
    // does the same. The values come from the daemon's OWN config rather than
    // being hardcoded, so this works on any configured machine.
    const configValue = async (key) => {
      const { body } = await json('/config/read', {
        method: 'POST', body: JSON.stringify({ key, is_secret: false }),
      });
      return typeof body === 'string' && body.length > 0 ? body : null;
    };
    const provider = process.env.BIOROUTER_HARNESS_PROVIDER ?? (await configValue('BIOROUTER_PROVIDER'));
    const model = process.env.BIOROUTER_HARNESS_MODEL ?? (await configValue('BIOROUTER_MODEL'));
    const attached = provider
      ? await api('/agent/update_provider', {
          method: 'POST',
          body: JSON.stringify({ provider, model, session_id: parentId }),
        })
      : null;
    // Asserted, not assumed: without this the tier's real failure ("no provider
    // is configured on this machine") would masquerade as "the spawn bridge is
    // broken", which is the exact misdiagnosis Task 40 must not be handed.
    assert(
      'LIVE tier attaches a provider to the parent session',
      attached?.status === 200,
      provider
        ? `POST /agent/update_provider → ${attached?.status} for provider=${provider} model=${model}`
        : 'no BIOROUTER_PROVIDER in the daemon config; set BIOROUTER_HARNESS_PROVIDER'
    );
    // Ask the parent to delegate; the instruction makes the child run long
    // enough to steer ("count slowly" + sleep-ish task).
    const replyDone = api('/reply', {
      method: 'POST',
      body: JSON.stringify({
        session_id: parentId,
        user_message: userMessage(
          'Use the subagent tool to delegate this task and wait for it: ' +
            'write a haiku about each of the numbers 1 through 20, one at a time.'
        ),
      }),
    }).then(async (r) => ({ status: r.status, text: await r.text() }));

    // Frames must arrive for SOME child within 60 s.
    //
    // ⚠ The pair must be CORRELATED. The plan searched for the open frame and
    // the badge independently, so with two children in flight the harness could
    // take `childId` from one and assert the parent link from the other — and
    // then steer a session that was never the one it identified.
    //
    // ⚠ `open_tab` is not the only open frame. `announce_open_frame` emits
    // `open_window` when the spawn's `placement` is "window" (frame-vocabulary
    // parity with `workspace_open`), and a model that picks that placement would
    // have read as "the spawn bridge is broken". Both are accepted; the badge —
    // which is sent for every announced child, capped ones included — is what
    // carries the parent link either way.
    const childFromFrames = await (async () => {
      const deadline = Date.now() + 60000;
      while (Date.now() < deadline) {
        const open = receivedFrames.find(
          (f) =>
            (f.cmd === 'open_tab' || f.cmd === 'open_window') && f.session_id !== parentId
        );
        const badge =
          open &&
          receivedFrames.find(
            (f) =>
              f.cmd === 'annotate_tab' &&
              f.badge === 'subagent' &&
              f.session_id === open.session_id
          );
        if (open && badge) return { open, badge };
        await new Promise((r) => setTimeout(r, 500));
      }
      return null;
    })();
    assert('spawn announces open_tab + annotate_tab frames', !!childFromFrames);
    if (childFromFrames) {
      const childId = childFromFrames.open.session_id;
      assert(
        'annotate_tab names the parent',
        childFromFrames.badge.parent_session_id === parentId
      );

      // Child observer: snapshot first, then live frames; spawn context is
      // messages[0] with provenance spawn_context. ONE stream, kept open across
      // the steer below — a second subscription taken after the steer landed
      // would carry it inside the snapshot, not as the live `Message` frame the
      // stamping assertion looks for.
      const childObserver = await openObserver(childId);
      assert(
        'child observer stream opens',
        !childObserver.error,
        childObserver.error ?? ''
      );
      const { frames: childFrames } = await childObserver.collect(
        (frames) => frames.some((f) => f.type === 'Message'),
        30000
      );
      // ⚠ The spawn-context record is NOT reliably in the FIRST snapshot, and
      // the plan read `childFrames.find(UpdateConversation)` — i.e. exactly
      // that one. The open/badge frames come from a detached `tokio::spawn` at
      // session creation, while `persist_spawn_context` runs later, inside
      // `get_agent_messages`, behind a `spawn_blocking` knowledge-base scan. A
      // subscription that wins that race snapshots an EMPTY conversation, and
      // the assertion failed with `null` against blameless code. So: take the
      // most recent snapshot that actually has rows, and if none does yet, poll
      // storage — the record is persisted either way, and `GET /sessions/{id}`
      // is the same source the snapshot is built from.
      const spawnContextRow = await (async () => {
        const deadline = Date.now() + 30000;
        const fromFrames = () =>
          [...childFrames]
            .reverse()
            .find(
              (f) =>
                f.type === 'UpdateConversation' &&
                conversationRows({ conversation: f.conversation }).length > 0
            );
        for (;;) {
          const framed = fromFrames();
          if (framed) return conversationRows({ conversation: framed.conversation })[0];
          const stored = conversationRows((await json(`/sessions/${childId}`)).body);
          if (stored.length > 0) return stored[0];
          if (Date.now() >= deadline) return null;
          await new Promise((r) => setTimeout(r, 1000));
        }
      })();
      assert(
        'spawn-context record is messages[0] with provenance spawn_context',
        spawnContextRow?.metadata?.provenance?.kind === 'spawn_context',
        JSON.stringify(spawnContextRow?.metadata ?? null)
      );

      // THE FLAGSHIP CHAIN (Task 33): steer the RUNNING child.
      const steer = await api('/interrupt', {
        method: 'POST',
        body: JSON.stringify({ session_id: childId, text: STEER_TEXT }),
      });
      assert(
        'POST /interrupt into the RUNNING child returns 202 (lease + registered agent)',
        steer.status === 202,
        `got ${steer.status} (409 = lease missing; the control plane bridge failed)`
      );
      // ⚠ Match the injected TEXT, not merely the provenance. `user_direct` is
      // stamped on every human message into a subagent session, so a bare
      // kind check is satisfied by any of them — including the tab-composer
      // message this same script sends later. The composer assertion below
      // already matches its own text; this one was the asymmetric half.
      const steerLanded = (frames) =>
        frames.some(
          (f) =>
            f.type === 'Message' &&
            f.message?.metadata?.provenance?.kind === 'user_direct' &&
            JSON.stringify(f.message?.content ?? '').includes(STEER_TEXT)
        );
      const { frames: steered } = await childObserver.collect(steerLanded, 30000);
      assert(
        'injected steer appears in the child stream stamped user_direct',
        steerLanded(steered),
        `no user_direct Message carrying the steer text among ${steered.length} frames`
      );
      await childObserver.close();

      // Stop: addressable cancel must find the child's ActiveTurn.
      //
      // ⚠ This assertion used to read
      //   `cancel.body?.cancelled === true || cancel.body?.cancelled === false`
      // — true for EVERY boolean, and therefore for a run in which `begin_turn`
      // was never called and nothing was ever registered. It was also the ONLY
      // automated check that Task 33's turn lease is held, and Task 40 calls it
      // a gate that blocks the phase. It could not block anything.
      //
      // The comment that justified it worried about a real race: the child may
      // have finished on its own. The fix is to MEASURE that condition rather
      // than absorb it into the assertion. `/interrupt` returns 202 only while
      // `is_turn_active(child)` is true — which is precisely the lease — so it
      // doubles as the liveness probe. A finished child yields exit 3
      // (inconclusive), never a green tick.
      //
      // Verified against the real handler (`pub async fn interrupt`,
      // routes/reply.rs — the SYMBOL, because the line numbers this comment
      // used to quote were already stale by the time it was read):
      //
      //   empty text                                  → 400
      //   `!state.is_turn_active(&req.session_id)`    → 409
      //   `get_agent_for_route` fails                 → 500
      //   `Err(InterruptRefused::TurnEnded)`          → 409
      //   otherwise                                   → 202
      //
      // ⚠ FOUR non-202 outcomes, not one. An earlier revision of this comment
      // claimed "there is no third outcome" — in a file whose stated standard
      // is that its comments are the operator's evidence. The classification
      // below is still safe, because it only ever reads a non-202 as "stop,
      // this is inconclusive" and never as a pass: a 500 (an agent that cannot
      // be constructed) is a real problem, but reporting it as a lease failure
      // would be a worse answer than reporting it as inconclusive, and the
      // status is printed either way.
      //
      // ⚠ This probe MUTATES. It queues a real steer into the child — it is
      // not the read-only liveness check the shape suggests — and the daemon
      // exposes no read-only "is a turn running" route (`is_turn_active` is
      // internal; no handler surfaces it). The queued text is harmless here
      // because the child is cancelled immediately afterwards, but a future
      // reader must not copy this as a general-purpose probe.
      const stillRunning = await api('/interrupt', {
        method: 'POST',
        body: JSON.stringify({ session_id: childId, text: 'keep going.' }),
      });
      if (stillRunning.status !== 202) {
        inconclusiveLive(
          'cancel of the child returns cancelled:true with a turn id (Task 33 lease held)',
          `the child was no longer running when the cancel was due (/interrupt → ${stillRunning.status}); ` +
            'lengthen the delegated task and re-run'
        );
      } else {
        const cancel = await json('/agent/cancel', {
          method: 'POST', body: JSON.stringify({ session_id: childId }),
        });
        const cancelHeldTheLease =
          cancel.body?.cancelled === true &&
          typeof cancel.body?.turn_id === 'string' &&
          cancel.body.turn_id.length > 0;
        // ⚠ The liveness probe proves the lease at time T; the cancel runs at
        // T + one round trip. `cancel_turn` returns `None` the instant the turn
        // guard drops, so a child that finished in that window answers
        // `cancelled:false` — reported, before this, as "Task 33's lease is not
        // held". The misdiagnosis the probe was added to prevent had merely
        // moved one step later. So when the cancel finds nothing, ask again
        // whether a turn is still there: `/interrupt` and `/agent/cancel` read
        // the SAME `active_turns` map (`is_turn_active` / `cancel_turn` in
        // `state.rs`), so a 409 now means the turn ended in the window — the
        // world moved, which is inconclusive, not a failure. A 202 now would be
        // a genuine contradiction (a turn is registered, yet the cancel found
        // none) and must fail.
        const afterwards = cancelHeldTheLease
          ? null
          : await api('/interrupt', {
              method: 'POST',
              body: JSON.stringify({ session_id: childId, text: 'still there?' }),
            });
        if (afterwards && afterwards.status !== 202) {
          inconclusiveLive(
            'cancel of the child returns cancelled:true with a turn id (Task 33 lease held)',
            `the turn ended between the liveness probe and the cancel ` +
              `(/agent/cancel → ${JSON.stringify(cancel.body)}, /interrupt afterwards → ` +
              `${afterwards.status}); lengthen the delegated task and re-run`
          );
        } else {
          // `pub struct CancelTurnResponse { cancelled: bool, turn_id:
          // Option<String> }` (routes/reply.rs). `cancelled: true` WITH a turn
          // id is the lease's observable effect: `state.cancel_turn` found an
          // `ActiveTurn` registered under the CHILD's session id, which only
          // Task 33 puts there.
          assert(
            'cancel of the child returns cancelled:true with a turn id (Task 33 lease held)',
            cancelHeldTheLease,
            `${JSON.stringify(cancel.body)}${
              afterwards ? ' — yet /interrupt afterwards still returned 202' : ''
            }`
          );
        }
      }

      // Task 35's OTHER call site: the tab composer. A human typing into the
      // child's tab posts /reply, and `run_turn` must stamp it user_direct. The
      // steer above went through /interrupt, which is a different code path in a
      // different file; nothing else in this plan drives this one end to end.
      // ⚠ The child is NOT reliably idle here, and the plan's comment asserted
      // it was ("cancelled or finished, so /reply is accepted"). `/agent/cancel`
      // trips the turn's cancellation token; the loop then unwinds at its next
      // boundary, which is not synchronous with the 200 the cancel returned. A
      // `/reply` that lands in that window is refused with 409 ("A turn is
      // already in progress for this session.", routes/reply.rs) and the message
      // is never stored — so the stamping assertion below failed while the code
      // under test was blameless. Retry until the turn has actually unwound.
      let composedStatus = 0;
      const composeDeadline = Date.now() + 60000;
      while (Date.now() < composeDeadline) {
        const res = await api('/reply', {
          method: 'POST',
          body: JSON.stringify({
            session_id: childId,
            user_message: userMessage(COMPOSER_TEXT),
          }),
        });
        composedStatus = res.status;
        await res.text();
        if (composedStatus !== 409) break;
        await new Promise((r) => setTimeout(r, 1000));
      }
      // Asserted so that "the message never landed" can never again read as
      // "the stamp is missing" — two very different bugs with one symptom.
      assert(
        'the tab-composer /reply is accepted by the now-idle child',
        composedStatus === 200,
        `POST /reply → ${composedStatus} (409 = the cancelled turn never unwound)`
      );
      const rows = conversationRows((await json(`/sessions/${childId}`)).body);
      assert(
        '/reply into a subagent session is stamped user_direct (the tab composer path)',
        rows.some(
          (m) =>
            m?.metadata?.provenance?.kind === 'user_direct' &&
            JSON.stringify(m?.content ?? '').includes(COMPOSER_TEXT)
        ),
        `no user_direct row for the composed message among ${rows.length} rows`
      );

      // Parent resolution: the tool result must carry human_intervened.
      // Assert the STRUCTURED field, not the substring "intervened": the
      // parent's /reply stream also carries the model's own prose, which can
      // contain that word for reasons having nothing to do with the flag.
      // `human_intervened` is `skip_serializing_if = "std::ops::Not::not"`, so
      // the key appears only when it is true.
      const parentReply = await replyDone;
      // Asserted for the same reason as the probe's and the composer's: a
      // non-200 parent `/reply` (a rejected body, a 409) means no parent turn
      // ever ran, and without this line that surfaces only as the flag being
      // missing — the wrong diagnosis, for the third time in this file.
      assert(
        'the parent /reply is accepted (the delegating turn actually starts)',
        parentReply.status === 200,
        `got ${parentReply.status} — no parent turn ran, so nothing was delegated`
      );
      assert(
        'parent transcript reports "human_intervened":true',
        parentReply.text.includes('"human_intervened":true'),
        'not found in the parent /reply stream — the child ran, was steered, and the ' +
          'parent was never told'
      );
    }
  }

  ws.close();
  if (failures > 0) {
    console.log(`\n${failures} FAILED`);
    process.exit(1);
  }
  if (inconclusive > 0) {
    console.log(
      `\n${inconclusive} LIVE assertion(s) INCONCLUSIVE — not a pass. ` +
        'Re-run; if it recurs, make the delegated task longer.'
    );
    process.exit(3);
  }
  console.log('\nAll assertions passed.');
  process.exit(0);
}

main().catch((error) => {
  console.error('harness crashed:', error);
  process.exit(2);
});
