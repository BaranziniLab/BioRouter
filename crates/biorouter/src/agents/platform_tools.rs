use indoc::indoc;
use rmcp::model::{Tool, ToolAnnotations};
use rmcp::object;
pub const PLATFORM_MANAGE_SCHEDULE_TOOL_NAME: &str = "platform__manage_schedule";
pub const PLATFORM_INGEST_CONVERSATION_TOOL_NAME: &str = "platform__ingest_conversation";

/// Tool that lets the user, mid-chat, fold conversation history (this session
/// and/or other sessions) into a knowledge base — "remember this chat".
pub fn ingest_conversation_tool() -> Tool {
    Tool::new(
        PLATFORM_INGEST_CONVERSATION_TOOL_NAME.to_string(),
        indoc! {r#"
            Digest BioRouter conversation/chat history into a knowledge base.

            Use this when the user asks to "save", "remember", "ingest", or
            "add this conversation to my knowledge base". It captures the full
            transcript — user input, model output, every tool call with its
            arguments, and every tool response — renders it to markdown, and runs
            the standard knowledge ingestion pipeline so it becomes wiki pages
            with credibility, links and git history.

            By default it ingests the CURRENT session. Pass `session_ids` to
            ingest specific (or multiple) sessions instead. Choose a target with
            exactly one of:
              - `kb_id`: ingest into an existing knowledge base, or
              - `new_kb_name`: create a new knowledge base with this display name
                and ingest into it.
            If neither is given, the currently active knowledge base is used.
        "#}
        .to_string(),
        object!({
            "type": "object",
            "properties": {
                "kb_id": {"type": "string", "description": "Existing knowledge base id to ingest into"},
                "new_kb_name": {"type": "string", "description": "Display name for a new knowledge base to create and ingest into"},
                "session_ids": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Session ids to ingest. Defaults to the current session when omitted."
                },
                "focus": {"type": "string", "description": "Optional guidance on what to emphasise while digesting"}
            }
        }),
    ).annotate(ToolAnnotations {
        title: Some("Ingest conversation into knowledge base".to_string()),
        read_only_hint: Some(false),
        destructive_hint: Some(false),
        idempotent_hint: Some(false),
        open_world_hint: Some(false),
    })
}

pub fn manage_schedule_tool() -> Tool {
    Tool::new(
        PLATFORM_MANAGE_SCHEDULE_TOOL_NAME.to_string(),
        indoc! {r#"
            Manage biorouter's internal scheduled workflow execution.

            Actions:
            - "list": List all biorouter scheduled jobs
            - "create": Create a new biorouter scheduled job from a workflow file
            - "run_now": Execute a biorouter scheduled job immediately
            - "pause": Pause a biorouter scheduled job
            - "unpause": Resume a paused biorouter scheduled job
            - "delete": Remove a biorouter scheduled job
            - "kill": Terminate a currently running biorouter scheduled job
            - "inspect": Get details about a running biorouter scheduled job
            - "sessions": List execution history for a biorouter scheduled job
            - "session_content": Get the full content (messages) of a specific session
        "#}
        .to_string(),
        object!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "create", "run_now", "pause", "unpause", "delete", "kill", "inspect", "sessions", "session_content"]
                },
                "job_id": {"type": "string", "description": "Job identifier for operations on existing jobs"},
                "workflow_path": {"type": "string", "description": "Path to workflow file for create action"},
                "cron_expression": {"type": "string", "description": "A cron expression for create action. Supports both 5-field (minute hour day month weekday) and 6-field (second minute hour day month weekday) formats. 5-field expressions are automatically converted to 6-field by prepending '0' for seconds."},
                "limit": {"type": "integer", "description": "Limit for sessions list", "default": 50},
                "session_id": {"type": "string", "description": "Session identifier for session_content action"}
            }
        }),
    ).annotate(ToolAnnotations {
        title: Some("Manage scheduled workflows".to_string()),
        read_only_hint: Some(false),
        destructive_hint: Some(true), // Can kill jobs
        idempotent_hint: Some(false),
        open_world_hint: Some(false),
    })
}
