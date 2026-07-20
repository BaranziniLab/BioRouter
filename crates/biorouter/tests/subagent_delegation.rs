//! SUB-NN gates: delegation to subagents, at the edges and under stress.
//!
//! Everything here drives the real agent loop through the real `subagent` tool;
//! only the provider is scripted (see `subagent_support`). The questions are the
//! ones a user asks of a delegating agent: did the child run, did its result come
//! back attached to *its own* call, does a failure surface honestly, and does
//! cancelling the parent take the children with it.

mod subagent_support;

use subagent_support::{drain, harness, structured_results, tool_responses, Call};

fn call(id: &str, c: Call) -> (String, Call) {
    (id.to_string(), c)
}

/// SUB: a single delegation runs the child and returns its summary, attached to
/// the call that asked for it.
#[tokio::test]
async fn a_single_subagent_runs_and_returns_its_own_summary() {
    let h = harness(vec![call("only", Call::sub("solo", "ok:solo"))]).await;
    let messages = drain(&h.agent, &h.session_id)
        .await
        .expect("turn completes");

    let responses = tool_responses(&messages);
    assert_eq!(responses.len(), 1, "one call, one response: {responses:?}");
    let (id, text, is_error) = &responses[0];
    assert_eq!(id, "only");
    assert!(!is_error, "a clean delegation is not an error");
    assert!(
        text.contains("child solo done"),
        "the child's own summary comes back: {text}"
    );

    assert_eq!(h.ledger.distinct_started(), vec!["ok:solo".to_string()]);

    let structured = structured_results(&messages);
    assert_eq!(structured["only"]["status"], "completed");
}

/// SUB: several DIFFERENT subagents in one batch. The headline requirement —
/// every result must stay welded to the call that asked for it.
#[tokio::test]
async fn parallel_subagents_keep_their_results_separate() {
    let h = harness(vec![
        call("alpha", Call::sub("alpha", "ok:alpha")),
        call("beta", Call::sub("beta", "ok:beta")),
        call("gamma", Call::sub("gamma", "ok:gamma")),
    ])
    .await;
    let messages = drain(&h.agent, &h.session_id)
        .await
        .expect("turn completes");

    let responses = tool_responses(&messages);
    assert_eq!(responses.len(), 3, "every call answered: {responses:?}");

    for (id, text, is_error) in &responses {
        assert!(!is_error, "{id} should not be an error: {text}");
        assert!(
            text.contains(&format!("child {id} done")),
            "response {id} carries {id}'s summary, not a sibling's: {text}"
        );
        // Cross-talk check: no other child's summary leaked into this response.
        for other in ["alpha", "beta", "gamma"] {
            if other != id {
                assert!(
                    !text.contains(&format!("child {other} done")),
                    "response {id} leaked {other}'s summary: {text}"
                );
            }
        }
    }

    let mut started = h.ledger.distinct_started();
    started.sort();
    assert_eq!(started, vec!["ok:alpha", "ok:beta", "ok:gamma"]);
}

/// SUB: a child cannot spawn a child. It is refused, the refusal reaches the
/// child as an ordinary tool error, and the child still returns a summary — the
/// recursion stops without taking the run down with it.
#[tokio::test]
async fn a_subagent_cannot_spawn_a_subagent_and_fails_cleanly() {
    let h = harness(vec![call("nester", Call::sub("nester", "nest:nester"))]).await;
    let messages = drain(&h.agent, &h.session_id)
        .await
        .expect("turn completes");

    let responses = tool_responses(&messages);
    assert_eq!(responses.len(), 1);
    let (_, text, is_error) = &responses[0];
    assert!(
        !is_error,
        "a refused nested spawn must not fail the parent's call: {text}"
    );
    assert!(
        text.contains("could not nest"),
        "the child reported the refusal in its summary: {text}"
    );

    // The grandchild never ran: only the child's own script was ever started.
    let started = h.ledger.distinct_started();
    assert_eq!(
        started,
        vec!["nest:nester".to_string()],
        "a grandchild must never start: {started:?}"
    );
}

/// SUB: three ways a child can go wrong, all in one batch — each surfaces
/// honestly and none of them swallows the others.
#[tokio::test]
async fn failing_silent_and_slow_children_all_surface() {
    let h = harness(vec![
        call("broken", Call::sub("broken", "fail:broken")),
        call("mute", Call::sub("mute", "silent:mute")),
        call("plodder", Call::sub("plodder", "slow:plodder:400")),
    ])
    .await;
    let messages = drain(&h.agent, &h.session_id)
        .await
        .expect("turn completes");

    let responses = tool_responses(&messages);
    assert_eq!(responses.len(), 3, "nothing vanished: {responses:?}");
    let structured = structured_results(&messages);

    let broken = responses.iter().find(|(id, ..)| id == "broken").unwrap();
    assert!(
        broken.2,
        "SUB-02: a child whose turn aborted must be flagged `is_error`, or the \
         parent's tool card renders a failed delegation green: {}",
        broken.1
    );
    assert!(
        broken.1.contains("Subagent failed"),
        "the failure says so in words: {}",
        broken.1
    );
    assert!(
        broken
            .1
            .contains("scripted provider failure for child broken"),
        "and names the underlying cause: {}",
        broken.1
    );
    assert_eq!(structured["broken"]["status"], "error");

    // A child that only ever calls tools and never writes a summary. It runs out
    // of turns, and the loop's own stop notice is what comes back.
    //
    // SUB-03 (documented, unfixed): that notice is an ordinary assistant text
    // message, so the envelope grades this run `completed`. The prose is honest
    // — it says outright that it stopped for the cap, "not because the task is
    // necessarily complete" — and this test pins that the parent receives it
    // verbatim, which is what lets the model react. The *status* is the part
    // that overstates, and correcting it needs a structural signal the agent
    // loop does not yet emit for a turn-limit stop. Pinned as-is so the day the
    // loop grows that signal, this assertion is the thing that fails and points
    // at the envelope.
    let mute = responses.iter().find(|(id, ..)| id == "mute").unwrap();
    assert!(
        mute.1.contains("action limit for this turn"),
        "the turn-cap stop notice must reach the parent verbatim: {}",
        mute.1
    );
    assert_eq!(
        structured["mute"]["status"], "completed",
        "SUB-03: a turn-capped child still grades `completed` — see the comment above"
    );
    assert!(
        !mute.1.contains("No text content in last message"),
        "never the old lossy placeholder: {}",
        mute.1
    );

    let plodder = responses.iter().find(|(id, ..)| id == "plodder").unwrap();
    assert!(!plodder.2);
    assert!(plodder.1.contains("child plodder done"));
}

/// SUB: many children at once. Nothing is lost, nothing crosses over, and the
/// concurrency cap is respected.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_wide_batch_of_subagents_loses_nothing() {
    const WIDTH: usize = 12;
    let batch: Vec<_> = (0..WIDTH)
        .map(|i| {
            let name = format!("w{i}");
            call(&name.clone(), Call::sub(&name, &format!("ok:{name}")))
        })
        .collect();

    let h = harness(batch).await;
    let messages = drain(&h.agent, &h.session_id)
        .await
        .expect("turn completes");

    let responses = tool_responses(&messages);
    assert_eq!(
        responses.len(),
        WIDTH,
        "every one of {WIDTH} subagents answered: {responses:?}"
    );
    for (id, text, is_error) in &responses {
        assert!(!is_error, "{id} failed: {text}");
        assert!(
            text.contains(&format!("child {id} done")),
            "{id} got somebody else's answer: {text}"
        );
    }

    assert_eq!(
        h.ledger.distinct_started().len(),
        WIDTH,
        "every child actually ran"
    );

    // The fork-bomb guard's default ceiling is 8 concurrent subagents.
    let peak = h.ledger.peak();
    assert!(
        peak <= 8,
        "concurrency cap breached: {peak} children ran at once"
    );
}

/// SUB: a batch that mixes delegation with ordinary parallel tool calls — the
/// two dispatch paths coexist and each result still lands on its own call.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn subagents_and_ordinary_tools_share_a_batch() {
    let h = harness(vec![
        call("sub_one", Call::sub("sub_one", "ok:sub_one")),
        call("shell_one", Call::Shell("echo shell-one-ran".into())),
        call("sub_two", Call::sub("sub_two", "ok:sub_two")),
        call("shell_two", Call::Shell("echo shell-two-ran".into())),
    ])
    .await;
    let messages = drain(&h.agent, &h.session_id)
        .await
        .expect("turn completes");

    let responses = tool_responses(&messages);
    assert_eq!(responses.len(), 4, "all four answered: {responses:?}");

    let by_id = |want: &str| {
        responses
            .iter()
            .find(|(id, ..)| id == want)
            .unwrap_or_else(|| panic!("no response for {want}"))
            .1
            .clone()
    };
    assert!(by_id("sub_one").contains("child sub_one done"));
    assert!(by_id("sub_two").contains("child sub_two done"));
    assert!(by_id("shell_one").contains("shell-one-ran"));
    assert!(by_id("shell_two").contains("shell-two-ran"));
}

/// SUB: the user steers while subagents are in flight. A soft interrupt must
/// not cost the delegated work — every child still reports, and the steer lands
/// in the same turn.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn steering_mid_delegation_costs_no_subagent_results() {
    let h = harness(vec![
        call("left", Call::sub("left", "slow:left:600")),
        call("right", Call::sub("right", "slow:right:600")),
    ])
    .await;

    let steerer = h.agent.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        steerer.queue_soft_interrupt("also summarise what each of them found".to_string());
    });

    let messages = drain(&h.agent, &h.session_id)
        .await
        .expect("turn completes");

    let responses = tool_responses(&messages);
    assert_eq!(
        responses.len(),
        2,
        "steering must not drop a delegation: {responses:?}"
    );
    assert!(responses
        .iter()
        .any(|(id, text, _)| id == "left" && text.contains("child left done")));
    assert!(responses
        .iter()
        .any(|(id, text, _)| id == "right" && text.contains("child right done")));

    let steer_landed = messages
        .iter()
        .filter(|m| m.role == rmcp::model::Role::User)
        .flat_map(|m| m.content.iter())
        .any(|c| match c {
            biorouter::conversation::message::MessageContent::Text(t) => {
                t.text.contains("also summarise what each of them found")
            }
            _ => false,
        });
    assert!(steer_landed, "the steer reached the conversation");
    assert!(
        !h.agent.has_soft_interrupts(),
        "the queued steer was consumed, not left pending"
    );
}
