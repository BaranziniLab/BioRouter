//! BR-71 §8.5 / decision 9: `biorouter sessions watch`, `send`, `attach` and
//! `cancel`.
//!
//! `watch` streams a session's live events from the observer route added in
//! Task 7 — the same frames the desktop renders, in a terminal. `send` injects
//! a turn into a session and (by default) watches it to completion, which is
//! `workspace_send_prompt mode:"turn" wait:"final_message"` without an agent in
//! the loop. `attach` (Task 38c) joins a session that is running *right now*:
//! it renders where the conversation has got to, follows it live, and delivers
//! what you type into the running turn. `cancel` stops that turn.
//!
//! **`attach` is not `--resume`, and cannot be.** `biorouter session --resume`
//! builds a second `Agent` in the CLI process over the shared sessions store,
//! while the daemon's single-turn lock (`AppState::active_turns`) is an
//! in-process map another process shares nothing with — so resuming a session
//! the daemon is running gives two uncoordinated writers to one conversation.
//! Attach opens no `Agent` at all: every mutation it causes is a request the
//! daemon adjudicates, and the turn task inside the daemon stays the only
//! writer.
//!
//! All of them talk to a running `biorouterd` over a raw TCP socket rather than
//! an HTTP client crate, matching `commands/apps.rs`'s `daemon_ok` — the CLI
//! deliberately carries no HTTP dependency.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use biorouter::session::session_manager::SessionType;
use biorouter::session::SessionManager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::apps::{configured_port, daemon_ok, DAEMON_HOST};
use super::session_grouping::{listed_session_types, SessionRow};

/// The daemon's secret, or an actionable error. `biorouterd` generates a random
/// key when this is unset (`commands/agent.rs:35`), in which case no client can
/// authenticate — say so instead of surfacing a bare 401.
fn secret_key() -> Result<String> {
    std::env::var("BIOROUTER_SERVER__SECRET_KEY").map_err(|_| {
        anyhow!(
            "BIOROUTER_SERVER__SECRET_KEY is not set, so this command cannot authenticate \
             with the daemon.\nStart the daemon with a known key and reuse it here:\n  \
             BIOROUTER_SERVER__SECRET_KEY=<key> biorouterd agent\n  \
             BIOROUTER_SERVER__SECRET_KEY=<key> biorouter sessions watch <id>"
        )
    })
}

pub(crate) fn build_get_request(path: &str, host: &str, secret: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nX-Secret-Key: {secret}\r\n\
         Accept: text/event-stream\r\nConnection: close\r\n\r\n"
    )
}

pub(crate) fn build_post_request(path: &str, host: &str, secret: &str, body: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nX-Secret-Key: {secret}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\
         Accept: text/event-stream\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Append `chunk` to `buffer` and drain every COMPLETE SSE frame into `out`.
/// A trailing partial frame stays in the buffer for the next read.
///
/// Only `data: `-prefixed lines are read, which is also what makes this
/// tolerate HTTP/1.1 **chunked** transfer encoding: hyper streams the SSE body
/// with no content-length, so the wire carries `<hex-size>\r\n` framing lines
/// between events. Those are not `data:` lines and are dropped here, and they
/// never fall inside an event because each event is written as one body frame.
pub(crate) fn feed(buffer: &mut String, chunk: &str, out: &mut Vec<serde_json::Value>) {
    buffer.push_str(chunk);
    while let Some(index) = buffer.find("\n\n") {
        let frame: String = buffer.drain(..index + 2).collect();
        for line in frame.lines() {
            if let Some(payload) = line.strip_prefix("data: ") {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) {
                    out.push(value);
                }
            }
        }
    }
}

/// One line of human output for a frame, or `None` for frames a human does not
/// need to see (heartbeats, token bookkeeping).
pub(crate) fn render_frame(frame: &serde_json::Value) -> Option<String> {
    match frame.get("type").and_then(serde_json::Value::as_str)? {
        "Ping" => None,
        "Message" => {
            let message = frame.get("message")?;
            let role = message
                .get("role")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            let text: String = message
                .get("content")?
                .as_array()?
                .iter()
                .filter_map(|c| c.get("text").and_then(serde_json::Value::as_str))
                .collect::<Vec<_>>()
                .join(" ");
            let tools: Vec<String> = message
                .get("content")?
                .as_array()?
                .iter()
                .filter(|c| {
                    c.get("type").and_then(serde_json::Value::as_str) == Some("toolRequest")
                })
                .map(|c| {
                    c.get("toolCall")
                        .and_then(|tc| tc.get("value"))
                        .and_then(|v| v.get("name"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("tool")
                        .to_string()
                })
                .collect();
            // BR-71 §5: an injected message is never rendered as if the local
            // user typed it.
            let provenance = message
                .get("metadata")
                .and_then(|m| m.get("provenance"))
                .map(|p| {
                    let kind = p
                        .get("kind")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("?");
                    match kind {
                        "agent_injection" => format!(
                            " [injected by {}]",
                            p.get("fromSessionName")
                                .or_else(|| p.get("fromSessionId"))
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("another agent")
                        ),
                        "user_direct" => " [direct user message]".to_string(),
                        "spawn_context" => " [spawn context]".to_string(),
                        other => format!(" [{other}]"),
                    }
                })
                .unwrap_or_default();
            if text.trim().is_empty() && tools.is_empty() {
                return None;
            }
            let mut line = format!("[{role}]{provenance} {text}");
            if !tools.is_empty() {
                line.push_str(&format!("  <tools: {}>", tools.join(", ")));
            }
            Some(line)
        }
        "ToolCallPending" => Some(format!(
            "[tool] {} …",
            frame
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?")
        )),
        "UpdateConversation" => Some("[snapshot] conversation resynced".to_string()),
        "ModelChange" => Some(format!(
            "[model] {}",
            frame
                .get("model")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?")
        )),
        "Error" => Some(format!(
            "[error:{}] {}",
            frame
                .get("code")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?"),
            frame
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
        )),
        "Finish" => Some(format!(
            "[finished] {}",
            frame
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("stop")
        )),
        _ => None,
    }
}

/// Render the observer stream's FIRST frame — the join snapshot — as a
/// transcript. `render_frame`'s one-liner is right for a mid-stream resync and
/// wrong for a join: it is the difference between "something changed" and "here
/// is where this conversation is."
pub(crate) fn render_join_snapshot(frame: &serde_json::Value) -> Vec<String> {
    // A bare array. `Conversation` is a newtype over `Arc<Vec<Message>>` whose
    // `Serialize` forwards to the inner `Vec` (`conversation/mod.rs`), so the
    // wire frame is `{"conversation":[…]}` — NOT `conversation.messages`.
    let Some(messages) = frame
        .get("conversation")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    messages
        .iter()
        // Reuse render_frame's per-message renderer so a transcript line and a
        // live line can never diverge in shape or in provenance labelling.
        .filter_map(|message| {
            render_frame(&serde_json::json!({ "type": "Message", "message": message }))
        })
        .collect()
}

/// How a streaming response is rendered.
///
/// A `bool` would not do: `attach` needs a third mode, and getting it by
/// duplicating the socket loop is exactly how two renderers drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Render {
    /// Every frame through `render_frame` — `watch` and `send`.
    Lines,
    /// Nothing at all. The `/reply` fallback in `attach`'s delivery ladder: the
    /// observer stream is already rendering that turn, so printing here would
    /// double every line — but the socket must still be READ to its terminal
    /// frame, because dropping it cancels the turn.
    Silent,
    /// The first `UpdateConversation` as a transcript, everything after it
    /// through `render_frame` — `attach`'s join.
    JoinThenLines,
}

/// The numeric status from an HTTP status line ("HTTP/1.1 202 Accepted" → 202).
fn status_code(status_line: &str) -> Option<u16> {
    status_line.split_whitespace().nth(1)?.parse().ok()
}

/// Stream `request` from the daemon and return the status it answered with.
///
/// On a 200 the body is consumed — rendering per `render` — until the stream
/// ends, a terminal frame arrives (when `stop_on_terminal`), or the future is
/// dropped. On any other status it returns at once, so a caller can branch on a
/// 409 without reading a body it does not want.
///
/// ⚠ Consuming a 200 `/reply` body to its terminal frame is not politeness.
/// `stream_event` trips the turn's `CancellationToken` the moment its
/// `tx.send` fails, so **dropping this future mid-turn cancels the turn it
/// started**. That is the whole reason `Render::Silent` exists rather than
/// simply not making the request's future.
///
/// `status`, when given, fires with the code as soon as the daemon's status line
/// is read — see the call site in `post_reply_quiet` for why the return value is
/// far too late.
async fn stream_request(
    request: String,
    stop_on_terminal: bool,
    render: Render,
    status: Option<tokio::sync::oneshot::Sender<u16>>,
) -> Result<u16> {
    let port = configured_port();
    if !daemon_ok(DAEMON_HOST, port).await {
        return Err(anyhow!(
            "no Biorouter daemon is listening on {DAEMON_HOST}:{port}. \
             Start one: BIOROUTER_SERVER__SECRET_KEY=<key> biorouterd agent"
        ));
    }
    let mut stream = tokio::net::TcpStream::connect(format!("{DAEMON_HOST}:{port}")).await?;
    stream.write_all(request.as_bytes()).await?;
    read_response(&mut stream, stop_on_terminal, render, status).await
}

/// Read one HTTP response off `stream` and return the status it carried.
///
/// Generic over the stream so the rules below — a non-200 answered from the
/// status line alone, a 200 consumed to its terminal frame, and a response that
/// never completes reported as an ERROR — can be pinned over an in-memory pipe
/// rather than only against a live daemon.
async fn read_response<S: tokio::io::AsyncRead + Unpin>(
    stream: &mut S,
    stop_on_terminal: bool,
    render: Render,
    status: Option<tokio::sync::oneshot::Sender<u16>>,
) -> Result<u16> {
    let mut status = status;
    let mut raw = Vec::new();
    let mut buffer = String::new();
    let mut headers_done = false;
    let mut joined = false;
    let mut chunk = [0u8; 8192];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        if !headers_done {
            raw.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&raw).to_string();
            // `split_once` rather than byte-indexing: `clippy::string_slice` is
            // warn-level workspace-wide and clippy runs with `-D warnings`.
            let Some((head, rest)) = text.split_once("\r\n\r\n") else {
                continue;
            };
            let status_line = head.lines().next().unwrap_or_default();
            let code = status_code(status_line).ok_or_else(|| {
                anyhow!("the daemon answered with a response carrying no status code: {status_line}")
            })?;
            // Reported for EVERY code, before the branch: the ladder's middle
            // rung needs the 409 as promptly as it needs the 200, and a caller
            // waiting on this channel must never be left waiting on a refusal.
            if let Some(tx) = status.take() {
                let _ = tx.send(code);
            }
            if code != 200 {
                return Ok(code);
            }
            headers_done = true;
            buffer.clear();
            let mut frames = Vec::new();
            feed(&mut buffer, rest, &mut frames);
            if print_frames(&frames, stop_on_terminal, render, &mut joined) {
                return Ok(code);
            }
            continue;
        }
        let mut frames = Vec::new();
        feed(
            &mut buffer,
            &String::from_utf8_lossy(&chunk[..read]),
            &mut frames,
        );
        if print_frames(&frames, stop_on_terminal, render, &mut joined) {
            return Ok(200);
        }
    }
    // ⚠ Reaching here without headers means the socket closed part-way through
    // the response — NOT that the daemon said 200. Falling through to `Ok(200)`
    // made `post_reply_quiet` answer `Started`, so attach printed "the turn you
    // started has ended" for a message the daemon never received: the one place
    // a non-delivery read as a delivery. `watch`/`send` inherited it as a silent
    // clean exit over a session that was still going.
    if !headers_done {
        return Err(anyhow!(
            "the daemon closed the connection before sending a complete response, \
             so it is not known whether the request was accepted"
        ));
    }
    Ok(200)
}

/// `stream_request` for the callers that treat any non-200 as fatal.
async fn stream_frames(request: String, stop_on_terminal: bool, render: Render) -> Result<()> {
    match stream_request(request, stop_on_terminal, render, None).await? {
        200 => Ok(()),
        code => Err(anyhow!(
            "daemon refused the request: HTTP {code}\n\
             (401 usually means BIOROUTER_SERVER__SECRET_KEY does not match the daemon's)"
        )),
    }
}

/// The lines one frame contributes to the terminal — and nothing else, no I/O,
/// so the join latch below can be pinned by a test.
///
/// `joined` latches the first `UpdateConversation` under `Render::JoinThenLines`
/// so only the *join* is expanded into a transcript. A later one is a mid-stream
/// resync (`bus_lag_resync_frame`), and re-printing the whole conversation for it
/// would bury the live output it exists to correct.
fn stream_frame_lines(frame: &serde_json::Value, render: Render, joined: &mut bool) -> Vec<String> {
    let kind = frame.get("type").and_then(serde_json::Value::as_str);
    match render {
        Render::Silent => Vec::new(),
        Render::JoinThenLines if !*joined && kind == Some("UpdateConversation") => {
            *joined = true;
            let mut lines = render_join_snapshot(frame);
            if lines.is_empty() {
                lines.push("(this session has no messages yet)".to_string());
            }
            lines.push("── live from here ──".to_string());
            lines
        }
        Render::Lines | Render::JoinThenLines => render_frame(frame).into_iter().collect(),
    }
}

/// Returns true when a terminal frame was seen and the caller should stop.
fn print_frames(
    frames: &[serde_json::Value],
    stop_on_terminal: bool,
    render: Render,
    joined: &mut bool,
) -> bool {
    let mut done = false;
    for frame in frames {
        for line in stream_frame_lines(frame, render, joined) {
            println!("{line}");
        }
        if stop_on_terminal
            && matches!(
                frame.get("type").and_then(serde_json::Value::as_str),
                Some("Finish") | Some("Error")
            )
        {
            done = true;
        }
    }
    done
}

/// The sessions holding a turn right now, read from the daemon (BR-71 Task 38b).
///
/// Returns `Err` — never an empty set — whenever the answer is not actually
/// known: no daemon listening, no usable secret, a non-200, a stalled read, or
/// a body this client cannot parse. The caller renders those as `state unknown`
/// instead of printing "done" over a run that is still going. `Ok(empty set)`
/// therefore means one specific thing: the daemon answered and nothing is
/// running.
///
/// Deliberately a one-shot read rather than `stream_frames`: the response is a
/// single JSON object, not SSE.
pub async fn running_session_ids() -> Result<std::collections::HashSet<String>> {
    let secret = secret_key()?;
    let port = configured_port();
    if !daemon_ok(DAEMON_HOST, port).await {
        return Err(anyhow!(
            "no Biorouter daemon is listening on {DAEMON_HOST}:{port}, so turn \
             liveness is not knowable from here"
        ));
    }
    // ⚠ A DEADLINE, unlike `handle_session_watch`'s deliberately unbounded SSE
    // read. This is one small request inside a listing: a daemon that answers
    // `/status` but stalls here (or trickles bytes) must not hang
    // `biorouter session list` forever. Timing out yields `Err`, which the
    // caller renders as `state unknown` — the honest answer.
    let raw = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let mut stream = tokio::net::TcpStream::connect(format!("{DAEMON_HOST}:{port}")).await?;
        stream
            .write_all(build_get_request("/sessions/running", DAEMON_HOST, &secret).as_bytes())
            .await?;

        let mut raw = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let read = stream.read(&mut chunk).await?;
            if read == 0 {
                break;
            }
            raw.extend_from_slice(&chunk[..read]);
        }
        Ok::<Vec<u8>, std::io::Error>(raw)
    })
    .await
    .map_err(|_| {
        anyhow!("the daemon did not answer GET /sessions/running within 5s, so turn liveness is not knowable from here")
    })??;

    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| anyhow!("daemon sent a malformed response"))?;
    let status = head.lines().next().unwrap_or_default();
    if !status.contains(" 200") {
        return Err(anyhow!(
            "daemon refused the request: {status}\n\
             (401 usually means BIOROUTER_SERVER__SECRET_KEY does not match the daemon's)"
        ));
    }
    parse_running_ids(body).ok_or_else(|| {
        anyhow!(
            "the daemon answered GET /sessions/running with a body this client could not \
             read, so turn liveness is not knowable from here"
        )
    })
}

/// Pull the id set out of a `/sessions/running` body. Tolerates HTTP/1.1
/// chunked framing by reading from the first `{` to the last `}` rather than
/// parsing the whole body — the same defensiveness `feed` applies to SSE.
///
/// ⚠ `None` means "this body did not answer the question", and the caller MUST
/// turn it into an error rather than an empty set. An empty set is a real
/// answer — "nothing is running" — so degrading into one would render every
/// child `○ done`, which is precisely the wrong answer the three-valued
/// `Liveness` exists to avoid. Only a well-formed, empty `session_ids` array
/// may produce `Some(empty)`.
pub(crate) fn parse_running_ids(body: &str) -> Option<std::collections::HashSet<String>> {
    let value = json_object(body)?;
    let ids = value.get("session_ids")?.as_array()?;
    Some(
        ids.iter()
            .filter_map(|id| id.as_str().map(str::to_string))
            .collect(),
    )
}

/// The JSON object in an HTTP response body, read from the first `{` to the
/// last `}` so HTTP/1.1 chunked framing lines around it are ignored — the same
/// defensiveness `feed` applies to SSE. `None` for anything that is not one
/// complete object, which every caller must treat as "this body did not answer
/// the question" rather than as a default.
fn json_object(body: &str) -> Option<serde_json::Value> {
    body.find('{')
        .zip(body.rfind('}'))
        .and_then(|(start, end)| body.get(start..=end))
        .and_then(|slice| serde_json::from_str(slice).ok())
}

/// One-shot POST to a JSON route: the status code and the response body.
///
/// Separate from `stream_request` because `/interrupt` and `/agent/cancel`
/// answer with a single JSON object rather than an SSE stream — and because a
/// request made from inside an interactive loop must not be able to hang it,
/// hence the deadline (as in `running_session_ids`).
async fn post_json(path: &str, body: &str, secret: &str) -> Result<(u16, String)> {
    let port = configured_port();
    if !daemon_ok(DAEMON_HOST, port).await {
        return Err(anyhow!(
            "no Biorouter daemon is listening on {DAEMON_HOST}:{port}. \
             Start one: BIOROUTER_SERVER__SECRET_KEY=<key> biorouterd agent"
        ));
    }
    let request = build_post_request(path, DAEMON_HOST, secret, body);
    let raw = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut stream = tokio::net::TcpStream::connect(format!("{DAEMON_HOST}:{port}")).await?;
        stream.write_all(request.as_bytes()).await?;
        let mut raw = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let read = stream.read(&mut chunk).await?;
            if read == 0 {
                break;
            }
            raw.extend_from_slice(&chunk[..read]);
        }
        Ok::<Vec<u8>, std::io::Error>(raw)
    })
    .await
    .map_err(|_| anyhow!("the daemon did not answer POST {path} within 10s"))??;

    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| anyhow!("daemon sent a malformed response to POST {path}"))?;
    let status = head.lines().next().unwrap_or_default();
    let code = status_code(status)
        .ok_or_else(|| anyhow!("daemon sent a response carrying no status code: {status}"))?;
    Ok((code, body.to_string()))
}

/// `biorouter sessions watch <id>` — read-only observation of a live session.
pub async fn handle_session_watch(session_id: &str, follow: bool) -> Result<()> {
    let secret = secret_key()?;
    eprintln!("watching session {session_id} (ctrl-c to stop)");
    stream_frames(
        build_get_request(
            &format!("/sessions/{session_id}/events"),
            DAEMON_HOST,
            &secret,
        ),
        !follow,
        Render::Lines,
    )
    .await
}

/// The `/reply` request body.
///
/// `id` and `metadata` are spelled out because `ChatRequest::user_message`
/// deserializes into a real `Message`, whose serde has no `#[serde(default)]`
/// on either — omitting them is a 422, not a defaulted message.
fn reply_body(session_id: &str, text: &str) -> String {
    serde_json::json!({
        "session_id": session_id,
        "user_message": {
            "id": null,
            "role": "user",
            "created": chrono::Utc::now().timestamp(),
            "content": [{ "type": "text", "text": text }],
            "metadata": { "userVisible": true, "agentVisible": true }
        }
    })
    .to_string()
}

/// `biorouter sessions send <id> <text>` — inject a turn and, unless
/// `--no-wait`, watch it to completion.
pub async fn handle_session_send(session_id: &str, text: &str, wait: bool) -> Result<()> {
    let secret = secret_key()?;
    // `/reply` streams the turn back, so a send that waits is one request.
    stream_frames(
        build_post_request(
            "/reply",
            DAEMON_HOST,
            &secret,
            &reply_body(session_id, text),
        ),
        wait,
        Render::Lines,
    )
    .await
}

// ──────────────────────────────────────────────────────────────────────────────
// Task 38c: attaching to a LIVE session.
//
// Acceptance is decided by the DAEMON, atomically, once per attempt — never by
// this client. Attach must not read liveness and then branch on it; that is the
// two-step #69 removed, and re-introducing it here would put the race back one
// process further out.
//
//   steer     POST /interrupt   Agent::try_queue_soft_interrupt   202 {turn_id} / 409
//   new turn  POST /reply       AppState::try_begin_turn_idempotent  200 (SSE) / 409
// ──────────────────────────────────────────────────────────────────────────────

/// Which route to try on attempt `attempts`, or `None` to stop.
///
/// Bounded at three deliberately: the daemon can legitimately flip between
/// "running" and "idle" twice while one line of typing is in flight, and a
/// fourth flip is a session under someone else's control, not a race worth
/// absorbing. Losing the message loudly beats delivering it twice, or into a
/// turn the user did not mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Delivery {
    /// `POST /interrupt` — steer the turn already running.
    Steer,
    /// `POST /reply` — start a turn.
    NewTurn,
}

pub(crate) fn next_attempt(attempts: u8) -> Option<Delivery> {
    match attempts {
        0 => Some(Delivery::Steer),
        1 => Some(Delivery::NewTurn),
        2 => Some(Delivery::Steer),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CtrlCAction {
    Detach,
    /// A `/reply` socket is open: exiting now cancels the turn (`stream_event`
    /// trips the turn's cancellation token when the client hangs up).
    WarnWouldCancel,
    ForceExit,
}

pub(crate) fn ctrl_c_action(reply_socket_open: bool, already_warned: bool) -> CtrlCAction {
    match (reply_socket_open, already_warned) {
        (false, _) => CtrlCAction::Detach,
        (true, false) => CtrlCAction::WarnWouldCancel,
        (true, true) => CtrlCAction::ForceExit,
    }
}

/// A ctrl-c source that holds ONE registration for the whole attach loop.
///
/// ⚠ Not `tokio::signal::ctrl_c()` called per `select!` iteration. That is an
/// `async fn`: it registers its listener when the future is first polled and
/// drops it when the future is dropped, so rebuilding it each time round leaves
/// a window — between one branch completing and the next poll — with no listener
/// registered at all. Tokio's driver still swaps the signal's `pending` flag and
/// broadcasts it ("ignore errors if there are no listeners"), and the receiver
/// created a moment later is `tx.subscribe()`, which starts from the *current*
/// version. That SIGINT is simply gone.
///
/// And it is gone *silently*, because the first registration replaced the
/// process's default SIGINT disposition permanently — tokio's own docs: "Even if
/// this `Signal` instance is dropped, subsequent `SIGINT` deliveries will end up
/// captured by Tokio, and the default platform behavior will NOT be reset." So
/// nothing terminates either: the user presses ctrl-c, attach does nothing, and
/// they cannot detach. The window is widest exactly when frames are arriving
/// fastest, which is when someone is most likely to want out.
///
/// Not unit-tested, deliberately: raising a real SIGINT is process-global and
/// would hit the whole test binary, and installing the handler at all would
/// change SIGINT behaviour for every other test in the process. The fix is
/// structural — the registration lives in this struct, which outlives the loop.
struct CtrlC {
    #[cfg(unix)]
    signal: tokio::signal::unix::Signal,
    #[cfg(windows)]
    signal: tokio::signal::windows::CtrlC,
}

impl CtrlC {
    fn listen() -> Result<Self> {
        #[cfg(unix)]
        let signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
        #[cfg(windows)]
        let signal = tokio::signal::windows::ctrl_c()?;
        Ok(Self { signal })
    }

    /// Cancel-safe (both platforms document `recv` as such): losing a `select!`
    /// race leaves the registration in place, which is the entire point.
    async fn recv(&mut self) {
        self.signal.recv().await;
    }
}

/// What `POST /interrupt` answered.
///
/// Two named outcomes rather than an `Option<String>`: which of them came back
/// is the entire subject of the ladder, and a bare `None` reads as "no turn id"
/// rather than "the daemon refused".
enum SteerOutcome {
    Queued { turn_id: String },
    Refused,
}

/// What `POST /reply` answered.
enum TurnOutcome {
    Started,
    Refused,
}

/// How many typed lines may be waiting for the delivery worker at once.
const SEND_QUEUE: usize = 16;

/// What actually happened to one line of typing, for printing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Delivered {
    Steered {
        turn_id: String,
    },
    NewTurn,
    /// Three refusals in a row: reported, never retried silently.
    Nothing,
}

/// Whether a `/reply` socket is open right now, and whether ctrl-c has already
/// warned about it. Shared between the attach loop and its delivery worker.
///
/// **Counted, not a flag.** A `/reply` socket outlives the `deliver` call that
/// opened it, so two can be streaming at once. A flag would let the first to
/// finish declare ctrl-c harmless while the second was still streaming, and
/// ctrl-c would then cancel that turn silently.
///
/// **One critical section, not two atomics.** Reading "open" and "warned"
/// separately and then latching the warning with no re-check let a delivery
/// finish in between, leaving the warning set on a window that had already shut.
/// The next delivery's FIRST ctrl-c then force-exited with no warning at all —
/// precisely what closing the window resets the warning to prevent. Deciding and
/// recording under one lock makes that interleaving unconstructible rather than
/// merely unlikely.
#[derive(Default)]
struct ReplyWindow {
    state: std::sync::Mutex<WindowState>,
}

#[derive(Default)]
struct WindowState {
    /// `/reply` sockets currently streaming.
    open: usize,
    /// A ctrl-c has already warned about the window that is open *now*.
    warned: bool,
}

impl ReplyWindow {
    /// Poisoning is recovered from rather than propagated: this lock guards two
    /// small fields and nothing panics while holding it, so a poisoned window is
    /// a bug elsewhere — and refusing to answer ctrl-c would be a worse response
    /// to it than answering from the state we have.
    fn lock(&self) -> std::sync::MutexGuard<'_, WindowState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn open(&self) {
        self.lock().open += 1;
    }

    fn close(&self) {
        let mut state = self.lock();
        state.open = state.open.saturating_sub(1);
        if state.open == 0 {
            // The warning belonged to a window that is now shut.
            state.warned = false;
        }
    }

    /// Decide what this ctrl-c does *and* record it, in one critical section.
    fn on_ctrl_c(&self) -> CtrlCAction {
        let mut state = self.lock();
        let action = ctrl_c_action(state.open > 0, state.warned);
        if action == CtrlCAction::WarnWouldCancel {
            state.warned = true;
        }
        action
    }
}

/// Queue `text` into the turn already running. 202 names the turn that took it,
/// so the caller can print something real; 409 means that turn has ended.
async fn post_interrupt(session_id: &str, text: &str, secret: &str) -> Result<SteerOutcome> {
    let body = serde_json::json!({ "session_id": session_id, "text": text }).to_string();
    match post_json("/interrupt", &body, secret).await? {
        (202, body) => Ok(SteerOutcome::Queued {
            turn_id: json_object(&body)
                .as_ref()
                .and_then(|v| v.get("turn_id"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?")
                .to_string(),
        }),
        (409, _) => Ok(SteerOutcome::Refused),
        (code, _) => Err(anyhow!(
            "the daemon refused the steer: HTTP {code}\n\
             (400 means the message was empty; 401 means BIOROUTER_SERVER__SECRET_KEY \
              does not match the daemon's)"
        )),
    }
}

/// Start a turn with `text`, and return as soon as the daemon has ruled on it.
///
/// ⚠ The socket is **held to the turn's terminal frame, on a detached task**,
/// and prints nothing. Three constraints meet here and they pull apart:
///
/// * Printing would double every line — the observer stream is already
///   rendering this turn.
/// * Abandoning the socket would CANCEL the turn: `/reply`'s `stream_event`
///   trips the turn's cancellation token the moment its `tx.send` fails. So the
///   body must be consumed and discarded, not dropped.
/// * But *waiting* for it here would block the delivery worker for the whole
///   turn, and a mid-turn correction the user typed would then sit in a local
///   queue and go out minutes later as a brand new turn — adjudicated by the
///   ladder against state long stale. Attach exists to steer running turns,
///   including the ones it started itself, so the wait has to go.
///
/// Hence: the acceptance comes from the **status line** over a channel, and the
/// holder task owns the socket (and the `ReplyWindow`, so ctrl-c still knows an
/// exit would cancel a turn) until the terminal frame.
async fn post_reply_quiet(
    session_id: &str,
    text: &str,
    secret: &str,
    window: Arc<ReplyWindow>,
) -> Result<TurnOutcome> {
    let request = build_post_request("/reply", DAEMON_HOST, secret, &reply_body(session_id, text));
    let (status_tx, status_rx) = tokio::sync::oneshot::channel();
    // Opened from before the request rather than from the 200: erring towards
    // warning about a turn that does not exist is harmless, erring the other way
    // silently kills one.
    window.open();
    let holder = tokio::spawn(async move {
        let outcome = stream_request(request, true, Render::Silent, Some(status_tx)).await;
        window.close();
        outcome
    });

    match status_rx.await {
        Ok(200) => {
            println!("[started a new turn]");
            // The end of the turn is announced by the holder, so that waiting
            // for it costs the user nothing.
            tokio::spawn(async move {
                match holder.await {
                    Ok(Ok(_)) => println!("[the turn you started has ended]"),
                    Ok(Err(err)) => println!(
                        "[the turn you started stopped streaming here: {err}]\n\
                         It may still be running — `biorouter session cancel <id>` stops it."
                    ),
                    // Only reachable if the runtime is shutting down, which is
                    // the process exiting anyway.
                    Err(_) => {}
                }
            });
            Ok(TurnOutcome::Started)
        }
        Ok(409) => Ok(TurnOutcome::Refused),
        Ok(code) => Err(anyhow!(
            "the daemon refused to start a turn: HTTP {code}\n\
             (401 usually means BIOROUTER_SERVER__SECRET_KEY does not match the daemon's)"
        )),
        // The sender was dropped without a status: no status line was ever
        // read, so the request failed outright. The holder carries the reason.
        Err(_) => match holder.await {
            Ok(Err(err)) => Err(err),
            Ok(Ok(code)) => Err(anyhow!(
                "the daemon answered POST /reply with HTTP {code} but never reported it"
            )),
            Err(join) => Err(anyhow!("the /reply request could not be run: {join}")),
        },
    }
}

/// The ladder itself, with the two routes supplied by the caller.
///
/// A `202` or a `200` ends it immediately, so no branch can send the text on two
/// routes; a route that errors ends it too, rather than climbing on and possibly
/// delivering after the user has been told it failed.
///
/// The routes are arguments purely so this can be driven over every refusal
/// order in a test. A test that walks `next_attempt` itself instead proves only
/// that the test's own loop breaks after one send.
async fn run_ladder<Steer, NewTurn, SteerFut, NewTurnFut>(
    mut steer: Steer,
    mut new_turn: NewTurn,
) -> Result<Delivered>
where
    Steer: FnMut() -> SteerFut,
    NewTurn: FnMut() -> NewTurnFut,
    SteerFut: std::future::Future<Output = Result<SteerOutcome>>,
    NewTurnFut: std::future::Future<Output = Result<TurnOutcome>>,
{
    let mut attempts = 0u8;
    while let Some(next) = next_attempt(attempts) {
        match next {
            Delivery::Steer => match steer().await? {
                SteerOutcome::Queued { turn_id } => return Ok(Delivered::Steered { turn_id }),
                SteerOutcome::Refused => attempts += 1,
            },
            Delivery::NewTurn => match new_turn().await? {
                TurnOutcome::Started => return Ok(Delivered::NewTurn),
                TurnOutcome::Refused => attempts += 1,
            },
        }
    }
    Ok(Delivered::Nothing)
}

/// Deliver `text` to `session_id`, letting the daemon decide which route is
/// legal at this instant. Returns what actually happened, for printing.
async fn deliver(
    session_id: &str,
    text: &str,
    secret: &str,
    window: &Arc<ReplyWindow>,
) -> Result<Delivered> {
    run_ladder(
        move || post_interrupt(session_id, text, secret),
        move || post_reply_quiet(session_id, text, secret, window.clone()),
    )
    .await
}

/// Read whole lines from stdin on a dedicated OS thread.
///
/// Not `tokio::io::stdin`: its read is not cancellation-safe inside a `select!`,
/// and a partial line lost to a cancelled read is a steer the user typed and
/// never sent.
fn spawn_stdin_reader(tx: tokio::sync::mpsc::Sender<String>) {
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.blocking_send(line).is_err() {
                break;
            }
        }
    });
}

/// The one running subagent of `parent`, or an error naming the candidates.
///
/// Never guesses. Two running children of one fan-out are two different runs,
/// and attaching to "the first" would silently steer the wrong one — the same
/// refusal `#45`'s "no write target" uses.
fn pick_running_child(
    rows: &[SessionRow],
    parent: &str,
    running: &std::collections::HashSet<String>,
) -> Result<String> {
    let children: Vec<&SessionRow> = rows
        .iter()
        .filter(|r| {
            r.session_type == SessionType::SubAgent
                && r.parent_session_id.as_deref() == Some(parent)
        })
        .collect();
    let live: Vec<&SessionRow> = children
        .iter()
        .copied()
        .filter(|r| running.contains(&r.id))
        .collect();
    match live.as_slice() {
        [only] => Ok(only.id.clone()),
        [] if children.is_empty() => Err(anyhow!(
            "session {parent} has not spawned any subagent runs."
        )),
        [] => Err(anyhow!(
            "no subagent of {parent} has a turn in flight. Its runs so far:\n{}",
            children
                .iter()
                .map(|r| format!("  {}  {}", r.id, r.name))
                .collect::<Vec<_>>()
                .join("\n")
        )),
        many => Err(anyhow!(
            "{} subagents of {parent} are running; name the one you mean:\n{}",
            many.len(),
            many.iter()
                .map(|r| format!("  biorouter session attach {}   # {}", r.id, r.name))
                .collect::<Vec<_>>()
                .join("\n")
        )),
    }
}

/// The one session called `name`, or an error naming the candidates.
///
/// Never guesses, for the same reason `pick_running_child` does not. Subagent
/// names are authored by the model, so one fan-out routinely leaves several
/// children sharing a name — and for attach `--name` is a WRITE target, not the
/// listing lookup `resolve_session_by_name` was written for. Taking its first
/// match here would steer the wrong run with no way for the user to tell.
fn pick_named(rows: &[SessionRow], name: &str) -> Result<String> {
    // An id is unique by construction, so matching one is not a guess. It is
    // also the escape hatch the ambiguity error below points at.
    if let Some(row) = rows.iter().find(|r| r.id == name) {
        return Ok(row.id.clone());
    }
    let matches: Vec<&SessionRow> = rows.iter().filter(|r| r.name == name).collect();
    match matches.as_slice() {
        [only] => Ok(only.id.clone()),
        [] => Err(anyhow!(
            "no session is named {name:?}. \
             `biorouter session list --subagents` lists them."
        )),
        many => Err(anyhow!(
            "{} sessions are named {name:?}; say which one by id:\n{}",
            many.len(),
            many.iter()
                .map(|r| format!(
                    "  biorouter session attach {}   # last active {}",
                    r.id,
                    r.updated_at.format("%Y-%m-%d %H:%M")
                ))
                .collect::<Vec<_>>()
                .join("\n")
        )),
    }
}

/// Which session `attach` is about: a positional id, a `--name`, or the running
/// child of `--of <parent>`. Exactly one of the three.
async fn resolve_attach_target(
    session_id: Option<String>,
    name: Option<String>,
    of: Option<String>,
) -> Result<String> {
    let given = [session_id.is_some(), name.is_some(), of.is_some()]
        .into_iter()
        .filter(|g| *g)
        .count();
    if given == 0 {
        return Err(anyhow!(
            "attach needs a target: a session id, --name <NAME>, or --of <PARENT_ID>.\n\
             `biorouter session list --subagents` shows which sessions are live."
        ));
    }
    if given > 1 {
        return Err(anyhow!(
            "attach takes exactly one target — a session id, --name or --of, not several."
        ));
    }
    if let Some(id) = session_id {
        return Ok(id);
    }
    // Both remaining paths resolve against the same listing.
    // `listed_session_types(true)` rather than an open-coded array: a subagent
    // row is filtered out in SQL, so the query itself has to widen.
    let session_manager = SessionManager::instance();
    let sessions = session_manager
        .list_sessions_by_types(listed_session_types(true))
        .await?;
    let rows: Vec<SessionRow> = sessions
        .iter()
        .map(|s| SessionRow {
            id: s.id.clone(),
            name: s.name.clone(),
            session_type: s.session_type,
            parent_session_id: s.parent_session_id.clone(),
            updated_at: s.updated_at,
            message_count: s.message_count,
        })
        .collect();
    if let Some(name) = name {
        return pick_named(&rows, &name);
    }
    let parent = of.expect("--of, by elimination");
    // The daemon owns liveness; an unreadable answer is an error, never an
    // empty set (see `running_session_ids`).
    let running = running_session_ids().await?;
    pick_running_child(&rows, &parent, &running)
}

/// `biorouter session attach <id>` — render where the session is, follow it
/// live, and steer it from stdin.
///
/// The stdin reader, the observer stream and ctrl-c are three sources in one
/// `select!` — the same shape `handle_agent_socket` uses for Agent Drafter's
/// `ui_ask`, and for the same reason: a blocking read on any one of them makes
/// the other two unresponsive.
pub async fn handle_session_attach(
    session_id: Option<String>,
    name: Option<String>,
    of: Option<String>,
    read_only: bool,
) -> Result<()> {
    // The secret first: `--of` needs the daemon to answer, and failing on a
    // missing key with the actionable message beats failing on a lookup.
    let secret = secret_key()?;
    let session_id = resolve_attach_target(session_id, name, of).await?;

    if read_only {
        eprintln!("attached to session {session_id} — observing only (ctrl-c to detach)");
    } else {
        eprintln!(
            "attached to session {session_id} — type a message and press enter to steer it \
             (ctrl-c to detach)"
        );
    }

    let window = Arc::new(ReplyWindow::default());

    let (line_tx, mut line_rx) = tokio::sync::mpsc::channel::<String>(16);
    if !read_only {
        spawn_stdin_reader(line_tx.clone());
    }
    // `line_tx` is deliberately held for the whole loop: with `--read-only` (and
    // after stdin reaches EOF) the receiver then parks forever instead of
    // spinning on a closed channel.
    let _line_tx = line_tx;

    // Deliveries run on their own task, one at a time, so that a slow round trip
    // to the daemon never freezes the live rendering — the one thing attach
    // exists to show.
    let (send_tx, mut send_rx) = tokio::sync::mpsc::channel::<String>(SEND_QUEUE);
    {
        let session_id = session_id.clone();
        let secret = secret.clone();
        let window = window.clone();
        tokio::spawn(async move {
            while let Some(text) = send_rx.recv().await {
                match deliver(&session_id, &text, &secret, &window).await {
                    Ok(Delivered::Steered { turn_id }) => println!("[steered turn {turn_id}]"),
                    // Both halves are announced from the socket itself — the
                    // start at the 200 in `post_reply_quiet`, the end when its
                    // holder reaches the terminal frame. `deliver` now returns
                    // at the 200, so anything said here would be a third copy,
                    // and at the wrong moment.
                    Ok(Delivered::NewTurn) => {}
                    Ok(Delivered::Nothing) => println!(
                        "[not sent] the session's turn state changed three times while this \
                         message was in flight, so it was not sent. Send it again:\n  {text}"
                    ),
                    Err(err) => println!("[not sent] {err}\n  {text}"),
                }
            }
        });
    }

    // The observer stream, exactly as `watch --follow`, except that its first
    // frame is rendered as a transcript. It is READ-ONLY: its task in the daemon
    // merely returns when the channel closes and cancels nothing, so detaching
    // can never stop the session.
    let observer = stream_request(
        build_get_request(
            &format!("/sessions/{session_id}/events"),
            DAEMON_HOST,
            &secret,
        ),
        false,
        Render::JoinThenLines,
        None,
    );
    tokio::pin!(observer);

    // Registered ONCE, before the loop: see `CtrlC`.
    let mut interrupts = CtrlC::listen()?;

    loop {
        tokio::select! {
            observed = &mut observer => {
                match observed? {
                    200 => {
                        eprintln!("the session's event stream ended");
                        return Ok(());
                    }
                    code => return Err(anyhow!(
                        "the daemon would not stream session {session_id}: HTTP {code}\n\
                         (404 means no such session; 401 means BIOROUTER_SERVER__SECRET_KEY \
                          does not match the daemon's)"
                    )),
                }
            }
            typed = line_rx.recv() => {
                // Unreachable while `_line_tx` lives; an error rather than a
                // `continue`, which would spin this loop at full speed.
                let line = typed.ok_or_else(|| anyhow!("the stdin channel closed"))?;
                if line.trim().is_empty() {
                    continue;
                }
                // `try_send`, never `send().await`: awaiting a full queue here
                // blocks the `select!`, and the live rendering stops with it.
                // Refusing one line loudly beats freezing the display.
                match send_tx.try_send(line) {
                    Ok(()) => {}
                    Err(tokio::sync::mpsc::error::TrySendError::Full(text)) => println!(
                        "[not sent] {SEND_QUEUE} messages are already waiting to go out; \
                         let them clear first, then send this again:\n  {text}"
                    ),
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                        return Err(anyhow!("the delivery worker stopped; nothing was sent"))
                    }
                }
            }
            _ = interrupts.recv() => {
                match window.on_ctrl_c() {
                    CtrlCAction::Detach => {
                        eprintln!(
                            "\ndetached. The session keeps running — \
                             `biorouter session cancel {session_id}` stops it."
                        );
                        return Ok(());
                    }
                    CtrlCAction::WarnWouldCancel => {
                        eprintln!(
                            "\n⚠ a turn you started from here is still streaming, and leaving \
                             now CANCELS it. Press ctrl-c again to leave anyway, or wait for it \
                             to finish."
                        );
                    }
                    CtrlCAction::ForceExit => {
                        eprintln!("\nleaving — the turn you started is being cancelled.");
                        return Ok(());
                    }
                }
            }
        }
    }
}

/// One line for `POST /agent/cancel`'s answer.
///
/// ⚠ `cancelled: false` is a **success**. `cancel_turn`'s own doc says so:
/// "cancelling a session with no turn in flight (a double-clicked Stop button,
/// a cancel that raced the turn's own completion) is a 200 with
/// `cancelled: false`, never an error." A CLI that exited non-zero on it would
/// re-introduce exactly the unreliability BR-62 removed. The only `Err` here is
/// a body that did not answer the question at all.
pub(crate) fn render_cancel(response: &serde_json::Value) -> Result<String> {
    let cancelled = response
        .get("cancelled")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| {
            anyhow!("the daemon's cancel response did not say whether it cancelled anything")
        })?;
    if !cancelled {
        return Ok("nothing to cancel: this session had no turn in flight".to_string());
    }
    Ok(format!(
        "cancelled turn {}",
        response
            .get("turn_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("(unnamed)")
    ))
}

/// `biorouter session cancel <id>` — stop the turn a session is running.
///
/// The third leg of "monitor and steer": watching a subagent go wrong and being
/// unable to stop it from the terminal is the gap this closes.
/// `workspace_close scope:"turn"` is the agent's version of the same act, and
/// `POST /agent/cancel` is the route the GUI's Stop button already uses.
pub async fn handle_session_cancel(session_id: &str) -> Result<()> {
    let secret = secret_key()?;
    let body = serde_json::json!({ "session_id": session_id }).to_string();
    let (code, body) = post_json("/agent/cancel", &body, &secret).await?;
    if code != 200 {
        return Err(anyhow!(
            "the daemon refused the cancel: HTTP {code}\n\
             (401 usually means BIOROUTER_SERVER__SECRET_KEY does not match the daemon's)"
        ));
    }
    let response = json_object(&body).ok_or_else(|| {
        anyhow!("the daemon answered POST /agent/cancel with a body this client could not read")
    })?;
    println!("{}", render_cancel(&response)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_frames_are_split_on_blank_lines_and_data_prefixed_lines_are_kept() {
        let mut buffer = String::new();
        let mut out = Vec::new();
        // Two complete frames plus a partial one — the partial must stay buffered.
        feed(
            &mut buffer,
            "data: {\"type\":\"Ping\"}\n\ndata: {\"type\":\"Finish\",\"reason\":\"stop\"}\n\ndata: {\"typ",
            &mut out,
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["type"], "Ping");
        assert_eq!(out[1]["reason"], "stop");
        assert_eq!(buffer, "data: {\"typ");

        feed(&mut buffer, "e\":\"Ping\"}\n\n", &mut out);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn render_frame_is_quiet_about_pings_and_loud_about_content() {
        assert_eq!(render_frame(&serde_json::json!({ "type": "Ping" })), None);
        let msg = render_frame(&serde_json::json!({
            "type": "Message",
            "message": {
                "role": "assistant",
                "content": [{ "type": "text", "text": "hello there" }],
                "metadata": { "userVisible": true, "agentVisible": true }
            }
        }))
        .unwrap();
        assert!(msg.contains("hello there"));
        assert!(msg.contains("assistant"));

        // Provenance is surfaced — the CLI is one of the places a human reads
        // an injected message (BR-71 §5).
        let injected = render_frame(&serde_json::json!({
            "type": "Message",
            "message": {
                "role": "user",
                "content": [{ "type": "text", "text": "steer left" }],
                "metadata": {
                    "userVisible": true,
                    "provenance": { "kind": "agent_injection", "fromSessionName": "Planner" }
                }
            }
        }))
        .unwrap();
        assert!(injected.contains("injected by Planner"));

        let err = render_frame(&serde_json::json!({
            "type": "Error", "error": "provider refused", "code": "provider_forbidden"
        }))
        .unwrap();
        assert!(err.contains("provider_forbidden"));
    }

    /// The three-valued liveness design exists so a listing never prints "done"
    /// over a run that is still going. A parser that answers `Some(empty set)`
    /// for a body it could not read defeats it from the inside: an empty set is
    /// indistinguishable from "nothing is running", so every child renders
    /// `○ done`. Only a genuinely empty `session_ids` array may say that.
    #[test]
    fn an_unreadable_running_body_is_unknown_not_an_empty_set() {
        // The real thing.
        let ids = parse_running_ids("{\"session_ids\":[\"a\",\"b\"]}").unwrap();
        assert_eq!(ids, ["a", "b"].map(str::to_string).into_iter().collect());

        // Genuinely nothing running — the ONE case that may be an empty set.
        assert!(parse_running_ids("{\"session_ids\":[]}")
            .expect("a well-formed empty list is knowledge, not ignorance")
            .is_empty());

        // Everything a caller must NOT read as "nothing is running".
        assert_eq!(parse_running_ids(""), None, "empty body");
        assert_eq!(parse_running_ids("not json at all"), None, "no object");
        assert_eq!(
            parse_running_ids("{\"session_ids\":[\"a\""),
            None,
            "truncated body — the shape a chunked or compressed read would leave"
        );
        assert_eq!(
            parse_running_ids("{\"error\":\"boom\"}"),
            None,
            "a 200 carrying some other object is not an answer to this question"
        );
        assert_eq!(
            parse_running_ids("{\"session_ids\":\"a\"}"),
            None,
            "session_ids present but not an array"
        );
    }

    #[test]
    fn requests_are_well_formed_http_with_the_secret_header() {
        let get = build_get_request("/sessions/abc/events", "127.0.0.1", "s3cret");
        assert!(get.starts_with("GET /sessions/abc/events HTTP/1.1\r\n"));
        assert!(get.contains("X-Secret-Key: s3cret\r\n"));
        assert!(get.contains("Accept: text/event-stream\r\n"));

        let post = build_post_request("/reply", "127.0.0.1", "s3cret", "{\"a\":1}");
        assert!(post.starts_with("POST /reply HTTP/1.1\r\n"));
        assert!(post.contains("Content-Length: 7\r\n"));
        assert!(post.ends_with("\r\n\r\n{\"a\":1}"));
    }

    #[test]
    fn the_join_snapshot_is_rendered_as_a_transcript_not_a_one_liner() {
        // ⚠ `conversation` is a BARE ARRAY on the wire, not `{ "messages": [...] }`.
        // `Conversation` is a newtype over `Arc<Vec<Message>>` whose `Serialize`
        // impl is `self.0.as_ref().serialize(serializer)` (`conversation/mod.rs`),
        // and its `ToSchema` declares an Array of `Message`. A fixture shaped
        // `{"messages": […]}` makes this test pass against an implementation that
        // is wrong on the real wire — which is the shape a reader assumes.
        let frame = serde_json::json!({
            "type": "UpdateConversation",
            "conversation": [
                { "role": "user", "content": [{ "type": "text", "text": "audit the migration" }],
                  "metadata": { "userVisible": true } },
                { "role": "assistant", "content": [{ "type": "text", "text": "found two gaps" }],
                  "metadata": { "userVisible": true } }
            ],
            "token_state": { "totalTokens": 12 }
        });
        let lines = render_join_snapshot(&frame);
        assert!(
            lines.len() >= 2,
            "the transcript, not a status line: {lines:?}"
        );
        assert!(lines.iter().any(|l| l.contains("audit the migration")));
        assert!(lines.iter().any(|l| l.contains("found two gaps")));
        // Ordering is the conversation's, oldest first — a reversed render puts
        // the user at the bottom of their own history.
        let user = lines
            .iter()
            .position(|l| l.contains("audit the migration"))
            .unwrap();
        let assistant = lines
            .iter()
            .position(|l| l.contains("found two gaps"))
            .unwrap();
        assert!(user < assistant);

        // Task 20's watch is unchanged: a MID-STREAM resync stays one line.
        assert_eq!(
            render_frame(&frame).as_deref(),
            Some("[snapshot] conversation resynced")
        );
    }

    /// The latch is the whole difference between "here is where this
    /// conversation is" and burying the live output under a full re-print.
    ///
    /// `the_join_snapshot_is_rendered_as_a_transcript_not_a_one_liner` does NOT
    /// cover this: it pins `render_join_snapshot` and `render_frame`, neither of
    /// which knows whether a join has already happened. An implementation that
    /// expanded *every* `UpdateConversation` — the first failure the gate names —
    /// passes every other test in this module.
    #[test]
    fn only_the_first_snapshot_is_expanded_the_rest_stay_one_liners() {
        let snapshot = serde_json::json!({
            "type": "UpdateConversation",
            "conversation": [
                { "role": "user", "content": [{ "type": "text", "text": "audit the migration" }],
                  "metadata": { "userVisible": true } }
            ],
            "token_state": { "totalTokens": 12 }
        });

        let mut joined = false;
        let first = stream_frame_lines(&snapshot, Render::JoinThenLines, &mut joined);
        assert!(joined, "the first snapshot latches the join");
        assert!(
            first.iter().any(|l| l.contains("audit the migration")),
            "the join is the transcript: {first:?}"
        );

        // A mid-stream resync (`bus_lag_resync_frame`) corrects the live output;
        // re-printing the whole conversation would bury it.
        let second = stream_frame_lines(&snapshot, Render::JoinThenLines, &mut joined);
        assert_eq!(
            second,
            vec!["[snapshot] conversation resynced".to_string()],
            "a later resync is one line, not a second transcript"
        );

        // `watch`/`send` never expand a snapshot at all, latch or no latch.
        let mut never_joins = false;
        assert_eq!(
            stream_frame_lines(&snapshot, Render::Lines, &mut never_joins),
            vec!["[snapshot] conversation resynced".to_string()]
        );
        assert!(!never_joins, "Render::Lines has no join to latch");

        // The `/reply` fallback prints nothing whatever it is handed — the
        // observer stream is already rendering that turn.
        let mut silent = false;
        assert!(stream_frame_lines(&snapshot, Render::Silent, &mut silent).is_empty());
        assert!(!silent);
    }

    /// Reading a response off an in-memory pipe, so the status/termination rules
    /// below are pinned without a daemon, a port or a race.
    async fn read_from(script: &'static [u8], stop_on_terminal: bool) -> Result<u16> {
        let (mut client, mut daemon) = tokio::io::duplex(4096);
        tokio::spawn(async move {
            let _ = daemon.write_all(script).await;
            // and the connection closes here.
        });
        read_response(&mut client, stop_on_terminal, Render::Silent, None).await
    }

    /// A response whose headers never arrive is NOT a 200.
    ///
    /// The read loop ended when the socket closed and `Ok(200)` fell out of the
    /// bottom — so `post_reply_quiet` answered `Started` and attach printed "the
    /// turn you started has ended" for a message the daemon never received. It
    /// is the one place a non-delivery could read as a delivery, and `watch`
    /// inherits the same lie as a silent clean exit.
    #[tokio::test]
    async fn a_response_that_dies_before_its_headers_is_an_error_not_a_200() {
        let err = read_from(b"HTTP/1.1 200 OK\r\nContent-Type: text/ev", true)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("closed the connection"),
            "the error must say the response never completed: {err}"
        );

        // Nothing at all on the wire is the same failure.
        let empty = read_from(b"", true).await.unwrap_err().to_string();
        assert!(empty.contains("closed the connection"), "{empty}");
    }

    /// The positive controls, so the check above cannot be satisfied by simply
    /// failing more often.
    #[tokio::test]
    async fn a_complete_response_still_reports_its_status() {
        // A turn that reaches its terminal frame.
        assert_eq!(
            read_from(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n\
                  data: {\"type\":\"Ping\"}\n\n\
                  data: {\"type\":\"Finish\",\"reason\":\"stop\"}\n\n",
                true,
            )
            .await
            .unwrap(),
            200
        );
        // A stream the daemon simply ends (`watch --follow`, session over).
        assert_eq!(
            read_from(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n\
                  data: {\"type\":\"Ping\"}\n\n",
                false,
            )
            .await
            .unwrap(),
            200
        );
    }

    /// A refusal is answered from the status line alone: the ladder must not
    /// wait on a body it is not going to read, still less on the socket closing.
    #[tokio::test]
    async fn a_refusal_is_returned_without_waiting_for_the_body() {
        let (mut client, mut daemon) = tokio::io::duplex(4096);
        daemon
            .write_all(
                b"HTTP/1.1 409 Conflict\r\nContent-Type: application/json\r\n\r\n\
                  {\"running_turn_id\":\"t-1\",\"duplicate\":false}",
            )
            .await
            .unwrap();
        let (status_tx, status_rx) = tokio::sync::oneshot::channel();
        let code = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            read_response(&mut client, true, Render::Silent, Some(status_tx)),
        )
        .await
        .expect("a 409 must not block on the socket closing")
        .unwrap();
        assert_eq!(code, 409);
        // The refusal reaches the status channel too. `post_reply_quiet` waits
        // on that channel, so a 409 that only came back as a return value would
        // strand the ladder on its middle rung.
        assert_eq!(status_rx.await.unwrap(), 409);
        // The daemon half is still open — deliberately, that is the point.
        drop(daemon);
    }

    /// Attach must be able to steer a turn it started itself.
    ///
    /// `deliver` used to return only when `/reply`'s socket reached the turn's
    /// TERMINAL frame, so the delivery worker was blocked for the whole turn: a
    /// mid-turn correction the user typed sat in a local queue and went out
    /// minutes later, adjudicated against state long stale, as a brand new turn.
    /// The fix rests on this — the acceptance is readable from the status line
    /// while the body is still streaming — and on the socket still being held
    /// afterwards, because dropping it cancels the turn.
    #[tokio::test]
    async fn acceptance_is_known_from_the_status_line_not_the_end_of_the_turn() {
        let (mut client, mut daemon) = tokio::io::duplex(4096);
        let (status_tx, status_rx) = tokio::sync::oneshot::channel();
        let holder = tokio::spawn(async move {
            read_response(&mut client, true, Render::Silent, Some(status_tx)).await
        });

        daemon
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n")
            .await
            .unwrap();
        // The turn is running: no terminal frame yet, socket still open.
        let code = tokio::time::timeout(std::time::Duration::from_secs(2), status_rx)
            .await
            .expect("the 200 must be reported while the turn is still streaming")
            .unwrap();
        assert_eq!(code, 200);
        assert!(
            !holder.is_finished(),
            "the socket must still be held — abandoning it cancels the turn"
        );

        daemon
            .write_all(b"data: {\"type\":\"Finish\",\"reason\":\"stop\"}\n\n")
            .await
            .unwrap();
        assert_eq!(holder.await.unwrap().unwrap(), 200);
    }

    #[test]
    fn the_delivery_ladder_is_bounded_and_alternates() {
        assert_eq!(next_attempt(0), Some(Delivery::Steer));
        assert_eq!(next_attempt(1), Some(Delivery::NewTurn));
        assert_eq!(next_attempt(2), Some(Delivery::Steer));
        assert_eq!(next_attempt(3), None);
        assert_eq!(next_attempt(9), None);
    }

    /// The property the ladder exists for: exactly ONE delivering request,
    /// whatever order the daemon refuses in.
    ///
    /// Driven through `run_ladder` — the control flow `deliver` really uses —
    /// with scripted route answers, so "at most once" is a claim about the
    /// production code. Re-implementing the ladder over `next_attempt` inside
    /// the test instead (`push` then `break`) makes the property true by
    /// construction of the test's own loop, and would not catch a ladder that
    /// kept going after a `202`.
    #[tokio::test]
    async fn a_message_is_delivered_at_most_once_however_the_daemon_refuses() {
        // `accept_on == 3` is "the daemon refused every attempt".
        for accept_on in 0usize..=3 {
            let attempted = std::cell::RefCell::new(Vec::<Delivery>::new());
            let delivered = std::cell::RefCell::new(Vec::<Delivery>::new());
            let (tried, sent) = (&attempted, &delivered);

            let outcome = run_ladder(
                move || async move {
                    let attempt = tried.borrow().len();
                    tried.borrow_mut().push(Delivery::Steer);
                    if attempt == accept_on {
                        sent.borrow_mut().push(Delivery::Steer);
                        Ok(SteerOutcome::Queued {
                            turn_id: "t-7".to_string(),
                        })
                    } else {
                        Ok(SteerOutcome::Refused)
                    }
                },
                move || async move {
                    let attempt = tried.borrow().len();
                    tried.borrow_mut().push(Delivery::NewTurn);
                    if attempt == accept_on {
                        sent.borrow_mut().push(Delivery::NewTurn);
                        Ok(TurnOutcome::Started)
                    } else {
                        Ok(TurnOutcome::Refused)
                    }
                },
            )
            .await
            .unwrap();

            assert!(
                delivered.borrow().len() <= 1,
                "accept_on={accept_on} delivered {:?}",
                delivered.borrow()
            );
            // The routes tried, in order — a ladder that retried the same route
            // twice, or ran on past an acceptance, shows up here.
            let expected_attempts = match accept_on {
                0 => vec![Delivery::Steer],
                1 => vec![Delivery::Steer, Delivery::NewTurn],
                _ => vec![Delivery::Steer, Delivery::NewTurn, Delivery::Steer],
            };
            assert_eq!(*attempted.borrow(), expected_attempts, "accept_on={accept_on}");
            match accept_on {
                0 | 2 => assert_eq!(
                    outcome,
                    Delivered::Steered {
                        turn_id: "t-7".to_string()
                    }
                ),
                1 => assert_eq!(outcome, Delivered::NewTurn),
                _ => assert_eq!(
                    outcome,
                    Delivered::Nothing,
                    "a fourth flip must report, not send"
                ),
            }
        }
    }

    /// A route that errors stops the ladder there. Climbing on would turn one
    /// unreachable daemon into three requests, and could deliver the text after
    /// the user has already been told something went wrong.
    #[tokio::test]
    async fn a_failing_route_stops_the_ladder_instead_of_climbing_it() {
        let attempted = std::cell::RefCell::new(0usize);
        let tried = &attempted;
        let err = run_ladder(
            move || async move {
                *tried.borrow_mut() += 1;
                Err(anyhow!("connection reset"))
            },
            move || async move {
                *tried.borrow_mut() += 1;
                Ok(TurnOutcome::Started)
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("connection reset"));
        assert_eq!(*attempted.borrow(), 1, "the ladder stopped at the failure");
    }

    #[test]
    fn ctrl_c_never_silently_cancels_a_turn_the_attach_started() {
        assert_eq!(ctrl_c_action(false, false), CtrlCAction::Detach);
        assert_eq!(ctrl_c_action(false, true), CtrlCAction::Detach);
        assert_eq!(ctrl_c_action(true, false), CtrlCAction::WarnWouldCancel);
        assert_eq!(ctrl_c_action(true, true), CtrlCAction::ForceExit);
    }

    fn row(id: &str, parent: Option<&str>, name: &str) -> SessionRow {
        SessionRow {
            id: id.to_string(),
            name: name.to_string(),
            session_type: if parent.is_some() {
                SessionType::SubAgent
            } else {
                SessionType::User
            },
            parent_session_id: parent.map(str::to_string),
            updated_at: chrono::Utc::now(),
            message_count: 3,
        }
    }

    fn running(ids: &[&str]) -> std::collections::HashSet<String> {
        ids.iter().map(|id| (*id).to_string()).collect()
    }

    /// `--of` is a WRITE target: whatever it picks is what the next line the
    /// user types gets steered into. A fan-out routinely leaves several children
    /// running at once, so "the first one" is not an answer — it is a silent
    /// steer into the wrong run. Each refusal must instead name the candidates,
    /// the same shape `#45`'s "no write target" failure uses.
    #[test]
    fn attaching_to_a_parents_child_never_guesses_which_child() {
        let rows = vec![
            row("p1", None, "Migration review"),
            row("c1", Some("p1"), "Subagent: audit schema"),
            row("c2", Some("p1"), "Subagent: audit data"),
            row("p2", None, "Other work"),
            row("c3", Some("p2"), "Subagent: elsewhere"),
        ];

        // Exactly one running child is the only case that may resolve.
        assert_eq!(
            pick_running_child(&rows, "p1", &running(&["c2"])).unwrap(),
            "c2"
        );
        // …and a running child of ANOTHER parent is not a candidate.
        let wrong_parent = pick_running_child(&rows, "p1", &running(&["c3"])).unwrap_err();
        assert!(
            wrong_parent.to_string().contains("no subagent of p1"),
            "{wrong_parent}"
        );

        // Two running: refuse, and name BOTH so the user can choose.
        let ambiguous = pick_running_child(&rows, "p1", &running(&["c1", "c2"])).unwrap_err();
        let ambiguous = ambiguous.to_string();
        assert!(ambiguous.contains("c1"), "{ambiguous}");
        assert!(ambiguous.contains("c2"), "{ambiguous}");
        assert!(
            ambiguous.contains("attach"),
            "the refusal must be runnable, not just a complaint: {ambiguous}"
        );

        // Children exist but none is live: say which ones ran, so the user can
        // tell "finished" from "never started".
        let none_live = pick_running_child(&rows, "p1", &running(&[])).unwrap_err();
        let none_live = none_live.to_string();
        assert!(none_live.contains("c1") && none_live.contains("c2"), "{none_live}");

        // Never spawned anything: a different error, because a different fix.
        let barren = pick_running_child(&rows, "p2-with-no-children", &running(&[]))
            .unwrap_err()
            .to_string();
        assert!(barren.contains("has not spawned any subagent"), "{barren}");
    }

    /// The ctrl-c decision and the warning it records are ONE critical section.
    ///
    /// Reading "is a socket open" and "have we warned" as two separate atomics
    /// and then latching the warning with no re-check let a delivery finish in
    /// between, leaving `warned` set on a window that had already shut. The next
    /// delivery's FIRST ctrl-c then read `(open, warned)` and force-exited —
    /// cancelling a turn with no warning at all, which is exactly what closing
    /// the window resets the warning to prevent.
    #[test]
    fn closing_the_window_re_arms_the_warning_for_the_next_turn() {
        let window = ReplyWindow::default();
        assert_eq!(window.on_ctrl_c(), CtrlCAction::Detach, "nothing to lose");

        window.open();
        assert_eq!(window.on_ctrl_c(), CtrlCAction::WarnWouldCancel);
        assert_eq!(window.on_ctrl_c(), CtrlCAction::ForceExit);
        window.close();

        // The turn ended; ctrl-c is free again.
        assert_eq!(window.on_ctrl_c(), CtrlCAction::Detach);

        // A LATER turn must be warned about from scratch — never force-exited on
        // the strength of a warning the user was given about a different turn.
        window.open();
        assert_eq!(
            window.on_ctrl_c(),
            CtrlCAction::WarnWouldCancel,
            "a warning spent on an earlier turn must not arm this one"
        );
    }

    /// A `/reply` socket outlives the `deliver` call that opened it, so two can
    /// be streaming at once. The window is COUNTED for that reason: a flag would
    /// let the first holder to finish declare ctrl-c harmless while the second
    /// was still streaming, and ctrl-c would then cancel that turn silently.
    #[test]
    fn the_reply_window_stays_open_until_the_last_socket_closes() {
        let window = ReplyWindow::default();
        window.open();
        window.open();
        window.close();
        assert_eq!(
            window.on_ctrl_c(),
            CtrlCAction::WarnWouldCancel,
            "one socket is still streaming"
        );
        window.close();
        assert_eq!(window.on_ctrl_c(), CtrlCAction::Detach);
    }

    /// `--name` is a WRITE target for attach, and subagent names are authored by
    /// the model, so one fan-out routinely leaves several children sharing a
    /// name. `resolve_session_by_name` answers with the FIRST match — right for
    /// the listing lookup it was written for, and for attach a silent steer into
    /// the wrong run. `--of` already refuses to guess; `--name` must too.
    #[test]
    fn attaching_by_name_refuses_when_the_name_is_not_unique() {
        let rows = vec![
            row("p1", None, "Migration review"),
            row("c1", Some("p1"), "Subagent: audit"),
            row("c2", Some("p1"), "Subagent: audit"),
            row("c3", Some("p1"), "Subagent: summarise"),
        ];

        // A name that identifies exactly one session still resolves, subagent or
        // not — widening the lookup (Task 38b) must not regress.
        assert_eq!(pick_named(&rows, "Subagent: summarise").unwrap(), "c3");
        assert_eq!(pick_named(&rows, "Migration review").unwrap(), "p1");
        // An id is unique by construction, so matching one is not a guess. It is
        // also the escape hatch the ambiguity error points at.
        assert_eq!(pick_named(&rows, "c2").unwrap(), "c2");

        let ambiguous = pick_named(&rows, "Subagent: audit").unwrap_err().to_string();
        assert!(
            ambiguous.contains("c1") && ambiguous.contains("c2"),
            "both candidates must be named: {ambiguous}"
        );
        assert!(
            !ambiguous.contains("c3"),
            "only the sessions that actually match: {ambiguous}"
        );

        let missing = pick_named(&rows, "nothing like this")
            .unwrap_err()
            .to_string();
        assert!(missing.contains("nothing like this"), "{missing}");
    }

    #[test]
    fn a_cancel_that_found_nothing_to_stop_is_reported_as_success() {
        let nothing = serde_json::json!({ "cancelled": false, "turn_id": null });
        let stopped = serde_json::json!({ "cancelled": true, "turn_id": "t-7" });
        assert!(render_cancel(&nothing).is_ok());
        assert!(render_cancel(&stopped).is_ok());
        assert_ne!(
            render_cancel(&nothing).unwrap(),
            render_cancel(&stopped).unwrap(),
            "the two outcomes are both successes and must still read differently"
        );
        assert!(render_cancel(&stopped).unwrap().contains("t-7"));
    }
}
