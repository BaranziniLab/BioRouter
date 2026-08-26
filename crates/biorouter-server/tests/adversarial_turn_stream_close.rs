//! ADVERSARIAL probes on the turn stream's CLOSE/TERMINAL discipline.
//!
//! Diagnostic, not a fix. Each test asserts a property the module documents and
//! FAILS against the current code. Deliberately disjoint from the probes in
//! `routes/reply.rs`'s `adversarial_*` modules: those attack who may close a
//! log; these attack what "terminal" means once one is logged.

// Redirects this binary's Biorouter data/config/state dirs at a throwaway root
// before `main`, so nothing here can open the developer's real `sessions.db`.
// The lib's copy is `#[cfg(test)]`, so it is NOT compiled into this binary —
// every integration test file must declare its own. `tests/every_test_binary_
// is_sandboxed.rs` is the guard that says so when a new file forgets.
#[path = "../src/test_sandbox.rs"]
mod test_sandbox;

use std::sync::Arc;
use std::time::Duration;

use biorouter::conversation::message::{Message, TokenState};
use biorouter_server::routes::reply::{MessageEvent, TurnErrorScope};
use biorouter_server::turn_stream::{ReaderEvent, TurnStream};

fn msg(text: &str) -> MessageEvent {
    MessageEvent::Message {
        message: Message::assistant().with_id("m-1").with_text(text),
        token_state: TokenState::default(),
    }
}

fn finish() -> MessageEvent {
    MessageEvent::Finish {
        turn_id: None,
        reason: "stop".to_string(),
        token_state: TokenState::default(),
    }
}

async fn frame_types(stream: &Arc<TurnStream>) -> Vec<String> {
    let mut reader = stream.attach(0);
    let mut out = Vec::new();
    loop {
        match reader.recv().await {
            ReaderEvent::Frame(frame, _) => {
                let sse = frame.live_sse();
                let json: serde_json::Value =
                    serde_json::from_str(sse.strip_prefix("data: ").unwrap().trim_end()).unwrap();
                out.push(json["type"].as_str().unwrap().to_string());
            }
            ReaderEvent::Gap => {}
            ReaderEvent::Closed => return out,
        }
    }
}

/// DEFECT A — `publish` refuses frames once the stream is CLOSED, but not once a
/// TERMINAL has been logged. So a frame can land after the terminal, and
/// `TurnStream::terminal_frame` — which is documented as "the last retained
/// frame IS the terminal" and returns `replay.back()` — hands back that frame
/// instead.
///
/// Reachable ordering, no race needed: the runner panics, `reply.rs`'s
/// `supervise_turn` publishes its `internal_error` frame straight into the
/// stream, and the pump — still draining bus events queued before the panic —
/// publishes more `Message` frames before it exits.
///
/// User-visible: a client re-POSTing that turn's id takes the `terminal_only`
/// path, is sent one `Message` frame, and the response ends. It is never told
/// the turn is over, so the composer stays in "thinking" forever — the exact
/// hang the wire contract promises cannot happen.
#[tokio::test(flavor = "multi_thread")]
async fn a_frame_after_the_terminal_must_not_become_the_terminal_frame() {
    let stream = TurnStream::new("post-terminal", "turn-1");
    stream.publish(&msg("half an answer"));
    stream.publish(&MessageEvent::Error {
        turn_id: None,
        error: "The model turn ended unexpectedly. Please retry.".into(),
        code: "internal_error".into(),
        scope: TurnErrorScope::Internal,
        retryable: true,
        provider_kind: None,
    });
    stream.publish(&msg("a frame the pump still had queued"));
    stream.close(); // terminal_logged is set, so nothing is synthesized

    let last = stream
        .terminal_frame()
        .expect("a closed turn has a terminal");
    let sse = last.live_sse();
    let json: serde_json::Value =
        serde_json::from_str(sse.strip_prefix("data: ").unwrap().trim_end()).unwrap();
    assert!(
        matches!(json["type"].as_str(), Some("Finish") | Some("Error")),
        "terminal_frame() must return a terminal frame; it returned a {}",
        json["type"]
    );
}

/// DEFECT B — `TurnStream::close` was a check-then-act across a lock release: it
/// read `!closed && !terminal_logged` under the lock, DROPPED the lock, and only
/// then published the synthetic terminal and latched `closed`. Any `publish`
/// landing in that window was either duplicated by a second terminal or, if
/// close won outright, silently refused.
///
/// The barrier below runs the two in parallel from separate OS threads.
///
/// ⚠ THE ASSERTION HAS BEEN CHANGED, and the reason matters more than the
/// change. As first written this test demanded BOTH "never two terminals" AND
/// "never lose the runner's `Finish`" from a genuinely concurrent close and
/// publish — which is not a bug report, it is a contradiction: if `close` wins
/// the race then the turn's log is over at that instant, and refusing a later
/// `Finish` is the correct behaviour rather than a lost frame. It measured
/// 3000/3000 "lost" because the closer thread parks at the barrier first and so
/// always wins; any implementation would score the same.
///
/// What was actually broken, and is asserted here, is atomicity: exactly ONE
/// terminal, every time, whichever side wins. And the second half — a healthy
/// turn keeping its real `Finish` — is now a STRUCTURAL property rather than a
/// race to be won, so it is asserted where it lives: `close` is reachable only
/// through `TurnWriter::drop`, i.e. only from the same owner that does the
/// publishing, so the two are ordered by construction and cannot interleave.
/// `sole_ownership_means_a_healthy_turn_never_loses_its_finish` below pins that.
#[tokio::test(flavor = "multi_thread")]
async fn close_must_be_atomic_against_a_concurrent_terminal_publish() {
    use std::sync::Barrier;
    let mut wrong_terminal_count = 0usize;
    let mut unterminated = 0usize;
    for _ in 0..3000 {
        let stream = TurnStream::new("close-race", "turn-1");
        stream.publish(&msg("partial"));
        let barrier = Arc::new(Barrier::new(2));

        let closer = {
            let stream = Arc::clone(&stream);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                stream.close();
            })
        };
        barrier.wait();
        stream.publish(&finish()); // the pump's real terminal
        closer.join().unwrap();

        let types = frame_types(&stream).await;
        let terminals = types
            .iter()
            .filter(|t| *t == "Finish" || *t == "Error")
            .count();
        if terminals != 1 {
            wrong_terminal_count += 1;
        }
        // Whatever the outcome, the log ENDS with its terminal: nothing may
        // follow it, so a late attach can never read past the end of the turn.
        if types.last().map(String::as_str) != Some("Finish")
            && types.last().map(String::as_str) != Some("Error")
        {
            unterminated += 1;
        }
    }
    assert_eq!(
        (wrong_terminal_count, unterminated),
        (0, 0),
        "close() raced a concurrent terminal publish: {wrong_terminal_count}/3000 runs \
         logged a number of terminals other than one, and {unterminated}/3000 did not \
         end with their terminal"
    );
}

/// The other half of DEFECT B, as the design now states it: a healthy turn's own
/// `Finish` cannot be lost, because the thing that closes a log IS the thing
/// that writes it.
///
/// `close` has exactly one caller now — `TurnWriter::drop` — and the writer is
/// held by the turn's pump for the pump's whole life. So "publish the terminal"
/// and "close the log" are two steps of one owner, in that order, and no amount
/// of concurrency elsewhere can reorder them. That is what makes the race above
/// unreachable in production rather than merely unlikely.
#[tokio::test(flavor = "multi_thread")]
async fn sole_ownership_means_a_healthy_turn_never_loses_its_finish() {
    let mut lost_finish = 0usize;
    for _ in 0..3000 {
        let stream = TurnStream::new("sole-owner", "turn-1");
        let writer = stream.claim_writer().expect("a fresh log has no writer");
        // A second closer cannot exist: the log is already owned.
        assert!(stream.claim_writer().is_none());

        stream.publish(&msg("partial"));
        stream.publish(&finish()); // the pump's real terminal…
        drop(writer); // …and only then does the pump exit.

        let types = frame_types(&stream).await;
        if types.last().map(String::as_str) != Some("Finish") {
            lost_finish += 1;
        }
    }
    assert_eq!(
        lost_finish, 0,
        "{lost_finish}/3000 healthy turns ended in the synthesized \
         'stream ended without a result' instead of their own Finish"
    );
}

/// DEFECT C — `reply.rs`'s `supervise_turn` released a stuck pump with
///
/// ```ignore
/// if tokio::time::timeout(RUNNER_EXIT_DRAIN_GRACE, pump).await.is_err() { cancel.cancel(); }
/// ```
///
/// `is_err()` is true only for `Elapsed`. A pump that PANICKED completes the
/// timeout with `Ok(Err(JoinError))`, so the supervisor read it as a clean exit
/// — but a panicking pump never reached its `stream.close()`. Nothing else
/// closed a turn's log, so every attached response parked forever on a turn that
/// was already dead, and the orphan reaper could not help: it only cancels a
/// token whose only consumer is the pump that is gone.
///
/// ⚠ THE TEST HAS BEEN REWRITTEN, because as first written it asserted
/// `timeout(250ms, panicking_task).await.is_err()` — a property of TOKIO, not of
/// this codebase, and one that is false by definition: a panicked task always
/// completes as `Ok(Err(JoinError))`. No implementation could have passed it. It
/// demonstrated the supervisor's bad condition; it could not detect its repair.
///
/// What is asserted instead is the behaviour the defect describes, at the level
/// where it was fixed. The supervisor's condition now distinguishes all three
/// outcomes (`reply.rs`), and — the load-bearing half — a pump no longer has to
/// reach any close statement at all: it holds a [`TurnWriter`] whose `Drop` runs
/// during the panic's unwind, so the log ends with a terminal and every attached
/// reader is released. Both are checked here through the public surface.
#[tokio::test(flavor = "multi_thread")]
async fn a_panicked_pump_still_ends_the_turns_log() {
    let stream = TurnStream::new("panicking-pump", "turn-1");
    let writer = stream.claim_writer().expect("a fresh log has no writer");

    // A reader that attached while the turn was healthy, and is now waiting for
    // frames that will never come.
    let watcher = {
        let stream = Arc::clone(&stream);
        tokio::spawn(async move { frame_types(&stream).await })
    };

    let pump = tokio::spawn(async move {
        let _writer = writer; // the pump owns the log for its whole life
        stream_a_little(&_writer).await;
        panic!("pump exploded");
    });
    assert!(
        pump.await.is_err(),
        "the pump must genuinely have panicked, or this proves nothing"
    );

    assert!(
        stream.is_closed(),
        "a panicking pump left the turn's log open; every attached response \
         parks on it forever and the orphan reaper cannot help, because the only \
         consumer of its token is the pump that is gone"
    );
    let types = tokio::time::timeout(Duration::from_secs(5), watcher)
        .await
        .expect("the attached reader must be released, not left parked")
        .unwrap();
    assert_eq!(
        types.last().map(String::as_str),
        Some("Error"),
        "a log ended by an unwinding writer still ends with a terminal frame: {types:?}"
    );
}

/// One frame, so the panicking pump above is a turn that had started producing
/// rather than an empty one.
async fn stream_a_little(writer: &biorouter_server::turn_stream::TurnWriter) {
    writer.stream().publish(&msg("half an answer"));
    tokio::task::yield_now().await;
}
