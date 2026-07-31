//! BR-71 §8.5 / decision 9: `biorouter sessions watch` and `biorouter sessions
//! send`.
//!
//! `watch` streams a session's live events from the observer route added in
//! Task 7 — the same frames the desktop renders, in a terminal. `send` injects
//! a turn into a session and (by default) watches it to completion, which is
//! `workspace_send_prompt mode:"turn" wait:"final_message"` without an agent in
//! the loop.
//!
//! Both talk to a running `biorouterd` over a raw TCP socket rather than an
//! HTTP client crate, matching `commands/apps.rs`'s `daemon_ok` — the CLI
//! deliberately carries no HTTP dependency.

use anyhow::{anyhow, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::apps::{configured_port, daemon_ok, DAEMON_HOST};

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

/// Stream `request` from the daemon, printing rendered frames until the stream
/// ends, a terminal frame arrives (when `stop_on_terminal`), or the process is
/// interrupted.
async fn stream_frames(request: String, stop_on_terminal: bool) -> Result<()> {
    let port = configured_port();
    if !daemon_ok(DAEMON_HOST, port).await {
        return Err(anyhow!(
            "no Biorouter daemon is listening on {DAEMON_HOST}:{port}. \
             Start one: BIOROUTER_SERVER__SECRET_KEY=<key> biorouterd agent"
        ));
    }
    let mut stream = tokio::net::TcpStream::connect(format!("{DAEMON_HOST}:{port}")).await?;
    stream.write_all(request.as_bytes()).await?;

    let mut raw = Vec::new();
    let mut buffer = String::new();
    let mut headers_done = false;
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
            let status = head.lines().next().unwrap_or_default();
            if !status.contains(" 200") {
                return Err(anyhow!(
                    "daemon refused the request: {status}\n\
                     (401 usually means BIOROUTER_SERVER__SECRET_KEY does not match the daemon's)"
                ));
            }
            headers_done = true;
            buffer.clear();
            let mut frames = Vec::new();
            feed(&mut buffer, rest, &mut frames);
            if print_frames(&frames, stop_on_terminal) {
                return Ok(());
            }
            continue;
        }
        let mut frames = Vec::new();
        feed(
            &mut buffer,
            &String::from_utf8_lossy(&chunk[..read]),
            &mut frames,
        );
        if print_frames(&frames, stop_on_terminal) {
            return Ok(());
        }
    }
    Ok(())
}

/// Returns true when a terminal frame was seen and the caller should stop.
fn print_frames(frames: &[serde_json::Value], stop_on_terminal: bool) -> bool {
    let mut done = false;
    for frame in frames {
        if let Some(line) = render_frame(frame) {
            println!("{line}");
        }
        let kind = frame.get("type").and_then(serde_json::Value::as_str);
        if stop_on_terminal && matches!(kind, Some("Finish") | Some("Error")) {
            done = true;
        }
    }
    done
}

/// The sessions holding a turn right now, read from the daemon (BR-71 Task 38b).
///
/// Returns `Err` — never an empty set — when no daemon answers, so the caller
/// can render `state unknown` instead of printing "done" over a run that is
/// still going. Deliberately a one-shot read rather than `stream_frames`: the
/// response is a single JSON object, not SSE.
pub async fn running_session_ids() -> Result<std::collections::HashSet<String>> {
    let secret = secret_key()?;
    let port = configured_port();
    if !daemon_ok(DAEMON_HOST, port).await {
        return Err(anyhow!(
            "no Biorouter daemon is listening on {DAEMON_HOST}:{port}, so turn \
             liveness is not knowable from here"
        ));
    }
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
    Ok(parse_running_ids(body))
}

/// Pull the id set out of a `/sessions/running` body. Tolerates HTTP/1.1
/// chunked framing by reading from the first `{` to the last `}` rather than
/// parsing the whole body — the same defensiveness `feed` applies to SSE.
pub(crate) fn parse_running_ids(body: &str) -> std::collections::HashSet<String> {
    let json = body
        .find('{')
        .zip(body.rfind('}'))
        .and_then(|(start, end)| body.get(start..=end))
        .and_then(|slice| serde_json::from_str::<serde_json::Value>(slice).ok());
    json.and_then(|value| {
        value
            .get("session_ids")
            .and_then(serde_json::Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| id.as_str().map(str::to_string))
                    .collect()
            })
    })
    .unwrap_or_default()
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
    )
    .await
}

/// `biorouter sessions send <id> <text>` — inject a turn and, unless
/// `--no-wait`, watch it to completion.
pub async fn handle_session_send(session_id: &str, text: &str, wait: bool) -> Result<()> {
    let secret = secret_key()?;
    // `id` and `metadata` are spelled out because `ChatRequest::user_message`
    // deserializes into a real `Message`, whose serde has no `#[serde(default)]`
    // on either — omitting them is a 422, not a defaulted message.
    let body = serde_json::json!({
        "session_id": session_id,
        "user_message": {
            "id": null,
            "role": "user",
            "created": chrono::Utc::now().timestamp(),
            "content": [{ "type": "text", "text": text }],
            "metadata": { "userVisible": true, "agentVisible": true }
        }
    })
    .to_string();
    // `/reply` streams the turn back, so a send that waits is one request.
    stream_frames(
        build_post_request("/reply", DAEMON_HOST, &secret, &body),
        wait,
    )
    .await
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
}
