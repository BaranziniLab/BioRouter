use indoc::indoc;
use rmcp::model::{Tool, ToolAnnotations};
use rmcp::object;
pub const PLATFORM_MANAGE_SCHEDULE_TOOL_NAME: &str = "platform__manage_schedule";
pub const PLATFORM_INGEST_CONVERSATION_TOOL_NAME: &str = "platform__ingest_conversation";
pub const PLATFORM_INGEST_SOURCE_TOOL_NAME: &str = "platform__ingest_source";
pub const PLATFORM_READ_SESSION_BLOB_TOOL_NAME: &str = "platform__read_session_blob";

/// BR-7: read back a tool result that was too large to keep inline in the
/// conversation. Only offered when lazy blob loading is on — otherwise the
/// payloads are spliced back in at load time and the model never sees a stub.
pub fn read_session_blob_tool() -> Tool {
    Tool::new(
        PLATFORM_READ_SESSION_BLOB_TOOL_NAME.to_string(),
        indoc! {r#"
            Read back the full output of an earlier tool call that was too large
            to keep in the conversation.

            When a tool returns a very large result, Biorouter stores it with the
            session and leaves a stub in its place: a size summary, a head/tail
            preview, and a blob id. Use this tool with that blob id to get the
            rest, instead of re-running the original tool.

            Read a slice, don't dump the whole thing:
              - `pattern`: a regular expression; only matching lines are returned
                (the fastest way to find what you need in a big result).
              - `offset` / `limit`: a 1-based line range.
            Output is capped; narrow the query if it comes back truncated.
        "#}
        .to_string(),
        object!({
            "type": "object",
            "required": ["blob_id"],
            "properties": {
                "blob_id": {"type": "string", "description": "The blob id printed in the stub that replaced the tool result"},
                "pattern": {"type": "string", "description": "Regular expression; return only lines that match it"},
                "offset": {"type": "integer", "description": "1-based first line to return (default 1)"},
                "limit": {"type": "integer", "description": "Maximum number of lines to return (default 200)"}
            }
        }),
    ).annotate(ToolAnnotations {
        title: Some("Read a stored tool result".to_string()),
        read_only_hint: Some(true),
        destructive_hint: Some(false),
        idempotent_hint: Some(true),
        open_world_hint: Some(false),
    })
}

/// Tool that lets the user, mid-chat, fold conversation history (this session
/// and/or other sessions) into a knowledge base — "remember this chat".
pub fn ingest_conversation_tool() -> Tool {
    Tool::new(
        PLATFORM_INGEST_CONVERSATION_TOOL_NAME.to_string(),
        indoc! {r#"
            Digest Biorouter conversation/chat history into a knowledge base.

            Use this when the user asks to "save", "remember", "ingest", or
            "add this conversation to my knowledge base". It captures the full
            transcript (user input, model output, every tool call with its
            arguments, and every tool response), renders it to markdown, and runs
            the standard knowledge ingestion pipeline so it becomes wiki pages
            with credibility, links and git history.

            By default it ingests the CURRENT session. Pass `session_ids` to
            ingest specific (or multiple) sessions instead. Choose a target with
            exactly one of:
              - `kb_id`: ingest into an existing knowledge base, or
              - `new_kb_name`: create a new knowledge base with this display name
                and ingest into it.
            If neither is given it uses this chat's primary knowledge base, and
            the result names the base it wrote to. If the chat has no primary,
            the error lists the bases you can pass as `kb_id`.
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

/// Tool that folds documents, folders and URLs into a knowledge base through
/// the **same** transactional ingest macro the desktop's dropzone uses.
///
/// It exists because a chat had no way to reach that macro at all: the knowledge
/// extension offers only low-level primitives, so a model asked to "ingest these
/// PDFs" hand-rolled extraction and page writes and ended with raw sources and no
/// curated pages (issue #108). The description below is deliberately emphatic
/// about not doing that, because the primitives are still there and still
/// callable.
pub fn ingest_source_tool() -> Tool {
    Tool::new(
        PLATFORM_INGEST_SOURCE_TOOL_NAME.to_string(),
        indoc! {r#"
            Ingest documents, folders or URLs into a knowledge base.

            Use this whenever the user asks to "ingest", "digest", "add", "load"
            or "read into" a knowledge base any file, folder, PDF, paper, note,
            spreadsheet, page or link. It runs Biorouter's real ingestion
            pipeline: it stages the raw source, opens a git transaction, runs the
            bounded knowledge sub-agent to write curated wiki pages, validates
            them, commits (or aborts and leaves the base untouched), rebuilds the
            graph, and re-scans what it committed. The result reports, per source,
            whether curated pages were actually written.

            ALWAYS use this instead of assembling pages yourself. Do NOT extract
            text, call kb_add_raw_source, and then write pages with kb_write_page
            in a script: that path has no transaction, no abort on failure and no
            verification, and it is how ingestion ends with raw files on disk and
            no knowledge in the base.

            Sources — give either one or a batch:
              - `sources`: a list. Each entry is a local path, an http(s) URL, or
                an object with `path`, `url`, or `text` (+ optional `title`).
              - or a single `path`, `url`, or `text` (+ `title`).
            A folder or archive path is expanded to the readable files inside it.

            Choose a target with exactly one of:
              - `kb_id`: an existing knowledge base, or
              - `new_kb_name`: create a new knowledge base with this display name.
            If neither is given it uses this chat's primary knowledge base; if the
            chat has no primary, the error lists the bases you can pass.

            The ingest runs on this chat's model unless you pass `model`. If the
            chat's model cannot drive the pipeline's tools, this tool says so and
            ingests nothing — it never moves the work to a different provider on
            its own.
        "#}
        .to_string(),
        object!({
            "type": "object",
            "properties": {
                "sources": {
                    "type": "array",
                    "description": "Sources to ingest. Each entry is a local path or http(s) URL as a string, or an object with one of `path`, `url`, `text` (plus optional `title`).",
                    "items": {}
                },
                "path": {"type": "string", "description": "A single local file or folder to ingest"},
                "url": {"type": "string", "description": "A single http(s) URL to ingest"},
                "text": {"type": "string", "description": "A single pasted document to ingest"},
                "title": {"type": "string", "description": "Title for `text`"},
                "kb_id": {"type": "string", "description": "Existing knowledge base id to ingest into"},
                "new_kb_name": {"type": "string", "description": "Display name for a new knowledge base to create and ingest into"},
                "focus": {"type": "string", "description": "Optional guidance on what to emphasise while digesting"},
                "model": {
                    "type": "object",
                    "description": "Run this ingest on a specific model instead of the chat's. Only when the user asked for it, or when the chat's model cannot drive the pipeline's tools.",
                    "properties": {
                        "provider": {"type": "string"},
                        "model": {"type": "string"}
                    },
                    "required": ["provider", "model"]
                }
            }
        }),
    ).annotate(ToolAnnotations {
        title: Some("Ingest documents into knowledge base".to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The knowledge extension's own instructions tell the model to reach for
    /// this tool instead of hand-rolling ingestion out of the `kb_*` primitives
    /// — which is the failure issue #108 describes. The two live in different
    /// crates (`biorouter-mcp` cannot depend on `biorouter`), so nothing but
    /// this assertion keeps the name in that prose from drifting away from the
    /// constant, and a stale name there is a silent instruction to do the wrong
    /// thing.
    #[test]
    fn the_knowledge_extension_instructions_name_this_tool() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("biorouter-mcp/src/knowledge/instructions.md");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "the audit must not pass vacuously: {} could not be read: {e}",
                path.display()
            )
        });
        assert!(
            text.contains(PLATFORM_INGEST_SOURCE_TOOL_NAME),
            "the knowledge extension's instructions must name `{PLATFORM_INGEST_SOURCE_TOOL_NAME}`"
        );
        assert!(
            text.contains("Do not hand-roll ingestion"),
            "the instructions must still say NOT to rebuild ingestion from the primitives"
        );
    }

    /// Every way a caller may name a source is in the schema. A missing property
    /// is not a compile error and not a runtime error either — the model simply
    /// never learns the argument exists.
    #[test]
    fn the_ingest_source_schema_offers_a_batch_and_each_single_source_form() {
        let tool = ingest_source_tool();
        assert_eq!(tool.name, PLATFORM_INGEST_SOURCE_TOOL_NAME);
        let props = tool
            .input_schema
            .get("properties")
            .and_then(|p| p.as_object())
            .expect("the schema has properties");
        for key in [
            "sources",
            "path",
            "url",
            "text",
            "title",
            "kb_id",
            "new_kb_name",
            "focus",
            "model",
        ] {
            assert!(props.contains_key(key), "the schema must offer `{key}`");
        }
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(
            description.contains("kb_write_page"),
            "the description must name the primitive it is replacing, or the model \
             reaches for it anyway"
        );
    }
}
