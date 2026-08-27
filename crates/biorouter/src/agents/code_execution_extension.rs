use crate::agents::extension::PlatformExtensionContext;
use crate::agents::extension_manager::get_parameter_names;
use crate::agents::mcp_client::{Error, McpClientTrait, McpMeta};
use anyhow::Result;
use async_trait::async_trait;
use base64::Engine as _;
use boa_engine::builtins::promise::PromiseState;
use boa_engine::context::HostHooks;
use boa_engine::module::{Module, ModuleLoader, Referrer, SyntheticModuleInitializer};
use boa_engine::realm::Realm;
use boa_engine::{
    js_string, Context, JsError, JsNativeError, JsResult, JsString, JsValue, NativeFunction, Source,
};
use indoc::indoc;
use regex::Regex;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, InitializeResult, JsonObject,
    ListToolsResult, ProtocolVersion, RawContent, ResourceContents, Role, ServerCapabilities,
    Tool as McpTool, ToolAnnotations, ToolsCapability,
};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::{Arc, Once};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

pub static EXTENSION_NAME: &str = "code_execution";

const MAX_JS_SOURCE_BYTES: usize = 256 * 1024;
const MAX_JS_RESULT_BYTES: usize = 4 * 1024 * 1024;
const MAX_JS_TOOL_ARGUMENT_BYTES: usize = 1024 * 1024;
const MAX_JS_TOOL_RESULT_BYTES: usize = 4 * 1024 * 1024;
const MAX_JS_LOOP_ITERATIONS: u64 = 1_000_000;
const MAX_JS_ARRAY_BUFFER_BYTES: u64 = 32 * 1024 * 1024;
const MAX_JS_TOOL_CALLS: usize = 256;
const MAX_MODULE_SEARCH_RESULTS: usize = 12;
const MAX_MODULE_SEARCH_TERMS: usize = 16;
const MAX_MODULE_SEARCH_TERM_CHARS: usize = 256;
const MAX_MODULE_SEARCH_TOKENS: usize = 32;
const MODULE_SEARCH_TOKEN_STOP_WORDS: &[&str] = &[
    "a", "an", "and", "create", "draft", "find", "for", "get", "in", "make", "maker", "of", "on",
    "or", "search", "show", "the", "to", "tool", "tools", "use", "with",
];
/// Cap on per-run sub-call telemetry records (issue #28) so a loop-heavy
/// script cannot grow the result meta without bound; calls past the cap are
/// counted, not recorded.
const MAX_TOOL_CALL_RECORDS: usize = 64;
/// Per-field byte cap for recorded sub-call args / error text.
const MAX_TOOL_CALL_RECORD_TEXT_BYTES: usize = 2048;
/// Byte cap for a recorded tool NAME — names are wire data from the script,
/// so they need a bound of their own (real prefixed names are well under it).
const MAX_TOOL_CALL_RECORD_NAME_BYTES: usize = 256;
/// TOTAL serialized-byte budget for the whole `biorouter/tool-calls` array
/// (Codex review of #28): the record-count and per-field caps alone still let
/// 64 worst-case failure records reach ~¼ MB of meta, which persists in every
/// transcript copy of the result. Records past the budget are counted in the
/// dropped counter, not stored.
const MAX_TOOL_CALL_META_TOTAL_BYTES: usize = 64 * 1024;
/// Result-meta key carrying the executed sub-call records for the UI.
const TOOL_CALLS_META_KEY: &str = "biorouter/tool-calls";
/// Result-meta key carrying how many sub-calls were executed but not recorded.
const TOOL_CALLS_DROPPED_META_KEY: &str = "biorouter/tool-calls-dropped";
/// How long a cancelled `execute_code` waits for its tool handler to wind down
/// before aborting it — see [`CodeExecutionClient::wind_down_tool_handler`]
/// (issue #72). Generous, because the normal case returns immediately and this
/// only bounds a pathological script.
const NESTED_CANCEL_GRACE: std::time::Duration = std::time::Duration::from_secs(5);
const MAX_COLLECTED_ARTIFACTS: usize = 16;
const MAX_COLLECTED_ARTIFACT_BYTES: usize = 32 * 1024 * 1024;
const MAX_EMBEDDED_ARTIFACT_HTML_BYTES: usize = 16 * 1024 * 1024;
const MAX_ENCODED_ARTIFACT_HTML_BYTES: usize = (MAX_EMBEDDED_ARTIFACT_HTML_BYTES * 4 / 3) + 4;
const MAX_BROWSER_RESOURCE_URI_BYTES: usize = 8 * 1024;
const MAX_URI_LIST_BYTES: usize = 64 * 1024;
const MAX_ENCODED_URI_LIST_BYTES: usize = (MAX_URI_LIST_BYTES * 4 / 3) + 4;
const BOA_RUNTIME_LIMIT_PANIC: &str =
    "The RuntimeLimit native error cannot be converted to an opaque type.";
static INSTALL_JS_PANIC_HOOK: Once = Once::new();

type ToolCallRequest = (
    String,
    String,
    tokio::sync::oneshot::Sender<Result<String, String>>,
);

struct SandboxHooks;

impl HostHooks for SandboxHooks {
    fn ensure_can_compile_strings(
        &self,
        _realm: Realm,
        _parameters: &[JsString],
        _body: &JsString,
        _direct: bool,
        _context: &mut Context,
    ) -> boa_engine::JsResult<()> {
        Err(JsNativeError::typ()
            .with_message("eval and dynamic Function compilation are disabled")
            .into())
    }

    fn max_buffer_size(&self, _context: &mut Context) -> u64 {
        MAX_JS_ARRAY_BUFFER_BYTES
    }
}

fn panic_payload_message(payload: &(dyn Any + Send)) -> Option<&str> {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
}

fn install_js_panic_hook() {
    INSTALL_JS_PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Boa represents a module runtime limit as a panic; run_js_module
            // catches it and returns a normal tool error, so do not print a
            // misleading process-panic banner for that one engine condition.
            if panic_payload_message(info.payload()) != Some(BOA_RUNTIME_LIMIT_PANIC) {
                previous(info);
            }
        }));
    });
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct ToolGraphNode {
    /// Tool name in format "server/tool" (e.g., "developer/shell")
    tool: String,
    /// Brief description of what this call does (e.g., "list files in /src")
    description: String,
    /// Indices of nodes this depends on (empty if no dependencies)
    #[serde(default)]
    depends_on: Vec<usize>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ExecuteCodeParams {
    /// JavaScript code with ES6 imports for MCP tools.
    code: String,
    /// DAG of tool calls showing execution flow. Each node represents a tool call.
    /// Use depends_on to show data flow (e.g., node 1 uses output from node 0).
    #[serde(default)]
    tool_graph: Vec<ToolGraphNode>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ReadModuleParams {
    /// Module path format:
    /// - For entire server: "server_name"
    /// - For specific tool: "server_name/tool_name"
    module_path: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct SearchModulesParams {
    /// Search terms to find servers/tools (case-insensitive). Can be a single string or array of strings.
    terms: SearchTerms,
    /// If true, treat search terms as regex patterns
    #[serde(default)]
    regex: bool,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
enum SearchTerms {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Default, Deserialize)]
struct InputSchema {
    #[serde(default)]
    properties: BTreeMap<String, Value>,
    #[serde(default)]
    required: Vec<String>,
}

fn quote_join(vals: &[&str]) -> String {
    format!("\"{}\"", vals.join("\" | \""))
}

fn infer_type(schema: &Value) -> Option<String> {
    if schema.get("properties").is_some() {
        Some("object".to_string())
    } else if schema.get("items").is_some() {
        Some("array".to_string())
    } else {
        None
    }
}

fn extract_type_from_schema(schema: &Value) -> Option<String> {
    // enum array (github-mcp style)
    if let Some(arr) = schema.get("enum").and_then(|e| e.as_array()) {
        let vals: Vec<_> = arr.iter().filter_map(|v| v.as_str()).collect();
        if !vals.is_empty() {
            return Some(quote_join(&vals));
        }
    }

    // oneOf with const (schemars enums)
    if let Some(arr) = schema.get("oneOf").and_then(|o| o.as_array()) {
        let vals: Vec<_> = arr
            .iter()
            .filter_map(|v| v.get("const")?.as_str())
            .collect();
        if !vals.is_empty() {
            return Some(quote_join(&vals));
        }
    }

    // anyOf (Option<T> or unions)
    if let Some(arr) = schema.get("anyOf").and_then(|o| o.as_array()) {
        let non_null: Vec<_> = arr
            .iter()
            .filter(|v| v.get("type").and_then(|t| t.as_str()) != Some("null"))
            .collect();
        if non_null.len() == 1 {
            return extract_type_from_schema(non_null[0]).or_else(|| infer_type(non_null[0]));
        }
        if non_null.len() > 1 {
            let types: Vec<_> = non_null
                .iter()
                .filter_map(|v| extract_type_from_schema(v).or_else(|| infer_type(v)))
                .collect();
            if !types.is_empty() {
                return Some(types.join(" | "));
            }
        }
    }

    // type field (string or array)
    match schema.get("type") {
        Some(Value::String(s)) if s == "array" => {
            let item_type = schema
                .get("items")
                .and_then(extract_type_from_schema)
                .unwrap_or_else(|| "any".to_string());
            Some(if item_type == "any" {
                "array".into()
            } else {
                format!("{item_type}[]")
            })
        }
        Some(Value::String(s)) if s == "object" => {
            let Some(props) = schema.get("properties").and_then(|p| p.as_object()) else {
                return Some("object".to_string());
            };
            let required: Vec<_> = schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            let mut fields: Vec<_> = props
                .iter()
                .map(|(name, schema)| {
                    let ty = extract_type_from_schema(schema).unwrap_or_else(|| "any".into());
                    let opt = if required.contains(&name.as_str()) {
                        ""
                    } else {
                        "?"
                    };
                    format!("{name}{opt}: {ty}")
                })
                .collect();
            fields.sort();
            Some(format!("{{ {} }}", fields.join(", ")))
        }
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Array(arr)) => {
            let non_null: Vec<_> = arr
                .iter()
                .filter_map(|v| v.as_str())
                .filter(|s| *s != "null")
                .collect();
            match non_null.len() {
                0 => None,
                1 => Some(non_null[0].to_string()),
                _ => Some(non_null.join(" | ")),
            }
        }
        _ => None,
    }
}

struct ToolInfo {
    server_name: String,
    tool_name: String,
    full_name: String,
    description: String,
    params: Vec<(String, String, bool)>,
    return_type: String,
}

impl ToolInfo {
    fn from_mcp_tool(tool: &McpTool) -> Option<Self> {
        let (server_name, tool_name) = tool.name.as_ref().split_once("__")?;
        let param_names = get_parameter_names(tool);

        let mut schema_value = Value::Object(tool.input_schema.as_ref().clone());
        let _ = unbinder::dereference_schema(&mut schema_value, unbinder::Options::default());
        let schema: InputSchema = serde_json::from_value(schema_value).unwrap_or_default();

        let params = param_names
            .iter()
            .map(|name| {
                let ty = schema
                    .properties
                    .get(name)
                    .and_then(extract_type_from_schema)
                    .unwrap_or_else(|| "any".to_string());
                let required = schema.required.contains(name);
                (name.clone(), ty, required)
            })
            .collect();

        let return_type = tool
            .output_schema
            .as_ref()
            .and_then(|schema| {
                let mut schema_value = Value::Object(schema.as_ref().clone());
                let _ =
                    unbinder::dereference_schema(&mut schema_value, unbinder::Options::default());
                extract_type_from_schema(&schema_value)
            })
            .unwrap_or_else(|| "string".to_string());

        Some(Self {
            server_name: server_name.to_string(),
            tool_name: tool_name.to_string(),
            full_name: tool.name.as_ref().to_string(),
            description: tool
                .description
                .as_ref()
                .map(|d| d.as_ref().to_string())
                .unwrap_or_default(),
            params,
            return_type,
        })
    }

    fn to_signature(&self) -> String {
        self.to_signature_with_module(&self.server_name)
    }

    fn to_signature_with_module(&self, module_name: &str) -> String {
        let params = self
            .params
            .iter()
            .map(|(name, ty, req)| format!("{name}{}: {ty}", if *req { "" } else { "?" }))
            .collect::<Vec<_>>()
            .join(", ");
        let desc = self
            .description
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("");
        format!(
            "{module_name}[\"{}\"]({{{params}}}): {} - {desc}",
            self.tool_name, self.return_type
        )
    }
}

enum ModuleSearchMatcher {
    Regex(Vec<Regex>),
    Plain {
        phrases: Vec<String>,
        tokens: Vec<String>,
    },
}

fn build_module_search_matcher(
    terms: &[String],
    use_regex: bool,
) -> Result<ModuleSearchMatcher, String> {
    if use_regex {
        let patterns = terms
            .iter()
            .map(|term| {
                Regex::new(&format!("(?i){term}"))
                    .map_err(|error| format!("Invalid regex '{term}': {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(ModuleSearchMatcher::Regex(patterns));
    }

    let phrases = terms
        .iter()
        .take(MAX_MODULE_SEARCH_TERMS)
        .map(|term| {
            term.chars()
                .take(MAX_MODULE_SEARCH_TERM_CHARS)
                .collect::<String>()
                .to_lowercase()
        })
        .collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    let tokens = phrases
        .iter()
        .filter(|phrase| phrase.split_whitespace().count() > 1)
        .flat_map(|phrase| phrase.split_whitespace())
        .map(|token| {
            token.trim_matches(|character: char| {
                !character.is_alphanumeric() && character != '_' && character != '-'
            })
        })
        .filter(|token| token.chars().count() >= 2)
        .filter(|token| !MODULE_SEARCH_TOKEN_STOP_WORDS.contains(token))
        .filter(|token| seen.insert((*token).to_string()))
        .take(MAX_MODULE_SEARCH_TOKENS)
        .map(str::to_string)
        .collect();
    Ok(ModuleSearchMatcher::Plain { phrases, tokens })
}

fn module_search_match_score(tool: &ToolInfo, matcher: &ModuleSearchMatcher) -> usize {
    match matcher {
        ModuleSearchMatcher::Regex(patterns) => patterns
            .iter()
            .map(|pattern| {
                usize::from(pattern.is_match(&tool.tool_name)) * 20
                    + usize::from(pattern.is_match(&tool.server_name)) * 8
                    + usize::from(pattern.is_match(&tool.description)) * 4
            })
            .sum(),
        ModuleSearchMatcher::Plain { phrases, tokens } => {
            let tool_name = tool.tool_name.to_lowercase();
            let server_name = tool.server_name.to_lowercase();
            let description = tool.description.to_lowercase();
            let phrase_score = phrases
                .iter()
                .map(|term| {
                    let tool_score = if tool_name == *term {
                        40
                    } else if tool_name.contains(term) {
                        20
                    } else {
                        0
                    };
                    let server_score = if server_name == *term {
                        16
                    } else if server_name.contains(term) {
                        8
                    } else {
                        0
                    };
                    let description_score = usize::from(description.contains(term)) * 4;
                    tool_score + server_score + description_score
                })
                .sum::<usize>();
            let token_score = tokens
                .iter()
                .map(|token| {
                    let tool_score = if tool_name == *token {
                        10
                    } else if tool_name.contains(token) {
                        5
                    } else {
                        0
                    };
                    let server_score = if server_name == *token {
                        4
                    } else if server_name.contains(token) {
                        2
                    } else {
                        0
                    };
                    let description_score = usize::from(description.contains(token)) * 2;
                    tool_score + server_score + description_score
                })
                .sum::<usize>();
            phrase_score + token_score
        }
    }
}

fn module_search_alias(server_name: &str) -> String {
    let sanitized = server_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("module_{sanitized}")
}

fn render_module_search_results(
    matching_tools: &[(usize, &ToolInfo)],
    total_matches: usize,
) -> String {
    let mut output = String::from(
        "## Matching Tools\n\
         These results include complete imports and signatures. Use them directly in execute_code; do not call read_module for a listed tool.\n\n\
         ### Imports\n",
    );
    let mut imported_servers = BTreeSet::new();
    for (_, tool) in matching_tools {
        if imported_servers.insert(tool.server_name.as_str()) {
            output.push_str(&format!(
                "import * as {} from \"{}\";\n",
                module_search_alias(&tool.server_name),
                tool.server_name
            ));
        }
    }
    output.push_str("\n### Signatures\n");
    for (_, tool) in matching_tools {
        let alias = module_search_alias(&tool.server_name);
        output.push_str(&format!(
            "- {}/{}\n  {}\n",
            tool.server_name,
            tool.tool_name,
            tool.to_signature_with_module(&alias)
        ));
    }
    if total_matches > matching_tools.len() {
        output.push_str(&format!(
            "\nShowing the {} best matches out of {total_matches}. Refine search_modules terms if the needed tool is not listed.\n",
            matching_tools.len()
        ));
    }
    output
}

thread_local! {
    static CALL_TX: std::cell::RefCell<Option<mpsc::UnboundedSender<ToolCallRequest>>> =
        const { std::cell::RefCell::new(None) };
    static RESULT_CELL: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// Build the synthetic module for one MCP server: every tool as a named export,
/// plus the server name bound to a namespace object carrying all of them.
///
/// ⚠ **The server name and a tool name can be the SAME string**, and when they
/// are, the two exports are ONE binding, not two. `Module::synthetic` collects
/// the export names into an `FxHashSet` (boa 0.21, `module/mod.rs`), so the
/// duplicate collapses silently, and `set_export` writes by name — so whichever
/// of the two ran last simply overwrote the other. Writing the namespace object
/// last therefore replaced the tool's function with a plain object, and every
/// call form the model is told to use threw `TypeError: not a callable
/// function`. `chatrecall` (extension key `chatrecall`, sole tool `chatrecall`)
/// is the built-in that hits this, and because `code_execution` strips every
/// other tool from the model's list, chat recall was 100% unreachable in the
/// shipped default configuration — issue #93. It was silent since the initial
/// commit because every test fixture used a server and tool with different
/// names.
///
/// Two decisions follow, and neither is cosmetic:
///
/// 1. **On a collision the namespace export is a CALLABLE that forwards to the
///    colliding tool.** A function is an object, so the sibling tools attach to
///    it as properties and all four documented import forms keep working —
///    `ns.tool()`, `ns["tool"]()`, `import { tool }`, and
///    `import { server }; server.other()` — with one binding serving both roles.
///    Merely dropping the server-name export would fix the first three and
///    break the fourth. The forwarder is a *fresh* function rather than the
///    tool's own; see the note at its construction for the two things that
///    buys.
///
/// 2. **Tools are attached with `create_data_property_or_throw`, not `set`.**
///    `set` respects the receiver's existing property attributes, and a function
///    object has non-writable own `name`/`length` and poisoned `caller`/
///    `arguments` accessors. Under `set`, a server that collides *and* exposes a
///    tool called `name` silently served the function's own name string instead
///    of the tool, and one called `caller` made the whole module fail to
///    construct — turning a one-tool bug into a whole-extension outage.
///    `defineProperty` semantics ignore writability and setters, so every tool
///    name behaves identically whatever the namespace object happens to be.
///    It also removes a live footgun: `set` returns a `bool` for "refused", and
///    the previous code mapped only the `Err` arm and dropped that `false`.
fn create_server_module(
    server_name: &str,
    server_tools: &[&ToolInfo],
    ctx: &mut Context,
) -> Module {
    let tool_data: Vec<(String, String)> = server_tools
        .iter()
        .map(|t| (t.tool_name.clone(), t.full_name.clone()))
        .collect();

    // De-duplicated: a tool whose name equals the server name contributes ONE
    // export, and boa would collapse the pair anyway. Making that explicit here
    // is what stops the collapse being invisible.
    let mut export_names: Vec<JsString> = server_tools
        .iter()
        .map(|t| js_string!(t.tool_name.as_str()))
        .collect();
    if !server_tools.iter().any(|t| t.tool_name == server_name) {
        export_names.push(js_string!(server_name));
    }

    let server_name_owned = server_name.to_string();

    Module::synthetic(
        &export_names,
        SyntheticModuleInitializer::from_copy_closure_with_captures(
            |module, (tools, server_name), context| {
                // Build every tool function first: the namespace object may have
                // to BE one of them.
                let mut functions = Vec::with_capacity(tools.len());
                for (tool_name, full_name) in tools.iter() {
                    let func = create_tool_function(full_name.clone());
                    functions.push((tool_name.clone(), func.to_js_function(context.realm())));
                }

                let colliding = functions
                    .iter()
                    .find(|(tool_name, _)| tool_name == server_name)
                    .map(|(_, js_func)| js_func.clone());

                let namespace_obj: boa_engine::JsObject = match &colliding {
                    // A FRESH function that forwards to the colliding tool —
                    // not the tool's own function object.
                    //
                    // Both shapes are callable and both carry the siblings, so
                    // both fix the bug. The reason to forward is ONE measured
                    // difference, not a general tidiness argument:
                    //
                    // Reusing the tool's function makes the namespace hold
                    // ITSELF (`ns.x.x.x…`). `record_result` serialises with
                    // `JsValue::to_json`, whose cycle detection returns `Err`,
                    // and the `.ok()` there swallows it — so a script that put
                    // the namespace in its result silently got boa's debug
                    // rendering with `[Cycle]` in it instead of JSON. A separate
                    // wrapper has no cycle and serialises normally.
                    //
                    // ⚠ What this does NOT buy, despite looking like it should:
                    // the value a script receives from `import { tool }` on a
                    // colliding server is this wrapper, and it carries the
                    // sibling tools as own properties either way. Measured:
                    // `Object.getOwnPropertyNames` returns
                    // `[length, name, <every tool>]` under both shapes. Only the
                    // inner function stays clean, and nothing reaches it except
                    // the double hop `ns.x.x`.
                    //
                    // Costs one extra call frame, on colliding servers only.
                    Some(js_func) => {
                        let target: boa_engine::JsObject = js_func.clone().into();
                        NativeFunction::from_copy_closure_with_captures(
                            |this, args, target: &boa_engine::JsObject, context| {
                                target.call(this, args, context)
                            },
                            target,
                        )
                        .to_js_function(context.realm())
                        .into()
                    }
                    None => boa_engine::JsObject::with_null_proto(),
                };

                for (tool_name, js_func) in &functions {
                    // `create_data_property_or_throw`, not `set` — see the note
                    // on this function.
                    namespace_obj
                        .create_data_property_or_throw(
                            js_string!(tool_name.as_str()),
                            js_func.clone(),
                            context,
                        )
                        .map_err(|e| {
                            JsNativeError::error().with_message(format!("Failed to set prop: {e}"))
                        })?;

                    // The colliding tool shares its binding with the server-name
                    // export below, which already holds the callable namespace.
                    // Exporting it here too would be the very overwrite this
                    // function exists to avoid.
                    if tool_name != server_name {
                        module
                            .set_export(&js_string!(tool_name.as_str()), js_func.clone().into())?;
                    }
                }

                module.set_export(&js_string!(server_name.as_str()), namespace_obj.into())?;

                Ok(())
            },
            (tool_data, server_name_owned),
        ),
        None,
        None,
        ctx,
    )
}

/// Boa's message for calling a non-callable value. It names neither the value
/// nor the call site, so on its own it is a dead end for the model.
const BOA_NOT_CALLABLE: &str = "not a callable function";

/// Attach a recovery hint to JS engine errors whose own text is too opaque to act on.
///
/// Boa reports only that *something* was not callable. Two distinct mistakes are
/// common: calling a module member that is not a function, and calling a string
/// method on a parsed JSON tool result. The hint must name both without claiming
/// either one happened, or a correct response to the wrong diagnosis burns more
/// turns instead of locating the bad callee.
fn annotate_opaque_js_error(message: &str) -> String {
    if message.contains(BOA_NOT_CALLABLE) {
        format!(
            "{message}. Something you called is not a function. Check each callee \
             with typeof, including the imported module member and any method on \
             an intermediate result. One possible cause is a tool result: JSON \
             results are parsed objects, so string methods such as .trim()/.split() \
             are not callable on them. Inspect the value with record_result(value); \
             use JSON.stringify(value) only when string output is actually needed."
        )
    } else {
        message.to_string()
    }
}

/// Bash/zsh parameter-expansion shapes (`${!v}`, `${#arr}`, `${VAR:-default}`,
/// `${VAR%suffix}`, `${VAR/x/y}` …) that are invalid as JS template
/// substitutions. `${VAR}` alone is deliberately NOT matched — it is valid JS
/// (a substitution referencing `VAR`) and fails at runtime, not parse time.
fn bash_param_expansion_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\$\{[!#]|\$\{[A-Za-z_][A-Za-z0-9_]*[:%#/\-]").expect("static regex compiles")
    })
}

/// Best-effort extraction of the `at line N, col M` position boa appends to
/// every lexer/parser error (`col` in parser errors, `column` in lexer
/// errors). Returns `(line, Some(col))`, or `(line, None)` if only the line
/// is present, or `None` when the message carries no position at all.
fn parse_error_position(message: &str) -> Option<(usize, Option<usize>)> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"at line (\d+)(?:, col(?:umn)? (\d+))?").expect("static regex compiles")
    });
    let caps = re.captures(message)?;
    let line = caps.get(1)?.as_str().parse().ok()?;
    let col = caps.get(2).and_then(|m| m.as_str().parse().ok());
    Some((line, col))
}

/// Byte offset of a 1-based `(line, col)` position in `code`, clamped to the
/// line's length. Column is treated as a character count (boa counts code
/// points); payloads are overwhelmingly ASCII, and the span check below only
/// needs the position to land on the right side of a backtick.
fn line_col_to_offset(code: &str, line: usize, col: usize) -> usize {
    let mut offset = 0;
    for (idx, text) in code.split('\n').enumerate() {
        if idx + 1 == line {
            let char_offset: usize = text
                .char_indices()
                .nth(col.saturating_sub(1))
                .map_or(text.len(), |(byte_idx, _)| byte_idx);
            return offset + char_offset;
        }
        offset += text.len() + 1;
    }
    code.len()
}

/// Whether the parse error's reported position falls within the source's
/// outermost backtick pair — the territory a template-literal payload
/// occupies, *including* the code a breakout backtick spills the payload into
/// (the "got 'prefers' in object literal" class lexes as plain JS between two
/// payload backticks). An error before the first or after the last backtick
/// cannot be the payload's fault. Falls back to line granularity when the
/// message carries no column, and stays quiet when it carries no position.
fn error_within_outermost_template(code: &str, message: &str) -> bool {
    let (Some(first), Some(last)) = (code.find('`'), code.rfind('`')) else {
        return false;
    };
    let Some((line, col)) = parse_error_position(message) else {
        return false;
    };
    match col {
        Some(col) => {
            let offset = line_col_to_offset(code, line, col);
            first <= offset && offset <= last
        }
        None => {
            let line_of =
                |offset: usize| code.bytes().take(offset).filter(|&b| b == b'\n').count() + 1;
            line_of(first) <= line && line <= line_of(last)
        }
    }
}

/// Evidence gate for the #23 hint. Each arm requires the failure to actually
/// sit in embedded-payload territory — the mere *presence* of `String.raw`
/// (or any unterminated string) is not enough, so a valid `String.raw`
/// template plus an unrelated syntax error elsewhere passes through unhinted.
///
/// 1. Boa itself locates the error in a template literal (parser context
///    `in template literal`, lexer `unterminated template literal`).
/// 2. `unterminated string literal`, corroborated by embedded-payload
///    evidence: a heredoc marker (`<<`), bash-style parameter expansion, or
///    the error position inside the outermost template span. A short
///    missing-quote typo has none of these.
/// 3. A message naming neither, but whose reported position falls within the
///    outermost backtick pair of a source that visibly embeds a payload
///    (`String.raw`, or a backtick template with bash-style expansion).
fn parse_error_is_embedded_payload_shaped(code: &str, message: &str) -> bool {
    if message.contains("template literal") {
        return true;
    }
    if message.contains("unterminated string literal") {
        return code.contains("<<")
            || bash_param_expansion_regex().is_match(code)
            || error_within_outermost_template(code, message);
    }
    let source_embeds_payload = code.contains("String.raw")
        || (code.contains('`') && bash_param_expansion_regex().is_match(code));
    source_embeds_payload && error_within_outermost_template(code, message)
}

/// Parse-time analogue of [`annotate_opaque_js_error`] (issue #23).
///
/// The dominant parse failure in real sessions is a shell/CSS/markdown payload
/// embedded in a template literal — usually behind `String.raw`, which the
/// model believes makes the payload inert. It does not: `String.raw` only
/// preserves backslash escapes; every `${…}` is still parsed as a JS
/// expression (so bash's `${!v:-}` is a syntax error) and any backtick in the
/// payload terminates the literal, dumping the rest of the payload into JS
/// context (the "got 'prefers' in object literal" class of error). Boa's raw
/// message names none of this, so the model retried the identical form.
///
/// Triggers only on evidence the failure is *within* a template literal or
/// embedded payload (see [`parse_error_is_embedded_payload_shaped`]).
/// Unrelated parse errors pass through untouched — including an ordinary typo
/// in a script that also happens to use `String.raw` correctly.
fn annotate_parse_error(code: &str, message: &str) -> String {
    if !parse_error_is_embedded_payload_shaped(code, message) {
        return message.to_string();
    }
    format!(
        "{message}. The script likely embeds shell/CSS/markdown text in a template \
         literal. String.raw does NOT make ${{…}} literal: every ${{…}} in ANY template \
         literal (String.raw included) is parsed as a JavaScript expression, and a \
         backtick inside the payload terminates the literal early. To emit a literal \
         dollar-brace write ${{\"$\"}}{{VAR}}, or avoid the template entirely: pass the \
         payload as a plain quoted JS string with \\n escapes, or write it to a file \
         with developer/text_editor (write) and run that file via developer/shell."
    )
}

/// Module loader for the sandbox.
///
/// Specifiers are extension (server) names, matched **verbatim** — there is no
/// filesystem behind this loader and no path semantics, so `boa`'s own
/// [`MapModuleLoader`](boa_engine::module::MapModuleLoader) (which resolves keys
/// as `PathBuf`s) is deliberately not used.
///
/// Its reason to exist is the miss path. `MapModuleLoader` answers an unknown
/// specifier with a bare `TypeError: Module could not be found.` — it names
/// neither the module that failed nor the ones that would have worked, so a model
/// that guessed `"fs"` has nothing to correct against and guesses again. This
/// loader answers with the failing name, the exact importable set, and the
/// correct primitive for the guess it most likely made.
#[derive(Debug, Default)]
struct ToolModuleLoader {
    modules: std::cell::RefCell<BTreeMap<String, Module>>,
}

/// Key the user's own script is registered under. Excluded from the "available
/// modules" list in errors — it is an implementation detail, not something to import.
const MAIN_MODULE_KEY: &str = "__main__";

/// Standard-library specifiers a model reaches for out of Node/browser habit,
/// none of which exist here. Named explicitly in the error so the correction is
/// unambiguous rather than a guess at what "available modules" implies.
const NON_EXISTENT_STDLIB_MODULES: &[&str] = &[
    "fs",
    "fs/promises",
    "node:fs",
    "path",
    "node:path",
    "os",
    "node:os",
    "child_process",
    "node:child_process",
    "http",
    "https",
    "util",
    "crypto",
    "process",
];

impl ToolModuleLoader {
    fn insert(&self, specifier: impl Into<String>, module: Module) {
        self.modules.borrow_mut().insert(specifier.into(), module);
    }

    /// Importable module names, sorted, excluding the user's own script.
    fn available(&self) -> Vec<String> {
        self.modules
            .borrow()
            .keys()
            .filter(|name| name.as_str() != MAIN_MODULE_KEY)
            .cloned()
            .collect()
    }

    fn not_found_error(&self, specifier: &str) -> JsError {
        JsError::from_native(
            JsNativeError::typ()
                .with_message(module_not_found_message(specifier, &self.available())),
        )
    }
}

/// Build the actionable "module not found" message.
///
/// Kept free-standing so the exact wording is unit-testable without standing up a
/// JS context.
fn module_not_found_message(specifier: &str, available: &[String]) -> String {
    let mut message = format!("Module \"{specifier}\" could not be found.");

    // A case-only miss ("Developer") is a different mistake from an invented name
    // and deserves a different correction, so check for it first.
    if let Some(matched) = available
        .iter()
        .find(|name| name.eq_ignore_ascii_case(specifier))
    {
        message.push_str(&format!(
            " Did you mean \"{matched}\"? Module names are case-sensitive."
        ));
    } else if NON_EXISTENT_STDLIB_MODULES
        .iter()
        .any(|name| name.eq_ignore_ascii_case(specifier))
    {
        message.push_str(
            " This sandbox has no Node.js or browser standard library, so there is no \
             \"fs\", \"path\", \"os\", \"child_process\", \"http\", or \"fetch\".",
        );
        // Only point at `developer` when it is actually importable here. The
        // two extensions are independently toggleable — code_execution is
        // force-injected as a platform extension while developer is an ordinary
        // one the user can switch off — so recommending it unconditionally can
        // send the model straight into a second "module not found" for the very
        // module we told it to use, which is the retry loop this message exists
        // to break.
        if available.iter().any(|name| name == "developer") {
            message.push_str(
                " For filesystem and command work import from \"developer\" instead: \
                 import { shell, text_editor } from \"developer\";",
            );
        }
    }

    if available.is_empty() {
        message.push_str(" No modules are importable in this session.");
    } else {
        message.push_str(&format!(
            " Importable modules are exactly: {}. Nothing else can be imported. \
             Call search_modules to find which one holds the tool you need, or \
             read_module(\"<module>\") to list its tools.",
            available.join(", ")
        ));
    }

    message
}

impl ModuleLoader for ToolModuleLoader {
    async fn load_imported_module(
        self: Rc<Self>,
        _referrer: Referrer,
        specifier: JsString,
        _context: &std::cell::RefCell<&mut Context>,
    ) -> JsResult<Module> {
        let name = specifier.to_std_string_escaped();
        let found = self.modules.borrow().get(&name).cloned();
        found.ok_or_else(|| self.not_found_error(&name))
    }
}

fn parse_result_to_js(result: &str, ctx: &mut Context) -> JsValue {
    serde_json::from_str::<serde_json::Value>(result)
        .ok()
        .and_then(|v| JsValue::from_json(&v, ctx).ok())
        .unwrap_or_else(|| JsValue::from(js_string!(result)))
}

fn js_string_exceeds_limit(value: &JsValue, limit: usize) -> bool {
    value.as_string().is_some_and(|string| string.len() > limit)
}

fn serialize_json_limited<T: Serialize>(
    value: &T,
    limit: usize,
    label: &str,
    pretty: bool,
) -> Result<String, String> {
    struct LimitedWriter {
        bytes: Vec<u8>,
        limit: usize,
    }

    impl std::io::Write for LimitedWriter {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            if self.bytes.len().saturating_add(buffer.len()) > self.limit {
                return Err(std::io::Error::other("serialized value exceeds limit"));
            }
            self.bytes.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut writer = LimitedWriter {
        bytes: Vec::new(),
        limit,
    };
    let serialized = if pretty {
        let mut serializer = serde_json::Serializer::pretty(&mut writer);
        value.serialize(&mut serializer)
    } else {
        serde_json::to_writer(&mut writer, value)
    };
    serialized.map_err(|_| format!("{label} exceeds the {limit} byte limit"))?;
    String::from_utf8(writer.bytes).map_err(|error| error.to_string())
}

fn create_tool_function(full_tool_name: String) -> NativeFunction {
    NativeFunction::from_copy_closure_with_captures(
        |_this, args, full_name: &String, ctx| {
            let args_value = args.first().cloned().unwrap_or(JsValue::undefined());
            if js_string_exceeds_limit(&args_value, MAX_JS_TOOL_ARGUMENT_BYTES) {
                return Err(JsNativeError::error()
                    .with_message(format!(
                        "Tool arguments exceed the {MAX_JS_TOOL_ARGUMENT_BYTES} byte limit"
                    ))
                    .into());
            }
            let args_json = args_value
                .to_json(ctx)
                .map_err(|e| JsNativeError::error().with_message(e.to_string()))?
                .unwrap_or(Value::Object(serde_json::Map::new()));

            let args_str = serialize_json_limited(
                &args_json,
                MAX_JS_TOOL_ARGUMENT_BYTES,
                "Tool arguments",
                false,
            )
            .map_err(|error| JsNativeError::error().with_message(error))?;
            let (tx, rx) = tokio::sync::oneshot::channel();

            CALL_TX
                .with(|call_tx| {
                    call_tx
                        .borrow()
                        .as_ref()
                        .and_then(|sender| sender.send((full_name.clone(), args_str, tx)).ok())
                })
                .ok_or_else(|| JsNativeError::error().with_message("Channel unavailable"))?;

            rx.blocking_recv()
                .map_err(|e| e.to_string())
                .and_then(|r| r)
                .map(|result| parse_result_to_js(&result, ctx))
                .map_err(|e| JsNativeError::error().with_message(e).into())
        },
        full_tool_name,
    )
}

fn run_js_module(
    code: &str,
    tools: &[ToolInfo],
    call_tx: mpsc::UnboundedSender<ToolCallRequest>,
) -> Result<String, String> {
    if code.len() > MAX_JS_SOURCE_BYTES {
        return Err(format!(
            "JavaScript source exceeds the {} byte limit",
            MAX_JS_SOURCE_BYTES
        ));
    }
    install_js_panic_hook();
    CALL_TX.with(|tx| *tx.borrow_mut() = Some(call_tx));
    RESULT_CELL.with(|cell| *cell.borrow_mut() = None);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_js_module_inner(code, tools)
    }))
    .unwrap_or_else(|payload| {
        let message =
            panic_payload_message(payload.as_ref()).unwrap_or("unknown JavaScript engine panic");
        if message == BOA_RUNTIME_LIMIT_PANIC {
            Err("JavaScript execution limit exceeded".to_string())
        } else {
            Err(format!("JavaScript engine failure: {message}"))
        }
    });
    CALL_TX.with(|tx| *tx.borrow_mut() = None);
    RESULT_CELL.with(|cell| *cell.borrow_mut() = None);
    result
}

fn run_js_module_inner(code: &str, tools: &[ToolInfo]) -> Result<String, String> {
    let loader = Rc::new(ToolModuleLoader::default());
    let mut ctx = Context::builder()
        .module_loader(loader.clone())
        .host_hooks(Rc::new(SandboxHooks))
        .build()
        .map_err(|e| format!("Failed to create JS context: {e}"))?;
    let limits = ctx.runtime_limits_mut();
    limits.set_loop_iteration_limit(MAX_JS_LOOP_ITERATIONS);
    limits.set_recursion_limit(256);
    limits.set_stack_size_limit(4096);

    let record_result = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let value = args.first().cloned().unwrap_or(JsValue::undefined());
        if js_string_exceeds_limit(&value, MAX_JS_RESULT_BYTES) {
            return Err(JsNativeError::error()
                .with_message(format!(
                    "JavaScript result exceeds the {MAX_JS_RESULT_BYTES} byte limit"
                ))
                .into());
        }
        let result_str = match value.to_json(ctx).ok().flatten() {
            Some(json) => {
                serialize_json_limited(&json, MAX_JS_RESULT_BYTES, "JavaScript result", true)
                    .map_err(|error| JsNativeError::error().with_message(error))?
            }
            None => value.display().to_string(),
        };
        RESULT_CELL.with(|cell| *cell.borrow_mut() = Some(result_str));
        Ok(value)
    });

    ctx.register_global_callable(js_string!("record_result"), 1, record_result)
        .map_err(|e| format!("Failed to register record_result: {e}"))?;

    let mut by_server: BTreeMap<&str, Vec<&ToolInfo>> = BTreeMap::new();
    for tool in tools {
        by_server.entry(&tool.server_name).or_default().push(tool);
    }

    for (server_name, server_tools) in &by_server {
        let module = create_server_module(server_name, server_tools, &mut ctx);
        loader.insert(*server_name, module);
    }

    let user_module = Module::parse(Source::from_bytes(code), None, &mut ctx).map_err(|e| {
        format!(
            "Parse error: {}",
            annotate_parse_error(code, &e.to_string())
        )
    })?;
    loader.insert(MAIN_MODULE_KEY, user_module.clone());

    let promise = user_module.load_link_evaluate(&mut ctx);
    ctx.run_jobs()
        .map_err(|e| format!("Job execution error: {e}"))?;

    match promise.state() {
        PromiseState::Fulfilled(_) => {
            let result = RESULT_CELL.with(|cell| cell.borrow().clone());
            let result = result.unwrap_or_else(|| "undefined".to_string());
            if result.len() > MAX_JS_RESULT_BYTES {
                Err(format!(
                    "JavaScript result exceeds the {} byte limit",
                    MAX_JS_RESULT_BYTES
                ))
            } else {
                Ok(result)
            }
        }
        PromiseState::Rejected(err) => Err(format!(
            "Module error: {}",
            annotate_opaque_js_error(&err.display().to_string())
        )),
        PromiseState::Pending => Err("Module evaluation did not complete".to_string()),
    }
}

pub struct CodeExecutionClient {
    info: InitializeResult,
    context: PlatformExtensionContext,
}

/// Truncate `text` to at most `max_bytes` bytes on a char boundary, marking
/// the cut with an ellipsis. Records are UI telemetry, so a lossy-but-bounded
/// copy beats an exact-but-unbounded one.
fn truncate_record_text(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    // The loop above lands `end` on a char boundary, so `get` cannot fail;
    // the fallback only defends against a future edit breaking that invariant.
    format!("{}…", text.get(..end).unwrap_or_default())
}

/// One executed sub-call inside an `execute_code` run (issue #28). Attached to
/// the result as `biorouter/tool-calls` META — never content — so the UI can
/// show exactly which tools ran, with which inputs, and which one failed,
/// without the records ever entering the model context.
#[derive(Debug, Clone, Serialize)]
struct ToolCallRecord {
    /// Prefixed tool name, e.g. "developer__shell".
    tool: String,
    /// The exact JSON arguments string, truncated to
    /// `MAX_TOOL_CALL_RECORD_TEXT_BYTES`.
    args: String,
    /// "ok" | "error".
    status: &'static str,
    /// User-audience error text on failure (truncated), or the sanitized
    /// `tool failed (details hidden): <kind>` placeholder when the tool
    /// produced none — never assistant-audience content, which the tool
    /// deliberately kept out of the user's view.
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// Size of the result handed back to the script on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    result_bytes: Option<usize>,
}

impl ToolCallRecord {
    fn ok(tool: &str, args_json: &str, result_bytes: usize) -> Self {
        Self {
            tool: truncate_record_text(tool, MAX_TOOL_CALL_RECORD_NAME_BYTES),
            args: truncate_record_text(args_json, MAX_TOOL_CALL_RECORD_TEXT_BYTES),
            status: "ok",
            error: None,
            result_bytes: Some(result_bytes),
        }
    }

    /// A failed sub-call. `user_error` must be text that is already safe to
    /// show the user — User-audience (or untagged) content of the failed
    /// result. When the tool produced none, the record carries a sanitized
    /// placeholder naming only the failure class. The script-facing error
    /// string is NEVER recorded here: it is built from assistant-audience
    /// content (`assistant_tool_result_text`) that the tool deliberately kept
    /// out of the user's view, and this record renders in the user's
    /// executed-calls view.
    fn failed(tool: &str, args_json: &str, user_error: Option<&str>, kind: &'static str) -> Self {
        let error = match user_error {
            Some(text) => truncate_record_text(text, MAX_TOOL_CALL_RECORD_TEXT_BYTES),
            None => format!("tool failed (details hidden): {kind}"),
        };
        Self {
            tool: truncate_record_text(tool, MAX_TOOL_CALL_RECORD_NAME_BYTES),
            args: truncate_record_text(args_json, MAX_TOOL_CALL_RECORD_TEXT_BYTES),
            status: "error",
            error: Some(error),
            result_bytes: None,
        }
    }

    /// Serialized size this record contributes to the `biorouter/tool-calls`
    /// array — what the total budget is charged. The +2 covers the record's
    /// separator and its share of the array brackets, so the sum strictly
    /// bounds the serialized array's length.
    fn serialized_bytes(&self) -> usize {
        serde_json::to_string(self)
            .map_or(MAX_TOOL_CALL_META_TOTAL_BYTES, |json| json.len())
            .saturating_add(2)
    }
}

#[derive(Default)]
struct CollectedArtifacts {
    content: Vec<Content>,
    encoded_bytes: usize,
    app_paths: Vec<String>,
    last_app_path: Option<String>,
    tool_calls: Vec<ToolCallRecord>,
    /// Serialized bytes the records in `tool_calls` occupy, charged against
    /// `MAX_TOOL_CALL_META_TOTAL_BYTES`.
    tool_call_bytes: usize,
    dropped_tool_calls: usize,
}

impl CollectedArtifacts {
    fn push_artifact(&mut self, artifact: &Content) -> bool {
        let bytes = artifact_content_size(artifact);
        let existing = artifact_content_uri(artifact).and_then(|uri| {
            self.content
                .iter()
                .position(|content| artifact_content_uri(content) == Some(uri))
        });
        let old_bytes = existing
            .map(|index| artifact_content_size(&self.content[index]))
            .unwrap_or(0);
        let next_bytes = self
            .encoded_bytes
            .saturating_sub(old_bytes)
            .saturating_add(bytes);
        if next_bytes > MAX_COLLECTED_ARTIFACT_BYTES
            || (existing.is_none() && self.content.len() >= MAX_COLLECTED_ARTIFACTS)
        {
            return false;
        }

        self.encoded_bytes = next_bytes;
        if let Some(index) = existing {
            self.content[index] = artifact.clone();
        } else {
            self.content.push(artifact.clone());
        }
        true
    }

    fn push_app_path(&mut self, path: String) {
        if self.app_paths.contains(&path) {
            self.last_app_path = Some(path);
        } else {
            if self.app_paths.len() >= MAX_COLLECTED_ARTIFACTS {
                self.app_paths.remove(0);
            }
            self.app_paths.push(path.clone());
            self.last_app_path = Some(path);
        }
    }

    fn push_tool_call(&mut self, record: ToolCallRecord) {
        let bytes = record.serialized_bytes();
        if self.tool_calls.len() >= MAX_TOOL_CALL_RECORDS
            || self.tool_call_bytes.saturating_add(bytes) > MAX_TOOL_CALL_META_TOTAL_BYTES
        {
            self.dropped_tool_calls += 1;
            return;
        }
        self.tool_call_bytes += bytes;
        self.tool_calls.push(record);
    }
}

fn is_html_mime(mime_type: Option<&str>) -> bool {
    mime_type.is_some_and(|mime_type| {
        let mime_type = mime_type.split(';').next().unwrap_or("").trim();
        mime_type.eq_ignore_ascii_case("text/html")
            || mime_type.eq_ignore_ascii_case("application/xhtml+xml")
    })
}

fn is_uri_list_mime(mime_type: Option<&str>) -> bool {
    mime_type.is_some_and(|mime_type| {
        mime_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .eq_ignore_ascii_case("text/uri-list")
    })
}

fn is_browser_resource_uri(uri: &str) -> bool {
    if uri.len() > MAX_BROWSER_RESOURCE_URI_BYTES {
        return false;
    }
    url::Url::parse(uri).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.username().is_empty()
            && url.password().is_none()
    })
}

fn uri_list_has_browser_url(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .any(|line| !line.is_empty() && !line.starts_with('#') && is_browser_resource_uri(line))
}

fn is_renderable_artifact_resource(resource: &ResourceContents) -> bool {
    match resource {
        ResourceContents::TextResourceContents {
            uri,
            mime_type,
            text,
            ..
        } => {
            (uri.len() <= MAX_BROWSER_RESOURCE_URI_BYTES
                && (uri.starts_with("ui://") || is_browser_resource_uri(uri)))
                && ((is_html_mime(mime_type.as_deref())
                    && text.len() <= MAX_EMBEDDED_ARTIFACT_HTML_BYTES)
                    || (is_uri_list_mime(mime_type.as_deref())
                        && text.len() <= MAX_URI_LIST_BYTES
                        && uri_list_has_browser_url(text)))
        }
        ResourceContents::BlobResourceContents {
            uri,
            mime_type,
            blob,
            ..
        } => {
            if uri.len() > MAX_BROWSER_RESOURCE_URI_BYTES
                || !(uri.starts_with("ui://") || is_browser_resource_uri(uri))
            {
                false
            } else if is_html_mime(mime_type.as_deref())
                && blob.len() <= MAX_ENCODED_ARTIFACT_HTML_BYTES
            {
                base64::engine::general_purpose::STANDARD
                    .decode(blob)
                    .is_ok_and(|bytes| {
                        bytes.len() <= MAX_EMBEDDED_ARTIFACT_HTML_BYTES
                            && std::str::from_utf8(&bytes).is_ok()
                    })
            } else {
                is_uri_list_mime(mime_type.as_deref())
                    && blob.len() <= MAX_ENCODED_URI_LIST_BYTES
                    && base64::engine::general_purpose::STANDARD
                        .decode(blob)
                        .is_ok_and(|bytes| {
                            bytes.len() <= MAX_URI_LIST_BYTES
                                && std::str::from_utf8(&bytes).is_ok_and(uri_list_has_browser_url)
                        })
            }
        }
    }
}

fn is_artifact_content(content: &Content) -> bool {
    if content
        .audience()
        .is_some_and(|audience| !audience.contains(&Role::User))
    {
        return false;
    }
    match &content.raw {
        RawContent::Resource(resource) => is_renderable_artifact_resource(&resource.resource),
        RawContent::ResourceLink(link) => is_browser_resource_uri(&link.uri),
        _ => false,
    }
}

fn artifact_content_size(content: &Content) -> usize {
    struct CappedSizeWriter(usize);

    impl std::io::Write for CappedSizeWriter {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(buffer.len());
            if self.0 > MAX_COLLECTED_ARTIFACT_BYTES {
                return Err(std::io::Error::other("artifact exceeds collection limit"));
            }
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let mut writer = CappedSizeWriter(0);
    serde_json::to_writer(&mut writer, content)
        .map(|()| writer.0)
        .unwrap_or(MAX_COLLECTED_ARTIFACT_BYTES + 1)
}

fn artifact_content_uri(content: &Content) -> Option<&str> {
    match &content.raw {
        RawContent::Resource(resource) => match &resource.resource {
            ResourceContents::TextResourceContents { uri, .. }
            | ResourceContents::BlobResourceContents { uri, .. } => Some(uri),
        },
        RawContent::ResourceLink(link) => Some(&link.uri),
        _ => None,
    }
}

fn app_paths_from_meta(meta: Option<&rmcp::model::Meta>) -> Vec<String> {
    let Some(meta) = meta else {
        return Vec::new();
    };
    let mut paths = meta
        .0
        .get("biorouter/app-paths")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|path| is_valid_app_path(path))
        .take(MAX_COLLECTED_ARTIFACTS)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if let Some(path) = meta
        .0
        .get("biorouter/app-path")
        .and_then(Value::as_str)
        .filter(|path| is_valid_app_path(path))
    {
        paths.push(path.to_string());
    }
    paths
}

fn is_valid_app_path(path: &str) -> bool {
    path.strip_prefix("/apps/")
        .and_then(|id| id.strip_suffix('/'))
        .is_some_and(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
}

/// User-audience text of a tool result: only segments the tool explicitly
/// aimed at the user, or left untagged (untagged content is visible to
/// everyone) — the same audience filter the desktop transcript applies.
/// Telemetry records read THIS, never `assistant_tool_result_text`, whose
/// output the tool deliberately kept away from the user.
fn user_tool_result_text(content: &[Content]) -> Option<String> {
    let text = content
        .iter()
        .filter(|item| {
            item.audience()
                .is_none_or(|audience| audience.contains(&Role::User))
        })
        .filter_map(|item| match &item.raw {
            RawContent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Final attribution for a sub-call failure (issue #28): EVERY failure path —
/// tool errors, dispatch failures, a missing extension manager, size limits —
/// must name its tool, so an uncaught failure surfaces at the step level
/// already attributed. Errors that already carry the standard prefix (or the
/// dispatch path's distinct one) pass through untouched.
fn attribute_sub_call_error(tool_name: &str, error: String) -> String {
    let attributed = format!("Tool error from {tool_name}:");
    let dispatch = format!("Dispatch error from {tool_name}:");
    if error.starts_with(&attributed) || error.starts_with(&dispatch) {
        error
    } else {
        format!("Tool error from {tool_name}: {error}")
    }
}

fn assistant_tool_result_text(
    content: &[Content],
    collected_any: bool,
    has_resources: bool,
) -> Result<String, String> {
    let mut output = String::new();
    let mut first = true;
    for segment in content
        .iter()
        .filter(|item| {
            item.audience()
                .is_none_or(|audience| audience.contains(&Role::Assistant))
        })
        .filter_map(|item| match &item.raw {
            RawContent::Text(text) => Some(text.text.as_str()),
            RawContent::Resource(resource) => match &resource.resource {
                ResourceContents::TextResourceContents { text, .. } => Some(text.as_str()),
                ResourceContents::BlobResourceContents { .. } => None,
            },
            _ => None,
        })
    {
        let separator_bytes = usize::from(!first);
        if output
            .len()
            .saturating_add(separator_bytes)
            .saturating_add(segment.len())
            > MAX_JS_TOOL_RESULT_BYTES
        {
            return Err(format!(
                "Tool result exceeds the {MAX_JS_TOOL_RESULT_BYTES} byte limit"
            ));
        }
        if !first {
            output.push('\n');
        }
        output.push_str(segment);
        first = false;
    }

    if output.is_empty() && collected_any {
        Ok("[artifact available in the host preview]".to_string())
    } else if output.is_empty() && has_resources {
        Ok("[artifact omitted because it exceeded host limits]".to_string())
    } else {
        Ok(output)
    }
}

impl CodeExecutionClient {
    pub fn new(context: PlatformExtensionContext) -> Result<Self> {
        let info = InitializeResult {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ServerCapabilities {
                tasks: None,
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                resources: None,
                prompts: None,
                completions: None,
                experimental: None,
                logging: None,
            },
            server_info: Implementation {
                name: EXTENSION_NAME.to_string(),
                title: Some("Code Execution".to_string()),
                version: "1.0.0".to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(indoc! {r#"
                Use execute_code to CHAIN MULTIPLE DEPENDENT TOOL CALLS INTO ONE round-trip.

                This extension exists to reduce round-trips when a task genuinely needs several
                tool calls whose outputs feed each other, or real computation / control flow
                (loops, conditionals, aggregation) over their results.

                WHEN NOT TO USE THIS EXTENSION:
                - Do NOT use execute_code for basic file or system operations. Listing a directory,
                  reading or writing a single file, copying, moving, deleting, or finding files, and
                  running a single command are simpler and clearer with the developer extension:
                  call `developer/shell` (ls, cp, mv, rm, mkdir, rg) or `developer/text_editor`
                  (view, write, str_replace) DIRECTLY, not wrapped in a JavaScript script.
                - A single tool call is a single tool call. Only reach for execute_code once you
                  have two or more calls that must be chained, or logic to run between them.

                IMPORTANT: All tool calls are SYNCHRONOUS. Do NOT use async/await.

                Workflow (for genuine multi-call chaining only):
                    1. If you do not know the tool, call search_modules once. Its results include complete imports and signatures.
                    2. Use read_module only when you already know a module but need a tool that search_modules did not return.
                    3. Write ONE script that imports and calls ALL tools needed for the task.
                    4. Chain results: use output from one tool as input to the next.

                Never call read_module for a tool whose complete signature was returned by search_modules.
            "#}.to_string()),
        };

        Ok(Self { info, context })
    }

    /// The importable-module catalogue.
    ///
    /// Issue #56 Gate E: this is a discovery surface — `search_modules` and
    /// `read_module` serve tool names, signatures and descriptions out of it —
    /// so a private extension is absent from it under a public model, exactly as
    /// it is absent from the system prompt.
    ///
    /// `admitted` is `Some` for every path that runs INSIDE a tool call, and
    /// that is not an optimisation: it is the rule this file's own comment
    /// states, that "a script's tool call inherits the script's permission". A
    /// resample here would let a model switch mid-turn and change what a running
    /// script can import. `get_moim` is the one caller with nothing to inherit.
    async fn get_tool_infos(
        &self,
        admitted: Option<crate::privacy::CallCapability>,
    ) -> Vec<ToolInfo> {
        let Some(manager) = self
            .context
            .extension_manager
            .as_ref()
            .and_then(|w| w.upgrade())
        else {
            return Vec::new();
        };

        match manager
            .get_prefixed_tools_excluding(EXTENSION_NAME, admitted)
            .await
        {
            Ok(tools) if !tools.is_empty() => {
                tools.iter().filter_map(ToolInfo::from_mcp_tool).collect()
            }
            _ => Vec::new(),
        }
    }

    async fn handle_execute_code(
        &self,
        session_id: &str,
        cap: crate::privacy::CallCapability,
        arguments: Option<JsonObject>,
        cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, String> {
        let code = arguments
            .as_ref()
            .and_then(|a| a.get("code"))
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: code")?
            .to_string();

        let tools = self.get_tool_infos(Some(cap)).await;
        let collected_artifacts = Arc::new(Mutex::new(CollectedArtifacts::default()));
        let (call_tx, call_rx) = mpsc::unbounded_channel();
        let tool_handler = tokio::spawn(Self::run_tool_handler(
            session_id.to_string(),
            // Issue #56: the capability this `execute_code` call was admitted
            // on, carried down to every sub-call the script makes. The bridge
            // holds a `Weak<ExtensionManager>` and no provider handle, so there
            // is nothing here it could sample even if it wanted to — which is
            // the point: a script's tool call inherits the script's permission.
            cap,
            call_rx,
            self.context.extension_manager.clone(),
            Arc::clone(&collected_artifacts),
            cancellation_token.clone(),
        ));

        let js_task = tokio::task::spawn_blocking(move || run_js_module(&code, &tools, call_tx));
        let js_result = tokio::select! {
            result = js_task => result.map_err(|e| format!("JS execution task failed: {e}"))?,
            () = cancellation_token.cancelled() => {
                Self::wind_down_tool_handler(tool_handler).await;
                return Err("JavaScript execution cancelled".to_string());
            }
        };

        tool_handler.abort();

        let mut collected = collected_artifacts.lock().await;
        let mut meta = JsonObject::new();
        if !collected.app_paths.is_empty() {
            if let Some(path) = &collected.last_app_path {
                meta.insert(
                    "biorouter/app-path".to_string(),
                    Value::String(path.clone()),
                );
            }
            meta.insert(
                "biorouter/app-paths".to_string(),
                serde_json::to_value(&collected.app_paths).unwrap_or_default(),
            );
        }
        if !collected.tool_calls.is_empty() {
            meta.insert(
                TOOL_CALLS_META_KEY.to_string(),
                serde_json::to_value(&collected.tool_calls).unwrap_or_default(),
            );
            if collected.dropped_tool_calls > 0 {
                meta.insert(
                    TOOL_CALLS_DROPPED_META_KEY.to_string(),
                    Value::from(collected.dropped_tool_calls),
                );
            }
        }
        let meta = (!meta.is_empty()).then_some(rmcp::model::Meta(meta));

        match js_result {
            Ok(r) => {
                let mut contents = vec![Content::text(format!("Result: {r}"))];
                contents.append(&mut collected.content);
                let mut result = CallToolResult::success(contents);
                result.meta = meta;
                Ok(result)
            }
            Err(error) => {
                // Keep the same error text `call_tool` used to produce, but
                // attach the telemetry meta: a failing run is exactly when the
                // executed-calls list matters most (issue #28).
                let mut result =
                    CallToolResult::error(vec![Content::text(format!("Error: {error}"))]);
                result.meta = meta;
                Ok(result)
            }
        }
    }

    async fn handle_read_module(
        &self,
        arguments: Option<JsonObject>,
        cap: crate::privacy::CallCapability,
    ) -> Result<Vec<Content>, String> {
        let path = arguments
            .as_ref()
            .and_then(|a| a.get("module_path"))
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: module_path")?;

        let tools = self.get_tool_infos(Some(cap)).await;
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();

        match parts.as_slice() {
            [server] => {
                let server_tools: Vec<_> =
                    tools.iter().filter(|t| t.server_name == *server).collect();
                if server_tools.is_empty() {
                    return Err(format!("Module not found: {server}"));
                }
                let sigs: Vec<_> = server_tools.iter().map(|t| t.to_signature()).collect();
                Ok(vec![Content::text(format!(
                    "// import * as {server} from \"{server}\";\n\n{}",
                    sigs.join("\n")
                ))])
            }
            [server, tool] => {
                let t = tools
                    .iter()
                    .find(|t| t.server_name == *server && t.tool_name == *tool)
                    .ok_or_else(|| format!("Tool not found: {server}/{tool}"))?;
                Ok(vec![Content::text(format!(
                    "// import * as {server} from \"{server}\";\n\n{}\n\n{}",
                    t.to_signature(),
                    t.description
                ))])
            }
            _ => Err(format!(
                "Invalid path: {path}. Use 'server' or 'server/tool'"
            )),
        }
    }

    async fn handle_search_modules(
        &self,
        arguments: Option<JsonObject>,
        cap: crate::privacy::CallCapability,
    ) -> Result<Vec<Content>, String> {
        let terms = arguments
            .as_ref()
            .and_then(|a| a.get("terms"))
            .ok_or("Missing required parameter: terms")?;

        let terms_vec = if let Some(arr) = terms.as_array() {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        } else if let Some(s) = terms.as_str() {
            if s.starts_with('[') && s.ends_with(']') {
                serde_json::from_str::<Vec<String>>(s).unwrap_or_else(|_| vec![s.to_string()])
            } else {
                vec![s.to_string()]
            }
        } else {
            return Err("Parameter 'terms' must be a string or array of strings".to_string());
        }
        .into_iter()
        .map(|term| term.trim().to_string())
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();

        if terms_vec.is_empty() {
            return Err("Search terms cannot be empty".to_string());
        }
        if terms_vec.len() > MAX_MODULE_SEARCH_TERMS {
            return Err(format!(
                "Search accepts at most {MAX_MODULE_SEARCH_TERMS} terms"
            ));
        }
        if let Some(term) = terms_vec
            .iter()
            .find(|term| term.chars().count() > MAX_MODULE_SEARCH_TERM_CHARS)
        {
            return Err(format!(
                "Each search term must be at most {MAX_MODULE_SEARCH_TERM_CHARS} characters; got {}",
                term.chars().count()
            ));
        }

        let use_regex = arguments
            .as_ref()
            .and_then(|a| a.get("regex"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let tools = self.get_tool_infos(Some(cap)).await;
        Self::handle_search(&tools, &terms_vec, use_regex)
    }

    fn handle_search(
        tools: &[ToolInfo],
        terms: &[String],
        use_regex: bool,
    ) -> Result<Vec<Content>, String> {
        let matcher = build_module_search_matcher(terms, use_regex)?;

        let mut matching_tools = tools
            .iter()
            .filter_map(|tool| {
                let score = module_search_match_score(tool, &matcher);
                (score > 0).then_some((score, tool))
            })
            .collect::<Vec<_>>();
        matching_tools.sort_by(|(left_score, left), (right_score, right)| {
            right_score
                .cmp(left_score)
                .then_with(|| left.server_name.cmp(&right.server_name))
                .then_with(|| left.tool_name.cmp(&right.tool_name))
        });

        if matching_tools.is_empty() {
            // An empty result set is a valid answer, not a tool failure (issue
            // #26): surfacing it as an error read as "broken tool" in the
            // transcript ([tool_error kind=tool_failure retryable=false]) and
            // fed the failure-streak counters for what was a perfectly good
            // search that simply matched nothing.
            return Ok(vec![Content::text(format!(
                "No tools matched: {}. This catalog contains only the installed MCP tools, \
                 it does not include skills, web search, documents, or knowledge bases, and \
                 it does not answer questions. Try broader or different terms, or call \
                 read_module(\"<module>\") for a module you already know from the \
                 \"Modules:\" list.",
                terms.join(", ")
            ))]);
        }

        let total_matches = matching_tools.len();
        matching_tools.truncate(MAX_MODULE_SEARCH_RESULTS);

        let output = render_module_search_results(&matching_tools, total_matches);
        Ok(vec![Content::text(output)])
    }

    /// Turn one COMPLETED dispatch into the script-facing value/error plus
    /// what telemetry may keep: the failure class, and the user-audience error
    /// text (the only verbatim error text a `ToolCallRecord` may carry — the
    /// script-facing error is built from assistant-audience content).
    async fn completed_sub_call_outcome(
        tool_name: &str,
        result: &CallToolResult,
        collected_artifacts: &Mutex<CollectedArtifacts>,
    ) -> (Result<String, String>, &'static str, Option<String>) {
        let is_error = result.is_error.unwrap_or(false);
        // Renderable resources are passed out-of-band because a JS string
        // cannot preserve an MCP UI artifact. Plain text/file resources stay
        // in the script result and are not duplicated.
        let resources = result
            .content
            .iter()
            .filter(|content| is_artifact_content(content));
        let has_resources = resources.clone().next().is_some();
        let mut collected_any = false;
        {
            let mut collected = collected_artifacts.lock().await;
            for path in app_paths_from_meta(result.meta.as_ref()) {
                collected.push_app_path(path);
            }
            for resource in resources {
                if collected.push_artifact(resource) {
                    collected_any = true;
                }
            }
        }

        let value = if let Some(sc) = &result.structured_content {
            serialize_json_limited(sc, MAX_JS_TOOL_RESULT_BYTES, "Tool result", false)
        } else {
            // Surface exactly what the model itself would see: content
            // targeted at the Assistant (or with no audience set), mirroring
            // `From<Content> for MessageContent`. This (a) drops the
            // duplicate User-audience copy some tools emit — e.g.
            // developer/shell returns the same text twice, which would
            // otherwise hand the script "40\n40" for `echo 40` — and (b)
            // unwraps embedded text resources (developer/text_editor returns
            // file contents as an Assistant-audience text resource).
            // When a tool (e.g. autovisualiser) returns only resource
            // content and no text, JS would receive an empty string and
            // the model would see `Result: ""` and loop retrying. Return
            // a terse confirmation so the model knows it succeeded.
            assistant_tool_result_text(&result.content, collected_any, has_resources)
        };
        let failure_kind = if value.is_err() {
            "result_too_large"
        } else {
            "tool_failure"
        };
        if is_error {
            let user_error = user_tool_result_text(&result.content);
            // Name the failing tool (issue #28): this string is what the
            // script throws, so an uncaught failure surfaces at the step
            // level already attributed.
            let value = match value {
                Ok(message) => Err(format!("Tool error from {tool_name}: {message}")),
                Err(error) => Err(error),
            };
            (value, failure_kind, user_error)
        } else {
            (value, failure_kind, None)
        }
    }

    /// Let a cancelled run's tool handler finish on its own before giving up on
    /// it (issue #72).
    ///
    /// This used to be a bare `tool_handler.abort()`. The handler may be parked
    /// inside a *nested* `dispatch_tool_call`, and the only thing that stops the
    /// work that call started — a foreground `developer/shell` command, whose
    /// process group is killed by the developer server's `on_cancelled` — is the
    /// MCP `notifications/cancelled` that the nested dispatch sends from its own
    /// cancellation branch. Aborting the task dropped that future before it could
    /// send, so Stop ended the turn and left the command running. Which of the two
    /// won was a scheduling race, which is exactly why the report says Stop does
    /// not *reliably* terminate the command.
    ///
    /// `run_tool_handler` stops accepting work the moment the token trips, so in
    /// the normal case this returns in microseconds. The bound is only there so a
    /// pathological script (one that swallows the cancellation errors and keeps
    /// calling tools) cannot hold Stop open.
    async fn wind_down_tool_handler(mut tool_handler: tokio::task::JoinHandle<()>) {
        if tokio::time::timeout(NESTED_CANCEL_GRACE, &mut tool_handler)
            .await
            .is_err()
        {
            tracing::warn!(
                "execute_code's tool handler did not wind down within {:?} of cancellation; \
                 aborting it",
                NESTED_CANCEL_GRACE
            );
            tool_handler.abort();
        }
    }

    /// Refuse one sub-call before it is dispatched: record the failure for
    /// telemetry and hand the script its error.
    ///
    /// The two pre-dispatch guards (the tool-call limit and the global-memory
    /// consent boundary) do the same three things in the same order, and both
    /// must record *before* answering the script — a refusal the record misses
    /// is a call the transparency view never shows.
    async fn refuse_sub_call(
        collected_artifacts: &Arc<Mutex<CollectedArtifacts>>,
        tool_name: &str,
        arguments: &str,
        failure_kind: &'static str,
        error: String,
        response_tx: tokio::sync::oneshot::Sender<Result<String, String>>,
    ) {
        collected_artifacts
            .lock()
            .await
            .push_tool_call(ToolCallRecord::failed(
                tool_name,
                arguments,
                None,
                failure_kind,
            ));
        let _ = response_tx.send(Err(error));
    }

    /// Dispatch one sub-call and report how it went.
    ///
    /// Returns the script-facing result plus the two telemetry facts the caller
    /// records. Telemetry may only carry USER-audience error text (Codex review
    /// of #28): the script-facing strings here are built from assistant-audience
    /// content, so `user_error` is the sole verbatim text a record may keep, and
    /// `failure_kind` names the failure class for the sanitized placeholder when
    /// the tool produced none.
    #[allow(clippy::too_many_arguments)]
    async fn dispatch_sub_call(
        session_id: &str,
        cap: crate::privacy::CallCapability,
        tool_name: &str,
        arguments: &str,
        extension_manager: Option<&std::sync::Weak<crate::agents::ExtensionManager>>,
        collected_artifacts: &Arc<Mutex<CollectedArtifacts>>,
        cancellation_token: &CancellationToken,
    ) -> (Result<String, String>, &'static str, Option<String>) {
        let Some(manager) = extension_manager.and_then(std::sync::Weak::upgrade) else {
            return (
                Err("Extension manager not available".to_string()),
                "unavailable",
                None,
            );
        };
        let tool_call = CallToolRequestParams {
            task: None,
            name: tool_name.to_string().into(),
            arguments: serde_json::from_str(arguments).ok(),
            meta: None,
        };
        match manager
            .dispatch_tool_call(session_id, tool_call, cap, cancellation_token.clone())
            .await
        {
            Ok(dispatch_result) => match dispatch_result.result.await {
                Ok(result) => {
                    let (value, kind, user) =
                        Self::completed_sub_call_outcome(tool_name, &result, collected_artifacts)
                            .await;
                    (value, kind, user)
                }
                Err(e) => (
                    Err(format!("Tool error from {tool_name}: {}", e.message)),
                    "tool_failure",
                    None,
                ),
            },
            Err(e) => (
                Err(format!("Dispatch error from {tool_name}: {e}")),
                "dispatch_error",
                None,
            ),
        }
    }

    /// The refusals this door owes because **no [`ToolInspector`] reaches it**.
    ///
    /// The JS sandbox hands a script's inner tool calls straight to
    /// `ExtensionManager::dispatch_tool_call`, so the whole inspector stack —
    /// which is where the global-memory consent gate, the session-store refusal
    /// and issue #56's first-crossing disclosure all live — is simply not on
    /// this path. Each of the three is therefore re-asked here, against the
    /// **already-evaluated** arguments rather than against the script text: a
    /// path or a payload the script computed at runtime is fully assembled by
    /// the time it arrives, which is what makes these boundary checks strictly
    /// stronger than their inspectors rather than copies of them.
    ///
    /// Returns the metric label and the sentence to answer with. Kept as one
    /// function so the loop has one refusal branch: three inline copies is how
    /// a fourth boundary gets added to two of them.
    ///
    /// [`ToolInspector`]: crate::tool_inspection::ToolInspector
    async fn uninspected_boundary_refusal(
        cap: crate::privacy::CallCapability,
        session_id: &str,
        tool_name: &str,
        evaluated: Option<&rmcp::model::JsonObject>,
    ) -> Option<(&'static str, String)> {
        const BOUNDARY: crate::security::UninspectedBoundary =
            crate::security::UninspectedBoundary::ExecuteCodeScript;

        if let Some(refusal) = crate::security::global_memory::uninspected_boundary_refusal(
            tool_name, evaluated, BOUNDARY,
        ) {
            return Some(("global_memory_consent", refusal));
        }
        // Issue #56. The same boundary, the same reason, for the transcript
        // store: `SessionStoreInspector`'s literal-path scan of the script text
        // is out-computed by exactly one line
        // (`const p = home + "/.config/biorouter/sessions/sessions.db"`). Here
        // the path has already been assembled, so there is nothing left to
        // compute. The store is every conversation on this machine.
        if let Some(refusal) = crate::security::session_store::uninspected_boundary_refusal(
            tool_name, evaluated, BOUNDARY,
        ) {
            return Some(("session_store_read", refusal));
        }
        // Issue #56, the first-crossing disclosure, and the sharpest of the
        // three: an undisclosed write here does not merely escape ONE approval
        // card — the handler then records the (caller, target) pair as having
        // crossed, so every later, properly-inspected write to that conversation
        // is silent too. One script call would permanently disable the
        // disclosure for that pair. Narrow by construction: same-tier writes,
        // already-approved pairs and payload-free tools all pass through.
        if let Some(refusal) = crate::agents::workspace_inspector::uninspected_crossing_refusal(
            cap, session_id, tool_name, evaluated, BOUNDARY,
        )
        .await
        {
            return Some(("workspace_tier_crossing", refusal));
        }
        None
    }

    async fn run_tool_handler(
        session_id: String,
        cap: crate::privacy::CallCapability,
        mut call_rx: mpsc::UnboundedReceiver<ToolCallRequest>,
        extension_manager: Option<std::sync::Weak<crate::agents::ExtensionManager>>,
        collected_artifacts: Arc<Mutex<CollectedArtifacts>>,
        cancellation_token: CancellationToken,
    ) {
        let mut tool_calls = 0;
        loop {
            // Issue #72: once the turn is cancelled, stop taking new work rather
            // than dispatching calls that would only be cancelled again — and,
            // more importantly, return promptly so `wind_down_tool_handler` does
            // not have to abort us while a nested call is still propagating its
            // cancellation downstream.
            let next = tokio::select! {
                biased;
                () = cancellation_token.cancelled() => None,
                message = call_rx.recv() => message,
            };
            let Some((tool_name, arguments, response_tx)) = next else {
                break;
            };
            tool_calls += 1;
            if tool_calls > MAX_JS_TOOL_CALLS {
                Self::refuse_sub_call(
                    &collected_artifacts,
                    &tool_name,
                    &arguments,
                    "call_limit",
                    format!("JavaScript exceeded the {MAX_JS_TOOL_CALLS} tool-call limit"),
                    response_tx,
                )
                .await;
                continue;
            }
            // Issue #63 review, finding 3. A script's tool calls go straight to
            // the extension manager below, so no `ToolInspector` — the
            // global-memory consent gate included — ever sees them. The gate
            // compensated by scanning the *script text* for an embedded memory
            // call, which a runtime-assembled call walks past
            // (`is_global: flag`). This is the same decision taken where there
            // is nothing left to compute: the dispatched name and the evaluated
            // arguments. A boundary that cannot ask the user refuses.
            let evaluated = serde_json::from_str::<serde_json::Value>(&arguments).ok();
            let evaluated = evaluated.as_ref().and_then(serde_json::Value::as_object);
            // Every boundary refusal this door owes, asked in one place. See
            // `uninspected_boundary_refusal` for why a door that no
            // `ToolInspector` reaches has to carry its own.
            if let Some((kind, refusal)) =
                Self::uninspected_boundary_refusal(cap, &session_id, &tool_name, evaluated).await
            {
                Self::refuse_sub_call(
                    &collected_artifacts,
                    &tool_name,
                    &arguments,
                    kind,
                    refusal,
                    response_tx,
                )
                .await;
                continue;
            }
            let (result, mut failure_kind, user_error) = Self::dispatch_sub_call(
                &session_id,
                cap,
                &tool_name,
                &arguments,
                extension_manager.as_ref(),
                &collected_artifacts,
                &cancellation_token,
            )
            .await;
            let result = result.and_then(|value| {
                if value.len() > MAX_JS_TOOL_RESULT_BYTES {
                    failure_kind = "result_too_large";
                    Err(format!(
                        "Tool result exceeds the {MAX_JS_TOOL_RESULT_BYTES} byte limit"
                    ))
                } else {
                    Ok(value)
                }
            });
            // Centralized attribution: whichever path failed, the error the
            // script (and therefore the step-level result) sees names the
            // tool — see `attribute_sub_call_error`.
            let result = result.map_err(|error| attribute_sub_call_error(&tool_name, error));
            // Per-call telemetry for the UI's executed-calls view (issue #28).
            // The Err string is intentionally NOT recorded — see
            // `ToolCallRecord::failed`.
            {
                let record = match &result {
                    Ok(value) => ToolCallRecord::ok(&tool_name, &arguments, value.len()),
                    Err(_) => ToolCallRecord::failed(
                        &tool_name,
                        &arguments,
                        user_error.as_deref(),
                        failure_kind,
                    ),
                };
                collected_artifacts.lock().await.push_tool_call(record);
            }
            let _ = response_tx.send(result);
        }
    }
}

#[async_trait]
impl McpClientTrait for CodeExecutionClient {
    #[allow(clippy::too_many_lines)]
    async fn list_tools(
        &self,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        fn schema<T: JsonSchema>() -> JsonObject {
            serde_json::to_value(schema_for!(T))
                .map(|v| v.as_object().unwrap().clone())
                .expect("valid schema")
        }

        Ok(ListToolsResult {
            tools: vec![
                McpTool::new(
                    "execute_code".to_string(),
                    indoc! {r#"
                        Chain multiple DEPENDENT MCP tool calls, or run computation/control flow over their
                        results, in ONE execution. This is the purpose of this tool: fewer round-trips when a
                        task genuinely needs several calls whose outputs feed each other.

                        DO NOT use this for basic file or system operations. Listing a directory, reading or
                        writing a single file, copying, moving, deleting, or finding files, and running one
                        command are simpler with the developer extension: call `developer/shell` (ls, cp, mv,
                        rm, mkdir, rg) or `developer/text_editor` (view, write, str_replace) DIRECTLY instead
                        of wrapping them in a script here.
                        - WRONG: execute_code to `ls`, copy a file, or `rm`; use developer/shell directly.
                        - WRONG: one execute_code call that wraps a single tool call; just call that tool.
                        - RIGHT: several dependent calls, or a loop/aggregation over their outputs, in one script.

                        EXAMPLE - Chain dependent calls with logic between them (ONE call):
                        ```javascript
                        import { shell } from "developer";
                        const branches = shell({ command: "git branch --format='%(refname:short)'" })
                          .split("\n").filter(Boolean);
                        const ahead = branches.map((b) => ({
                          branch: b,
                          count: shell({ command: `git rev-list --count main..${b}` }).trim(),
                        }));
                        record_result({ ahead });
                        ```

                        EXAMPLE - Fan out one call's output into follow-up calls (ONE call):
                        ```javascript
                        import { shell, text_editor } from "developer";
                        const files = shell({ command: "rg --files -g '*.md' docs" }).split("\n").filter(Boolean);
                        const headings = files.map((f) => ({
                          file: f,
                          first: text_editor({ path: f, command: "view" }).split("\n")[0],
                        }));
                        record_result({ headings });
                        ```

                        SYNTAX:
                        - Import: import { tool1, tool2 } from "serverName";
                        - Call: toolName({ param1: value, param2: value })
                        - Result: record_result(value) - call this to return a value from the script
                        - All calls are synchronous.
                        - "not a callable function" means some callee is not a function. Check imported
                          module members and intermediate values with typeof. One possible cause is a
                          PARSED OBJECT from a JSON tool result: `.trim()`/`.split()` are not callable on
                          objects. Inspect with record_result(value), or use JSON.stringify(value) when
                          string output is actually needed.

                        MODULES:
                        - Only the modules listed in "Modules:" above are importable: these and only these.
                        - There is NO Node.js or browser standard library here: no "fs", "path", "os",
                          "child_process", "http", "https", "crypto", "process", and no fetch/require.
                          For files and commands import from "developer": import { shell, text_editor } from "developer";
                        - Module names are case-sensitive and are the extension names, not package names.

                        MULTILINE SCRIPT ARGUMENTS:
                        - String.raw`...` ONLY preserves backslashes (\n stays two characters). It does NOT
                          make ${...} literal: every ${...} in ANY template literal is still parsed as a JS
                          expression (bash's ${VAR:-x} or ${!v} is a syntax error), and a backtick inside the
                          payload terminates the literal. Escape a literal dollar-brace as ${"$"}{ .
                        - A payload containing backticks or ${...} (shell parameter expansion, markdown code
                          fences, nested scripts) is safer passed as a plain quoted string with \n escapes, or
                          written to a file with developer/text_editor (write) and run via developer/shell.
                        - Prefer one scripting language. Avoid nesting another interpreter unless the task requires it.

                        TOOL_GRAPH: Always provide tool_graph to describe the execution flow for the UI.
                        Each node has: tool (server/name), description (what it does), depends_on (indices of dependencies).
                        Example for chained operations:
                        [
                          {"tool": "developer/shell", "description": "list files", "depends_on": []},
                          {"tool": "developer/text_editor", "description": "read README.md", "depends_on": []},
                          {"tool": "developer/text_editor", "description": "write output.txt", "depends_on": [0, 1]}
                        ]

                        DISCOVERY:
                        - Unknown tool: call search_modules once; its result contains complete, ready-to-use imports and signatures.
                        - Known module, missing tool: call read_module once for that module or tool.
                        - Do not call read_module after search_modules already returned the signature you need.
                    "#}
                    .to_string(),
                    schema::<ExecuteCodeParams>(),
                )
                .annotate(ToolAnnotations {
                    title: Some("Execute JavaScript".to_string()),
                    read_only_hint: Some(false),
                    destructive_hint: Some(true),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(true),
                }),
                McpTool::new(
                    "read_module".to_string(),
                    indoc! {r#"
                        Read tool definitions for a module you already know.

                        PATHS:
                        - "serverName" → lists all tools with signatures (shows required vs optional params)
                        - "serverName/toolName" → full details for one tool including description

                        USE THIS BEFORE execute_code when:
                        - You know the module name but search_modules did not return the tool you need
                        - You need to inspect other tools in the same module
                        - A previous call failed because the signature itself was incomplete or misunderstood

                        Do not call this for a tool whose full signature was already returned by search_modules.

                        The signature format is: toolName({ param1: type, param2?: type }): string
                        Parameters with ? are optional; others are required.
                    "#}
                    .to_string(),
                    schema::<ReadModuleParams>(),
                )
                .annotate(ToolAnnotations {
                    title: Some("Read module".to_string()),
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
                McpTool::new(
                    "search_modules".to_string(),
                    indoc! {r#"
                        Find which MCP TOOL/MODULE to import inside an execute_code script, by matching a
                        tool's name or description. This searches the local catalog of installed tools only:
                        it does NOT search the web, documents, or knowledge bases, and it does not answer
                        questions. Use it solely to locate a tool you intend to call from execute_code.

                        USAGE:
                        - Single term: terms="github" (just a plain string)
                        - Multiple terms: terms=["git", "shell"] (a JSON array, NOT a string)
                        - Regex patterns: terms="sh.*", regex=true

                        IMPORTANT: Do NOT stringify arrays. Use terms=["a","b"] not terms="[\"a\",\"b\"]"

                        Returns ranked tools with complete import syntax and parameter signatures, ready for execute_code.
                        Use this only when you are about to write an execute_code script and don't know which
                        module contains the tool you need. For a web-research or factual question, use a web
                        tool or answer directly, not this.
                        Do not follow it with read_module unless the needed tool was not returned.
                    "#}
                    .to_string(),
                    schema::<SearchModulesParams>(),
                )
                .annotate(ToolAnnotations {
                    title: Some("Search modules".to_string()),
                    read_only_hint: Some(true),
                    destructive_hint: Some(false),
                    idempotent_hint: Some(true),
                    open_world_hint: Some(false),
                }),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
        meta: McpMeta,
        cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        if name == "execute_code" {
            return Ok(self
                .handle_execute_code(
                    &meta.session_id,
                    meta.capability,
                    arguments,
                    cancellation_token,
                )
                .await
                .unwrap_or_else(|error| {
                    CallToolResult::error(vec![Content::text(format!("Error: {error}"))])
                }));
        }
        let content = match name {
            // Issue #56 Gate E: these two ARE the discovery surface, so they see
            // the world the capability this call was admitted on may see.
            "read_module" => self.handle_read_module(arguments, meta.capability).await,
            "search_modules" => self.handle_search_modules(arguments, meta.capability).await,
            _ => Err(format!("Unknown tool: {name}")),
        };

        match content {
            Ok(content) => Ok(CallToolResult::success(content)),
            Err(error) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Error: {error}"
            ))])),
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }

    async fn get_moim(&self, _session_id: &str) -> Option<String> {
        // The one catalogue read with no admitted call to inherit from: MOIM is
        // assembled for the prompt, not inside a tool call, so it samples.
        let tools = self.get_tool_infos(None).await;
        if tools.is_empty() {
            return None;
        }

        let mut servers: BTreeSet<&str> = BTreeSet::new();
        for tool in &tools {
            servers.insert(&tool.server_name);
        }

        let server_list: Vec<_> = servers.into_iter().collect();

        // Keep only the live module inventory here; the batching rule now lives
        // in system.md's "# Tool Use" so it holds regardless of mode. See BR-4.
        Some(format!(
            indoc::indoc! {r#"
                Modules: {}

                Those are the only importable modules: there is no Node.js or browser standard library
                (no "fs", "path", "os", "child_process", "http"); use `import {{ shell, text_editor }} from "developer"`
                for files and commands. Names are case-sensitive.
                A tool call returns a parsed object when its result is JSON, a string otherwise.
                String.raw does NOT make ${{...}} literal: a ${{...}} or backtick inside an embedded
                shell/markdown payload still breaks the parse; write such payloads to a file instead.

                For an unfamiliar task, call search_modules once. Its results contain complete imports and signatures.
                Use read_module only when you know the module but the needed tool was not in the search results.
            "#},
            server_list.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use test_case::test_case;

    #[tokio::test]
    async fn test_execute_code_simple() {
        let temp_dir = tempfile::tempdir().unwrap();
        let session_manager = Arc::new(crate::session::SessionManager::new(
            temp_dir.path().to_path_buf(),
        ));
        let context = PlatformExtensionContext {
            extension_manager: None,
            session_manager,
        };
        let client = CodeExecutionClient::new(context).unwrap();

        let mut args = JsonObject::new();
        args.insert(
            "code".to_string(),
            Value::String("record_result(2 + 2)".to_string()),
        );

        let result = client
            .call_tool(
                "execute_code",
                Some(args),
                McpMeta::new(
                    "test-session-id",
                    crate::privacy::CallCapability::for_test_restricted(),
                ),
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        if let RawContent::Text(text) = &result.content[0].raw {
            assert_eq!(text.text, "Result: 4");
        } else {
            panic!("Expected text content");
        }
    }

    #[tokio::test]
    async fn test_record_result_outputs_valid_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let session_manager = Arc::new(crate::session::SessionManager::new(
            temp_dir.path().to_path_buf(),
        ));
        let context = PlatformExtensionContext {
            extension_manager: None,
            session_manager,
        };
        let client = CodeExecutionClient::new(context).unwrap();

        // Nested array in object - this triggers truncation with display() (e.g., "items: Array(3)")
        let mut args = JsonObject::new();
        args.insert(
            "code".to_string(),
            Value::String("record_result({items: [1, 2, 3], count: 3})".to_string()),
        );

        let result = client
            .call_tool(
                "execute_code",
                Some(args),
                McpMeta::new(
                    "test-session-id",
                    crate::privacy::CallCapability::for_test_restricted(),
                ),
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        if let RawContent::Text(text) = &result.content[0].raw {
            let json_str = text.text.strip_prefix("Result: ").unwrap_or(&text.text);
            let parsed: serde_json::Value = serde_json::from_str(json_str)
                .unwrap_or_else(|_| panic!("Output should be valid JSON, got: {}", text.text));
            assert_eq!(parsed["items"].as_array().unwrap().len(), 3);
            assert_eq!(parsed["count"], 3);
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn javascript_runtime_enforces_source_result_and_execution_limits() {
        let (tx, _rx) = mpsc::unbounded_channel();

        let oversized_source = "x".repeat(MAX_JS_SOURCE_BYTES + 1);
        assert!(run_js_module(&oversized_source, &[], tx.clone())
            .unwrap_err()
            .contains("source exceeds"));

        let oversized_result = format!("record_result('x'.repeat({}))", MAX_JS_RESULT_BYTES + 1);
        assert!(run_js_module(&oversized_result, &[], tx.clone())
            .unwrap_err()
            .contains("result exceeds"));

        assert!(run_js_module("while (true) {}", &[], tx.clone())
            .unwrap_err()
            .contains("execution limit exceeded"));
        assert!(run_js_module("eval('record_result(1)')", &[], tx)
            .unwrap_err()
            .contains("dynamic Function compilation are disabled"));

        let (tx, _rx) = mpsc::unbounded_channel();
        assert!(
            run_js_module("record_result(new ArrayBuffer(33 * 1024 * 1024))", &[], tx,)
                .unwrap_err()
                .contains("Module error")
        );
    }

    #[tokio::test]
    async fn cancelled_javascript_returns_an_error_result() {
        let temp_dir = tempfile::tempdir().unwrap();
        let session_manager = Arc::new(crate::session::SessionManager::new(
            temp_dir.path().to_path_buf(),
        ));
        let client = CodeExecutionClient::new(PlatformExtensionContext {
            extension_manager: None,
            session_manager,
        })
        .unwrap();
        let mut args = JsonObject::new();
        args.insert(
            "code".to_string(),
            Value::String("while (true) {}".to_string()),
        );
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let result = client
            .call_tool(
                "execute_code",
                Some(args),
                McpMeta::new(
                    "cancelled-session",
                    crate::privacy::CallCapability::for_test_restricted(),
                ),
                cancellation,
            )
            .await
            .unwrap();

        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn only_browser_renderable_user_resources_are_relayed() {
        let html = Content::resource(ResourceContents::TextResourceContents {
            uri: "ui://chart/report".to_string(),
            mime_type: Some("text/html; charset=utf-8".to_string()),
            text: "<!doctype html>".to_string(),
            meta: None,
        });
        let plain = Content::resource(ResourceContents::BlobResourceContents {
            uri: "ui://chart/plain".to_string(),
            mime_type: Some("text/plain".to_string()),
            blob: "PHNjcmlwdD4=".to_string(),
            meta: None,
        });
        let assistant_only = html.clone().with_audience(vec![Role::Assistant]);
        let invalid_blob = Content::resource(ResourceContents::BlobResourceContents {
            uri: "ui://chart/invalid".to_string(),
            mime_type: Some("text/html".to_string()),
            blob: "not base64".to_string(),
            meta: None,
        });
        let oversized_html = Content::resource(ResourceContents::TextResourceContents {
            uri: "ui://chart/oversized".to_string(),
            mime_type: Some("text/html".to_string()),
            text: "x".repeat(MAX_EMBEDDED_ARTIFACT_HTML_BYTES + 1),
            meta: None,
        });
        let oversized_ui_uri = Content::resource(ResourceContents::TextResourceContents {
            uri: format!("ui://chart/{}", "x".repeat(MAX_BROWSER_RESOURCE_URI_BYTES)),
            mime_type: Some("text/html".to_string()),
            text: "<!doctype html>".to_string(),
            meta: None,
        });
        let uri_list = Content::resource(ResourceContents::TextResourceContents {
            uri: "ui://report/published".to_string(),
            mime_type: Some("text/uri-list".to_string()),
            text: "# report\nhttps://example.test/report.html\n".to_string(),
            meta: None,
        });
        let unsafe_uri_list = Content::resource(ResourceContents::TextResourceContents {
            uri: "ui://report/unsafe".to_string(),
            mime_type: Some("text/uri-list".to_string()),
            text: "javascript:alert(1)".to_string(),
            meta: None,
        });
        let safe_link = Content::resource_link(rmcp::model::RawResource::new(
            "https://example.test/report.html",
            "report",
        ));
        let unsafe_link = Content::resource_link(rmcp::model::RawResource::new(
            "javascript:alert(1)",
            "unsafe",
        ));
        let local_file_link = Content::resource_link(rmcp::model::RawResource::new(
            "file:///etc/passwd",
            "local file",
        ));
        let credential_link = Content::resource_link(rmcp::model::RawResource::new(
            "https://user:secret@example.test/report",
            "credentials",
        ));
        let oversized_link = Content::resource_link(rmcp::model::RawResource::new(
            format!(
                "https://example.test/{}",
                "x".repeat(MAX_BROWSER_RESOURCE_URI_BYTES)
            ),
            "oversized",
        ));

        assert!(is_artifact_content(&html));
        assert!(!is_artifact_content(&plain));
        assert!(!is_artifact_content(&assistant_only));
        assert!(!is_artifact_content(&invalid_blob));
        assert!(!is_artifact_content(&oversized_html));
        assert!(!is_artifact_content(&oversized_ui_uri));
        assert!(is_artifact_content(&uri_list));
        assert!(!is_artifact_content(&unsafe_uri_list));
        assert!(is_artifact_content(&safe_link));
        assert!(!is_artifact_content(&unsafe_link));
        assert!(!is_artifact_content(&local_file_link));
        assert!(!is_artifact_content(&credential_link));
        assert!(!is_artifact_content(&oversized_link));
    }

    #[test]
    fn later_artifacts_replace_earlier_versions_of_the_same_uri() {
        let first = Content::resource(ResourceContents::TextResourceContents {
            uri: "ui://dashboard/report".to_string(),
            mime_type: Some("text/html".to_string()),
            text: "first".to_string(),
            meta: None,
        });
        let final_version = Content::resource(ResourceContents::TextResourceContents {
            uri: "ui://dashboard/report".to_string(),
            mime_type: Some("text/html".to_string()),
            text: "final".to_string(),
            meta: None,
        });
        let mut collected = CollectedArtifacts::default();

        assert!(collected.push_artifact(&first));
        assert!(collected.push_artifact(&final_version));
        assert_eq!(collected.content.len(), 1);
        assert_eq!(
            collected.encoded_bytes,
            artifact_content_size(&final_version)
        );
        assert!(matches!(
            &collected.content[0].raw,
            RawContent::Resource(resource)
                if matches!(
                    &resource.resource,
                    ResourceContents::TextResourceContents { text, .. } if text == "final"
                )
        ));
    }

    #[test]
    fn artifact_limit_counts_resource_link_metadata() {
        let mut link = rmcp::model::RawResource::new("https://example.test/report", "report");
        link.description = Some("x".repeat(MAX_COLLECTED_ARTIFACT_BYTES));
        let artifact = Content::resource_link(link);
        let mut collected = CollectedArtifacts::default();

        assert!(!collected.push_artifact(&artifact));
        assert!(collected.content.is_empty());
        assert_eq!(collected.encoded_bytes, 0);
    }

    #[test]
    fn json_and_text_tool_results_fail_before_exceeding_limits() {
        assert!(
            serialize_json_limited(&serde_json::json!({ "value": "long" }), 8, "value", false)
                .unwrap_err()
                .contains("value exceeds")
        );

        let content = vec![Content::text("x".repeat(MAX_JS_TOOL_RESULT_BYTES + 1))];
        assert!(assistant_tool_result_text(&content, false, false)
            .unwrap_err()
            .contains("Tool result exceeds"));
    }

    #[test]
    fn app_launch_metadata_is_filtered_before_relay() {
        let meta = rmcp::model::Meta(
            serde_json::from_value(serde_json::json!({
                "biorouter/app-path": "/apps/direct/",
                "biorouter/app-paths": [
                    "/apps/nested/",
                    "/apps/../escape/",
                    "https://example.test/"
                ]
            }))
            .unwrap(),
        );

        assert_eq!(
            app_paths_from_meta(Some(&meta)),
            vec!["/apps/nested/".to_string(), "/apps/direct/".to_string()]
        );
    }

    // --- issue #28: per-sub-call telemetry -------------------------------

    #[test]
    fn record_text_truncation_is_bounded_and_char_safe() {
        assert_eq!(truncate_record_text("short", 2048), "short");

        let truncated = truncate_record_text(&"x".repeat(5000), 2048);
        assert!(truncated.len() <= 2048 + '…'.len_utf8());
        assert!(truncated.ends_with('…'));

        // Cutting inside a multi-byte char must back up to a boundary.
        let multi = "é".repeat(100);
        let truncated = truncate_record_text(&multi, 3);
        assert!(truncated.starts_with('é'));
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn tool_call_records_are_capped_not_unbounded() {
        let mut collected = CollectedArtifacts::default();
        for index in 0..(MAX_TOOL_CALL_RECORDS + 5) {
            collected.push_tool_call(ToolCallRecord::ok(
                &format!("developer__shell_{index}"),
                "{}",
                4,
            ));
        }
        assert_eq!(collected.tool_calls.len(), MAX_TOOL_CALL_RECORDS);
        assert_eq!(collected.dropped_tool_calls, 5);
    }

    // Codex review of #28: the record-count cap alone still let 64 worst-case
    // failure records reach ~¼ MB of persistent result meta. The WHOLE array
    // must respect a total serialized-byte budget.
    #[test]
    fn tool_call_records_respect_a_total_byte_budget() {
        let mut collected = CollectedArtifacts::default();
        // Worst-case records: args and user error text both at the per-field
        // cap (the 4096-byte inputs are truncated to 2 KB each).
        let args = format!(r#"{{"data":"{}"}}"#, "a".repeat(4096));
        let error = "e".repeat(4096);
        for _ in 0..MAX_TOOL_CALL_RECORDS {
            collected.push_tool_call(ToolCallRecord::failed(
                "developer__shell",
                &args,
                Some(&error),
                "tool_failure",
            ));
        }

        assert!(
            collected.dropped_tool_calls > 0,
            "worst-case records must overflow the byte budget before the count cap"
        );
        assert_eq!(
            collected.tool_calls.len() + collected.dropped_tool_calls,
            MAX_TOOL_CALL_RECORDS,
            "every call is either recorded or counted as dropped"
        );
        let serialized = serde_json::to_string(&collected.tool_calls).unwrap();
        assert!(
            serialized.len() <= MAX_TOOL_CALL_META_TOTAL_BYTES,
            "the whole serialized array stays within the budget: {} bytes",
            serialized.len()
        );
    }

    #[test]
    fn tool_call_record_tool_name_is_capped() {
        let record = ToolCallRecord::ok(&"n".repeat(10_000), "{}", 1);
        assert!(record.tool.len() <= MAX_TOOL_CALL_RECORD_NAME_BYTES + '…'.len_utf8());
        assert!(record.tool.ends_with('…'));
    }

    #[test]
    fn tool_call_record_serializes_args_and_status() {
        let ok = serde_json::to_value(ToolCallRecord::ok(
            "developer__shell",
            r#"{"command":"echo hi"}"#,
            42,
        ))
        .unwrap();
        assert_eq!(ok["tool"], "developer__shell");
        assert_eq!(ok["args"], r#"{"command":"echo hi"}"#);
        assert_eq!(ok["status"], "ok");
        assert_eq!(ok["result_bytes"], 42);
        assert!(ok.get("error").is_none());

        let err = serde_json::to_value(ToolCallRecord::failed(
            "developer__text_editor",
            r#"{"command":"view"}"#,
            Some("no such file"),
            "tool_failure",
        ))
        .unwrap();
        assert_eq!(err["status"], "error");
        assert_eq!(err["error"], "no such file");
        assert!(err.get("result_bytes").is_none());
    }

    // Codex review of #28: the recorded error must be user-audience text or a
    // sanitized placeholder — never the script-facing (assistant-audience)
    // error string.
    #[test]
    fn failed_record_uses_user_text_or_sanitized_placeholder() {
        let with_user_text = ToolCallRecord::failed(
            "developer__shell",
            "{}",
            Some("cat: /tmp/x: No such file or directory"),
            "tool_failure",
        );
        assert_eq!(
            with_user_text.error.as_deref(),
            Some("cat: /tmp/x: No such file or directory")
        );

        let sanitized = ToolCallRecord::failed("developer__shell", "{}", None, "dispatch_error");
        assert_eq!(
            sanitized.error.as_deref(),
            Some("tool failed (details hidden): dispatch_error")
        );

        // The user text is still bounded.
        let long = "x".repeat(5000);
        let truncated =
            ToolCallRecord::failed("developer__shell", "{}", Some(&long), "tool_failure");
        assert!(truncated.error.unwrap().len() <= MAX_TOOL_CALL_RECORD_TEXT_BYTES + '…'.len_utf8());
    }

    // Codex review of #28: extraction errors, the unavailable-manager error,
    // and result-size errors reached the script unattributed. Attribution is
    // now centralized, idempotent, and preserves the dispatch prefix.
    #[test]
    fn every_sub_call_failure_names_its_tool() {
        assert_eq!(
            attribute_sub_call_error(
                "developer__shell",
                "Extension manager not available".to_string()
            ),
            "Tool error from developer__shell: Extension manager not available"
        );
        assert_eq!(
            attribute_sub_call_error(
                "developer__shell",
                "Tool result exceeds the 4194304 byte limit".to_string()
            ),
            "Tool error from developer__shell: Tool result exceeds the 4194304 byte limit"
        );
        // Already-attributed errors are not double-prefixed…
        assert_eq!(
            attribute_sub_call_error(
                "developer__shell",
                "Tool error from developer__shell: boom".to_string()
            ),
            "Tool error from developer__shell: boom"
        );
        // …and the dispatch path keeps its distinct prefix.
        assert_eq!(
            attribute_sub_call_error(
                "developer__shell",
                "Dispatch error from developer__shell: no such tool".to_string()
            ),
            "Dispatch error from developer__shell: no such tool"
        );
    }

    #[test]
    fn user_tool_result_text_honours_audience_annotations() {
        use rmcp::model::Role;

        // Assistant-only content must never surface.
        let assistant_only =
            vec![Content::text("secret internals").with_audience(vec![Role::Assistant])];
        assert_eq!(user_tool_result_text(&assistant_only), None);

        // User-tagged and untagged content are both user-visible.
        let mixed = vec![
            Content::text("assistant copy").with_audience(vec![Role::Assistant]),
            Content::text("user copy").with_audience(vec![Role::User]),
            Content::text("untagged note"),
        ];
        assert_eq!(
            user_tool_result_text(&mixed).as_deref(),
            Some("user copy\nuntagged note")
        );

        // Whitespace-only user text counts as absent.
        let blank = vec![Content::text("   ").with_audience(vec![Role::User])];
        assert_eq!(user_tool_result_text(&blank), None);
    }

    /// Issue #72, half one: the wind-down must *wait* for the handler.
    ///
    /// This was `tool_handler.abort()`, which dropped a nested
    /// `dispatch_tool_call` mid-flight — and that dispatch is the only thing that
    /// sends the MCP `notifications/cancelled` the developer server needs in
    /// order to kill a foreground shell's process group. Cutting the handler off
    /// here is exactly how Stop left a `find "$HOME" …` scan running.
    #[tokio::test]
    async fn wind_down_lets_an_in_flight_tool_handler_finish() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let finished = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&finished);
        let handler = tokio::spawn(async move {
            // Stands in for a nested dispatch delivering its cancellation.
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            flag.store(true, Ordering::SeqCst);
        });

        CodeExecutionClient::wind_down_tool_handler(handler).await;

        assert!(
            finished.load(Ordering::SeqCst),
            "a cancelled execute_code must let its tool handler finish propagating \
             the cancellation downstream, not abort it"
        );
    }

    /// Issue #72, half two: waiting is only safe because the handler stops itself.
    /// If it kept parking on `recv()` for a still-open sender, every Stop would
    /// burn the whole grace window before aborting anyway.
    #[tokio::test]
    async fn run_tool_handler_stops_itself_once_the_turn_is_cancelled() {
        let collected = Arc::new(Mutex::new(CollectedArtifacts::default()));
        // The sender stays alive for the whole test: the handler must exit on the
        // cancellation, not on the channel closing.
        let (_call_tx, call_rx) = mpsc::unbounded_channel();
        let token = CancellationToken::new();
        let handler = tokio::spawn(CodeExecutionClient::run_tool_handler(
            "cancel-session".to_string(),
            crate::privacy::CallCapability::for_test_restricted(),
            call_rx,
            None,
            Arc::clone(&collected),
            token.clone(),
        ));

        token.cancel();

        tokio::time::timeout(std::time::Duration::from_secs(5), handler)
            .await
            .expect("the tool handler must return once the turn is cancelled")
            .expect("the tool handler must not panic");
    }

    #[tokio::test]
    async fn run_tool_handler_records_per_call_telemetry() {
        let collected = Arc::new(Mutex::new(CollectedArtifacts::default()));
        let (call_tx, call_rx) = mpsc::unbounded_channel();
        let handler = tokio::spawn(CodeExecutionClient::run_tool_handler(
            "telemetry-session".to_string(),
            crate::privacy::CallCapability::for_test_restricted(),
            call_rx,
            None,
            Arc::clone(&collected),
            CancellationToken::new(),
        ));

        let (tx, rx) = tokio::sync::oneshot::channel();
        call_tx
            .send((
                "developer__shell".to_string(),
                r#"{"command":"echo hi"}"#.to_string(),
                tx,
            ))
            .unwrap();
        let error = rx
            .await
            .unwrap()
            .expect_err("no manager: the call must fail");
        // Even this infrastructure failure names its tool for the script.
        assert_eq!(
            error,
            "Tool error from developer__shell: Extension manager not available"
        );
        drop(call_tx);
        handler.await.unwrap();

        let collected = collected.lock().await;
        assert_eq!(collected.tool_calls.len(), 1);
        let record = &collected.tool_calls[0];
        assert_eq!(record.tool, "developer__shell");
        assert!(record.args.contains("echo hi"), "args: {}", record.args);
        assert_eq!(record.status, "error");
        // No user-audience error text exists on this path, so the record must
        // carry the sanitized placeholder — not the internal error string.
        assert_eq!(
            record.error.as_deref(),
            Some("tool failed (details hidden): unavailable"),
            "error: {:?}",
            record.error
        );
        assert!(record.result_bytes.is_none());
    }

    #[test]
    fn last_launched_app_path_keeps_call_order() {
        let mut collected = CollectedArtifacts::default();
        collected.push_app_path("/apps/zebra/".to_string());
        collected.push_app_path("/apps/alpha/".to_string());
        collected.push_app_path("/apps/zebra/".to_string());

        assert_eq!(
            collected.app_paths,
            vec!["/apps/zebra/".to_string(), "/apps/alpha/".to_string()]
        );
        assert_eq!(collected.last_app_path.as_deref(), Some("/apps/zebra/"));
    }

    #[tokio::test]
    async fn test_read_module_not_found() {
        let temp_dir = tempfile::tempdir().unwrap();
        let session_manager = Arc::new(crate::session::SessionManager::new(
            temp_dir.path().to_path_buf(),
        ));
        let context = PlatformExtensionContext {
            extension_manager: None,
            session_manager,
        };
        let client = CodeExecutionClient::new(context).unwrap();

        let mut args = JsonObject::new();
        args.insert(
            "module_path".to_string(),
            Value::String("nonexistent".to_string()),
        );

        let result = client
            .handle_read_module(
                Some(args),
                crate::privacy::CallCapability::for_test_restricted(),
            )
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_search_plain_text() {
        let tools = vec![
            ToolInfo {
                server_name: "developer".to_string(),
                tool_name: "shell".to_string(),
                full_name: "developer__shell".to_string(),
                description: "Execute shell commands".to_string(),
                params: vec![("command".to_string(), "string".to_string(), true)],
                return_type: "string".to_string(),
            },
            ToolInfo {
                server_name: "developer".to_string(),
                tool_name: "text_editor".to_string(),
                full_name: "developer__text_editor".to_string(),
                description: "Edit text files".to_string(),
                params: vec![("path".to_string(), "string".to_string(), true)],
                return_type: "string".to_string(),
            },
            ToolInfo {
                server_name: "git".to_string(),
                tool_name: "commit".to_string(),
                full_name: "git__commit".to_string(),
                description: "Commit changes to git".to_string(),
                params: vec![("message".to_string(), "string".to_string(), true)],
                return_type: "string".to_string(),
            },
        ];

        // Search for "shell" - should match tool name
        let result =
            CodeExecutionClient::handle_search(&tools, &["shell".to_string()], false).unwrap();
        let text = match &result[0].raw {
            RawContent::Text(t) => &t.text,
            _ => panic!("Expected text"),
        };
        assert!(text.contains("developer/shell"));
        assert!(!text.contains("git/commit"));

        // Search for "developer" - should match server name
        let result =
            CodeExecutionClient::handle_search(&tools, &["developer".to_string()], false).unwrap();
        let text = match &result[0].raw {
            RawContent::Text(t) => &t.text,
            _ => panic!("Expected text"),
        };
        assert!(text.contains("import * as module_developer from \"developer\""));
        assert!(text.contains("developer/shell"));
        assert!(text.contains("developer/text_editor"));

        // Search for "edit" - should match description
        let result =
            CodeExecutionClient::handle_search(&tools, &["edit".to_string()], false).unwrap();
        let text = match &result[0].raw {
            RawContent::Text(t) => &t.text,
            _ => panic!("Expected text"),
        };
        assert!(text.contains("developer/text_editor"));

        // Search for multiple terms
        let result = CodeExecutionClient::handle_search(
            &tools,
            &["shell".to_string(), "git".to_string()],
            false,
        )
        .unwrap();
        let text = match &result[0].raw {
            RawContent::Text(t) => &t.text,
            _ => panic!("Expected text"),
        };
        assert!(text.contains("developer/shell"));
        assert!(text.contains("git/commit"));

        // Search with no matches is a SUCCESS carrying guidance, not a tool
        // failure (issue #26) — an empty result set is a valid answer.
        let result =
            CodeExecutionClient::handle_search(&tools, &["nonexistent".to_string()], false)
                .expect("no-match search must be Ok");
        let text = match &result[0].raw {
            RawContent::Text(t) => &t.text,
            _ => panic!("Expected text"),
        };
        assert!(
            text.contains("No tools matched: nonexistent"),
            "must state that nothing matched, got: {text}"
        );
        assert!(
            text.contains("installed MCP tools"),
            "must scope what the catalog covers, got: {text}"
        );
        assert!(
            text.contains("broader or different terms"),
            "must suggest the recovery, got: {text}"
        );
    }

    #[test]
    fn natural_language_module_search_tokenizes_phrases_with_a_fixed_bound() {
        let tools = vec![ToolInfo {
            server_name: "chatrecall".to_string(),
            tool_name: "chatrecall".to_string(),
            full_name: "chatrecall__chatrecall".to_string(),
            description: "Search past chat or load session summaries".to_string(),
            params: vec![],
            return_type: "string".to_string(),
        }];

        let result = CodeExecutionClient::handle_search(
            &tools,
            &[
                "conversation history search".to_string(),
                "past sessions recall".to_string(),
            ],
            false,
        )
        .unwrap();
        let text = match &result[0].raw {
            RawContent::Text(text) => text.text.as_str(),
            _ => panic!("Expected text"),
        };
        assert!(
            text.contains("chatrecall/chatrecall"),
            "natural-language phrases must match their useful words: {text}"
        );

        let mut words = (0..MAX_MODULE_SEARCH_TOKENS)
            .map(|index| format!("x{index}"))
            .collect::<Vec<_>>();
        words.push("chatrecall".to_string());
        let bounded =
            CodeExecutionClient::handle_search(&tools, &[words.join(" ")], false).unwrap();
        let text = match &bounded[0].raw {
            RawContent::Text(text) => text.text.as_str(),
            _ => panic!("Expected text"),
        };
        assert!(
            text.contains("No tools matched"),
            "only the first {MAX_MODULE_SEARCH_TOKENS} phrase tokens may affect matching: {text}"
        );

        let generic_intent = CodeExecutionClient::handle_search(
            &tools,
            &[
                "create skill".to_string(),
                "make skill".to_string(),
                "skill maker".to_string(),
                "draft skill".to_string(),
            ],
            false,
        )
        .unwrap();
        let text = match &generic_intent[0].raw {
            RawContent::Text(text) => text.text.as_str(),
            _ => panic!("Expected text"),
        };
        assert!(
            text.contains("No tools matched"),
            "generic intent verbs must not manufacture a module-search match: {text}"
        );
    }

    #[test]
    fn web_news_search_returns_ranked_ready_to_execute_signatures() {
        let tools = vec![
            ToolInfo {
                server_name: "computercontroller".to_string(),
                tool_name: "web_scrape".to_string(),
                full_name: "computercontroller__web_scrape".to_string(),
                description:
                    "Fetch web, RSS, and news search-result URLs and return content inline"
                        .to_string(),
                params: vec![
                    ("save_as".to_string(), "string".to_string(), false),
                    ("url".to_string(), "string".to_string(), true),
                ],
                return_type: "string".to_string(),
            },
            ToolInfo {
                server_name: "computercontroller".to_string(),
                tool_name: "automation_script".to_string(),
                full_name: "computercontroller__automation_script".to_string(),
                description: "Run network-aware scripts for web, RSS, or news searches".to_string(),
                params: vec![("script".to_string(), "string".to_string(), true)],
                return_type: "string".to_string(),
            },
            ToolInfo {
                server_name: "cdwagent".to_string(),
                tool_name: "CDW-search_notes".to_string(),
                full_name: "cdwagent__CDW-search_notes".to_string(),
                description: "Search clinical notes".to_string(),
                params: vec![],
                return_type: "string".to_string(),
            },
            ToolInfo {
                server_name: "computercontroller".to_string(),
                tool_name: "xlsx_tool".to_string(),
                full_name: "computercontroller__xlsx_tool".to_string(),
                description: "Read and write spreadsheets".to_string(),
                params: vec![],
                return_type: "string".to_string(),
            },
        ];

        let result = CodeExecutionClient::handle_search(
            &tools,
            &[
                "web".to_string(),
                "search".to_string(),
                "browser".to_string(),
                "news".to_string(),
            ],
            false,
        )
        .unwrap();
        let text = match &result[0].raw {
            RawContent::Text(text) => text.text.as_str(),
            _ => panic!("Expected text"),
        };

        assert!(text.contains("complete imports and signatures"));
        assert!(text.contains("do not call read_module"));
        assert!(text.contains("import * as module_computercontroller from \"computercontroller\";"));
        assert!(text.contains(
            "module_computercontroller[\"web_scrape\"]({save_as?: string, url: string})"
        ));
        assert!(!text.contains("xlsx_tool"));
        assert!(
            text.find("computercontroller/web_scrape").unwrap()
                < text.find("cdwagent/CDW-search_notes").unwrap()
        );
    }

    #[test]
    fn module_search_alias_is_valid_for_non_identifier_server_names() {
        assert_eq!(module_search_alias("123-tools.dev"), "module_123_tools_dev");
    }

    #[test]
    fn signatures_use_first_nonempty_description_line() {
        let tool = ToolInfo {
            server_name: "computercontroller".to_string(),
            tool_name: "automation_script".to_string(),
            full_name: "computercontroller__automation_script".to_string(),
            description: "\n    Run scripts for web and API research.\n    More detail."
                .to_string(),
            params: vec![],
            return_type: "string".to_string(),
        };

        assert!(tool
            .to_signature()
            .ends_with(" - Run scripts for web and API research."));
    }

    #[test]
    fn test_search_regex() {
        let tools = vec![
            ToolInfo {
                server_name: "developer".to_string(),
                tool_name: "shell".to_string(),
                full_name: "developer__shell".to_string(),
                description: "Execute shell commands".to_string(),
                params: vec![],
                return_type: "string".to_string(),
            },
            ToolInfo {
                server_name: "developer".to_string(),
                tool_name: "text_editor".to_string(),
                full_name: "developer__text_editor".to_string(),
                description: "Edit text files".to_string(),
                params: vec![],
                return_type: "string".to_string(),
            },
        ];

        // Regex search for "sh.*" - should match shell
        let result =
            CodeExecutionClient::handle_search(&tools, &["sh.*".to_string()], true).unwrap();
        let text = match &result[0].raw {
            RawContent::Text(t) => &t.text,
            _ => panic!("Expected text"),
        };
        assert!(text.contains("developer/shell"));

        // Regex search for "^text" - should match text_editor
        let result =
            CodeExecutionClient::handle_search(&tools, &["^text".to_string()], true).unwrap();
        let text = match &result[0].raw {
            RawContent::Text(t) => &t.text,
            _ => panic!("Expected text"),
        };
        assert!(text.contains("developer/text_editor"));

        // Invalid regex should error
        let result = CodeExecutionClient::handle_search(&tools, &["[invalid".to_string()], true);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid regex"));
    }

    #[test_case(
        "github__get_me",
        serde_json::json!({"type": "object", "properties": {}}),
        None,
        "github[\"get_me\"]({}): string - Get details of the authenticated user";
        "no params, no output schema"
    )]
    #[test_case(
        "filesystem__read_text_file",
        serde_json::json!({"type": "object", "properties": {"path": {"type": "string"}, "tail": {"type": "number"}, "head": {"type": "number"}}, "required": ["path"]}),
        Some(serde_json::json!({"type": "object", "properties": {"content": {"type": "string"}}, "required": ["content"]})),
        "filesystem[\"read_text_file\"]({head?: number, path: string, tail?: number}): { content: string } - Read the complete contents of a file";
        "optional number params, object output"
    )]
    #[test_case(
        "memory__create_entities",
        serde_json::json!({"type": "object", "properties": {"entities": {"type": "array", "items": {"type": "object", "properties": {"name": {"type": "string"}, "entityType": {"type": "string"}, "observations": {"type": "array", "items": {"type": "string"}}}, "required": ["name", "entityType", "observations"]}}}, "required": ["entities"]}),
        Some(serde_json::json!({"type": "object", "properties": {"entities": {"type": "array", "items": {"type": "object", "properties": {"name": {"type": "string"}, "entityType": {"type": "string"}, "observations": {"type": "array", "items": {"type": "string"}}}, "required": ["name", "entityType", "observations"]}}}, "required": ["entities"]})),
        "memory[\"create_entities\"]({entities: { entityType: string, name: string, observations: string[] }[]}): { entities: { entityType: string, name: string, observations: string[] }[] } - Create multiple new entities";
        "nested object array with typed props"
    )]
    #[test_case(
        "github__dismiss_notification",
        serde_json::json!({"type": "object", "properties": {
            "threadID": {"type": "string"},
            "state": {"type": "string", "enum": ["read", "done"]}
        }, "required": ["threadID", "state"]}),
        None,
        "github[\"dismiss_notification\"]({state: \"read\" | \"done\", threadID: string}): string - Dismiss a notification";
        "enum param, no output schema"
    )]
    #[test_case(
        "computercontroller__web_scrape",
        serde_json::json!({"type": "object", "properties": {
            "url": {"type": "string"},
            "save_as": {"oneOf": [{"const": "text"}, {"const": "json"}, {"const": "binary"}]}
        }, "required": ["url"]}),
        None,
        "computercontroller[\"web_scrape\"]({save_as?: \"text\" | \"json\" | \"binary\", url: string}): string - Scrape content from URL";
        "oneOf const param (schemars), no output schema"
    )]
    #[test_case(
        "kiwitravel__search-flight",
        serde_json::json!({"type": "object", "properties": {
            "flyFrom": {"type": "string"},
            "flyTo": {"type": "string"},
            "departureDate": {"type": "string"}
        }, "required": ["flyFrom", "flyTo", "departureDate"]}),
        None,
        "kiwitravel[\"search-flight\"]({departureDate: string, flyFrom: string, flyTo: string}): string - Search for flights";
        "hyphenated tool name uses bracket notation"
    )]
    fn test_mcp_tool_signature(
        name: &str,
        input: serde_json::Value,
        output: Option<serde_json::Value>,
        expected: &str,
    ) {
        let input_schema: serde_json::Map<String, serde_json::Value> =
            serde_json::from_value(input).unwrap();
        let output_schema = output.map(|v| {
            Arc::new(
                serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(v).unwrap(),
            )
        });
        let desc = expected.split(" - ").nth(1).unwrap_or("").to_string();
        let tool = McpTool {
            name: name.to_string().into(),
            title: None,
            description: Some(desc.into()),
            input_schema: Arc::new(input_schema),
            output_schema,
            annotations: None,
            icons: None,
            meta: None,
        };
        let info = ToolInfo::from_mcp_tool(&tool).unwrap();
        assert_eq!(info.to_signature(), expected);
    }

    #[test_case(serde_json::json!({"type": "string"}), "string"; "string")]
    #[test_case(serde_json::json!({"type": "number"}), "number"; "number")]
    #[test_case(serde_json::json!({"type": "boolean"}), "boolean"; "boolean")]
    #[test_case(serde_json::json!({"type": "array"}), "array"; "array bare")]
    #[test_case(serde_json::json!({"type": "array", "items": {"type": "string"}}), "string[]"; "array with items")]
    #[test_case(serde_json::json!({"type": "object"}), "object"; "object bare")]
    #[test_case(serde_json::json!({"type": "object", "properties": {"a": {"type": "string"}}, "required": ["a"]}), "{ a: string }"; "object with prop")]
    #[test_case(serde_json::json!({"type": "object", "properties": {"a": {"type": "string"}}}), "{ a?: string }"; "object optional prop")]
    #[test_case(serde_json::json!({"type": "object", "properties": {"a": {"type": "array", "items": {"type": "string"}}}, "required": ["a"]}), "{ a: string[] }"; "object with array prop")]
    #[test_case(serde_json::json!({"enum": ["a", "b"]}), "\"a\" | \"b\""; "enum array")]
    #[test_case(serde_json::json!({"oneOf": [{"const": "x"}, {"const": "y"}]}), "\"x\" | \"y\""; "oneOf const")]
    fn test_extract_type_from_schema(schema: serde_json::Value, expected: &str) {
        assert_eq!(
            extract_type_from_schema(&schema),
            Some(expected.to_string())
        );
    }

    fn eval_with_tools(code: &str, tools: &[(&str, &str)]) -> String {
        let mut ctx = Context::default();
        for &(name, response) in tools {
            let resp = response.to_string();
            let func = NativeFunction::from_copy_closure_with_captures(
                |_this, _args, resp: &String, ctx| Ok(parse_result_to_js(resp, ctx)),
                resp,
            );
            ctx.register_global_callable(js_string!(name), 0, func)
                .unwrap();
        }
        ctx.eval(Source::from_bytes(code))
            .unwrap()
            .display()
            .to_string()
    }

    #[test_case("2 + 2", &[], "4"; "pure_js")]
    #[test_case("get_data({}).content", &[("get_data", r#"{"content":"hello"}"#)], "\"hello\""; "structured_property_access")]
    #[test_case("typeof shell({})", &[("shell", "plain text")], "\"string\""; "plain_text_is_string")]
    #[test_case("shell({}).content", &[("shell", "plain text")], "undefined"; "plain_text_no_property")]
    fn test_tool_result(code: &str, tools: &[(&str, &str)], expected: &str) {
        assert_eq!(eval_with_tools(code, tools), expected);
    }

    fn one_tool(server: &str, tool: &str) -> Vec<ToolInfo> {
        server_tools(server, &[tool])
    }

    /// A server exposing several tools. Needed because the interesting cases for
    /// issue #93 are about how tools on the SAME server interact with the
    /// server-name export — a one-tool fixture cannot express them.
    fn server_tools(server: &str, tools: &[&str]) -> Vec<ToolInfo> {
        tools
            .iter()
            .map(|tool| ToolInfo {
                server_name: server.to_string(),
                tool_name: (*tool).to_string(),
                full_name: format!("{server}__{tool}"),
                description: "A tool".to_string(),
                params: vec![],
                return_type: "string".to_string(),
            })
            .collect()
    }

    /// An unknown module specifier must name the module that failed AND the ones
    /// that would have worked. `MapModuleLoader`'s bare "Module could not be
    /// found." gave the model nothing to correct against, so it guessed again —
    /// the observed `fs` → `node:child_process` escalation.
    #[test]
    fn unknown_module_import_names_the_module_and_lists_the_available_ones() {
        let tools = one_tool("developer", "shell");
        let (tx, _rx) = mpsc::unbounded_channel();

        let error = run_js_module(r#"import fs from "fs"; record_result(1);"#, &tools, tx)
            .expect_err("importing a non-existent module must fail");

        assert!(
            error.contains(r#"Module "fs" could not be found"#),
            "error must name the failing module, got: {error}"
        );
        assert!(
            error.contains("Importable modules are exactly: developer"),
            "error must list the importable modules, got: {error}"
        );
        assert!(
            error.contains("search_modules"),
            "error must point at the recovery path, got: {error}"
        );
    }

    /// Node/browser builtins are the guesses that actually occur in the wild, so
    /// they get a named correction rather than a bare inventory.
    #[test]
    fn node_builtin_import_is_corrected_towards_the_developer_module() {
        let tools = one_tool("developer", "shell");
        let (tx, _rx) = mpsc::unbounded_channel();

        let error = run_js_module(
            r#"import { spawnSync } from "node:child_process"; record_result(1);"#,
            &tools,
            tx,
        )
        .expect_err("node builtins are not importable");

        assert!(
            error.contains("no Node.js or browser standard library"),
            "error must say the stdlib is absent, got: {error}"
        );
        assert!(
            error.contains(r#"import { shell, text_editor } from "developer""#),
            "error must show the correct primitive, got: {error}"
        );
    }

    /// A case-only miss is a different mistake from an invented name.
    #[test]
    fn case_mismatched_module_import_suggests_the_real_name() {
        let tools = one_tool("developer", "shell");
        let (tx, _rx) = mpsc::unbounded_channel();

        let error = run_js_module(r#"import { shell } from "Developer";"#, &tools, tx)
            .expect_err("module names are case-sensitive");

        assert!(
            error.contains(r#"Did you mean "developer"?"#),
            "error must suggest the correctly-cased name, got: {error}"
        );
    }

    /// `not a callable function` is Boa's message for calling any non-function,
    /// and it names neither the value nor the call site. A JSON result is one
    /// possible cause, not a diagnosis the engine actually made.
    #[test]
    fn calling_a_string_method_on_a_json_tool_result_explains_itself() {
        let tools = one_tool("developer", "shell");
        let (tx, mut rx) = mpsc::unbounded_channel::<ToolCallRequest>();

        // Answer the sandbox's tool call with JSON, which is parsed into an object.
        std::thread::spawn(move || {
            while let Some((_name, _args, responder)) = rx.blocking_recv() {
                let _ = responder.send(Ok(r#"{"output":"a\nb"}"#.to_string()));
            }
        });

        let error = run_js_module(
            r#"import { shell } from "developer"; record_result(shell({}).trim());"#,
            &tools,
            tx,
        )
        .expect_err("string methods do not exist on a parsed JSON result");

        assert!(
            error.contains(BOA_NOT_CALLABLE),
            "the engine's own message must survive, got: {error}"
        );
        assert!(
            error.contains("JSON results are parsed objects"),
            "error must explain the JSON-result possibility, got: {error}"
        );
        assert!(
            error.contains("imported module member") && error.contains("One possible cause"),
            "error must distinguish module-member and result-shape causes, got: {error}"
        );
        assert!(
            error.contains("JSON.stringify"),
            "error must name the recovery, got: {error}"
        );
    }

    #[test]
    fn calling_a_plain_object_gets_multi_cause_guidance() {
        let tools = one_tool("developer", "shell");
        let (tx, _rx) = mpsc::unbounded_channel();

        let error = run_js_module(
            r#"const moduleMember = {}; record_result(moduleMember());"#,
            &tools,
            tx,
        )
        .expect_err("an object is not callable");

        assert!(error.contains(BOA_NOT_CALLABLE), "got: {error}");
        assert!(
            error.contains("Check each callee with typeof")
                && error.contains("imported module member")
                && error.contains("One possible cause is a tool result"),
            "the hint must offer checks and possibilities without inventing one cause: {error}"
        );
    }

    /// Errors the engine already describes well must pass through untouched.
    #[test]
    fn unrelated_js_errors_are_not_annotated() {
        let message = "TypeError: cannot read properties of undefined";
        assert_eq!(annotate_opaque_js_error(message), message);
    }

    // ---- issue #23: parse-error self-correction ------------------------------

    /// The distinctive fragment of the recovery hint appended by
    /// [`annotate_parse_error`]; asserting on it keeps the tests stable if the
    /// surrounding wording is polished.
    const PARSE_HINT_MARKER: &str = "String.raw does NOT make ${…} literal";

    /// Transcript occurrence 1: bash indirect expansion inside `String.raw`.
    /// Boa's message names the template literal, so the message trigger fires.
    #[test]
    fn bash_expansion_in_string_raw_gets_the_escape_hint() {
        let code = r#"const s = String.raw`for v in A B; do echo "${!v:-}"; done`;"#;
        let message = "SyntaxError: expected token '}', got ':' in template literal at line 1";
        let annotated = annotate_parse_error(code, message);
        assert!(
            annotated.starts_with(message),
            "the engine's own message must survive, got: {annotated}"
        );
        assert!(
            annotated.contains(PARSE_HINT_MARKER),
            "hint must correct the String.raw belief, got: {annotated}"
        );
        assert!(
            annotated.contains(r#"${"$"}{VAR}"#),
            "hint must teach the literal dollar-brace escape, got: {annotated}"
        );
        assert!(
            annotated.contains("developer/text_editor") && annotated.contains("developer/shell"),
            "hint must offer the write-to-file recovery, got: {annotated}"
        );
    }

    /// Transcript occurrence 2 class: a payload backtick (markdown fence)
    /// terminated the literal early and CSS landed in JS context. The message
    /// no longer mentions templates, but the source visibly uses String.raw.
    #[test]
    fn css_payload_in_object_position_still_gets_the_hint() {
        let code = "const doc = String.raw`# Theme\n```css\n@media (prefers-color-scheme: dark) { body { background: black } }\n```\n`;";
        let message =
            "SyntaxError: expected one of ',' or '}', got 'prefers' in object literal at line 3";
        let annotated = annotate_parse_error(code, message);
        assert!(annotated.contains(PARSE_HINT_MARKER), "got: {annotated}");
        assert!(
            annotated.contains("backtick inside the payload terminates the literal"),
            "hint must explain the early-termination mechanism, got: {annotated}"
        );
    }

    /// Transcript occurrences 3/4: heredoc quoting → unterminated string
    /// literal. No String.raw in source, but the heredoc marker corroborates
    /// that a multi-line payload is embedded in the string.
    #[test]
    fn unterminated_string_literal_message_gets_the_hint() {
        let code = "const cmd = 'python3 - <<PY\nprint(1)\nPY';";
        let message = "SyntaxError: unterminated string literal at line 1";
        let annotated = annotate_parse_error(code, message);
        assert!(annotated.contains(PARSE_HINT_MARKER), "got: {annotated}");
    }

    /// A plain backtick template (no String.raw) with bash parameter expansion
    /// triggers on the source pattern alone.
    #[test]
    fn bare_template_with_bash_expansion_triggers_on_source() {
        let code = "const s = `echo ${HOME:-/tmp}`;";
        let message = "SyntaxError: expected token '}', got ':' at line 1, col 18";
        let annotated = annotate_parse_error(code, message);
        assert!(annotated.contains(PARSE_HINT_MARKER), "got: {annotated}");
    }

    /// Parse errors unrelated to embedded payloads must pass through untouched
    /// — no hint noise on an ordinary typo.
    #[test]
    fn unrelated_parse_errors_are_not_annotated() {
        let code = "record_result( this is not valid js )";
        let message = "SyntaxError: expected token ')', got 'is' at line 1, col 20";
        assert_eq!(annotate_parse_error(code, message), message);
    }

    /// Review follow-up on #23: VALID `String.raw` usage plus an unrelated
    /// syntax error on a later line must NOT earn the template hint — the
    /// error position sits outside the template span, so `String.raw`'s mere
    /// presence is not evidence.
    #[test]
    fn valid_string_raw_with_unrelated_error_is_not_annotated() {
        let code = "const path = String.raw`C:\\temp\\report.txt`;\nrecord_result( this is not valid js );";
        let message = "SyntaxError: expected token ')', got 'is' at line 2, col 22";
        assert_eq!(annotate_parse_error(code, message), message);
    }

    /// Same on ONE line: the reported column lands after the template's
    /// closing backtick, so the span check must still say "unrelated".
    #[test]
    fn valid_string_raw_same_line_unrelated_error_is_not_annotated() {
        let code = "const s = String.raw`ok`; record_result( this is not valid js );";
        let message = "SyntaxError: expected token ')', got 'is' at line 1, col 48";
        assert_eq!(annotate_parse_error(code, message), message);
    }

    /// A short missing-quote typo also reads "unterminated string literal",
    /// but carries no payload evidence (no heredoc, no template, no bash
    /// expansion) — it must not be blamed on template embedding.
    #[test]
    fn plain_unterminated_quote_typo_is_not_annotated() {
        let code = "const s = 'oops;\nrecord_result(s);";
        let message = "SyntaxError: unterminated string literal at line 1, col 11";
        assert_eq!(annotate_parse_error(code, message), message);
    }

    /// The position extractor understands both parser (`col`) and lexer
    /// (`column`) forms, tolerates a missing column, and stays quiet on a
    /// message with no position.
    #[test]
    fn parse_error_position_handles_boa_forms() {
        assert_eq!(
            parse_error_position("expected token ')', got 'is' at line 3, col 7"),
            Some((3, Some(7)))
        );
        assert_eq!(
            parse_error_position("unexpected 'x' at line 12, column 4"),
            Some((12, Some(4)))
        );
        assert_eq!(
            parse_error_position("got 'prefers' in object literal at line 3"),
            Some((3, None))
        );
        assert_eq!(parse_error_position("no position here"), None);
    }

    /// `${VAR}` alone is valid JS (a substitution) — the bash-shape regex must
    /// not fire on it, nor on scripts with no template at all.
    #[test]
    fn plain_js_substitution_is_not_treated_as_bash() {
        assert!(!bash_param_expansion_regex().is_match("const s = `count: ${n}`;"));
        assert!(bash_param_expansion_regex().is_match(r#"`echo "${!v:-}"`"#));
        assert!(bash_param_expansion_regex().is_match("`echo ${#arr}`"));
        assert!(bash_param_expansion_regex().is_match("`echo ${VAR%.txt}`"));
        assert!(bash_param_expansion_regex().is_match("`echo ${VAR:-default}`"));
    }

    /// The message is assembled without a JS context so its exact wording is
    /// pinned independently of the engine.
    #[test]
    fn module_not_found_message_lists_every_available_module() {
        let available = vec!["computercontroller".to_string(), "developer".to_string()];
        let message = module_not_found_message("path", &available);

        assert!(message.starts_with(r#"Module "path" could not be found."#));
        assert!(message.contains("Importable modules are exactly: computercontroller, developer"));

        let empty = module_not_found_message("developer", &[]);
        assert!(
            empty.contains("No modules are importable in this session."),
            "the no-modules case must say so rather than print an empty list, got: {empty}"
        );
    }

    /// Redirecting a stdlib miss to "developer" is only useful advice when
    /// "developer" is importable. code_execution is force-injected as a platform
    /// extension while developer is an ordinary one the user can switch off, so
    /// the two can genuinely come apart — and telling the model to import a
    /// module that will also miss is the retry loop this message exists to break.
    #[test]
    fn the_developer_redirect_is_only_offered_when_developer_is_importable() {
        let with_developer =
            module_not_found_message("fs", &["developer".to_string(), "memory".to_string()]);
        assert!(
            with_developer.contains(r#"import { shell, text_editor } from "developer""#),
            "the redirect must still be offered when developer is loaded, got: {with_developer}"
        );

        let without_developer = module_not_found_message("fs", &["memory".to_string()]);
        assert!(
            without_developer.contains("no Node.js or browser standard library"),
            "the diagnosis of WHY it missed is still useful, got: {without_developer}"
        );
        assert!(
            !without_developer.contains(r#"from "developer""#),
            "must not send the model at a module it cannot import, got: {without_developer}"
        );
    }

    /// A registered module must still resolve — the improved miss path must not
    /// have broken the hit path.
    #[test]
    fn a_registered_module_still_resolves() {
        let tools = one_tool("developer", "shell");
        let (tx, _rx) = mpsc::unbounded_channel();

        let result = run_js_module(
            r#"import { shell } from "developer"; record_result(typeof shell);"#,
            &tools,
            tx,
        );

        assert_eq!(result.as_deref(), Ok("\"function\""), "got {result:?}");
    }

    /// Every documented import form must yield a *callable* value. The older
    /// namespace test only asserted `run_js_module` returned `Ok`, which
    /// `typeof` satisfies even when the binding is not a function — so a
    /// "not a callable function" regression could pass it.
    #[test_case(r#"import { shell } from "developer"; record_result(typeof shell);"#; "named")]
    #[test_case(r#"import * as developer from "developer"; record_result(typeof developer.shell);"#; "namespace")]
    #[test_case(r#"import { developer } from "developer"; record_result(typeof developer.shell);"#; "server_named")]
    #[test_case(r#"import * as developer from "developer"; record_result(typeof developer["shell"]);"#; "bracket")]
    fn every_import_form_yields_a_callable_tool(code: &str) {
        let tools = one_tool("developer", "shell");
        let (tx, _rx) = mpsc::unbounded_channel();

        let result = run_js_module(code, &tools, tx);

        assert_eq!(
            result.as_deref(),
            Ok("\"function\""),
            "import form did not produce a callable tool: {result:?}"
        );
    }

    /// ⚠ Issue #93. The twin of the test above, on a server whose name EQUALS
    /// its tool's name — the shape `chatrecall` has, and the ONLY shape that
    /// triggers the export collision.
    ///
    /// The test above cannot catch it and never could: `one_tool("developer",
    /// "shell")` gives two distinct export names, so the server-name export and
    /// the tool export land in different bindings. Every fixture in this file
    /// was that shape, which is why a defect present since the initial commit
    /// went unseen until a user enabled Chat Recall.
    #[test_case(r#"import { chatrecall } from "chatrecall"; record_result(typeof chatrecall);"#; "named")]
    #[test_case(r#"import * as ns from "chatrecall"; record_result(typeof ns.chatrecall);"#; "namespace")]
    #[test_case(r#"import * as ns from "chatrecall"; record_result(typeof ns["chatrecall"]);"#; "bracket")]
    #[test_case(r#"import { chatrecall as srv } from "chatrecall"; record_result(typeof srv.chatrecall);"#; "server_named")]
    fn every_import_form_yields_a_callable_tool_when_the_server_shares_its_name(code: &str) {
        let tools = one_tool("chatrecall", "chatrecall");
        let (tx, _rx) = mpsc::unbounded_channel();

        let result = run_js_module(code, &tools, tx);

        assert_eq!(
            result.as_deref(),
            Ok("\"function\""),
            "a server named after its own tool must still export a callable: {result:?}"
        );
    }

    /// The shadowing is per-TOOL, not per-server: on a multi-tool server only
    /// the colliding tool was eaten, so the failure read as "one flaky tool"
    /// rather than "this extension is broken". Both must be callable, and the
    /// siblings must be reachable through the colliding name as well, since that
    /// is what the server-named import form resolves to.
    #[test]
    fn a_colliding_tool_does_not_shadow_its_siblings() {
        let tools = server_tools("fetch", &["fetch", "fetch_html"]);
        let (tx, _rx) = mpsc::unbounded_channel();

        let result = run_js_module(
            r#"import * as ns from "fetch";
record_result([typeof ns.fetch, typeof ns.fetch_html, typeof ns.fetch.fetch_html].join(","));"#,
            &tools,
            tx,
        );

        assert_eq!(
            result.as_deref(),
            Ok("\"function,function,function\""),
            "a colliding tool must stay callable and must still carry its siblings: {result:?}"
        );
    }

    /// ⚠ Why the namespace properties are defined with
    /// `create_data_property_or_throw` and not `set`.
    ///
    /// On a collision the namespace object IS a function, and a function has
    /// non-writable own `name`/`length` and poisoned `caller`/`arguments`
    /// accessors. Under `set` semantics a sibling tool called `name` silently
    /// resolved to the function's own name STRING, and one called `caller`
    /// made the entire module fail to construct — a whole-extension outage
    /// caused by a tool's name. `defineProperty` ignores writability and
    /// setters, so the tool wins in every case.
    ///
    /// These names are not hypothetical for a third-party MCP server, and the
    /// cost of getting them wrong is silent (`name`) or total (`caller`).
    #[test_case("name"; "function_own_name")]
    #[test_case("length"; "function_own_length")]
    #[test_case("caller"; "poisoned_caller")]
    #[test_case("arguments"; "poisoned_arguments")]
    #[test_case("prototype"; "function_prototype")]
    #[test_case("constructor"; "inherited_constructor")]
    #[test_case("toString"; "inherited_tostring")]
    #[test_case("__proto__"; "proto_accessor")]
    fn a_tool_named_after_a_function_property_still_wins_on_a_colliding_server(tool: &str) {
        let tools = server_tools("srv", &["srv", tool]);
        let (tx, _rx) = mpsc::unbounded_channel();

        let code = format!(
            r#"import * as ns from "srv"; import {{ srv }} from "srv";
record_result([typeof ns["{tool}"], typeof srv["{tool}"], typeof ns.srv].join(","));"#
        );
        let result = run_js_module(&code, &tools, tx);

        assert_eq!(
            result.as_deref(),
            Ok("\"function,function,function\""),
            "tool `{tool}` was lost to a Function property on the namespace: {result:?}"
        );
    }

    /// The colliding server's namespace must NOT be the tool's own function.
    ///
    /// This pins the structural half of that choice: the namespace carries the
    /// siblings, the underlying tool does not. The half that actually matters to
    /// a script is pinned by `a_colliding_namespace_still_serialises_as_json` —
    /// reusing the tool's function makes the namespace self-referential and
    /// costs `record_result` its JSON. Note what is NOT asserted here: the value
    /// `import { fetch }` yields is the namespace, so it carries the siblings
    /// under either shape.
    #[test]
    fn a_colliding_server_does_not_pollute_the_tools_own_function() {
        let tools = server_tools("fetch", &["fetch", "fetch_html"]);
        let (tx, _rx) = mpsc::unbounded_channel();

        let result = run_js_module(
            r#"import { fetch_html } from "fetch"; import * as ns from "fetch";
record_result([
  Object.getOwnPropertyNames(ns.fetch.fetch).join("|"),
  Object.getOwnPropertyNames(ns.fetch).join("|"),
].join(" // "));"#,
            &tools,
            tx,
        );

        assert_eq!(
            result.as_deref(),
            Ok("\"length|name // length|name|fetch|fetch_html\""),
            "the namespace must carry the siblings and the underlying tool must not: {result:?}"
        );
    }

    /// A script may put the namespace in its result. That must still serialise.
    ///
    /// `record_result` uses `JsValue::to_json`, whose cycle detection returns
    /// `Err`, and the `.ok()` there swallows it and falls back to boa's debug
    /// rendering. A namespace that held itself therefore came back as
    /// `Function { … [Cycle] }` instead of JSON — silently, and only on the
    /// colliding path.
    #[test]
    fn a_colliding_namespace_still_serialises_as_json() {
        let tools = one_tool("chatrecall", "chatrecall");
        let (tx, _rx) = mpsc::unbounded_channel();

        let result = run_js_module(
            r#"import * as ns from "chatrecall"; record_result({ held: ns.chatrecall });"#,
            &tools,
            tx,
        );

        let out = result.expect("the module must evaluate");
        assert!(
            !out.contains("Cycle"),
            "the namespace is self-referential, so `record_result` lost JSON: {out}"
        );
        assert!(
            out.starts_with('{'),
            "expected a JSON object, got boa's debug rendering: {out}"
        );
    }

    /// The degenerate case: the colliding name is ALSO a Function property.
    #[test]
    fn a_server_and_tool_both_named_name_still_export_a_callable() {
        let tools = one_tool("name", "name");
        let (tx, _rx) = mpsc::unbounded_channel();

        let result = run_js_module(
            r#"import * as ns from "name"; record_result(typeof ns.name);"#,
            &tools,
            tx,
        );

        assert_eq!(result.as_deref(), Ok("\"function\""), "got {result:?}");
    }

    /// A colliding tool must still DISPATCH under its fully-qualified MCP name.
    /// Making the namespace object double as the function must not disturb the
    /// `server__tool` routing `create_tool_function` closes over — otherwise the
    /// call would reach the wrong tool, which is worse than not reaching one.
    #[test]
    fn a_colliding_tool_dispatches_under_its_full_mcp_name() {
        let tools = one_tool("chatrecall", "chatrecall");
        let (tx, mut rx) = mpsc::unbounded_channel();

        let handle = std::thread::spawn(move || {
            run_js_module(
                r#"import { chatrecall } from "chatrecall"; record_result(chatrecall({ query: "x" }));"#,
                &tools,
                tx,
            )
        });

        let (full_name, args_json, responder) = rx.blocking_recv().expect("a tool call");
        assert_eq!(
            full_name, "chatrecall__chatrecall",
            "the colliding tool must dispatch under its prefixed MCP name"
        );
        assert!(
            args_json.contains("\"query\""),
            "arguments were not forwarded: {args_json}"
        );
        responder.send(Ok("ok".to_string())).unwrap();

        let result = handle.join().unwrap();
        assert_eq!(result.as_deref(), Ok("\"ok\""), "got {result:?}");
    }

    #[test]
    fn test_namespace_import_with_synthetic_module() {
        let tools = vec![ToolInfo {
            server_name: "testserver".to_string(),
            tool_name: "get_value".to_string(),
            full_name: "testserver__get_value".to_string(),
            description: "Get a value".to_string(),
            params: vec![],
            return_type: "string".to_string(),
        }];

        let (tx, _rx) = mpsc::unbounded_channel();

        let code_named = r#"import { get_value } from "testserver"; typeof get_value"#;
        let result = run_js_module(code_named, &tools, tx.clone());
        assert!(
            result.is_ok(),
            "Named import should work: {:?}",
            result.err()
        );

        let code_namespace =
            r#"import * as testserver from "testserver"; typeof testserver.get_value"#;
        let result = run_js_module(code_namespace, &tools, tx.clone());
        assert!(
            result.is_ok(),
            "Namespace import should work: {:?}",
            result.err()
        );

        let code_server_named =
            r#"import { testserver } from "testserver"; typeof testserver.get_value"#;
        let result = run_js_module(code_server_named, &tools, tx.clone());
        assert!(
            result.is_ok(),
            "Server-named import should work: {:?}",
            result.err()
        );

        let code_bracket =
            r#"import { testserver } from "testserver"; typeof testserver["get_value"]"#;
        let result = run_js_module(code_bracket, &tools, tx);
        assert!(
            result.is_ok(),
            "Bracket notation should work: {:?}",
            result.err()
        );
    }
}

#[cfg(test)]
mod gate_c_bridge_tests {
    //! Issue #56 Gate C, path 3 of the four that converge on
    //! `ExtensionManager::dispatch_tool_call`.
    //!
    //! The `execute_code` bridge re-enters the **ExtensionManager's** dispatch
    //! from inside a running tool — not the Agent's — so it carries no
    //! `ToolInspector` and an inspector-shaped Gate C would be invisible to it.
    //! It lives here rather than beside the other three paths
    //! (`agents::agent::gate_c_dispatch_tests`) only because
    //! `dispatch_sub_call` is private to this module.

    use super::*;

    async fn manager_with_the_private_extension(
        dir: &std::path::Path,
    ) -> Arc<crate::agents::ExtensionManager> {
        let manager = Arc::new(crate::agents::ExtensionManager::new(
            Arc::new(Mutex::new(None)),
            Arc::new(crate::session::SessionManager::new(dir.to_path_buf())),
        ));
        manager
            .add_inprocess_server(
                "ucsfomopagent",
                biorouter_mcp::datasql::server::DataSqlServer::new(std::collections::HashMap::new()),
            )
            .await
            .expect("inject the private extension");
        manager
    }

    #[tokio::test]
    async fn the_execute_code_bridge_cannot_reach_a_private_extension() {
        let dir = tempfile::tempdir().unwrap();
        let manager = manager_with_the_private_extension(dir.path()).await;
        let weak = Arc::downgrade(&manager);
        let artifacts = Arc::new(Mutex::new(CollectedArtifacts::default()));

        let (result, kind, _user) = CodeExecutionClient::dispatch_sub_call(
            "gate-c",
            crate::privacy::CallCapability::for_test(crate::privacy::ProviderTier::Public, true),
            "ucsfomopagent__data_sources",
            "{}",
            Some(&weak),
            &artifacts,
            &CancellationToken::default(),
        )
        .await;

        assert_eq!(kind, "dispatch_error");
        let text =
            result.expect_err("a script must not reach a private extension from a public model");
        // The WHOLE refusal: `Tool '…' not found` also names the extension, so
        // asserting on the name alone would pass on a fixture that never loaded
        // it.
        let refusal = crate::privacy::refusal::privacy_refusal(
            "ucsfomopagent",
            crate::privacy::ProviderTier::Private,
            crate::privacy::ProviderTier::Public,
        )
        .expect("the pure refusal")
        .message
        .to_string();
        assert!(text.contains(&refusal), "{text}");
        assert!(!text.contains("The user has declined"), "{text}");
    }

    /// The other direction: the bridge is not simply broken for this extension.
    #[tokio::test]
    async fn a_private_script_still_reaches_it_through_the_bridge() {
        let dir = tempfile::tempdir().unwrap();
        let manager = manager_with_the_private_extension(dir.path()).await;
        let weak = Arc::downgrade(&manager);
        let artifacts = Arc::new(Mutex::new(CollectedArtifacts::default()));

        let (result, kind, _user) = CodeExecutionClient::dispatch_sub_call(
            "gate-c",
            crate::privacy::CallCapability::for_test(crate::privacy::ProviderTier::Private, true),
            "ucsfomopagent__data_sources",
            "{}",
            Some(&weak),
            &artifacts,
            &CancellationToken::default(),
        )
        .await;

        assert_ne!(kind, "dispatch_error", "{result:?}");
        result.expect("a private model may call a private extension from a script");
    }
}
