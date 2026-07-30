//! BR-71 §4.3: the daemon→GUI workspace command channel. One bridge per GUI
//! WINDOW (Agent Drafter's `UiBridge` is per app session — same anatomy, one
//! level up): generation-guarded attach/detach, a pending map for blocking
//! round trips, cancel_all on disconnect, and the window's last layout echo.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

#[derive(Clone)]
pub struct WorkspaceBridge {
    inner: Arc<BridgeInner>,
}

struct BridgeInner {
    tx: Mutex<Option<mpsc::UnboundedSender<Value>>>,
    /// Guards attach/detach: only the connection that owns the current
    /// generation can tear it down (control.rs `UiBridge::detach` rationale).
    generation: AtomicU64,
    pending: Mutex<HashMap<String, oneshot::Sender<Value>>>,
    last_echo: Mutex<Option<Value>>,
    last_attach: Mutex<Option<Instant>>,
    request_seq: AtomicU64,
}

/// Opaque proof of which connection generation a socket owns.
#[derive(Debug, Clone, Copy)]
pub struct ConnToken(u64);

impl Default for WorkspaceBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceBridge {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(BridgeInner {
                tx: Mutex::new(None),
                generation: AtomicU64::new(0),
                pending: Mutex::new(HashMap::new()),
                last_echo: Mutex::new(None),
                last_attach: Mutex::new(None),
                request_seq: AtomicU64::new(1),
            }),
        }
    }

    pub fn attach(&self) -> (mpsc::UnboundedReceiver<Value>, ConnToken) {
        let generation = self.inner.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let (tx, rx) = mpsc::unbounded_channel();
        *lock(&self.inner.tx) = Some(tx);
        *lock(&self.inner.last_attach) = Some(Instant::now());
        (rx, ConnToken(generation))
    }

    /// No-op unless `token` owns the current generation, so a slow old socket
    /// unwinding cannot sever its replacement.
    pub fn detach(&self, token: ConnToken) {
        if self.inner.generation.load(Ordering::Acquire) != token.0 {
            return;
        }
        *lock(&self.inner.tx) = None;
        self.cancel_all();
    }

    pub fn is_attached(&self) -> bool {
        lock(&self.inner.tx).is_some()
    }

    pub fn emit(&self, frame: Value) -> Result<(), String> {
        let guard = lock(&self.inner.tx);
        let tx = guard.as_ref().ok_or("no GUI window attached")?;
        tx.send(frame)
            .map_err(|_| "GUI window channel closed".to_string())
    }

    /// Emit with a minted `request_id` and park until the renderer's
    /// `workspace_result` resolves it (bounded).
    pub async fn emit_and_wait(
        &self,
        mut frame: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let request_id = format!(
            "wsreq-{}",
            self.inner.request_seq.fetch_add(1, Ordering::Relaxed)
        );
        frame["request_id"] = Value::String(request_id.clone());
        let (tx, rx) = oneshot::channel();
        lock(&self.inner.pending).insert(request_id.clone(), tx);
        if let Err(e) = self.emit(frame) {
            lock(&self.inner.pending).remove(&request_id);
            return Err(e);
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_)) => Err("GUI window disconnected before replying".into()),
            Err(_) => {
                lock(&self.inner.pending).remove(&request_id);
                Err("timed out waiting for the GUI".into())
            }
        }
    }

    pub fn resolve(&self, request_id: &str, value: Value) {
        if let Some(tx) = lock(&self.inner.pending).remove(request_id) {
            let _ = tx.send(value);
        }
    }

    pub fn cancel_all(&self) {
        for (_, tx) in lock(&self.inner.pending).drain() {
            drop(tx); // receivers observe Err and unpark
        }
    }

    pub fn store_echo(&self, echo: Value) {
        *lock(&self.inner.last_echo) = Some(echo);
    }

    pub fn last_echo(&self) -> Option<Value> {
        lock(&self.inner.last_echo).clone()
    }

    fn last_attach(&self) -> Option<Instant> {
        *lock(&self.inner.last_attach)
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Registry keyed by window_id; entries retained for the process's life,
/// mirroring `UI_BRIDGES` in `routes/apps.rs`.
static BRIDGES: LazyLock<Mutex<HashMap<String, WorkspaceBridge>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn bridge_for(window_id: &str) -> WorkspaceBridge {
    lock(&BRIDGES)
        .entry(window_id.to_string())
        // `or_default()` rather than `or_insert_with(WorkspaceBridge::new)`:
        // clippy's `unwrap_or_default` fires on the latter and CI runs
        // `clippy -D warnings`. `Default` delegates to `new`, so this is the
        // same constructor.
        .or_default()
        .clone()
}

pub fn any_attached() -> bool {
    lock(&BRIDGES).values().any(WorkspaceBridge::is_attached)
}

/// Multi-window aggregation (§4.3): commands target the focused window (per its
/// echo), else the most recently attached.
pub fn focused_or_recent() -> Option<WorkspaceBridge> {
    let map = lock(&BRIDGES);
    let attached: Vec<_> = map.values().filter(|b| b.is_attached()).cloned().collect();
    drop(map);
    pick_target(attached)
}

/// The routing RULE of `focused_or_recent`, separated from its registry walk so
/// it can be tested against a supplied candidate list.
///
/// This split exists for one reason and it is not tidiness: `BRIDGES` is a
/// process-wide static, so a test that asserts *which* bridge `focused_or_recent`
/// returns is not containment-safe against the other tests in this binary — and a
/// test that only asserts `.is_some()` does not test the rule at all. Keep the
/// two halves apart; `focused_or_recent` must stay a one-liner over this.
pub(crate) fn pick_target(attached: Vec<WorkspaceBridge>) -> Option<WorkspaceBridge> {
    attached
        .iter()
        .find(|b| {
            b.last_echo()
                .and_then(|e| e.get("focused_session").cloned())
                .is_some_and(|f| !f.is_null())
        })
        .cloned()
        .or_else(|| {
            attached
                .into_iter()
                .max_by_key(|b| b.last_attach().unwrap_or_else(Instant::now))
        })
}

/// All windows' last echoes, merged — what workspace_list renders as `gui`.
pub fn merged_layout() -> Option<serde_json::Value> {
    let echoes: Vec<Value> = lock(&BRIDGES)
        .values()
        .filter(|b| b.is_attached())
        .filter_map(|b| b.last_echo())
        .collect();
    if echoes.is_empty() {
        None
    } else {
        Some(Value::Array(echoes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stale_detach_cannot_tear_down_a_newer_connection() {
        let bridge = WorkspaceBridge::new();
        let (_rx1, token1) = bridge.attach();
        let (_rx2, _token2) = bridge.attach(); // reconnect claims a new generation
        bridge.detach(token1); // stale detach: must be a no-op
        assert!(bridge.is_attached());
    }

    #[test]
    fn emit_delivers_to_the_current_connection_and_fails_detached() {
        let bridge = WorkspaceBridge::new();
        assert!(bridge.emit(json!({"cmd": "open_tab"})).is_err());

        let (mut rx, token) = bridge.attach();
        bridge
            .emit(json!({"cmd": "open_tab", "session_id": "s1"}))
            .unwrap();
        let frame = rx.try_recv().unwrap();
        assert_eq!(frame["cmd"], "open_tab");

        bridge.detach(token);
        assert!(bridge.emit(json!({"cmd": "notify"})).is_err());
    }

    #[tokio::test]
    async fn round_trip_resolves_and_detach_cancels_parked_requests() {
        let bridge = WorkspaceBridge::new();
        let (mut rx, token) = bridge.attach();

        let waiter = {
            let bridge = bridge.clone();
            tokio::spawn(async move {
                bridge
                    .emit_and_wait(
                        json!({"cmd": "open_tab", "session_id": "s1"}),
                        std::time::Duration::from_secs(5),
                    )
                    .await
            })
        };
        // The socket loop would read the frame, act, and reply by request_id.
        let frame = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if let Ok(f) = rx.try_recv() {
                    break f;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let request_id = frame["request_id"].as_str().unwrap().to_string();
        bridge.resolve(&request_id, json!({"ok": true}));
        assert_eq!(waiter.await.unwrap().unwrap()["ok"], true);

        // A parked request must not hang forever on disconnect.
        let waiter2 = {
            let bridge = bridge.clone();
            tokio::spawn(async move {
                bridge
                    .emit_and_wait(
                        json!({"cmd": "open_tab"}),
                        std::time::Duration::from_secs(5),
                    )
                    .await
            })
        };
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        bridge.detach(token); // cancel_all unparks
        assert!(waiter2.await.unwrap().is_err());
    }

    #[test]
    fn registry_tracks_focus_and_merges_layouts() {
        // BRIDGES is a process-wide static shared with every other test in this
        // binary (now and later). Use unique window ids and CONTAINMENT
        // assertions — never exact global counts — so parallel `cargo test`
        // and future bridge-attaching tests cannot flake this one.
        // (uuid is not a biorouter-server dep — a nanos timestamp is unique
        // enough for two ids minted in one test.)
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let win_a = format!("win-a-{nonce}");
        let win_b = format!("win-b-{nonce}");
        let a = bridge_for(&win_a);
        let b = bridge_for(&win_b);
        let (_ra, ta) = a.attach();
        let (_rb, tb) = b.attach();
        a.store_echo(json!({"window_id": win_a, "focused_session": "s1", "layout": []}));
        b.store_echo(json!({"window_id": win_b, "focused_session": null, "layout": []}));
        assert!(any_attached());
        let merged = merged_layout().unwrap();
        let ids: Vec<String> = merged
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|e| {
                e.get("window_id")
                    .and_then(|w| w.as_str())
                    .map(String::from)
            })
            .collect();
        assert!(ids.contains(&win_a), "merged layout carries window {win_a}");
        assert!(ids.contains(&win_b), "merged layout carries window {win_b}");

        // A CLOSED window must drop out of the merged layout: `workspace_list`'s
        // `gui` block is a claim about what the user can see right now, and a
        // stale echo from a window that is gone is a lie the model will act on.
        // (`last_echo` is retained deliberately — the registry never forgets a
        // window — so only the `is_attached()` filter separates the two cases,
        // and nothing else in this suite exercises it.)
        b.detach(tb);
        let merged_after = merged_layout().unwrap();
        let ids_after: Vec<String> = merged_after
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|e| {
                e.get("window_id")
                    .and_then(|w| w.as_str())
                    .map(String::from)
            })
            .collect();
        assert!(ids_after.contains(&win_a), "the live window is still there");
        assert!(
            !ids_after.contains(&win_b),
            "a detached window's stale echo must not be reported as GUI state: {ids_after:?}"
        );

        // Clean up so this test leaves no attached bridges for others to see.
        a.detach(ta);
    }

    /// §4.3's routing rule, tested as a RULE: *commands target the focused
    /// window (per its echo), else the most recently attached.*
    ///
    /// ⚠ This replaces `assert!(focused_or_recent().is_some())`, which was true
    /// for **every** implementation that returned any attached bridge — including
    /// one that ignores `focused_session` entirely, which in a two-window session
    /// sends every `workspace_open` to the wrong window. Do not simplify it back:
    /// `is_some()` on a function whose whole job is *which one* asserts nothing
    /// about the choice.
    ///
    /// It runs against `pick_target`, a locally-supplied candidate list, rather
    /// than `focused_or_recent()`'s global `BRIDGES` walk, for the reason the
    /// test above documents: another test in this binary may hold an attached,
    /// focused bridge, and a global assertion about *which* bridge wins is not
    /// containment-safe. `focused_or_recent()` is `pick_target(attached())` and
    /// nothing else, so the rule is fully covered here.
    #[test]
    fn the_command_target_prefers_a_focused_window_over_a_more_recent_one() {
        let focused = WorkspaceBridge::new();
        let (_r1, _t1) = focused.attach();
        focused.store_echo(json!({"window_id": "w-focused", "focused_session": "s1"}));
        // `last_attach` is an Instant; sleep so the ordering is a fact, not a
        // tie broken by clock resolution.
        std::thread::sleep(Duration::from_millis(5));
        let recent = WorkspaceBridge::new();
        let (_r2, _t2) = recent.attach(); // attaches LATER than the focused one
        recent.store_echo(json!({"window_id": "w-recent", "focused_session": null}));

        // Rule 1 — focus beats recency. The candidate order is deliberately
        // "recent first": a `pick_target` that returns `candidates[0]` fails here.
        let picked = pick_target(vec![recent.clone(), focused.clone()]).expect("a target");
        assert_eq!(
            picked.last_echo().unwrap()["window_id"],
            "w-focused",
            "the window whose echo names a focused session wins"
        );

        // Rule 2 — with nobody focused, the most recently attached wins. SAME
        // candidate order, so a `pick_target` that returns `candidates.last()`
        // (which would have passed Rule 1) fails here.
        focused.store_echo(json!({"window_id": "w-focused", "focused_session": null}));
        let picked = pick_target(vec![recent.clone(), focused.clone()]).expect("a target");
        assert_eq!(
            picked.last_echo().unwrap()["window_id"],
            "w-recent",
            "with no focus anywhere, the newest connection is the target"
        );

        // Rule 3 — no candidates is None, not a panic: that is the headless
        // degradation path every workspace tool branches on.
        assert!(pick_target(vec![]).is_none());
    }
}
