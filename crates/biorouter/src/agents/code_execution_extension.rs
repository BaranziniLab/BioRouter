use crate::agents::extension::PlatformExtensionContext;
use crate::agents::extension_manager::get_parameter_names;
use crate::agents::mcp_client::{Error, McpClientTrait, McpMeta};
use anyhow::Result;
use async_trait::async_trait;
use boa_engine::builtins::promise::PromiseState;
use boa_engine::module::{MapModuleLoader, Module, SyntheticModuleInitializer};
use boa_engine::{js_string, Context, JsNativeError, JsString, JsValue, NativeFunction, Source};
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
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

pub static EXTENSION_NAME: &str = "code_execution";
const MAX_MODULE_SEARCH_RESULTS: usize = 12;

type ToolCallRequest = (
    String,
    String,
    tokio::sync::oneshot::Sender<Result<String, String>>,
);

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
            "{}[\"{}\"]({{{params}}}): {} - {desc}",
            module_name, self.tool_name, self.return_type
        )
    }
}

fn module_search_alias(server_name: &str) -> String {
    let safe_name = server_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("module_{safe_name}")
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

fn create_server_module(
    server_name: &str,
    server_tools: &[&ToolInfo],
    ctx: &mut Context,
) -> Module {
    let tool_data: Vec<(String, String)> = server_tools
        .iter()
        .map(|t| (t.tool_name.clone(), t.full_name.clone()))
        .collect();

    let mut export_names: Vec<JsString> = server_tools
        .iter()
        .map(|t| js_string!(t.tool_name.as_str()))
        .collect();
    export_names.push(js_string!(server_name));

    let server_name_owned = server_name.to_string();

    Module::synthetic(
        &export_names,
        SyntheticModuleInitializer::from_copy_closure_with_captures(
            |module, (tools, server_name), context| {
                let namespace_obj = boa_engine::JsObject::with_null_proto();

                for (tool_name, full_name) in tools.iter() {
                    let func = create_tool_function(full_name.clone());
                    let js_func = func.to_js_function(context.realm());
                    module.set_export(&js_string!(tool_name.as_str()), js_func.clone().into())?;
                    namespace_obj
                        .set(js_string!(tool_name.as_str()), js_func, false, context)
                        .map_err(|e| {
                            JsNativeError::error().with_message(format!("Failed to set prop: {e}"))
                        })?;
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

fn parse_result_to_js(result: &str, ctx: &mut Context) -> JsValue {
    serde_json::from_str::<serde_json::Value>(result)
        .ok()
        .and_then(|v| JsValue::from_json(&v, ctx).ok())
        .unwrap_or_else(|| JsValue::from(js_string!(result)))
}

fn create_tool_function(full_tool_name: String) -> NativeFunction {
    NativeFunction::from_copy_closure_with_captures(
        |_this, args, full_name: &String, ctx| {
            let args_json = args
                .first()
                .cloned()
                .unwrap_or(JsValue::undefined())
                .to_json(ctx)
                .map_err(|e| JsNativeError::error().with_message(e.to_string()))?
                .unwrap_or(Value::Object(serde_json::Map::new()));

            let args_str = serde_json::to_string(&args_json).unwrap_or_else(|_| "{}".to_string());
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
    CALL_TX.with(|tx| *tx.borrow_mut() = Some(call_tx));
    RESULT_CELL.with(|cell| *cell.borrow_mut() = None);

    let loader = Rc::new(MapModuleLoader::new());
    let mut ctx = Context::builder()
        .module_loader(loader.clone())
        .build()
        .map_err(|e| format!("Failed to create JS context: {e}"))?;

    let record_result = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let value = args.first().cloned().unwrap_or(JsValue::undefined());
        let fallback = || value.display().to_string();
        let result_str = value
            .to_json(ctx)
            .ok()
            .flatten()
            .map(|v| serde_json::to_string_pretty(&v).unwrap_or_else(|_| fallback()))
            .unwrap_or_else(fallback);
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

    let user_module = Module::parse(Source::from_bytes(code), None, &mut ctx)
        .map_err(|e| format!("Parse error: {e}"))?;
    loader.insert("__main__", user_module.clone());

    let promise = user_module.load_link_evaluate(&mut ctx);
    ctx.run_jobs()
        .map_err(|e| format!("Job execution error: {e}"))?;

    match promise.state() {
        PromiseState::Fulfilled(_) => {
            let result = RESULT_CELL.with(|cell| cell.borrow().clone());
            Ok(result.unwrap_or_else(|| "undefined".to_string()))
        }
        PromiseState::Rejected(err) => Err(format!("Module error: {}", err.display())),
        PromiseState::Pending => Err("Module evaluation did not complete".to_string()),
    }
}

pub struct CodeExecutionClient {
    info: InitializeResult,
    context: PlatformExtensionContext,
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
                BATCH MULTIPLE TOOL CALLS INTO ONE execute_code CALL.

                This extension exists to reduce round-trips. When a task requires multiple tool calls:
                - WRONG: Multiple execute_code calls, each with one tool
                - RIGHT: One execute_code call with a script that calls all needed tools

                IMPORTANT: All tool calls are SYNCHRONOUS. Do NOT use async/await.

                Workflow:
                    1. If you do not know the tool, call search_modules once. Its results include complete imports and signatures.
                    2. Use read_module only when you already know a module but need a tool that search_modules did not return.
                    3. Write ONE script that imports and calls ALL tools needed for the task.
                    4. Chain results: use output from one tool as input to the next.

                Never call read_module for a tool whose complete signature was returned by search_modules.
            "#}.to_string()),
        };

        Ok(Self { info, context })
    }

    async fn get_tool_infos(&self) -> Vec<ToolInfo> {
        let Some(manager) = self
            .context
            .extension_manager
            .as_ref()
            .and_then(|w| w.upgrade())
        else {
            return Vec::new();
        };

        match manager.get_prefixed_tools_excluding(EXTENSION_NAME).await {
            Ok(tools) if !tools.is_empty() => {
                tools.iter().filter_map(ToolInfo::from_mcp_tool).collect()
            }
            _ => Vec::new(),
        }
    }

    async fn handle_execute_code(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let code = arguments
            .as_ref()
            .and_then(|a| a.get("code"))
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: code")?
            .to_string();

        let tools = self.get_tool_infos().await;
        let collected_resources: Arc<Mutex<Vec<Content>>> = Arc::new(Mutex::new(Vec::new()));
        let (call_tx, call_rx) = mpsc::unbounded_channel();
        let tool_handler = tokio::spawn(Self::run_tool_handler(
            session_id.to_string(),
            call_rx,
            self.context.extension_manager.clone(),
            Arc::clone(&collected_resources),
        ));

        let js_result = tokio::task::spawn_blocking(move || run_js_module(&code, &tools, call_tx))
            .await
            .map_err(|e| format!("JS execution task failed: {e}"))?;

        tool_handler.abort();

        let mut contents = js_result.map(|r| vec![Content::text(format!("Result: {r}"))])?;
        // Append any resource content collected from tool calls (e.g. autovisualiser HTML charts).
        // The UI renders these inline when the resource URI uses the ui:// scheme.
        contents.extend(collected_resources.lock().await.drain(..));
        Ok(contents)
    }

    async fn handle_read_module(
        &self,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let path = arguments
            .as_ref()
            .and_then(|a| a.get("module_path"))
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: module_path")?;

        let tools = self.get_tool_infos().await;
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

        let use_regex = arguments
            .as_ref()
            .and_then(|a| a.get("regex"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let tools = self.get_tool_infos().await;
        Self::handle_search(&tools, &terms_vec, use_regex)
    }

    fn handle_search(
        tools: &[ToolInfo],
        terms: &[String],
        use_regex: bool,
    ) -> Result<Vec<Content>, String> {
        enum Matcher {
            Regex(Vec<Regex>),
            Plain(Vec<String>),
        }

        let matcher = if use_regex {
            let patterns: Result<Vec<_>, _> = terms
                .iter()
                .map(|t| {
                    Regex::new(&format!("(?i){t}")).map_err(|e| format!("Invalid regex '{t}': {e}"))
                })
                .collect();
            Matcher::Regex(patterns?)
        } else {
            Matcher::Plain(terms.iter().map(|t| t.to_lowercase()).collect())
        };

        let match_score = |tool: &ToolInfo| -> usize {
            match &matcher {
                Matcher::Regex(patterns) => patterns
                    .iter()
                    .map(|pattern| {
                        usize::from(pattern.is_match(&tool.tool_name)) * 20
                            + usize::from(pattern.is_match(&tool.server_name)) * 8
                            + usize::from(pattern.is_match(&tool.description)) * 4
                    })
                    .sum(),
                Matcher::Plain(terms) => {
                    let tool_name = tool.tool_name.to_lowercase();
                    let server_name = tool.server_name.to_lowercase();
                    let description = tool.description.to_lowercase();
                    terms
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
                        .sum()
                }
            }
        };

        let mut matching_tools = tools
            .iter()
            .filter_map(|tool| {
                let score = match_score(tool);
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
            return Err(format!("No matches found for: {}", terms.join(", ")));
        }

        let total_matches = matching_tools.len();
        matching_tools.truncate(MAX_MODULE_SEARCH_RESULTS);

        let output = render_module_search_results(&matching_tools, total_matches);
        Ok(vec![Content::text(output)])
    }

    async fn run_tool_handler(
        session_id: String,
        mut call_rx: mpsc::UnboundedReceiver<ToolCallRequest>,
        extension_manager: Option<std::sync::Weak<crate::agents::ExtensionManager>>,
        collected_resources: Arc<Mutex<Vec<Content>>>,
    ) {
        while let Some((tool_name, arguments, response_tx)) = call_rx.recv().await {
            let result = match extension_manager.as_ref().and_then(|w| w.upgrade()) {
                Some(manager) => {
                    let tool_call = CallToolRequestParams {
                        task: None,
                        name: tool_name.into(),
                        arguments: serde_json::from_str(&arguments).ok(),
                        meta: None,
                    };
                    match manager
                        .dispatch_tool_call(&session_id, tool_call, CancellationToken::new())
                        .await
                    {
                        Ok(dispatch_result) => match dispatch_result.result.await {
                            Ok(result) => {
                                let is_error = result.is_error.unwrap_or(false);
                                // Collect *binary/blob* resource content (e.g. autovisualiser
                                // ui:// HTML blobs) so handle_execute_code can append them to its
                                // result for inline UI rendering. These are not representable as JS
                                // strings, so they are passed out-of-band via the shared collector.
                                // Text resources (e.g. developer/text_editor file contents) are NOT
                                // collected here — they are surfaced to the script as text below,
                                // and appending them again would duplicate the payload.
                                let resources: Vec<Content> = result
                                    .content
                                    .iter()
                                    .filter(|c| {
                                        matches!(
                                            &c.raw,
                                            RawContent::Resource(r)
                                                if matches!(
                                                    r.resource,
                                                    ResourceContents::BlobResourceContents { .. }
                                                )
                                        )
                                    })
                                    .cloned()
                                    .collect();
                                let has_resources = !resources.is_empty();
                                if has_resources {
                                    collected_resources.lock().await.extend(resources);
                                }

                                let output = if let Some(sc) = &result.structured_content {
                                    serde_json::to_string(sc).unwrap_or_default()
                                } else {
                                    // Surface exactly what the model itself would see: content
                                    // targeted at the Assistant (or with no audience set), mirroring
                                    // `From<Content> for MessageContent`. This (a) drops the
                                    // duplicate User-audience copy some tools emit — e.g.
                                    // developer/shell returns the same text twice, which would
                                    // otherwise hand the script "40\n40" for `echo 40` — and (b)
                                    // unwraps embedded text resources (developer/text_editor returns
                                    // file contents as an Assistant-audience text resource).
                                    let text: String = result
                                        .content
                                        .iter()
                                        .filter(|c| {
                                            c.audience()
                                                .is_none_or(|a| a.contains(&Role::Assistant))
                                        })
                                        .filter_map(|c| match &c.raw {
                                            RawContent::Text(t) => Some(t.text.clone()),
                                            RawContent::Resource(r) => match &r.resource {
                                                ResourceContents::TextResourceContents {
                                                    text,
                                                    ..
                                                } => Some(text.clone()),
                                                ResourceContents::BlobResourceContents {
                                                    ..
                                                } => None,
                                            },
                                            _ => None,
                                        })
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    // When a tool (e.g. autovisualiser) returns only resource
                                    // content and no text, JS would receive an empty string and
                                    // the model would see `Result: ""` and loop retrying. Return
                                    // a terse confirmation so the model knows it succeeded.
                                    if text.is_empty() && has_resources {
                                        "[rendered inline]".to_string()
                                    } else {
                                        text
                                    }
                                };
                                if is_error {
                                    Err(format!("Tool error: {output}"))
                                } else {
                                    Ok(output)
                                }
                            }
                            Err(e) => Err(format!("Tool error: {}", e.message)),
                        },
                        Err(e) => Err(format!("Dispatch error: {e}")),
                    }
                }
                None => Err("Extension manager not available".to_string()),
            };
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
                        Batch multiple MCP tool calls into ONE execution. This is the primary purpose of this tool.

                        CRITICAL: Always combine related operations into a single execute_code call.
                        - WRONG: execute_code to read → execute_code to write (2 calls)
                        - RIGHT: execute_code that reads AND writes in one script (1 call)

                        EXAMPLE - Read file and write to another (ONE call):
                        ```javascript
                        import { text_editor } from "developer";
                        const content = text_editor({ path: "/path/to/source.md", command: "view" });
                        text_editor({ path: "/path/to/dest.md", command: "write", file_text: content });
                        record_result({ copied: true });
                        ```

                        EXAMPLE - Multiple operations chained:
                        ```javascript
                        import { shell, text_editor } from "developer";
                        const files = shell({ command: "ls -la" });
                        const readme = text_editor({ path: "./README.md", command: "view" });
                        const status = shell({ command: "git status" });
                        record_result({ files, readme, status });
                        ```

                        SYNTAX:
                        - Import: import { tool1, tool2 } from "serverName";
                        - Call: toolName({ param1: value, param2: value })
                        - Result: record_result(value) - call this to return a value from the script
                        - All calls are synchronous, return strings

                        MULTILINE SCRIPT ARGUMENTS:
                        - Use String.raw`...` when passing shell, Ruby, PowerShell, or other source text.
                        - Prefer one scripting language. Avoid nesting another interpreter unless the task requires it.
                        - String.raw preserves backslashes such as \n instead of turning them into accidental source-code newlines.

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
                        Search for tools by name or description across all available modules.

                        USAGE:
                        - Single term: terms="github" (just a plain string)
                        - Multiple terms: terms=["git", "shell"] (a JSON array, NOT a string)
                        - Regex patterns: terms="sh.*", regex=true

                        IMPORTANT: Do NOT stringify arrays. Use terms=["a","b"] not terms="[\"a\",\"b\"]"

                        Returns ranked tools with complete import syntax and parameter signatures, ready for execute_code.
                        Use this when you don't know which module contains the tool you need.
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
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let content = match name {
            "execute_code" => self.handle_execute_code(&meta.session_id, arguments).await,
            "read_module" => self.handle_read_module(arguments).await,
            "search_modules" => self.handle_search_modules(arguments).await,
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
        let tools = self.get_tool_infos().await;
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
                McpMeta::new("test-session-id"),
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
                McpMeta::new("test-session-id"),
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

        let result = client.handle_read_module(Some(args)).await;
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

        // Search with no matches
        let result =
            CodeExecutionClient::handle_search(&tools, &["nonexistent".to_string()], false);
        assert!(result.is_err());
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
