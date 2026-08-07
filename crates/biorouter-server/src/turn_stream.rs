//! The live turn stream: one sequence-numbered, replayable frame log per turn,
//! with 0..N simultaneous observers.
//!
//! # Why this exists
//!
//! Before this module a turn's output went to a single `mpsc::channel(100)` —
//! the body of the `POST /reply` that started it. Exactly one consumer. When
//! that consumer went away (window closed, tab moved, renderer reloaded), the
//! next `tx.send` failed and the handler called `cancel_token.cancel()`, so
//! **"nobody is listening" and "stop working" were the same event**. Nothing
//! buffered, so nothing could catch up: the turn belonged to an HTTP *request*
//! rather than to a *session*.
//!
//! A `TurnStream` is that missing session-scoped object. One pump (the task
//! `/reply` spawns) maps the session bus into frames and appends them here;
//! every HTTP response that wants to watch the turn takes a [`StreamReader`]
//! and gets the whole log from seq 0, then the live tail. Readers come and go
//! without the turn noticing.
//!
//! # The two binding requirements
//!
//! **R1 — no progress is ever lost.** Every frame is appended to a replay
//! buffer that is sized to hold a whole turn ([`REPLAY_BYTE_BUDGET`]). If a
//! pathological turn overruns it, the OLDEST frames are dropped and the reader
//! is told so ([`StreamReader::take_gap`]); `/reply` then prepends a
//! whole-conversation snapshot read from the session store before the retained
//! tail. Eviction is therefore never a loss: the evicted prefix is exactly the
//! part that has already been persisted, and the un-persisted part (the running
//! assistant message) is exactly the part that is retained.
//!
//! **R2 — seamless in the UI.** Frames carry a monotonic per-turn `seq` so a
//! client can apply them idempotently after re-attaching, and replayed frames
//! carry `"replay": true` so the client can tell a backlog from the live tail.
//! The backlog is written to the socket as fast as it will take it — never
//! re-paced at the original speed.
//!
//! # Wire format
//!
//! Every frame is one SSE `data:` line holding the existing
//! [`crate::routes::reply::MessageEvent`] JSON object, with up to three fields
//! added at the top level:
//!
//! | field     | on                          | meaning |
//! |-----------|-----------------------------|---------|
//! | `seq`     | every logged frame          | monotonic per-turn index, from 0 |
//! | `turn_id` | every logged frame          | the server-assigned turn this belongs to |
//! | `replay`  | replayed frames only        | `true` — already-seen history, apply idempotently |
//!
//! A frame with **no `seq`** is connection-local and carries no ordering
//! guarantee: the `Ping` heartbeat, and the storage-resync `UpdateConversation`
//! emitted ahead of a gapped backlog. A client keyed on `seq` must ignore
//! ordering for those and simply apply them.
//!
//! # Concurrency
//!
//! All state lives behind one `std::sync::Mutex` held for a few instructions at
//! a time and never across an `await`. [`TurnStream::publish`] appends to the
//! replay buffer **and** broadcasts under that one lock, and
//! [`TurnStream::attach`] subscribes **and** snapshots the buffer under it too.
//! That pairing is what makes an attach atomic: a frame published after the
//! subscribe is guaranteed to arrive on the broadcast, and one published before
//! it is guaranteed to be in the snapshot. Cloning the sender out and
//! subscribing afterwards would open a window in which a frame is in neither.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::routes::reply::MessageEvent;

/// How much serialized frame text one turn may retain for replay.
///
/// A turn is bounded by the context window, so a normal one — even a long
/// tool-heavy one — is a few hundred KB of frames. 8 MiB is roughly an order of
/// magnitude above that, chosen so that R1 holds *without* the storage fallback
/// for every turn a user will actually run, while still bounding the pathology
/// (an un-coalesced 100k-token answer is one frame per provider chunk and can
/// reach tens of MB). At most one turn per session is live, so the real ceiling
/// is 8 MiB × concurrent chats.
pub const REPLAY_BYTE_BUDGET: usize = 8 * 1024 * 1024;

/// Frame-count companion to [`REPLAY_BYTE_BUDGET`], so a turn that emits an
/// enormous number of tiny frames is bounded too.
pub const REPLAY_FRAME_BUDGET: usize = 200_000;

/// What a *finished* turn keeps, once its result is persisted.
///
/// The whole log is no longer needed: everything a completed turn produced is
/// in the session store, so a late attach can be answered from storage plus the
/// terminal frame. Keeping a smaller tail means the common case (a turn well
/// under this size) is still replayed exactly, without a five-minute retention
/// window pinning 8 MiB per recently-finished chat.
pub const CLOSED_REPLAY_BYTE_BUDGET: usize = 2 * 1024 * 1024;

/// Broadcast ring for the live tail. Deliberately modest: a reader that falls
/// behind it is repaired from the replay buffer, which is the authoritative
/// record, so this only has to absorb ordinary scheduling jitter.
const LIVE_RING_CAPACITY: usize = 512;

/// How long a turn may run with **zero** observers before it is treated as
/// abandoned and cancelled.
///
/// The number this must beat is a window reload, measured at ~4.6 s. Five
/// minutes is ~65× that, which also comfortably covers a laptop sleep/resume, a
/// network blip, a renderer crash-and-restart, and a user dragging a tab
/// between windows and getting distracted. It is short enough that a genuinely
/// abandoned turn cannot spend money indefinitely: the worst case is five more
/// minutes of tokens, and every *deliberate* stop — the Stop button,
/// `POST /agent/cancel`, session teardown — is still instant.
///
/// Overridable with `BIOROUTER_TURN_ORPHAN_TIMEOUT_MS` so tests can use a
/// sub-second value; production never sets it.
pub const DEFAULT_ORPHAN_TIMEOUT: Duration = Duration::from_secs(300);

/// The orphan reaper's poll interval, capped so a short test timeout is still
/// observed promptly.
fn reaper_tick(timeout: Duration) -> Duration {
    (timeout / 10).clamp(Duration::from_millis(20), Duration::from_secs(5))
}

/// [`DEFAULT_ORPHAN_TIMEOUT`], or the `BIOROUTER_TURN_ORPHAN_TIMEOUT_MS`
/// override. An unparseable or zero value falls back to the default rather than
/// reaping instantly — a typo in an env var must not start killing live turns.
pub fn orphan_timeout() -> Duration {
    std::env::var("BIOROUTER_TURN_ORPHAN_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|ms| *ms > 0)
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_ORPHAN_TIMEOUT)
}

/// One logged frame: its per-turn sequence number and the serialized JSON
/// object (already carrying `seq` and `turn_id`).
#[derive(Clone, Debug)]
pub struct SeqFrame {
    pub seq: u64,
    json: Arc<str>,
}

impl SeqFrame {
    /// The SSE bytes for a LIVE delivery of this frame.
    pub fn live_sse(&self) -> String {
        format!("data: {}\n\n", self.json)
    }

    /// The SSE bytes for a REPLAYED delivery: the same object with
    /// `"replay":true` spliced in as its first member.
    ///
    /// String surgery rather than a re-serialize, and it is exact: `json` is
    /// always a JSON *object*, so it always starts with `{`, and member order
    /// is not significant. `serde_json` emits no whitespace after the brace, so
    /// the only case needing care is an empty object, which takes no separator.
    /// Pinned by `replay_marker_is_spliced_in_exactly`.
    pub fn replay_sse(&self) -> String {
        let Some(rest) = self.json.strip_prefix('{') else {
            // Unreachable — `encode_frame` only ever produces an object. Degrade
            // to a live delivery rather than emit a corrupt frame; a client that
            // re-renders one frame is a far smaller failure than one that
            // cannot parse the stream at all.
            tracing::error!("a logged frame was not a JSON object; replay marker skipped");
            return self.live_sse();
        };
        let separator = if rest.starts_with('}') { "" } else { "," };
        format!("data: {{\"replay\":true{separator}{rest}\n\n")
    }
}

/// What a reader saw next.
#[derive(Debug)]
pub enum ReaderEvent {
    /// A frame, and whether it came from the replay buffer rather than live.
    Frame(SeqFrame, bool),
    /// The reader fell so far behind that the frames it still needed have been
    /// evicted. The consumer must resync from storage before continuing; the
    /// reader has already skipped forward to the oldest frame still retained.
    Gap,
    /// The turn is over and every frame has been delivered.
    Closed,
}

/// The mutable half of a [`TurnStream`].
#[derive(Debug)]
struct Inner {
    next_seq: u64,
    replay: VecDeque<SeqFrame>,
    replay_bytes: usize,
    /// The lowest seq still retained. Anything below this was evicted, and a
    /// reader asking for it gets [`ReaderEvent::Gap`].
    oldest_seq: u64,
    /// True once a `Finish`/`Error` frame has been logged, so [`TurnStream::close`]
    /// knows whether it must synthesize one.
    terminal_logged: bool,
    closed: bool,
    /// Live readers right now. Zero is an ordinary state, not an error.
    observers: usize,
    /// True once anything has EVER attached. The orphan reaper will not fire
    /// before this: a turn started by a caller that never intended to watch it
    /// (an injected workspace turn) has no orphan to reap.
    ever_attached: bool,
    /// When `observers` last fell to zero, if it is zero now. `None` while
    /// somebody is watching.
    idle_since: Option<Instant>,
}

/// A turn's frame log: append-only, sequence-numbered, replayable, fanned out.
#[derive(Debug)]
pub struct TurnStream {
    session_id: String,
    turn_id: String,
    tx: broadcast::Sender<SeqFrame>,
    inner: Mutex<Inner>,
    /// Mirrors `Inner::observers` for cheap, lock-free introspection in logs
    /// and tests. Authoritative count stays under the lock.
    observers: AtomicU64,
}

impl TurnStream {
    pub fn new(session_id: impl Into<String>, turn_id: impl Into<String>) -> Arc<Self> {
        let (tx, _) = broadcast::channel(LIVE_RING_CAPACITY);
        Arc::new(Self {
            session_id: session_id.into(),
            turn_id: turn_id.into(),
            tx,
            inner: Mutex::new(Inner {
                next_seq: 0,
                replay: VecDeque::new(),
                replay_bytes: 0,
                oldest_seq: 0,
                terminal_logged: false,
                closed: false,
                observers: 0,
                ever_attached: false,
                idle_since: None,
            }),
            observers: AtomicU64::new(0),
        })
    }

    /// Poisoning cannot leave this logically inconsistent — every critical
    /// section below is a handful of infallible field updates — so a panic
    /// elsewhere must not turn every live turn into a dead stream.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }

    pub fn observer_count(&self) -> u64 {
        self.observers.load(Ordering::Relaxed)
    }

    pub fn is_closed(&self) -> bool {
        self.lock().closed
    }

    /// Frames logged so far (tests/introspection).
    pub fn next_seq(&self) -> u64 {
        self.lock().next_seq
    }

    /// This turn's terminal frame, once it has one.
    ///
    /// The whole answer for a client re-POSTing a turn that has already
    /// finished: its transcript was read back from the session store and already
    /// contains the turn, so all it still needs is the fact that the turn ended.
    /// The last retained frame IS the terminal — `close` guarantees a terminal
    /// exists and nothing may be appended after it (`publish` refuses once
    /// closed), and eviction only ever drops from the FRONT.
    pub fn terminal_frame(&self) -> Option<SeqFrame> {
        let inner = self.lock();
        if !inner.terminal_logged {
            return None;
        }
        inner.replay.back().cloned()
    }

    /// Append one frame and fan it out. Returns its sequence number.
    ///
    /// Never blocks and never fails: a stream with no observers is an ordinary
    /// state, and a broadcast send with no receivers is a no-op. This is the
    /// single most important property in the module — it is what makes "nobody
    /// is listening" stop meaning "stop working".
    pub fn publish(&self, event: &MessageEvent) -> u64 {
        let terminal = matches!(
            event,
            MessageEvent::Finish { .. } | MessageEvent::Error { .. }
        );
        let mut inner = self.lock();
        if inner.closed {
            // The turn is over; nothing may be appended after its terminal
            // frame or a late attach would see a frame past the end.
            return inner.next_seq;
        }
        let seq = inner.next_seq;
        let json: Arc<str> = Arc::from(encode_frame(event, seq, &self.turn_id));
        let frame = SeqFrame { seq, json };

        inner.next_seq += 1;
        inner.terminal_logged |= terminal;
        inner.replay_bytes += frame.json.len();
        inner.replay.push_back(frame.clone());
        evict_to_budget(&mut inner, REPLAY_BYTE_BUDGET, REPLAY_FRAME_BUDGET);

        // Under the same lock as the append — see the module docs.
        let _ = self.tx.send(frame);
        seq
    }

    /// End the turn's log.
    ///
    /// Guarantees a terminal frame exists: a pump that exits on cancellation
    /// without ever seeing the runner's `TurnFinished` would otherwise leave a
    /// late-attaching client waiting forever for an end that never comes. The
    /// synthesized frame is an `Error`, not a `Finish`, because a stream that
    /// ended without its runner saying why did not finish normally, and telling
    /// a client "stop, all good" when the turn's fate is unknown is the worse
    /// lie of the two.
    pub fn close(&self) {
        let synthetic = {
            let inner = self.lock();
            !inner.closed && !inner.terminal_logged
        };
        if synthetic {
            self.publish(&MessageEvent::error(
                "The stream for this turn ended without a result. Please retry.",
                "stream_ended_without_terminal",
                crate::routes::reply::TurnErrorScope::Internal,
                true,
                None,
            ));
        }
        let mut inner = self.lock();
        inner.closed = true;
        // A finished turn's output is persisted; keep only enough to replay a
        // normal turn verbatim (see CLOSED_REPLAY_BYTE_BUDGET).
        evict_to_budget(&mut inner, CLOSED_REPLAY_BYTE_BUDGET, REPLAY_FRAME_BUDGET);
        drop(inner);
        // Wake every parked reader so it observes `closed` and ends its
        // response instead of blocking on a channel nothing will send to.
        let _ = self.tx.send(SeqFrame {
            seq: u64::MAX,
            json: Arc::from("{}"),
        });
    }

    /// Attach an observer that already holds frames `0..from_seq`.
    ///
    /// Subscribes and snapshots the replay buffer under one lock, so no frame
    /// can fall between the backlog and the live tail.
    pub fn attach(self: &Arc<Self>, from_seq: u64) -> StreamReader {
        let mut inner = self.lock();
        let rx = self.tx.subscribe();
        let gap = from_seq < inner.oldest_seq;
        let backlog: VecDeque<SeqFrame> = inner
            .replay
            .iter()
            .filter(|f| f.seq >= from_seq)
            .cloned()
            .collect();
        let next = backlog.front().map_or(from_seq, |f| f.seq);

        inner.observers += 1;
        inner.ever_attached = true;
        inner.idle_since = None;
        self.observers
            .store(inner.observers as u64, Ordering::Relaxed);
        let closed = inner.closed;
        drop(inner);

        StreamReader {
            stream: Arc::clone(self),
            rx,
            backlog,
            next,
            gap,
            closed,
        }
    }

    /// Cancel the turn once it has been observerless for `timeout`.
    ///
    /// The reaper is the ONLY thing that ends a turn because of its audience,
    /// and it is deliberately generous — see [`DEFAULT_ORPHAN_TIMEOUT`]. It
    /// will not fire on a turn nothing ever attached to, and it exits as soon
    /// as the stream closes, so a completed turn never trips it.
    pub fn spawn_orphan_reaper(
        self: &Arc<Self>,
        cancel: CancellationToken,
        timeout: Duration,
    ) -> tokio::task::JoinHandle<()> {
        let stream = Arc::clone(self);
        let tick = reaper_tick(timeout);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = cancel.cancelled() => return,
                    () = tokio::time::sleep(tick) => {}
                }
                let idle_for = {
                    let inner = stream.lock();
                    if inner.closed {
                        return;
                    }
                    if !inner.ever_attached || inner.observers > 0 {
                        continue;
                    }
                    inner.idle_since.map(|since| since.elapsed())
                };
                if idle_for.is_some_and(|elapsed| elapsed >= timeout) {
                    tracing::warn!(
                        counter.biorouter.turn_orphan_reaped = 1,
                        session_id = %stream.session_id,
                        turn_id = %stream.turn_id,
                        timeout_ms = timeout.as_millis() as u64,
                        "no client has observed this turn for the orphan timeout; cancelling"
                    );
                    cancel.cancel();
                    return;
                }
            }
        })
    }

    fn detach(&self) {
        let mut inner = self.lock();
        inner.observers = inner.observers.saturating_sub(1);
        if inner.observers == 0 {
            inner.idle_since = Some(Instant::now());
        }
        self.observers
            .store(inner.observers as u64, Ordering::Relaxed);
    }
}

/// Drop frames from the front until the buffer fits, remembering how far the
/// eviction reached so a reader asking for an evicted seq gets a
/// [`ReaderEvent::Gap`] rather than a silently short stream.
fn evict_to_budget(inner: &mut Inner, byte_budget: usize, frame_budget: usize) {
    while (inner.replay_bytes > byte_budget || inner.replay.len() > frame_budget)
        && inner.replay.len() > 1
    {
        if let Some(dropped) = inner.replay.pop_front() {
            inner.replay_bytes -= dropped.json.len();
            inner.oldest_seq = dropped.seq + 1;
        }
    }
}

/// Serialize one frame and stamp it with its sequence number and turn id.
///
/// The `seq`/`turn_id` fields are ADDED to the existing `MessageEvent` object
/// rather than wrapping it in an envelope, so every client that parses today's
/// frames keeps parsing them unchanged.
fn encode_frame(event: &MessageEvent, seq: u64, turn_id: &str) -> String {
    let mut value = serde_json::to_value(event).unwrap_or_else(|e| {
        serde_json::json!({
            "type": "Error",
            "error": format!("Failed to serialize stream event: {e}"),
            "code": "stream_serialization_failed",
            "scope": "internal",
            "retryable": false,
        })
    });
    if let Some(object) = value.as_object_mut() {
        object.insert("seq".to_string(), serde_json::json!(seq));
        object.insert("turn_id".to_string(), serde_json::json!(turn_id));
    }
    value.to_string()
}

/// One observer of a [`TurnStream`]. Reclaims its slot on drop, which is what
/// the orphan reaper counts.
#[derive(Debug)]
pub struct StreamReader {
    stream: Arc<TurnStream>,
    rx: broadcast::Receiver<SeqFrame>,
    backlog: VecDeque<SeqFrame>,
    /// The next sequence number this reader still needs. Frames below it are
    /// dropped, which is what makes a re-attach idempotent.
    next: u64,
    gap: bool,
    closed: bool,
}

impl StreamReader {
    /// True when the frames this reader asked for had already been evicted, so
    /// the consumer must recover the prefix from storage (R1's fallback).
    /// Cleared once reported.
    pub fn take_gap(&mut self) -> bool {
        std::mem::take(&mut self.gap)
    }

    pub fn turn_id(&self) -> &str {
        self.stream.turn_id()
    }

    /// The next frame, from the backlog first and then live.
    ///
    /// Returns [`ReaderEvent::Closed`] once the turn is over and the backlog is
    /// drained — never a hang, which is what makes attaching to an
    /// already-finished turn safe.
    pub async fn recv(&mut self) -> ReaderEvent {
        loop {
            if let Some(frame) = self.backlog.pop_front() {
                self.next = frame.seq + 1;
                return ReaderEvent::Frame(frame, true);
            }
            if self.closed {
                return ReaderEvent::Closed;
            }
            match self.rx.recv().await {
                Ok(frame) => {
                    if frame.seq == u64::MAX {
                        // The close wake-up. Re-snapshot: frames may have been
                        // appended (the synthesized terminal) after our last read.
                        self.refill();
                        continue;
                    }
                    if frame.seq < self.next {
                        continue; // already delivered — idempotent by construction
                    }
                    if frame.seq > self.next {
                        // A frame went missing between the ring and us; repair
                        // from the replay buffer rather than skipping it.
                        self.refill();
                        continue;
                    }
                    self.next = frame.seq + 1;
                    return ReaderEvent::Frame(frame, false);
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    // The replay buffer is the authoritative record; re-read
                    // from it instead of resyncing the whole conversation.
                    self.refill();
                    if self.gap {
                        return ReaderEvent::Gap;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => {
                    self.refill();
                    if self.backlog.is_empty() {
                        return ReaderEvent::Closed;
                    }
                }
            }
        }
    }

    /// Re-snapshot the replay buffer from `next`, recording whether the frames
    /// we needed have been evicted.
    fn refill(&mut self) {
        let inner = self.stream.lock();
        if self.next < inner.oldest_seq {
            self.gap = true;
        }
        self.backlog = inner
            .replay
            .iter()
            .filter(|f| f.seq >= self.next)
            .cloned()
            .collect();
        if let Some(front) = self.backlog.front() {
            self.next = front.seq;
        }
        self.closed = inner.closed;
    }
}

impl Drop for StreamReader {
    fn drop(&mut self) {
        self.stream.detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use biorouter::conversation::message::{Message, TokenState};

    fn text_frame(text: &str) -> MessageEvent {
        MessageEvent::Message {
            message: Message::assistant().with_id("m-1").with_text(text),
            token_state: TokenState::default(),
        }
    }

    fn finish() -> MessageEvent {
        MessageEvent::Finish {
            reason: "stop".to_string(),
            token_state: TokenState::default(),
        }
    }

    fn seq_of(sse: &str) -> u64 {
        let json = sse.strip_prefix("data: ").unwrap().trim_end();
        serde_json::from_str::<serde_json::Value>(json).unwrap()["seq"]
            .as_u64()
            .unwrap()
    }

    /// Frames are numbered from 0, monotonically, and the number reaches the
    /// wire — without which a client cannot dedupe and R2 is unachievable.
    #[tokio::test]
    async fn frames_are_numbered_from_zero_and_the_number_reaches_the_wire() {
        let stream = TurnStream::new("s", "turn-1");
        assert_eq!(stream.publish(&text_frame("a")), 0);
        assert_eq!(stream.publish(&text_frame("b")), 1);
        assert_eq!(stream.publish(&finish()), 2);
        // Without this the reader below would correctly park on the live tail
        // forever: a stream that has logged a terminal is not the same thing as
        // a stream that has been CLOSED, and only the pump closes one.
        stream.close();

        let mut reader = stream.attach(0);
        let mut seqs = Vec::new();
        while let ReaderEvent::Frame(frame, _) = reader.recv().await {
            assert_eq!(seq_of(&frame.live_sse()), frame.seq);
            seqs.push(frame.seq);
        }
        assert_eq!(seqs, vec![0, 1, 2]);
    }

    /// The replay marker is spliced in without disturbing the object, so a
    /// client parses the same frame either way.
    #[test]
    fn replay_marker_is_spliced_in_exactly() {
        let stream = TurnStream::new("s", "turn-1");
        stream.publish(&text_frame("hello"));
        let frame = stream.lock().replay[0].clone();

        let live: serde_json::Value =
            serde_json::from_str(frame.live_sse().strip_prefix("data: ").unwrap().trim_end())
                .unwrap();
        let replayed: serde_json::Value = serde_json::from_str(
            frame
                .replay_sse()
                .strip_prefix("data: ")
                .unwrap()
                .trim_end(),
        )
        .unwrap();

        assert!(live.get("replay").is_none(), "a live frame is not replay");
        assert_eq!(replayed["replay"], serde_json::json!(true));
        assert_eq!(replayed["type"], "Message");
        assert_eq!(replayed["seq"], serde_json::json!(0));
        assert_eq!(replayed["turn_id"], "turn-1");
        // Everything else is byte-identical.
        for (key, value) in live.as_object().unwrap() {
            assert_eq!(&replayed[key], value, "field {key} changed under replay");
        }

        // The two degenerate shapes the splice has to survive: an empty object
        // (a stray comma would make it invalid JSON) and a non-object (which
        // must degrade, not corrupt).
        let empty = SeqFrame {
            seq: 0,
            json: Arc::from("{}"),
        };
        assert_eq!(empty.replay_sse(), "data: {\"replay\":true}\n\n");
        let not_an_object = SeqFrame {
            seq: 0,
            json: Arc::from("\"scalar\""),
        };
        assert_eq!(not_an_object.replay_sse(), not_an_object.live_sse());
    }

    /// Publishing with nobody attached is an ordinary no-op — the property the
    /// whole fix rests on.
    #[tokio::test]
    async fn publishing_with_no_observers_is_ordinary_and_replayable_later() {
        let stream = TurnStream::new("s", "turn-1");
        assert_eq!(stream.observer_count(), 0);
        for i in 0..10 {
            stream.publish(&text_frame(&format!("chunk-{i}")));
        }
        let mut reader = stream.attach(0);
        let mut seen = 0;
        while let ReaderEvent::Frame(_, replay) = reader.recv().await {
            assert!(replay, "everything published before the attach is replay");
            seen += 1;
            if seen == 10 {
                break;
            }
        }
        assert_eq!(seen, 10);
    }

    /// Two readers of one turn see every frame, in identical order.
    #[tokio::test]
    async fn two_simultaneous_readers_see_identical_frames() {
        let stream = TurnStream::new("s", "turn-1");
        let a = stream.attach(0);
        let b = stream.attach(0);
        assert_eq!(stream.observer_count(), 2);

        for i in 0..5 {
            stream.publish(&text_frame(&format!("t{i}")));
        }
        stream.publish(&finish());
        stream.close();

        async fn drain(mut r: StreamReader) -> Vec<u64> {
            let mut out = Vec::new();
            while let ReaderEvent::Frame(f, _) = r.recv().await {
                out.push(f.seq);
            }
            out
        }
        let (from_a, from_b) = tokio::join!(drain(a), drain(b));
        assert_eq!(from_a, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(
            from_a, from_b,
            "two observers of one turn must see identical frames in identical order"
        );
    }

    /// `from_seq` lets a client that already holds a prefix ask only for the
    /// rest — the cheap path for a reconnect that lost nothing.
    #[tokio::test]
    async fn from_seq_skips_frames_the_client_already_holds() {
        let stream = TurnStream::new("s", "turn-1");
        for i in 0..4 {
            stream.publish(&text_frame(&format!("t{i}")));
        }
        stream.close();
        let mut reader = stream.attach(2);
        let mut seqs = Vec::new();
        while let ReaderEvent::Frame(f, _) = reader.recv().await {
            seqs.push(f.seq);
        }
        // 4 is the synthesized terminal `close` appended (no Finish was logged).
        assert_eq!(seqs, vec![2, 3, 4]);
    }

    /// Attaching to a turn that is already over ENDS, promptly. A hang here is
    /// the failure the contract calls out by name.
    #[tokio::test]
    async fn attaching_after_the_turn_ended_never_hangs() {
        let stream = TurnStream::new("s", "turn-1");
        stream.publish(&text_frame("all of it"));
        stream.publish(&finish());
        stream.close();

        let drained = tokio::time::timeout(Duration::from_secs(5), async {
            let mut reader = stream.attach(0);
            let mut out = Vec::new();
            loop {
                match reader.recv().await {
                    ReaderEvent::Frame(f, replay) => out.push((f.seq, replay)),
                    ReaderEvent::Gap => {}
                    ReaderEvent::Closed => break,
                }
            }
            out
        })
        .await
        .expect("a late attach must terminate, not block on a turn that is over");
        assert_eq!(drained, vec![(0, true), (1, true)]);
    }

    /// `close` never leaves a stream without a terminal frame, or a late
    /// attacher waits forever for an end that already happened.
    #[tokio::test]
    async fn close_synthesizes_a_terminal_when_the_pump_died_without_one() {
        let stream = TurnStream::new("s", "turn-1");
        stream.publish(&text_frame("half an answer"));
        stream.close();

        let mut reader = stream.attach(0);
        let mut kinds = Vec::new();
        while let ReaderEvent::Frame(f, _) = reader.recv().await {
            let json: serde_json::Value =
                serde_json::from_str(f.live_sse().strip_prefix("data: ").unwrap().trim_end())
                    .unwrap();
            kinds.push(json["type"].as_str().unwrap().to_string());
        }
        assert_eq!(kinds, vec!["Message", "Error"]);
    }

    /// Nothing may be appended after the terminal, or a late attach would read
    /// past the end of the turn.
    #[tokio::test]
    async fn a_closed_stream_refuses_further_frames() {
        let stream = TurnStream::new("s", "turn-1");
        stream.publish(&finish());
        stream.close();
        let before = stream.next_seq();
        stream.publish(&text_frame("too late"));
        assert_eq!(stream.next_seq(), before);
    }

    /// Eviction drops the OLDEST frames and reports a gap, so the consumer
    /// knows to recover the prefix from storage rather than shipping a hole.
    #[tokio::test]
    async fn overrunning_the_budget_evicts_the_prefix_and_reports_a_gap() {
        let stream = TurnStream::new("s", "turn-1");
        for i in 0..64 {
            stream.publish(&text_frame(&format!("chunk-{i}")));
        }
        {
            let mut inner = stream.lock();
            evict_to_budget(&mut inner, 0, usize::MAX);
        }
        let mut reader = stream.attach(0);
        assert!(reader.take_gap(), "an evicted prefix must be reported");
        assert!(
            !reader.take_gap(),
            "…and reported once, so it is not resynced on every frame"
        );
        match reader.recv().await {
            ReaderEvent::Frame(f, _) => assert_eq!(
                f.seq, 63,
                "the newest frame — the un-persisted tail — is what survives"
            ),
            other => panic!("expected the retained tail, got {other:?}"),
        }
    }

    /// The reaper fires on a turn nobody is watching…
    #[tokio::test(flavor = "multi_thread")]
    async fn the_orphan_reaper_cancels_a_turn_with_no_observers() {
        let stream = TurnStream::new("s", "turn-1");
        let cancel = CancellationToken::new();
        let reader = stream.attach(0);
        let reaper = stream.spawn_orphan_reaper(cancel.clone(), Duration::from_millis(120));

        drop(reader); // the last window closes
        tokio::time::timeout(Duration::from_secs(5), cancel.cancelled())
            .await
            .expect("an abandoned turn must be reaped");
        reaper.abort();
    }

    /// …and does NOT fire across a gap the length of a window reload, which is
    /// the whole reason the timeout is minutes rather than seconds.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_orphan_reaper_survives_a_reload_length_gap() {
        let stream = TurnStream::new("s", "turn-1");
        let cancel = CancellationToken::new();
        // 5 s stands in for the production 5 min; the gap below stands in for a
        // measured ~4.6 s reload scaled by the same factor.
        let reaper = stream.spawn_orphan_reaper(cancel.clone(), Duration::from_millis(2_000));

        let reader = stream.attach(0);
        drop(reader);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let _reattached = stream.attach(0);
        tokio::time::sleep(Duration::from_millis(400)).await;

        assert!(
            !cancel.is_cancelled(),
            "a reload-length gap must not reap a live turn"
        );
        reaper.abort();
    }

    /// A turn nothing ever attached to is not an orphan — an injected workspace
    /// turn has no window to lose.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_never_attached_turn_is_not_reaped() {
        let stream = TurnStream::new("s", "turn-1");
        let cancel = CancellationToken::new();
        let reaper = stream.spawn_orphan_reaper(cancel.clone(), Duration::from_millis(50));
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(!cancel.is_cancelled());
        reaper.abort();
    }

    /// Attach-before-detach: during a tab handoff both windows are attached at
    /// once, so there is no instant with zero observers and nothing to catch up
    /// on.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_handoff_with_overlapping_readers_never_goes_idle() {
        let stream = TurnStream::new("s", "turn-1");
        let cancel = CancellationToken::new();
        let reaper = stream.spawn_orphan_reaper(cancel.clone(), Duration::from_millis(80));

        let old_window = stream.attach(0);
        let new_window = stream.attach(0); // attaches BEFORE the old one leaves
        drop(old_window);
        assert_eq!(stream.observer_count(), 1);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            !cancel.is_cancelled(),
            "an overlapping handoff must never be seen as abandonment"
        );
        drop(new_window);
        reaper.abort();
    }

    /// A reader that falls off the live ring is repaired from the replay
    /// buffer, in order and with no hole — the buffer, not the ring, is the
    /// record.
    #[tokio::test]
    async fn a_reader_that_falls_off_the_ring_is_repaired_from_the_buffer() {
        let stream = TurnStream::new("s", "turn-1");
        let mut reader = stream.attach(0);
        for i in 0..(LIVE_RING_CAPACITY + 32) {
            stream.publish(&text_frame(&format!("t{i}")));
        }
        stream.close();

        let mut seqs = Vec::new();
        loop {
            match reader.recv().await {
                ReaderEvent::Frame(f, _) => seqs.push(f.seq),
                ReaderEvent::Gap => panic!("the replay buffer was big enough; no gap expected"),
                ReaderEvent::Closed => break,
            }
        }
        let expected: Vec<u64> = (0..(LIVE_RING_CAPACITY as u64 + 33)).collect();
        assert_eq!(seqs, expected, "no frame may be skipped on a ring overrun");
    }

    #[test]
    fn a_bad_orphan_timeout_env_falls_back_rather_than_reaping_instantly() {
        // Not a `set_var` test (process-global, races the suite): the parse is
        // exercised directly through the same predicate the reader uses.
        let parse = |raw: &str| {
            raw.trim()
                .parse::<u64>()
                .ok()
                .filter(|ms| *ms > 0)
                .map(Duration::from_millis)
                .unwrap_or(DEFAULT_ORPHAN_TIMEOUT)
        };
        assert_eq!(parse("0"), DEFAULT_ORPHAN_TIMEOUT);
        assert_eq!(parse("banana"), DEFAULT_ORPHAN_TIMEOUT);
        assert_eq!(parse("250"), Duration::from_millis(250));
        assert_eq!(orphan_timeout(), DEFAULT_ORPHAN_TIMEOUT);
    }
}
