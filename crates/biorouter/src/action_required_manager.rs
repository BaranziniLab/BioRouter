use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::timeout;
use tracing::warn;
use uuid::Uuid;

use crate::conversation::message::{Message, MessageContent};

struct PendingRequest {
    /// `Some(data)` = the user answered; `None` = the user (or an
    /// unattended run) cancelled the elicitation. Carried as an `Option` so
    /// a cancellation unparks the waiting tool call with a **model-visible**
    /// `ElicitationAction::Cancel` instead of leaving it to time out.
    response_tx: Option<tokio::sync::oneshot::Sender<Option<Value>>>,
}

pub struct ActionRequiredManager {
    pending: Arc<RwLock<HashMap<String, Arc<Mutex<PendingRequest>>>>>,
    request_tx: mpsc::UnboundedSender<Message>,
    pub request_rx: Mutex<mpsc::UnboundedReceiver<Message>>,
    /// Signalled every time a request message is queued on `request_tx`, so a
    /// consumer can *race* "an elicitation arrived" against other work without
    /// holding the `request_rx` lock across an await (#40). The tool call that
    /// raises an elicitation is itself parked inside its batch until the
    /// request is answered or cancelled — a consumer that only drains the
    /// queue after its tool stream yields would therefore never see the
    /// request before the elicitation timeout.
    request_notify: tokio::sync::Notify,
}

impl ActionRequiredManager {
    fn new() -> Self {
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            request_tx,
            request_rx: Mutex::new(request_rx),
            request_notify: tokio::sync::Notify::new(),
        }
    }

    pub fn global() -> &'static Self {
        static INSTANCE: once_cell::sync::Lazy<ActionRequiredManager> =
            once_cell::sync::Lazy::new(ActionRequiredManager::new);
        &INSTANCE
    }

    /// Park until the user answers (`Ok(Some(data))`), explicitly cancels
    /// (`Ok(None)`), the timeout elapses, or the channel dies.
    pub async fn request_and_wait(
        &self,
        message: String,
        schema: Value,
        timeout_duration: Duration,
    ) -> Result<Option<Value>> {
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let pending_request = PendingRequest {
            response_tx: Some(tx),
        };

        self.pending
            .write()
            .await
            .insert(id.clone(), Arc::new(Mutex::new(pending_request)));

        let action_required_message = Message::assistant().with_content(
            MessageContent::action_required_elicitation(id.clone(), message, schema),
        );

        if let Err(e) = self.request_tx.send(action_required_message) {
            warn!("Failed to send action required message: {}", e);
        } else {
            // Wake a consumer parked on [`Self::request_arrived`] — `notify_one`
            // stores a permit when nobody is waiting yet, so the wake-up cannot
            // be lost to a race with the consumer re-entering its select loop.
            self.request_notify.notify_one();
        }

        let result = match timeout(timeout_duration, rx).await {
            Ok(Ok(user_data)) => Ok(user_data),
            Ok(Err(_)) => {
                warn!("Response channel closed for request: {}", id);
                Err(anyhow::anyhow!("Response channel closed"))
            }
            Err(_) => {
                warn!("Timeout waiting for response: {}", id);
                Err(anyhow::anyhow!("Timeout waiting for user response"))
            }
        };

        self.pending.write().await.remove(&id);

        result
    }

    /// Resolves as soon as an elicitation request may be waiting on
    /// [`Self::request_rx`] (one wake per queued request; a wake whose queue
    /// was already drained by another consumer is a harmless no-op for the
    /// caller). This is the seam that lets an agent loop whose entire tool
    /// batch is *parked on the elicitation itself* surface the request — and
    /// lets a headless run cancel it — instead of waiting out the elicitation
    /// timeout (#40). Does not consume the queue: pair with a `try_recv`
    /// drain of `request_rx`.
    pub async fn request_arrived(&self) {
        self.request_notify.notified().await;
    }

    pub async fn submit_response(&self, request_id: String, user_data: Value) -> Result<()> {
        self.deliver(request_id, Some(user_data)).await
    }

    /// Cancel a pending elicitation: the parked [`Self::request_and_wait`]
    /// returns `Ok(None)`, which the MCP client maps to
    /// `ElicitationAction::Cancel` — a model-visible outcome, unlike letting
    /// the request sit until its timeout. Used by unattended runs
    /// (non-interactive / no TTY) that can never collect the input (#40).
    pub async fn submit_cancellation(&self, request_id: String) -> Result<()> {
        self.deliver(request_id, None).await
    }

    async fn deliver(&self, request_id: String, outcome: Option<Value>) -> Result<()> {
        let pending_arc = {
            let pending = self.pending.read().await;
            pending
                .get(&request_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Request not found: {}", request_id))?
        };

        let mut pending = pending_arc.lock().await;
        if let Some(tx) = pending.response_tx.take() {
            if tx.send(outcome).is_err() {
                warn!("Failed to send response through oneshot channel");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::ActionRequiredData;

    /// Read the id the manager minted for the pending elicitation off its
    /// request channel.
    async fn next_request_id(manager: &ActionRequiredManager) -> String {
        let message = manager
            .request_rx
            .lock()
            .await
            .recv()
            .await
            .expect("an elicitation request message");
        message
            .content
            .iter()
            .find_map(|content| match content {
                MessageContent::ActionRequired(action) => match &action.data {
                    ActionRequiredData::Elicitation { id, .. } => Some(id.clone()),
                    _ => None,
                },
                _ => None,
            })
            .expect("the request message carries the elicitation id")
    }

    #[tokio::test]
    async fn submit_response_unparks_with_the_data() {
        let manager = std::sync::Arc::new(ActionRequiredManager::new());
        let waiter = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager
                    .request_and_wait(
                        "Need input".to_string(),
                        serde_json::json!({}),
                        Duration::from_secs(5),
                    )
                    .await
            })
        };
        let id = next_request_id(&manager).await;
        manager
            .submit_response(id, serde_json::json!({"name": "ada"}))
            .await
            .unwrap();
        let outcome = waiter.await.unwrap().unwrap();
        assert_eq!(outcome, Some(serde_json::json!({"name": "ada"})));
    }

    /// #40: a cancellation unparks the waiter with `Ok(None)` — the outcome
    /// the MCP client maps to `ElicitationAction::Cancel`, so an unattended
    /// run resolves the tool call visibly instead of stalling to timeout.
    #[tokio::test]
    async fn submit_cancellation_unparks_with_none() {
        let manager = std::sync::Arc::new(ActionRequiredManager::new());
        let waiter = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager
                    .request_and_wait(
                        "Need input".to_string(),
                        serde_json::json!({}),
                        Duration::from_secs(5),
                    )
                    .await
            })
        };
        let id = next_request_id(&manager).await;
        manager.submit_cancellation(id).await.unwrap();
        let outcome = waiter.await.unwrap().unwrap();
        assert_eq!(outcome, None, "cancellation must be Ok(None), not an error");
    }

    /// #40: `request_arrived` must resolve — and the cancel must land — while
    /// the requesting tool call is still parked in `request_and_wait`, i.e.
    /// WITHOUT any other event feeding the consumer. The generous
    /// `request_and_wait` timeout stands in for the production 300 s one: if
    /// the wake-up depended on the waiter finishing first, the short
    /// `timeout()`s here would fire long before it, failing the test.
    #[tokio::test]
    async fn request_arrived_wakes_and_cancel_resolves_while_the_tool_is_parked() {
        let manager = std::sync::Arc::new(ActionRequiredManager::new());
        let waiter = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager
                    .request_and_wait(
                        "Need input".to_string(),
                        serde_json::json!({}),
                        Duration::from_secs(300),
                    )
                    .await
            })
        };
        // The notify must fire from the request itself — nobody has consumed
        // the queue and no other stream item will ever arrive.
        timeout(Duration::from_secs(2), manager.request_arrived())
            .await
            .expect("request_arrived must wake without another stream item");
        let id = timeout(Duration::from_secs(2), next_request_id(&manager))
            .await
            .expect("the queued request must be drainable after the wake");
        manager.submit_cancellation(id).await.unwrap();
        let outcome = timeout(Duration::from_secs(2), waiter)
            .await
            .expect("cancel must unpark the waiter promptly, not at its timeout")
            .unwrap()
            .unwrap();
        assert_eq!(outcome, None, "cancellation resolves as Ok(None)");
    }

    #[tokio::test]
    async fn delivering_to_an_unknown_id_errors() {
        let manager = ActionRequiredManager::new();
        assert!(manager
            .submit_cancellation("no-such-request".to_string())
            .await
            .is_err());
    }
}
