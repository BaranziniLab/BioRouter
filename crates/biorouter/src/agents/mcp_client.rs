use crate::action_required_manager::ActionRequiredManager;
use crate::agents::types::SharedProvider;
use crate::session_context::SESSION_ID_HEADER;
use rmcp::model::{
    Content, CreateElicitationRequestParams, CreateElicitationResult, ElicitationAction, ErrorCode,
    Extensions, JsonObject, Meta, NumberOrString, ProgressToken,
};
/// MCP client implementation for Biorouter
use rmcp::{
    model::{
        CallToolRequest, CallToolRequestParams, CallToolResult, CancelledNotification,
        CancelledNotificationMethod, CancelledNotificationParam, ClientCapabilities, ClientInfo,
        ClientRequest, CreateMessageRequestParams, CreateMessageResult, GetPromptRequest,
        GetPromptRequestParams, GetPromptResult, Implementation, InitializeResult,
        ListPromptsRequest, ListPromptsResult, ListResourcesRequest, ListResourcesResult,
        ListToolsRequest, ListToolsResult, LoggingMessageNotification,
        LoggingMessageNotificationMethod, PaginatedRequestParams, ProgressNotification,
        ProgressNotificationMethod, ProtocolVersion, ReadResourceRequest,
        ReadResourceRequestParams, ReadResourceResult, RequestId, Role, SamplingMessage,
        ServerNotification, ServerResult,
    },
    service::{
        ClientInitializeError, PeerRequestOptions, RequestContext, RequestHandle, RunningService,
        ServiceRole,
    },
    transport::IntoTransport,
    ClientHandler, ErrorData, Peer, RoleClient, ServiceError, ServiceExt,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::{sync::Arc, time::Duration};
use tokio::sync::{
    mpsc::{self, Sender},
    Mutex,
};
use tokio_util::sync::CancellationToken;

pub type BoxError = Box<dyn std::error::Error + Sync + Send>;

pub type Error = rmcp::ServiceError;

/// The shared handle type for an MCP client. A `SharedMcpPool` hands out clones
/// of one of these so N agents address one process (BR-54); the unpooled path
/// still owns a unique one per extension.
///
/// H6: this is deliberately NOT wrapped in a `Mutex`. `McpClientTrait::call_tool`
/// takes `&self` and every implementation is internally synchronized (the rmcp
/// transport guards its own framing; in-process servers guard their own state),
/// so an outer mutex bought nothing and cost everything: it was held across the
/// entire `call_tool` await, converting `max(tool durations)` into
/// `sum(tool durations)` for concurrent calls on one extension.
/// See `h6_parallel_same_extension` in `extension_manager.rs`.
pub type McpClientBox = Arc<dyn McpClientTrait>;

/// Per-dispatch notification routes, keyed by the MCP progress token assigned to
/// a single tool call. Shared between the [`McpClient`] and its [`BioRouterClient`]
/// handler so server progress notifications can be routed back to exactly the
/// session that made the call, instead of broadcast to every session sharing the
/// process (the isolation core of the SharedMcpPool).
type ProgressRoutes = Arc<Mutex<HashMap<String, Sender<ServerNotification>>>>;

#[derive(Clone, Debug)]
pub struct McpMeta {
    pub session_id: String,
    /// Per-dispatch MCP progress token. When set, it is written into the call's
    /// `_meta.progressToken` so the server echoes it on progress notifications,
    /// letting a pooled (shared) client route those notifications to exactly this
    /// dispatch's session (BR-54). `None` on the unpooled path (legacy broadcast).
    pub progress_token: Option<String>,
}

impl McpMeta {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            progress_token: None,
        }
    }

    /// Attach a progress token so this call's notifications can be routed back to
    /// this session on a shared client.
    pub fn with_progress_token(mut self, token: impl Into<String>) -> Self {
        self.progress_token = Some(token.into());
        self
    }

    fn inject_into_extensions(&self, extensions: Extensions) -> Extensions {
        let mut extensions = inject_session_id_into_extensions(extensions, &self.session_id);
        if let Some(token) = &self.progress_token {
            // Add the progressToken to the SAME `_meta` object the session id
            // rides in (rmcp serializes `extensions.get::<Meta>()` as params._meta),
            // so we never set the params.meta field and can't collide on the wire.
            let mut meta = extensions.get::<Meta>().cloned().unwrap_or_default();
            meta.set_progress_token(ProgressToken(NumberOrString::String(token.clone().into())));
            extensions.insert(meta);
        }
        extensions
    }
}

#[async_trait::async_trait]
pub trait McpClientTrait: Send + Sync {
    async fn list_tools(
        &self,
        next_cursor: Option<String>,
        cancel_token: CancellationToken,
    ) -> Result<ListToolsResult, Error>;

    async fn call_tool(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
        meta: McpMeta,
        cancel_token: CancellationToken,
    ) -> Result<CallToolResult, Error>;

    fn get_info(&self) -> Option<&InitializeResult>;

    async fn list_resources(
        &self,
        _next_cursor: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListResourcesResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn read_resource(
        &self,
        _uri: &str,
        _cancel_token: CancellationToken,
    ) -> Result<ReadResourceResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn list_prompts(
        &self,
        _next_cursor: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListPromptsResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn get_prompt(
        &self,
        _name: &str,
        _arguments: Value,
        _cancel_token: CancellationToken,
    ) -> Result<GetPromptResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
        mpsc::channel(1).1
    }

    /// Register a per-dispatch notification route and return the progress token to
    /// attach to the call plus the receiver that will get ONLY this dispatch's
    /// notifications. The default implementation returns no token and falls back
    /// to the broadcast [`subscribe`](Self::subscribe) — i.e. legacy behavior for
    /// clients that are not shared across sessions.
    async fn register_dispatch(&self) -> (Option<String>, mpsc::Receiver<ServerNotification>) {
        (None, self.subscribe().await)
    }

    /// Whether the client's transport is still believed to be alive. Used by the
    /// SharedMcpPool to evict and respawn a dead shared process instead of handing
    /// it to a new session. Default: assume alive.
    fn is_running(&self) -> bool {
        true
    }

    async fn get_moim(&self, _session_id: &str) -> Option<String> {
        None
    }
}

pub struct BioRouterClient {
    /// Legacy broadcast subscribers (the unpooled path). Empty on pooled clients.
    notification_handlers: Arc<Mutex<Vec<Sender<ServerNotification>>>>,
    /// Per-dispatch routes keyed by progress token (the pooled/isolated path).
    progress_routes: ProgressRoutes,
    /// When true (a pooled client shared across sessions), a notification that
    /// cannot be attributed to a registered progress token is DROPPED rather than
    /// broadcast, so one session never sees another's progress/log stream.
    routed_only: bool,
    provider: SharedProvider,
}

impl BioRouterClient {
    pub fn new(
        handlers: Arc<Mutex<Vec<Sender<ServerNotification>>>>,
        provider: SharedProvider,
    ) -> Self {
        Self::with_routing(
            handlers,
            Arc::new(Mutex::new(HashMap::new())),
            false,
            provider,
        )
    }

    pub fn with_routing(
        handlers: Arc<Mutex<Vec<Sender<ServerNotification>>>>,
        progress_routes: ProgressRoutes,
        routed_only: bool,
        provider: SharedProvider,
    ) -> Self {
        BioRouterClient {
            notification_handlers: handlers,
            progress_routes,
            routed_only,
            provider,
        }
    }

    /// Deliver one notification, either to the single subscriber that owns
    /// `token` (isolated routing) or, when there is no token match, to the legacy
    /// broadcast set — unless this is a shared (`routed_only`) client, in which
    /// case an unattributable notification is dropped. Factored out so the
    /// routing decision is unit-testable without a live MCP server.
    async fn deliver(&self, token: Option<&str>, notification: ServerNotification) {
        if let Some(token) = token {
            if let Some(sender) = self.progress_routes.lock().await.get(token) {
                let _ = sender.try_send(notification);
                return;
            }
        }
        if self.routed_only {
            // Shared client, no owning session -> drop rather than bleed across sessions.
            return;
        }
        for handler in self.notification_handlers.lock().await.iter() {
            let _ = handler.try_send(notification.clone());
        }
    }
}

/// Extract the string form of a progress token for use as a route key.
fn progress_token_key(token: &ProgressToken) -> String {
    match &token.0 {
        NumberOrString::String(s) => s.to_string(),
        NumberOrString::Number(n) => n.to_string(),
    }
}

impl ClientHandler for BioRouterClient {
    async fn on_progress(
        &self,
        params: rmcp::model::ProgressNotificationParam,
        context: rmcp::service::NotificationContext<rmcp::RoleClient>,
    ) {
        let token = progress_token_key(&params.progress_token);
        let notification = ServerNotification::ProgressNotification(ProgressNotification {
            params,
            method: ProgressNotificationMethod,
            extensions: context.extensions.clone(),
        });
        self.deliver(Some(&token), notification).await;
    }

    async fn on_logging_message(
        &self,
        params: rmcp::model::LoggingMessageNotificationParam,
        context: rmcp::service::NotificationContext<rmcp::RoleClient>,
    ) {
        // Logging notifications carry no progress token, so a shared client cannot
        // attribute them to a session -> `deliver` drops them when `routed_only`.
        let notification =
            ServerNotification::LoggingMessageNotification(LoggingMessageNotification {
                params,
                method: LoggingMessageNotificationMethod,
                extensions: context.extensions.clone(),
            });
        self.deliver(None, notification).await;
    }

    async fn create_message(
        &self,
        params: CreateMessageRequestParams,
        _context: RequestContext<RoleClient>,
    ) -> Result<CreateMessageResult, ErrorData> {
        let provider = self
            .provider
            .lock()
            .await
            .as_ref()
            .ok_or(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "Could not use provider",
                None,
            ))?
            .clone();

        let provider_ready_messages: Vec<crate::conversation::message::Message> = params
            .messages
            .iter()
            .map(|msg| {
                let base = match msg.role {
                    Role::User => crate::conversation::message::Message::user(),
                    Role::Assistant => crate::conversation::message::Message::assistant(),
                };

                match msg.content.as_text() {
                    Some(text) => base.with_text(&text.text),
                    None => base.with_content(msg.content.clone().into()),
                }
            })
            .collect();

        let system_prompt = params
            .system_prompt
            .as_deref()
            .unwrap_or("You are a general-purpose AI agent called biorouter");

        let (response, usage) = provider
            .complete(system_prompt, &provider_ready_messages, &[])
            .await
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Unexpected error while completing the prompt",
                    Some(Value::from(e.to_string())),
                )
            })?;

        Ok(CreateMessageResult {
            model: usage.model,
            stop_reason: Some(CreateMessageResult::STOP_REASON_END_TURN.to_string()),
            message: SamplingMessage {
                role: Role::Assistant,
                // TODO(alexhancock): MCP sampling currently only supports one content on each SamplingMessage
                // https://modelcontextprotocol.io/specification/draft/client/sampling#messages
                // This doesn't mesh well with biorouter's approach which has Vec<MessageContent>
                // There is a proposal to MCP which is agreed to go in the next version to have SamplingMessages support multiple content parts
                // https://github.com/modelcontextprotocol/modelcontextprotocol/pull/198
                // Until that is formalized, we can take the first message content from the provider and use it
                content: if let Some(content) = response.content.first() {
                    match content {
                        crate::conversation::message::MessageContent::Text(text) => {
                            Content::text(&text.text)
                        }
                        crate::conversation::message::MessageContent::Image(img) => {
                            Content::image(&img.data, &img.mime_type)
                        }
                        // TODO(alexhancock) - Content::Audio? biorouter's messages don't currently have it
                        _ => Content::text(""),
                    }
                } else {
                    Content::text("")
                },
            },
        })
    }

    async fn create_elicitation(
        &self,
        request: CreateElicitationRequestParams,
        _context: RequestContext<RoleClient>,
    ) -> Result<CreateElicitationResult, ErrorData> {
        let schema_value = serde_json::to_value(&request.requested_schema).map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to serialize elicitation schema: {}", e),
                None,
            )
        })?;

        ActionRequiredManager::global()
            .request_and_wait(
                request.message.clone(),
                schema_value,
                Duration::from_secs(300),
            )
            .await
            .map(|outcome| match outcome {
                Some(user_data) => CreateElicitationResult {
                    action: ElicitationAction::Accept,
                    content: Some(user_data),
                },
                // The user (or an unattended run that can never collect
                // input, #40) cancelled: a first-class, model-visible MCP
                // outcome — the server sees `action: cancel` instead of a
                // 300s timeout error.
                None => CreateElicitationResult {
                    action: ElicitationAction::Cancel,
                    content: None,
                },
            })
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Elicitation request timed out or failed: {}", e),
                    None,
                )
            })
    }

    fn get_info(&self) -> ClientInfo {
        ClientInfo {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ClientCapabilities::builder()
                .enable_sampling()
                .enable_elicitation()
                .build(),
            client_info: Implementation {
                name: "biorouter".to_string(),
                version: std::env::var("BIOROUTER_MCP_CLIENT_VERSION")
                    .unwrap_or(env!("CARGO_PKG_VERSION").to_owned()),
                icons: None,
                title: None,
                website_url: None,
            },
            meta: None,
        }
    }
}

/// The MCP client is the interface for MCP operations.
pub struct McpClient {
    client: Mutex<RunningService<RoleClient, BioRouterClient>>,
    notification_subscribers: Arc<Mutex<Vec<mpsc::Sender<ServerNotification>>>>,
    /// Per-dispatch progress-token routes (shared with the `BioRouterClient`).
    progress_routes: ProgressRoutes,
    /// When true, this client is shared across sessions (via a SharedMcpPool) and
    /// notifications are routed per-dispatch instead of broadcast.
    routed_only: bool,
    /// Monotonic counter feeding unique per-dispatch progress tokens.
    next_token: AtomicU64,
    /// Cleared when the transport is observed closed, so a pooled client can be
    /// evicted and respawned rather than handed to a new session (BR-54 recovery).
    healthy: Arc<AtomicBool>,
    server_info: Option<InitializeResult>,
    timeout: std::time::Duration,
}

impl McpClient {
    /// Connect an unpooled client: notifications broadcast to every subscriber
    /// (legacy behavior). Kept as the entry point for per-session/per-app clients.
    pub async fn connect<T, E, A>(
        transport: T,
        timeout: std::time::Duration,
        provider: SharedProvider,
    ) -> Result<Self, ClientInitializeError>
    where
        T: IntoTransport<RoleClient, E, A>,
        E: std::error::Error + From<std::io::Error> + Send + Sync + 'static,
    {
        Self::connect_routed(transport, timeout, provider, false).await
    }

    /// Connect a client, choosing whether its notifications are isolated per
    /// dispatch (`routed_only = true`, for SharedMcpPool clients shared across
    /// sessions) or broadcast to all subscribers (`false`, legacy per-session).
    pub async fn connect_routed<T, E, A>(
        transport: T,
        timeout: std::time::Duration,
        provider: SharedProvider,
        routed_only: bool,
    ) -> Result<Self, ClientInitializeError>
    where
        T: IntoTransport<RoleClient, E, A>,
        E: std::error::Error + From<std::io::Error> + Send + Sync + 'static,
    {
        let notification_subscribers =
            Arc::new(Mutex::new(Vec::<mpsc::Sender<ServerNotification>>::new()));
        let progress_routes: ProgressRoutes = Arc::new(Mutex::new(HashMap::new()));

        let client = BioRouterClient::with_routing(
            notification_subscribers.clone(),
            progress_routes.clone(),
            routed_only,
            provider,
        );
        let client: rmcp::service::RunningService<rmcp::RoleClient, BioRouterClient> =
            client.serve(transport).await?;
        let server_info = client.peer_info().cloned();

        Ok(Self {
            client: Mutex::new(client),
            notification_subscribers,
            progress_routes,
            routed_only,
            next_token: AtomicU64::new(0),
            healthy: Arc::new(AtomicBool::new(true)),
            server_info,
            timeout,
        })
    }

    /// A shared handle to this client's health flag (for the SharedMcpPool).
    pub fn health_flag(&self) -> Arc<AtomicBool> {
        self.healthy.clone()
    }

    async fn send_request(
        &self,
        request: ClientRequest,
        cancel_token: CancellationToken,
    ) -> Result<ServerResult, Error> {
        let handle = self
            .client
            .lock()
            .await
            .send_cancellable_request(request, PeerRequestOptions::no_options())
            .await
            .inspect_err(|_| self.healthy.store(false, Ordering::Relaxed))?;

        let result = await_response(handle, self.timeout, &cancel_token).await;
        if matches!(result, Err(ServiceError::TransportClosed)) {
            self.healthy.store(false, Ordering::Relaxed);
        }
        result
    }

    /// Mint a unique progress token, register a bounded channel under it, and
    /// return the token plus the receiver. Only used on shared (`routed_only`)
    /// clients — the returned token is attached to the call so the server echoes
    /// it, and `deregister_progress` removes the route when the call finishes.
    async fn register_progress(&self) -> (String, mpsc::Receiver<ServerNotification>) {
        let token = format!(
            "bp-{:x}-{}",
            self as *const _ as usize,
            self.next_token.fetch_add(1, Ordering::Relaxed)
        );
        let (tx, rx) = mpsc::channel(16);
        let mut routes = self.progress_routes.lock().await;
        // Opportunistically drop routes whose receiver is gone (e.g. a dispatch
        // cancelled before its call ran) so the map can't grow without bound.
        routes.retain(|_, sender| !sender.is_closed());
        routes.insert(token.clone(), tx);
        (token, rx)
    }

    async fn deregister_progress(&self, token: &str) {
        self.progress_routes.lock().await.remove(token);
    }
}

async fn await_response(
    handle: RequestHandle<RoleClient>,
    timeout: Duration,
    cancel_token: &CancellationToken,
) -> Result<<RoleClient as ServiceRole>::PeerResp, ServiceError> {
    let receiver = handle.rx;
    let peer = handle.peer;
    let request_id = handle.id;
    tokio::select! {
        result = receiver => {
            result.map_err(|_e| ServiceError::TransportClosed)?
        }
        _ = tokio::time::sleep(timeout) => {
            send_cancel_message(&peer, request_id, Some("timed out".to_owned())).await?;
            Err(ServiceError::Timeout{timeout})
        }
        _ = cancel_token.cancelled() => {
            send_cancel_message(&peer, request_id, Some("operation cancelled".to_owned())).await?;
            Err(ServiceError::Cancelled { reason: None })
        }
    }
}

async fn send_cancel_message(
    peer: &Peer<RoleClient>,
    request_id: RequestId,
    reason: Option<String>,
) -> Result<(), ServiceError> {
    peer.send_notification(
        CancelledNotification {
            params: CancelledNotificationParam { request_id, reason },
            method: CancelledNotificationMethod,
            extensions: Default::default(),
        }
        .into(),
    )
    .await
}

#[async_trait::async_trait]
impl McpClientTrait for McpClient {
    fn get_info(&self) -> Option<&InitializeResult> {
        self.server_info.as_ref()
    }

    async fn list_resources(
        &self,
        cursor: Option<String>,
        cancel_token: CancellationToken,
    ) -> Result<ListResourcesResult, Error> {
        let res = self
            .send_request(
                ClientRequest::ListResourcesRequest(ListResourcesRequest {
                    params: Some(PaginatedRequestParams { cursor, meta: None }),
                    method: Default::default(),
                    extensions: inject_current_session_id_into_extensions(Default::default()),
                }),
                cancel_token,
            )
            .await?;

        match res {
            ServerResult::ListResourcesResult(result) => Ok(result),
            _ => Err(ServiceError::UnexpectedResponse),
        }
    }

    async fn read_resource(
        &self,
        uri: &str,
        cancel_token: CancellationToken,
    ) -> Result<ReadResourceResult, Error> {
        let res = self
            .send_request(
                ClientRequest::ReadResourceRequest(ReadResourceRequest {
                    params: ReadResourceRequestParams {
                        uri: uri.to_string(),
                        meta: None,
                    },
                    method: Default::default(),
                    extensions: inject_current_session_id_into_extensions(Default::default()),
                }),
                cancel_token,
            )
            .await?;

        match res {
            ServerResult::ReadResourceResult(result) => Ok(result),
            _ => Err(ServiceError::UnexpectedResponse),
        }
    }

    async fn list_tools(
        &self,
        cursor: Option<String>,
        cancel_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        let res = self
            .send_request(
                ClientRequest::ListToolsRequest(ListToolsRequest {
                    params: Some(PaginatedRequestParams { cursor, meta: None }),
                    method: Default::default(),
                    extensions: inject_current_session_id_into_extensions(Default::default()),
                }),
                cancel_token,
            )
            .await?;

        match res {
            ServerResult::ListToolsResult(result) => Ok(result),
            _ => Err(ServiceError::UnexpectedResponse),
        }
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
        meta: McpMeta,
        cancel_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        // Remember the per-dispatch progress route so we can clean it up after the
        // call completes (or errors), regardless of outcome.
        let progress_token = meta.progress_token.clone();
        let res = self
            .send_request(
                ClientRequest::CallToolRequest(CallToolRequest {
                    params: CallToolRequestParams {
                        task: None,
                        name: name.to_string().into(),
                        arguments,
                        meta: None,
                    },
                    method: Default::default(),
                    extensions: meta.inject_into_extensions(Default::default()),
                }),
                cancel_token,
            )
            .await;

        if let Some(token) = progress_token {
            self.deregister_progress(&token).await;
        }

        match res? {
            ServerResult::CallToolResult(result) => Ok(result),
            _ => Err(ServiceError::UnexpectedResponse),
        }
    }

    async fn list_prompts(
        &self,
        cursor: Option<String>,
        cancel_token: CancellationToken,
    ) -> Result<ListPromptsResult, Error> {
        let res = self
            .send_request(
                ClientRequest::ListPromptsRequest(ListPromptsRequest {
                    params: Some(PaginatedRequestParams { cursor, meta: None }),
                    method: Default::default(),
                    extensions: inject_current_session_id_into_extensions(Default::default()),
                }),
                cancel_token,
            )
            .await?;

        match res {
            ServerResult::ListPromptsResult(result) => Ok(result),
            _ => Err(ServiceError::UnexpectedResponse),
        }
    }

    async fn get_prompt(
        &self,
        name: &str,
        arguments: Value,
        cancel_token: CancellationToken,
    ) -> Result<GetPromptResult, Error> {
        let arguments = match arguments {
            Value::Object(map) => Some(map),
            _ => None,
        };
        let res = self
            .send_request(
                ClientRequest::GetPromptRequest(GetPromptRequest {
                    params: GetPromptRequestParams {
                        name: name.to_string(),
                        arguments,
                        meta: None,
                    },
                    method: Default::default(),
                    extensions: inject_current_session_id_into_extensions(Default::default()),
                }),
                cancel_token,
            )
            .await?;

        match res {
            ServerResult::GetPromptResult(result) => Ok(result),
            _ => Err(ServiceError::UnexpectedResponse),
        }
    }

    async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
        let (tx, rx) = mpsc::channel(16);
        self.notification_subscribers.lock().await.push(tx);
        rx
    }

    async fn register_dispatch(&self) -> (Option<String>, mpsc::Receiver<ServerNotification>) {
        if self.routed_only {
            // Shared client: mint a token and route ONLY this dispatch's
            // notifications, so a concurrent call from another session cannot
            // land in this receiver.
            let (token, rx) = self.register_progress().await;
            (Some(token), rx)
        } else {
            // Unpooled client: legacy broadcast subscription, no token.
            (None, self.subscribe().await)
        }
    }

    fn is_running(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
    }
}

/// Injects the given session_id into Extensions._meta.
fn inject_session_id_into_extensions(mut extensions: Extensions, session_id: &str) -> Extensions {
    let mut meta_map = extensions
        .get::<Meta>()
        .map(|meta| meta.0.clone())
        .unwrap_or_default();

    // JsonObject is case-sensitive, so we use retain for case-insensitive removal
    meta_map.retain(|k, _| !k.eq_ignore_ascii_case(SESSION_ID_HEADER));

    meta_map.insert(
        SESSION_ID_HEADER.to_string(),
        Value::String(session_id.to_string()),
    );

    extensions.insert(Meta(meta_map));
    extensions
}

/// Injects session ID from task-local context into Extensions._meta.
fn inject_current_session_id_into_extensions(extensions: Extensions) -> Extensions {
    if let Some(session_id) = crate::session_context::current_session_id() {
        inject_session_id_into_extensions(extensions, &session_id)
    } else {
        extensions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::Meta;

    fn empty_provider() -> SharedProvider {
        Arc::new(Mutex::new(None))
    }

    fn progress_notif(token: &str) -> ServerNotification {
        ServerNotification::ProgressNotification(ProgressNotification {
            params: rmcp::model::ProgressNotificationParam {
                progress_token: ProgressToken(NumberOrString::String(token.into())),
                progress: 1.0,
                total: None,
                message: None,
            },
            method: ProgressNotificationMethod,
            extensions: Default::default(),
        })
    }

    /// The isolation core: on a shared (`routed_only`) client, a progress
    /// notification for session B's token must reach ONLY session B's receiver,
    /// never session A's. A naive broadcast would fail this.
    #[tokio::test]
    async fn test_shared_client_routes_progress_to_owning_session_only() {
        let routes: ProgressRoutes = Arc::new(Mutex::new(HashMap::new()));
        let client = BioRouterClient::with_routing(
            Arc::new(Mutex::new(Vec::new())),
            routes.clone(),
            true,
            empty_provider(),
        );

        let (tx_a, mut rx_a) = mpsc::channel(4);
        let (tx_b, mut rx_b) = mpsc::channel(4);
        {
            let mut r = routes.lock().await;
            r.insert("tok-A".to_string(), tx_a);
            r.insert("tok-B".to_string(), tx_b);
        }

        client.deliver(Some("tok-B"), progress_notif("tok-B")).await;

        assert!(
            rx_b.try_recv().is_ok(),
            "session B must receive its own progress"
        );
        assert!(
            rx_a.try_recv().is_err(),
            "session A must NOT receive session B's progress (no cross-session bleed)"
        );
    }

    /// A shared client drops any notification it cannot attribute to a token
    /// (unknown token, or an untokened logging message) rather than bleeding it.
    #[tokio::test]
    async fn test_shared_client_drops_unattributable_notifications() {
        let routes: ProgressRoutes = Arc::new(Mutex::new(HashMap::new()));
        let (tx_a, mut rx_a) = mpsc::channel(4);
        routes.lock().await.insert("tok-A".to_string(), tx_a);

        let client = BioRouterClient::with_routing(
            Arc::new(Mutex::new(Vec::new())),
            routes,
            true,
            empty_provider(),
        );

        // Unknown token -> dropped.
        client
            .deliver(Some("tok-unknown"), progress_notif("tok-unknown"))
            .await;
        // Untokened (e.g. a logging message) -> dropped.
        client.deliver(None, progress_notif("ignored")).await;

        assert!(
            rx_a.try_recv().is_err(),
            "an unattributable notification must not leak to another session"
        );
    }

    /// An unpooled client keeps the legacy broadcast behavior: an untokened
    /// notification reaches every subscriber.
    #[tokio::test]
    async fn test_unpooled_client_broadcasts_untokened_notifications() {
        let subscribers = Arc::new(Mutex::new(Vec::new()));
        let (tx, mut rx) = mpsc::channel(4);
        subscribers.lock().await.push(tx);

        let client = BioRouterClient::with_routing(
            subscribers,
            Arc::new(Mutex::new(HashMap::new())),
            false,
            empty_provider(),
        );

        client.deliver(None, progress_notif("whatever")).await;

        assert!(
            rx.try_recv().is_ok(),
            "legacy (unpooled) client must still broadcast untokened notifications"
        );
    }

    #[test]
    fn test_progress_token_key_forms() {
        assert_eq!(
            progress_token_key(&ProgressToken(NumberOrString::String("abc".into()))),
            "abc"
        );
        assert_eq!(
            progress_token_key(&ProgressToken(NumberOrString::Number(42))),
            "42"
        );
    }

    /// The progress token attaches to the SAME `_meta` that carries the session
    /// id, so both ride the wire together and neither clobbers the other.
    #[tokio::test]
    async fn test_meta_carries_session_and_progress_token_together() {
        let meta = McpMeta::new("sess-1").with_progress_token("tok-xyz");
        let ext = meta.inject_into_extensions(Default::default());
        let m = ext.get::<Meta>().expect("meta present");
        assert_eq!(
            m.0.get(SESSION_ID_HEADER).and_then(|v| v.as_str()),
            Some("sess-1")
        );
        assert_eq!(
            m.get_progress_token(),
            Some(ProgressToken(NumberOrString::String("tok-xyz".into())))
        );
    }

    #[tokio::test]
    async fn test_session_id_in_mcp_meta() {
        use serde_json::json;

        let session_id = "test-session-789";
        crate::session_context::with_session_id(Some(session_id.to_string()), async {
            let extensions = inject_current_session_id_into_extensions(Default::default());
            let meta = extensions.get::<Meta>().unwrap();

            assert_eq!(
                &meta.0,
                json!({
                    SESSION_ID_HEADER: session_id
                })
                .as_object()
                .unwrap()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn test_no_session_id_in_mcp_when_absent() {
        let extensions = inject_current_session_id_into_extensions(Default::default());
        let meta = extensions.get::<Meta>();

        assert!(meta.is_none());
    }

    #[tokio::test]
    async fn test_all_mcp_operations_include_session() {
        use serde_json::json;

        let session_id = "consistent-session-id";
        crate::session_context::with_session_id(Some(session_id.to_string()), async {
            let ext1 = inject_current_session_id_into_extensions(Default::default());
            let ext2 = inject_current_session_id_into_extensions(Default::default());
            let ext3 = inject_current_session_id_into_extensions(Default::default());

            for ext in [&ext1, &ext2, &ext3] {
                assert_eq!(
                    &ext.get::<Meta>().unwrap().0,
                    json!({
                        SESSION_ID_HEADER: session_id
                    })
                    .as_object()
                    .unwrap()
                );
            }
        })
        .await;
    }

    #[tokio::test]
    async fn test_session_id_case_insensitive_replacement() {
        use rmcp::model::{Extensions, Meta};
        use serde_json::{from_value, json};

        let session_id = "new-session-id";
        crate::session_context::with_session_id(Some(session_id.to_string()), async {
            let mut extensions = Extensions::new();
            extensions.insert(
                from_value::<Meta>(json!({
                    "BIOROUTER-SESSION-ID": "old-session-1",
                    "Biorouter-Session-Id": "old-session-2",
                    "other-key": "preserve-me"
                }))
                .unwrap(),
            );

            let extensions = inject_current_session_id_into_extensions(extensions);
            let meta = extensions.get::<Meta>().unwrap();

            assert_eq!(
                &meta.0,
                json!({
                    SESSION_ID_HEADER: session_id,
                    "other-key": "preserve-me"
                })
                .as_object()
                .unwrap()
            );
        })
        .await;
    }
}
