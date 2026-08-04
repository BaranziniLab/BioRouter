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

/// Issue #56 DR-26 / Task 50 Step 2: **a subagent inherits its parent's
/// cross-affiliation grants and can never exceed them.**
///
/// The inheritance is `privacy::grant::is_granted`'s walk up
/// `sessions.parent_session_id`, and this is the end-to-end half of it: that
/// walk is only meaningful if a REAL delegation stamps that column, and only
/// safe if a spawn manufactures no authority of its own. Both are asserted here,
/// against a child this run actually created.
///
/// ⚠ **The mint side is deliberately NOT here, and could not be.** Writing a
/// grant requires naming `privacy::grant`'s `UserCrossAffiliationGrant`, and
/// Task 49's `the_proof_of_user_is_constructed_in_exactly_one_place` fails the
/// build for any file under `crates/` outside that module and the one HTTP
/// handler whose *code* mentions it — an integration binary cannot mint one,
/// which is the control working. (That audit skips comments, so this paragraph
/// may name the type; it did not until review found it scanning whole files, and
/// the contortion of writing a comment that avoids a word is exactly the tax a
/// comment-blind audit charges. `grant::record_for_test` is `#[cfg(test)]
/// pub(crate)`, so it is not reachable from an integration binary either —
/// making it reachable is the one repair that would actually cost something.)
///
/// So the granted-parent direction lives in
/// `privacy::grant::tests::a_subagent_inherits_its_parents_grants_and_the_parent_inherits_nothing`,
/// where `record_for_test` IS reachable, and it is the discriminating half: the
/// "nothing is granted" assertions below would pass equally against an
/// `is_granted` that returned `false` unconditionally. What that test cannot
/// see, and this one can, is whether production ever links the child to the
/// parent at all: with `parent_session_id` unstamped, the walk silently finds
/// nothing and every inheritance assertion elsewhere stays green while the
/// feature is dead.
///
/// ⚠ **Residual, on the gate's own wording.** "A child cannot hold a grant its
/// parent lacks" is not what `is_granted` enforces: its walk reads *upward*, so
/// a grant recorded directly on a child is honoured for that child and invisible
/// to the parent. Nothing in production mints one there — the single writer is
/// an HTTP handler behind `X-User-Action`, i.e. the user — so it is latent
/// rather than live, and narrowing the walk to "the ancestor chain must hold it"
/// would silently void an approval a user gave at the point of refusal inside a
/// subagent's turn. What IS enforced, and what this test asserts, is the half
/// DR-26 states: a spawn manufactures no authority of its own.
#[tokio::test]
async fn a_subagent_is_linked_to_its_parent_and_gains_no_grant_by_being_spawned() {
    let h = harness(vec![call("only", Call::sub("solo", "ok:solo"))]).await;
    drain(&h.agent, &h.session_id)
        .await
        .expect("turn completes");

    let sm = &h.agent.config.session_manager;
    // `list_sessions()` is the History projection and filters `sub_agent` rows
    // out; asking for the type explicitly is what makes an absent child
    // distinguishable from a hidden one.
    let children: Vec<_> = sm
        .list_sessions_by_types(&[biorouter::session::session_manager::SessionType::SubAgent])
        .await
        .expect("the session store is readable")
        .into_iter()
        .filter(|s| s.parent_session_id.as_deref() == Some(h.session_id.as_str()))
        .collect();
    assert_eq!(
        children.len(),
        1,
        "a delegation must stamp exactly one child with this parent's id — without \
         that column the grant walk has nothing to climb, and 'a subagent inherits \
         its parent's grants' is true of nothing"
    );
    let child = &children[0];
    assert_ne!(child.id, h.session_id);

    // Nobody granted anything, so nothing is granted — at the parent, at the
    // child, and for every shape of model affiliation the triple can take. A
    // spawn is not a way to acquire authority.
    let institution = biorouter::privacy::affiliation::ModelAffiliation::Institution(
        biorouter::privacy::affiliation::InstitutionId::new("ucsf"),
    );
    for model in [
        None,
        Some(biorouter::privacy::affiliation::ModelAffiliation::Local),
        Some(institution),
    ] {
        for session in [h.session_id.as_str(), child.id.as_str()] {
            assert!(
                !biorouter::privacy::grant::is_granted(sm, session, "ucsfomopagent", model).await,
                "session {session} holds a cross-affiliation grant for {model:?} that no \
                 user ever gave"
            );
        }
    }
}
