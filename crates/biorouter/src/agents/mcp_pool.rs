//! BR-54 Slice B — `SharedMcpPool`: share MCP server processes across agents and
//! sessions.
//!
//! Today every `Agent` builds its own `ExtensionManager` and every enabled server
//! is spawned into that agent's private map, so N live sessions each spawn M MCP
//! child processes (a `uvx` Python or Node server is 40–150 MB) with zero sharing
//! — the dominant RAM multiplier. This pool sits *behind* each `ExtensionManager`:
//! the manager still owns its own `extensions` map and tool prefixing, but a
//! poolable server's client becomes a **handle into a process-global pool** keyed
//! by spawn identity, so N managers share one process.
//!
//! Correctness invariants:
//! - **Isolation.** Two sessions may share ONE process only if their spawn
//!   identity ([`PoolKey`]: transport + working dir + env fingerprint) matches, so
//!   cwd/secret differences force separate processes. Notifications are isolated
//!   per dispatch by a progress-token router in `mcp_client` (shared clients are
//!   `routed_only`), so one session never sees another's progress/log stream.
//! - **Reference counting.** The pool holds only a `Weak<PooledEntry>`; each
//!   `ExtensionManager` holds a strong `Arc`. When the last manager releases it,
//!   the entry drops and its child process is reaped.
//! - **Single-flight.** Concurrent `get_or_spawn` for the same key share ONE
//!   spawn rather than racing two processes onto the same identity.
//! - **Recovery, not hang.** A dead shared client is detected via
//!   [`McpClientTrait::is_running`] and respawned for the next session instead of
//!   being handed out again; in-flight callers on a dead transport get a
//!   `TransportClosed` error (bounded by the extension timeout), never a hang.
//!
//! Gated by `BIOROUTER_SHARED_MCP_POOL` (default off). When off,
//! [`SharedMcpPool::is_enabled`] returns false and `add_extension` spawns a
//! private, unshared, broadcast-notification client exactly as before — the risky
//! path is opt-in.
//!
//! Known limitation (matches the design's phasing): a shared client keeps the
//! `SharedProvider` captured when it was first spawned, so server-initiated
//! *sampling* (`create_message`) on a shared client uses the spawner's provider.
//! Servers that never sample (the common case) are unaffected; per-call provider
//! isolation is a follow-up.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock, Weak};

use rmcp::model::ServerInfo;
use tempfile::TempDir;
use tokio::sync::Mutex;

use super::extension::ExtensionResult;
use super::mcp_client::McpClientBox;

/// The identity two sessions must share to safely reuse ONE MCP process.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct PoolKey {
    /// Canonical transport identity (e.g. `stdio:<cmd>\0<args>`, `http:<uri>`,
    /// `builtin:<name>`, `inline_python:<name>\0<code-hash>`).
    pub transport: String,
    /// Resolved working directory the process is spawned in (`None` => process cwd).
    /// Two sessions on different folders must not share a stdio process.
    pub working_dir: Option<PathBuf>,
    /// Stable hash of the merged env (direct envs + declared secret key names).
    pub env_fingerprint: u64,
}

/// One shared MCP client plus the metadata a manager needs to register it. Dropped
/// when the last holder releases it, which reaps the underlying process.
pub struct PooledEntry {
    client: McpClientBox,
    server_info: Option<ServerInfo>,
    /// Held for the lifetime of an InlinePython server (its temp script dir).
    _temp_dir: Option<TempDir>,
}

impl PooledEntry {
    pub fn new(
        client: McpClientBox,
        server_info: Option<ServerInfo>,
        temp_dir: Option<TempDir>,
    ) -> Self {
        Self {
            client,
            server_info,
            _temp_dir: temp_dir,
        }
    }

    /// A clone of the shared client handle (an `Arc`, so cheap).
    pub fn client(&self) -> McpClientBox {
        self.client.clone()
    }

    pub fn server_info(&self) -> Option<ServerInfo> {
        self.server_info.clone()
    }
}

/// A per-key single-flight guard around the pooled entry's `Weak`.
type Slot = Arc<Mutex<Weak<PooledEntry>>>;

/// Process-global pool of MCP clients keyed by spawn identity.
pub struct SharedMcpPool {
    slots: StdMutex<HashMap<PoolKey, Slot>>,
}

impl SharedMcpPool {
    fn new() -> Self {
        Self {
            slots: StdMutex::new(HashMap::new()),
        }
    }

    /// The process-global pool (mirrors `AgentManager`'s singleton).
    pub fn global() -> &'static Arc<SharedMcpPool> {
        static POOL: OnceLock<Arc<SharedMcpPool>> = OnceLock::new();
        POOL.get_or_init(|| Arc::new(SharedMcpPool::new()))
    }

    /// Whether MCP-process sharing is enabled. Off by default; opt in with
    /// `BIOROUTER_SHARED_MCP_POOL=1` (also accepts `true`/`on`/`yes`).
    pub fn is_enabled() -> bool {
        flag_is_truthy(std::env::var("BIOROUTER_SHARED_MCP_POOL").ok().as_deref())
    }

    /// Get the per-key single-flight slot, pruning slots whose entry has been
    /// reaped so the map cannot grow without bound across many distinct keys.
    fn slot_for(&self, key: &PoolKey) -> Slot {
        let mut slots = self.slots.lock().unwrap();
        slots.retain(|_, slot| {
            // Keep if another task is holding/awaiting this slot (a builder).
            if Arc::strong_count(slot) > 1 {
                return true;
            }
            match slot.try_lock() {
                Ok(weak) => weak.strong_count() > 0, // keep only if entry still alive
                Err(_) => true,                      // locked by a builder -> keep
            }
        });
        slots
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(Weak::new())))
            .clone()
    }

    /// Return a live shared client for `key`, else build one via `spawn` and
    /// insert it. Single-flight per key: two concurrent callers for the same key
    /// serialize on the slot lock, so the first spawns and the rest reuse it.
    pub async fn get_or_spawn<F, Fut>(
        &self,
        key: PoolKey,
        spawn: F,
    ) -> ExtensionResult<Arc<PooledEntry>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = ExtensionResult<PooledEntry>>,
    {
        let slot = self.slot_for(&key);
        let mut guard = slot.lock().await;
        if let Some(existing) = guard.upgrade() {
            if entry_is_alive(&existing).await {
                return Ok(existing);
            }
            // The shared process died: fall through and respawn a fresh one for
            // this and future sessions rather than hand out a dead client.
        }
        let entry = Arc::new(spawn().await?);
        *guard = Arc::downgrade(&entry);
        Ok(entry)
    }
}

/// Parse the `BIOROUTER_SHARED_MCP_POOL` flag value. Pure so it is testable
/// without mutating the process-global environment.
fn flag_is_truthy(value: Option<&str>) -> bool {
    match value {
        Some(v) => {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "on" || v == "yes"
        }
        None => false,
    }
}

/// Whether a pooled entry's client is still usable. Never blocks on an in-flight
/// call: if the client lock is held (a call is running) the transport is alive.
async fn entry_is_alive(entry: &PooledEntry) -> bool {
    match entry.client.try_lock() {
        Ok(client) => client.is_running(),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::mcp_client::{Error, McpClientTrait, McpMeta};
    use rmcp::model::{CallToolResult, InitializeResult, JsonObject, ListToolsResult};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio_util::sync::CancellationToken;

    /// Minimal client whose liveness is fixed at construction.
    struct MockPoolClient {
        running: bool,
    }

    #[async_trait::async_trait]
    impl McpClientTrait for MockPoolClient {
        async fn list_tools(
            &self,
            _cursor: Option<String>,
            _cancel: CancellationToken,
        ) -> Result<ListToolsResult, Error> {
            Ok(ListToolsResult {
                tools: vec![],
                next_cursor: None,
                meta: None,
            })
        }

        async fn call_tool(
            &self,
            _name: &str,
            _arguments: Option<JsonObject>,
            _meta: McpMeta,
            _cancel: CancellationToken,
        ) -> Result<CallToolResult, Error> {
            Err(Error::TransportClosed)
        }

        fn get_info(&self) -> Option<&InitializeResult> {
            None
        }

        fn is_running(&self) -> bool {
            self.running
        }
    }

    fn key(transport: &str) -> PoolKey {
        PoolKey {
            transport: transport.to_string(),
            working_dir: None,
            env_fingerprint: 0,
        }
    }

    fn mock_entry(running: bool) -> PooledEntry {
        PooledEntry::new(
            Arc::new(Mutex::new(
                Box::new(MockPoolClient { running }) as Box<dyn McpClientTrait>
            )),
            None,
            None,
        )
    }

    #[tokio::test]
    async fn test_shared_key_returns_same_arc() {
        let pool = SharedMcpPool::new();
        let count = Arc::new(AtomicUsize::new(0));

        let spawn = || {
            let count = count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(mock_entry(true))
            }
        };

        let a = pool.get_or_spawn(key("stdio:x"), spawn).await.unwrap();
        let b = pool.get_or_spawn(key("stdio:x"), spawn).await.unwrap();

        assert!(Arc::ptr_eq(&a, &b), "same key must share one entry");
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "only one spawn for one key"
        );
    }

    #[tokio::test]
    async fn test_distinct_keys_spawn_distinct_processes() {
        let pool = SharedMcpPool::new();
        let count = Arc::new(AtomicUsize::new(0));
        let spawn = || {
            let count = count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(mock_entry(true))
            }
        };

        let a = pool.get_or_spawn(key("stdio:x"), spawn).await.unwrap();
        let b = pool.get_or_spawn(key("stdio:y"), spawn).await.unwrap();

        assert!(!Arc::ptr_eq(&a, &b));
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_single_flight_concurrent_same_key() {
        let pool = Arc::new(SharedMcpPool::new());
        let count = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..8 {
            let pool = pool.clone();
            let count = count.clone();
            handles.push(tokio::spawn(async move {
                pool.get_or_spawn(key("stdio:z"), || {
                    let count = count.clone();
                    async move {
                        // Delay so racers pile up behind the slot lock.
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                        count.fetch_add(1, Ordering::SeqCst);
                        Ok(mock_entry(true))
                    }
                })
                .await
                .unwrap()
            }));
        }

        let entries: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "concurrent racers must share ONE spawn"
        );
        for e in &entries[1..] {
            assert!(Arc::ptr_eq(&entries[0], e));
        }
    }

    #[tokio::test]
    async fn test_last_release_reaps_and_respawns() {
        let pool = SharedMcpPool::new();
        let count = Arc::new(AtomicUsize::new(0));
        let spawn = || {
            let count = count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(mock_entry(true))
            }
        };

        let a = pool.get_or_spawn(key("stdio:reap"), spawn).await.unwrap();
        let b = pool.get_or_spawn(key("stdio:reap"), spawn).await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);

        // Release every strong handle -> the Weak in the pool goes dead.
        drop(a);
        drop(b);

        let c = pool.get_or_spawn(key("stdio:reap"), spawn).await.unwrap();
        assert_eq!(
            count.load(Ordering::SeqCst),
            2,
            "after last release the process is reaped and a fresh one is spawned"
        );
        drop(c);
    }

    #[tokio::test]
    async fn test_dead_client_is_respawned() {
        let pool = SharedMcpPool::new();
        let count = Arc::new(AtomicUsize::new(0));

        // First spawn returns a client that reports itself dead.
        let dead = pool
            .get_or_spawn(key("stdio:dead"), || {
                let count = count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(mock_entry(false))
                }
            })
            .await
            .unwrap();

        // A new session asking for the same key must NOT get the dead client back;
        // it must respawn (recover, not hang) even though `dead` is still held.
        let fresh = pool
            .get_or_spawn(key("stdio:dead"), || {
                let count = count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(mock_entry(true))
                }
            })
            .await
            .unwrap();

        assert_eq!(count.load(Ordering::SeqCst), 2, "dead client is respawned");
        assert!(!Arc::ptr_eq(&dead, &fresh));
    }

    #[test]
    fn test_flag_parsing() {
        // Pure parser -> no global-env mutation, no cross-test races.
        assert!(!flag_is_truthy(None), "absent => off (default)");
        for on in ["1", "true", "TRUE", "on", "Yes", " 1 "] {
            assert!(flag_is_truthy(Some(on)), "{on:?} => on");
        }
        for off in ["0", "false", "off", "no", "", "banana"] {
            assert!(!flag_is_truthy(Some(off)), "{off:?} => off");
        }
    }
}
