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
}

impl ActionRequiredManager {
    fn new() -> Self {
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            request_tx,
            request_rx: Mutex::new(request_rx),
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

    #[tokio::test]
    async fn delivering_to_an_unknown_id_errors() {
        let manager = ActionRequiredManager::new();
        assert!(manager
            .submit_cancellation("no-such-request".to_string())
            .await
            .is_err());
    }
}
