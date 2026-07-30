use indoc::formatdoc;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, ErrorCode, ErrorData, Implementation, ServerCapabilities,
        ServerInfo,
    },
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

/// Parameters for the remember_memory tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RememberMemoryParams {
    /// The category to store the memory in
    pub category: String,
    /// The data to remember
    pub data: String,
    /// Optional tags for the memory
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether to store globally or locally
    pub is_global: bool,
}

/// Parameters for the retrieve_memories tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RetrieveMemoriesParams {
    /// The category to retrieve memories from (use "*" for all)
    pub category: String,
    /// Whether to retrieve from global or local storage
    pub is_global: bool,
}

/// Parameters for the remove_memory_category tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RemoveMemoryCategoryParams {
    /// The category to remove (use "*" for all)
    pub category: String,
    /// Whether to remove from global or local storage
    pub is_global: bool,
}

/// Parameters for the remove_specific_memory tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RemoveSpecificMemoryParams {
    /// The category containing the memory
    pub category: String,
    /// The content of the memory to remove
    pub memory_content: String,
    /// Whether to remove from global or local storage
    pub is_global: bool,
}

/// Memory MCP Server using official RMCP SDK
#[derive(Clone)]
pub struct MemoryServer {
    tool_router: ToolRouter<Self>,
    instructions: String,
    global_memory_dir: PathBuf,
    local_memory_dir: PathBuf,
}

/// Where the *global* (cross-project) memory store lives.
///
/// `<config>/memory`, resolved through [`crate::paths`] — the one resolver in
/// this crate that honours `BIOROUTER_PATH_ROOT`:
/// - macOS/Linux: `~/.config/biorouter/memory/`
/// - Windows:     `~\AppData\Roaming\BaranziniLab\Biorouter\config\memory`
/// - sandboxed:   `$BIOROUTER_PATH_ROOT/config/memory`
///
/// This used to hand-roll `choose_app_strategy(…).in_config_dir("memory")`, a
/// fourth resolver that ignored the override — so a sandboxed run (test drive,
/// worktree, per-app jail) read *and rewrote* the user's real global memories.
fn global_memory_dir() -> PathBuf {
    crate::paths::in_config_dir("memory")
}

/// A memory category is a **name**, not a path (issue #73).
///
/// `category` arrives as a model-supplied `String` on all four memory tools and
/// ends up as a filename, so the only safe reading of it is "one plain path
/// segment". Two escapes fell out of not saying so: `..` walked out of the
/// store, and — because [`Path::join`] *discards* the base when its argument is
/// absolute, and the argument is `format!("{category}.txt")` — an absolute
/// category replaced the store outright (`"/etc/hosts"` → `/etc/hosts.txt`).
///
/// The rule is containment, deliberately not a charset allowlist: `*` (the
/// documented "all" sentinel), dots, spaces and non-ASCII are ordinary names a
/// model legitimately picks, and rejecting them would break the feature to fix
/// the bug. Separators are refused on *every* platform, not only the one where
/// they happen to separate, because a category is written to disk here and read
/// back somewhere else.
fn validated_category(category: &str) -> io::Result<&str> {
    let reject = |why: &str| {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "invalid memory category {category:?}: {why}. A category is a plain name such as \
                 \"development\" or \"personal\", not a path — it cannot be empty, contain a path \
                 separator, or point outside the memory store."
            ),
        ))
    };

    if category.is_empty() {
        return reject("it is empty");
    }
    if category.contains('/') || category.contains('\\') {
        return reject("it contains a path separator");
    }
    if category.contains('\0') {
        return reject("it contains a NUL byte");
    }

    // Belt and braces for anything the platform still parses as more than a
    // plain segment — a root, a Windows drive prefix, `.`, `..`. The equality
    // check also catches a name the parser *normalised* (e.g. a trailing
    // separator), which must not silently become a different category.
    let mut components = Path::new(category).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(segment)), None) if segment == OsStr::new(category) => Ok(category),
        _ => reject("it is not a single path segment"),
    }
}

/// Heads the *index* of global memory categories in the system prompt.
///
/// Bodies deliberately do not appear — see [`MemoryServer::compose_instructions`].
const GLOBAL_INDEX_HEADER: &str = "\n\nGlobal Memories — categories only, contents NOT loaded:\n\
     These were saved by other sessions and are shared by every project on this machine, so their\n\
     contents are deliberately kept out of this prompt. If one of the categories below looks\n\
     relevant to what the user is asking, read it with\n\
     `retrieve_memories(category=\"<category>\", is_global=true)` — a tool call the user can see.\n\
     Never guess at, or claim to know, the contents of a category you have not retrieved.\n";

/// Heads the inlined local memories.
const LOCAL_SECTION_HEADER: &str = "\n\nLocal Memories (this project's .biorouter/memory):\n";

impl Default for MemoryServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl MemoryServer {
    #[allow(clippy::too_many_lines)]
    pub fn new() -> Self {
        let instructions = formatdoc! {r#"
             This extension allows storage and retrieval of categorized information with tagging support. It's designed to help
             manage important information across sessions in a systematic and organized manner.
             Capabilities:
             1. Store information in categories with optional tags for context-based retrieval.
             2. Search memories by content or specific tags to find relevant information.
             3. List all available memory categories for easy navigation.
             4. Remove entire categories of memories when they are no longer needed.
             When to call memory tools:
             - These are examples where the assistant should proactively call the memory tool because the user is providing recurring preferences, project details, or workflow habits that they may expect to be remembered.
             - Preferred Development Tools & Conventions
             - User-specific data (e.g., name, preferences)
             - Project-related configurations
             - Workflow descriptions
             - Other critical settings
             Interaction Protocol:
             When important information is identified, such as:
             - User-specific data (e.g., name, preferences)
             - Project-related configurations
             - Workflow descriptions
             - Other critical settings
             The protocol is:
             1. Identify the critical piece of information.
             2. Ask the user if they'd like to store it for later reference.
             3. Upon agreement:
                - Suggest a relevant category like "personal" for user data or "development" for project preferences.
                - Inquire about any specific tags they want to apply for easier lookup.
                - Confirm the desired storage location:
                  - Local storage (.biorouter/memory) for project-specific details. This is the default; prefer it.
                  - Global storage (~/.config/biorouter/memory) for user-wide data. A global memory is readable by every Biorouter session on this machine, in every project — only choose it when the user has asked for something that should follow them across projects, and say so when you store it.
                - Use the remember_memory tool to store the information.
                  - `remember_memory(category, data, tags, is_global)`
             Keywords that trigger memory tools:
             - "remember"
             - "forget"
             - "memory"
             - "save"
             - "save memory"
             - "remove memory"
             - "clear memory"
             - "search memory"
             - "find memory"
             Suggest the user to use memory tools when:
             - When the user mentions a keyword that triggers a memory tool
             - When the user performs a routine task
             - When the user executes a command and would benefit from remembering the exact command
             Example Interaction for Storing Information:
             User: "For this project, we use black for code formatting"
             Assistant: "You've mentioned a development preference. Would you like to remember this for future conversations?
             User: "Yes, please."
             Assistant: "I'll store this in the 'development' category. Any specific tags to add? Suggestions: #formatting
             #tools"
             User: "Yes, use those tags."
             Assistant: "Shall I store this locally for this project only, or globally for all projects?"
             User: "Locally, please."
             Assistant: *Stores the information under category="development", tags="formatting tools", scope="local"*
             Another Example Interaction for Storing Information:
             User: "Remember the gh command to view github comments"
             Assistant: "Shall I store this locally for this project only, or globally for all projects?"
             User: "Globally, please."
             Assistant: *Stores the gh command under category="github", tags="comments", scope="global"*
             Example Interaction suggesting memory tools:
             User: "I'm using the gh command to view github comments"
             Assistant: "You've mentioned a command. Would you like to remember this for future conversations?
             User: "Yes, please."
             Assistant: "I'll store this in the 'github' category. Any specific tags to add? Suggestions: #comments #gh"
             Retrieving Memories:
             To access stored information, utilize the memory retrieval protocols:
             - **Search by Category**:
               - Provides all memories within the specified context.
               - Use: `retrieve_memories(category="development", is_global=False)`
               - Note: If you want to retrieve all local memories, use `retrieve_memories(category="*", is_global=False)`
               - Note: If you want to retrieve all global memories, use `retrieve_memories(category="*", is_global=True)`
             - **Filter by Tags**:
               - Enables targeted retrieval based on specific tags.
               - Use: Provide tag filters to refine search.
            To remove a memory, use the following protocol:
            - **Remove by Category**:
              - Removes all memories within the specified category.
              - Use: `remove_memory_category(category="development", is_global=False)`
              - Note: If you want to remove all local memories, use `remove_memory_category(category="*", is_global=False)`
              - Note: If you want to remove all global memories, use `remove_memory_category(category="*", is_global=True)`
            The Protocol is:
             1. Confirm what kind of information the user seeks by category or keyword.
             2. Suggest categories or relevant tags based on the user's request.
             3. Use the retrieve function to access relevant memory entries.
             4. Present a summary of findings, offering detailed exploration upon request.
             Example Interaction for Retrieving Information:
             User: "What configuration do we use for code formatting?"
             Assistant: "Let me check the 'development' category for any related memories. Searching using #formatting tag."
             Assistant: *Executes retrieval: `retrieve_memories(category="development", is_global=False)`*
             Assistant: "We have 'black' configured for code formatting, specific to this project. Would you like further
             details?"
             Memory Overview:
             - Categories can include a wide range of topics, structured to keep information grouped logically.
             - Tags enable quick filtering and identification of specific entries.
             Operational Guidelines:
             - Always confirm with the user before saving information.
             - Propose suitable categories and tag suggestions.
             - Discuss storage scope thoroughly to align with user needs.
             - Never save globally something the user has not asked to be remembered across projects. When in doubt, save locally — a local memory can be re-saved globally later, but a global one has already crossed into every other session.
             - Global memory contents are not loaded into your context automatically; only the category names are. Retrieve a category before relying on what is in it.
             - Acknowledge the user about what is stored and where, for transparency and ease of future retrieval.
            "#};

        // Check for .biorouter/memory in current directory
        let local_memory_dir = std::env::var("BIOROUTER_WORKING_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::current_dir().unwrap())
            .join(".biorouter")
            .join("memory");

        let global_memory_dir = global_memory_dir();

        let mut memory_router = Self {
            tool_router: Self::tool_router(),
            instructions: instructions.clone(),
            global_memory_dir,
            local_memory_dir,
        };

        let updated_instructions = memory_router.compose_instructions(&instructions);
        memory_router.set_instructions(updated_instructions);

        memory_router
    }

    /// Assemble what this extension contributes to the agent's system prompt:
    /// the protocol above, plus whatever the two memory stores hold.
    ///
    /// **Local** memories are inlined. They live in
    /// `<working dir>/.biorouter/memory`, so they only ever reach a session the
    /// user opened *that* directory in — the same act that put them there.
    ///
    /// **Global** memories are only *indexed*: category names, never bodies and
    /// never tags. The global store is machine-wide, so inlining it meant a
    /// memory one session wrote with `is_global=true` was read by every later
    /// session — any project, any working directory, any model — with no tool
    /// call in the receiving session, nothing in its transcript, and nothing
    /// shown to the user (issue #58). `is_global` is an argument the *model*
    /// supplies, so nothing but the model's judgement stood between a summary
    /// of sensitive work and every future conversation.
    ///
    /// The index keeps the feature discoverable while making the read an
    /// explicit `retrieve_memories(…, is_global=true)` call in the receiving
    /// session — visible in the transcript, gateable by the permission
    /// inspectors, and cancellable by the user. That is the bar the issue sets
    /// when it calls `chatrecall` the weaker channel "because it at least
    /// requires the receiving session to ask". It is also retroactive: memories
    /// already on disk stop being injected the moment this ships, which a
    /// write-side gate could not achieve.
    ///
    /// What this deliberately does **not** do, and so remains for #56:
    /// 1. The *write* is still ungated. An in-process MCP server has no channel
    ///    to the user, so it cannot ask before a memory is marked global; all it
    ///    can do is name the scope in the tool result (see `remember_memory`) so
    ///    the transcript shows it. A real confirmation needs the permission
    ///    path in `biorouter::permission`, not this crate.
    /// 2. The line is drawn by *store* — global vs local — not by the
    ///    sensitivity of the session that wrote the entry. A sensitive note
    ///    saved locally still lands in the prompt of every session opened in
    ///    that directory. Only classification can draw the finer line.
    /// 3. A category *name* is model-chosen text and still crosses sessions.
    ///    It is a short label rather than a body, and it is what lets the model
    ///    fetch one category instead of `category="*"`, so it is kept — but it
    ///    is not zero.
    /// 4. Nothing surfaces the global store in the UI, so a user still cannot
    ///    see or prune what accumulated there without asking the agent.
    fn compose_instructions(&self, base: &str) -> String {
        let retrieved_global_memories = self.retrieve_all(true);
        let retrieved_local_memories = self.retrieve_all(false);

        let mut updated_instructions = base.to_string();

        let memories_follow_up_instructions = formatdoc! {r#"
            **Here are the user's currently saved memories:**
            Local memories — this project only — are listed below in full. Global memories are listed by category name only; their contents are NOT in this prompt and have to be fetched with retrieve_memories.
            Please keep what is listed in mind when answering future questions.
            Do not bring up memories unless relevant.
            Note: if the user has not saved any memories, these sections will be empty.
            Note: if the user removes a memory that was previously loaded into the system, please remove it from the system instructions.
            "#};

        updated_instructions.push_str("\n\n");
        updated_instructions.push_str(&memories_follow_up_instructions);

        // Global: the index, and only the index.
        if let Ok(global_memories) = retrieved_global_memories {
            let mut categories: Vec<&str> = global_memories.keys().map(String::as_str).collect();
            // The extension instructions are part of the system prompt; a
            // `HashMap`-ordered listing reshuffles between launches and defeats
            // prompt caching for nothing.
            categories.sort_unstable();
            if !categories.is_empty() {
                updated_instructions.push_str(GLOBAL_INDEX_HEADER);
                for category in categories {
                    updated_instructions.push_str(&format!("- {}\n", category));
                }
            }
        }

        if let Ok(local_memories) = retrieved_local_memories {
            let mut by_category: Vec<(&String, &Vec<String>)> = local_memories.iter().collect();
            by_category.sort_unstable_by_key(|(category, _)| *category);
            if !by_category.is_empty() {
                updated_instructions.push_str(LOCAL_SECTION_HEADER);
                for (category, memories) in by_category {
                    updated_instructions.push_str(&format!("\nCategory: {}\n", category));
                    for memory in memories {
                        updated_instructions.push_str(&format!("- {}\n", memory));
                    }
                }
            }
        }

        updated_instructions
    }

    // Add a setter method for instructions
    pub fn set_instructions(&mut self, new_instructions: String) {
        self.instructions = new_instructions;
    }

    pub fn get_instructions(&self) -> &str {
        &self.instructions
    }

    /// The single point every memory tool reaches the filesystem through — and
    /// therefore the one place a category has to be proved a name (issue #73).
    fn get_memory_file(&self, category: &str, is_global: bool) -> io::Result<PathBuf> {
        let category = validated_category(category)?;
        // Defaults to local memory if no is_global flag is provided
        let base_dir = if is_global {
            &self.global_memory_dir
        } else {
            &self.local_memory_dir
        };
        Ok(base_dir.join(format!("{}.txt", category)))
    }

    /// Surface a store error to the model. A refused category is the *caller's*
    /// mistake — `INVALID_PARAMS`, which the model can act on — not
    /// `INTERNAL_ERROR`, which reads as "the server broke, retry it".
    fn tool_error(e: &io::Error) -> ErrorData {
        let code = if e.kind() == io::ErrorKind::InvalidInput {
            ErrorCode::INVALID_PARAMS
        } else {
            ErrorCode::INTERNAL_ERROR
        };
        ErrorData::new(code, e.to_string(), None)
    }

    pub fn retrieve_all(&self, is_global: bool) -> io::Result<HashMap<String, Vec<String>>> {
        let base_dir = if is_global {
            &self.global_memory_dir
        } else {
            &self.local_memory_dir
        };
        let mut memories = HashMap::new();
        if base_dir.exists() {
            for entry in fs::read_dir(base_dir)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    let category = &entry.file_name().to_string_lossy().replace(".txt", "");
                    let category_memories = self.retrieve(category, is_global)?;
                    memories.insert(
                        category.to_string(),
                        category_memories.into_iter().flat_map(|(_, v)| v).collect(),
                    );
                }
            }
        }
        Ok(memories)
    }

    pub fn remember(
        &self,
        _context: &str,
        category: &str,
        data: &str,
        tags: &[&str],
        is_global: bool,
    ) -> io::Result<()> {
        let memory_file_path = self.get_memory_file(category, is_global)?;

        if let Some(parent) = memory_file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&memory_file_path)?;
        if !tags.is_empty() {
            writeln!(file, "# {}", tags.join(" "))?;
        }
        writeln!(file, "{}\n", data)?;

        Ok(())
    }

    pub fn retrieve(
        &self,
        category: &str,
        is_global: bool,
    ) -> io::Result<HashMap<String, Vec<String>>> {
        let memory_file_path = self.get_memory_file(category, is_global)?;
        if !memory_file_path.exists() {
            return Ok(HashMap::new());
        }

        let mut file = fs::File::open(memory_file_path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;

        let mut memories = HashMap::new();
        for entry in content.split("\n\n") {
            let mut lines = entry.lines();
            if let Some(first_line) = lines.next() {
                if let Some(stripped) = first_line.strip_prefix('#') {
                    let tags = stripped
                        .split_whitespace()
                        .map(String::from)
                        .collect::<Vec<_>>();
                    memories.insert(tags.join(" "), lines.map(String::from).collect());
                } else {
                    let entry_data: Vec<String> = std::iter::once(first_line.to_string())
                        .chain(lines.map(String::from))
                        .collect();
                    memories
                        .entry("untagged".to_string())
                        .or_insert_with(Vec::new)
                        .extend(entry_data);
                }
            }
        }

        Ok(memories)
    }

    pub fn remove_specific_memory_internal(
        &self,
        category: &str,
        memory_content: &str,
        is_global: bool,
    ) -> io::Result<()> {
        let memory_file_path = self.get_memory_file(category, is_global)?;
        if !memory_file_path.exists() {
            return Ok(());
        }

        let mut file = fs::File::open(&memory_file_path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;

        let memories: Vec<&str> = content.split("\n\n").collect();
        let new_content: Vec<String> = memories
            .into_iter()
            .filter(|entry| !entry.contains(memory_content))
            .map(|s| s.to_string())
            .collect();

        fs::write(memory_file_path, new_content.join("\n\n"))?;

        Ok(())
    }

    pub fn clear_memory(&self, category: &str, is_global: bool) -> io::Result<()> {
        let memory_file_path = self.get_memory_file(category, is_global)?;
        if memory_file_path.exists() {
            fs::remove_file(memory_file_path)?;
        }

        Ok(())
    }

    pub fn clear_all_global_or_local_memories(&self, is_global: bool) -> io::Result<()> {
        let base_dir = if is_global {
            &self.global_memory_dir
        } else {
            &self.local_memory_dir
        };
        if base_dir.exists() {
            fs::remove_dir_all(base_dir)?;
        }
        Ok(())
    }

    /// Stores a memory with optional tags in a specified category
    #[tool(
        name = "remember_memory",
        description = "Stores a memory with optional tags in a specified category. is_global=false \
                       keeps it in this project's .biorouter/memory; is_global=true writes to the \
                       machine-wide store that every Biorouter session, in every project, can read. \
                       Store locally unless the user has asked for something that should follow \
                       them across projects, and tell them which one you used."
    )]
    pub async fn remember_memory(
        &self,
        params: Parameters<RememberMemoryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;

        if params.data.is_empty() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "Data must not be empty when remembering a memory".to_string(),
                None,
            ));
        }

        let tags: Vec<&str> = params.tags.iter().map(|s| s.as_str()).collect();
        self.remember(
            "context",
            &params.category,
            &params.data,
            &tags,
            params.is_global,
        )
        .map_err(|e| Self::tool_error(&e))?;

        // The scope is an argument the *model* supplies, and an MCP server has
        // no channel back to the user to ask. What it does have is this result,
        // which the transcript shows — so a machine-wide write has to name
        // itself rather than read as "Stored memory in category: x", which was
        // indistinguishable from a project-local note.
        let message = if params.is_global {
            format!(
                "Stored memory globally in category: {category}. Global memories live in the \
                 machine-wide store and are readable by every Biorouter session, in any project — \
                 not just this one. To undo: remove_specific_memory(category=\"{category}\", \
                 memory_content=…, is_global=true).",
                category = params.category
            )
        } else {
            format!(
                "Stored memory locally in category: {category}. Local memories stay in this \
                 project's .biorouter/memory and are read only by sessions working in this \
                 directory.",
                category = params.category
            )
        };

        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    /// Retrieves all memories from a specified category
    #[tool(
        name = "retrieve_memories",
        description = "Retrieves all memories from a specified category"
    )]
    pub async fn retrieve_memories(
        &self,
        params: Parameters<RetrieveMemoriesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;

        let memories = if params.category == "*" {
            self.retrieve_all(params.is_global)
        } else {
            self.retrieve(&params.category, params.is_global)
        }
        .map_err(|e| Self::tool_error(&e))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Retrieved memories: {:?}",
            memories
        ))]))
    }

    /// Removes all memories within a specified category
    #[tool(
        name = "remove_memory_category",
        description = "Removes all memories within a specified category"
    )]
    pub async fn remove_memory_category(
        &self,
        params: Parameters<RemoveMemoryCategoryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;

        let message = if params.category == "*" {
            self.clear_all_global_or_local_memories(params.is_global)
                .map_err(|e| Self::tool_error(&e))?;
            format!(
                "Cleared all memory {} categories",
                if params.is_global { "global" } else { "local" }
            )
        } else {
            self.clear_memory(&params.category, params.is_global)
                .map_err(|e| Self::tool_error(&e))?;
            format!("Cleared memories in category: {}", params.category)
        };

        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    /// Removes a specific memory within a specified category
    #[tool(
        name = "remove_specific_memory",
        description = "Removes a specific memory within a specified category"
    )]
    pub async fn remove_specific_memory(
        &self,
        params: Parameters<RemoveSpecificMemoryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;

        self.remove_specific_memory_internal(
            &params.category,
            &params.memory_content,
            params.is_global,
        )
        .map_err(|e| Self::tool_error(&e))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Removed specific memory from category: {}",
            params.category
        ))]))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MemoryServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "biorouter-memory".to_string(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                title: None,
                icons: None,
                website_url: None,
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(self.instructions.clone()),
            ..Default::default()
        }
    }
}

// Remove the old MemoryArgs struct since we're using the new parameter structs

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// A server over throwaway stores, so a test never touches the real ones.
    fn server_at(base: &std::path::Path) -> MemoryServer {
        MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: base.join("global"),
            local_memory_dir: base.join("local"),
        }
    }

    fn result_text(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .find_map(|c| c.as_text())
            .expect("tool result carries text")
            .text
            .clone()
    }

    /// The contents of a file that lives *outside* the memory store. Every
    /// escape test asserts this is still exactly what is on disk afterwards.
    const UNTOUCHED: &str = "ORIGINAL FILE CONTENTS sk-victim-8811\n";

    /// #73. `category` is a model-supplied `String` on all four memory tools,
    /// and it was pasted straight into a filename —
    /// `base_dir.join(format!("{category}.txt"))` — with no containment check
    /// and no re-resolution. A category holding `..` therefore walked *out* of
    /// the memory store, and each tool did its own thing to whatever it landed
    /// on: append (`remember_memory`), read (`retrieve_memories`), rewrite
    /// (`remove_specific_memory`) and **delete** (`remove_memory_category`).
    ///
    /// A category is a NAME, not a path.
    #[tokio::test]
    async fn a_traversing_category_cannot_escape_the_memory_store() {
        let temp = tempdir().unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("victim.txt");
        fs::write(&victim, UNTOUCHED).unwrap();

        // <temp>/store/{local,global}/../../outside/victim.txt
        let server = server_at(&temp.path().join("store"));
        let escaping = "../../outside/victim";

        let wrote = server
            .remember_memory(Parameters(RememberMemoryParams {
                category: escaping.into(),
                data: "smuggled".into(),
                tags: vec![],
                is_global: false,
            }))
            .await;
        assert!(
            wrote.is_err(),
            "remember_memory accepted a traversing category"
        );
        assert_eq!(
            fs::read_to_string(&victim).unwrap(),
            UNTOUCHED,
            "remember_memory appended to a file outside the memory store"
        );

        let read = server
            .retrieve_memories(Parameters(RetrieveMemoriesParams {
                category: escaping.into(),
                is_global: false,
            }))
            .await;
        assert!(
            read.is_err(),
            "retrieve_memories read a file outside the memory store: {}",
            read.as_ref().map(result_text).unwrap_or_default()
        );

        let rewrote = server
            .remove_specific_memory(Parameters(RemoveSpecificMemoryParams {
                category: escaping.into(),
                memory_content: "ORIGINAL".into(),
                is_global: false,
            }))
            .await;
        assert!(
            rewrote.is_err(),
            "remove_specific_memory accepted a traversing category"
        );
        assert_eq!(
            fs::read_to_string(&victim).unwrap(),
            UNTOUCHED,
            "remove_specific_memory rewrote a file outside the memory store"
        );

        // The delete is the worst of the four, so it is checked in both scopes:
        // the only difference between them is which base dir gets escaped from.
        for is_global in [false, true] {
            let deleted = server
                .remove_memory_category(Parameters(RemoveMemoryCategoryParams {
                    category: escaping.into(),
                    is_global,
                }))
                .await;
            assert!(
                deleted.is_err(),
                "remove_memory_category accepted a traversing category (is_global={is_global})"
            );
            assert!(
                victim.exists(),
                "remove_memory_category deleted a file outside the memory store \
                 (is_global={is_global})"
            );
        }
    }

    /// #73, the second escape. `Path::join` *discards* the base when its
    /// argument is absolute, and the argument here is `format!("{category}.txt")`
    /// — so an absolute category did not merely traverse out of the store, it
    /// replaced it outright (`category="/etc/hosts"` → `/etc/hosts.txt`). The
    /// victim below is inside the tempdir for the same reason `/etc` is the
    /// scary example: the mechanism does not care which.
    #[tokio::test]
    async fn an_absolute_category_cannot_replace_the_memory_store() {
        let temp = tempdir().unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("victim.txt");
        fs::write(&victim, UNTOUCHED).unwrap();

        let server = server_at(&temp.path().join("store"));
        // Absolute, and `<this>.txt` is exactly the victim.
        let escaping = outside.join("victim").to_string_lossy().into_owned();

        let wrote = server
            .remember_memory(Parameters(RememberMemoryParams {
                category: escaping.clone(),
                data: "smuggled".into(),
                tags: vec![],
                is_global: true,
            }))
            .await;
        assert!(
            wrote.is_err(),
            "remember_memory accepted an absolute category"
        );
        assert_eq!(
            fs::read_to_string(&victim).unwrap(),
            UNTOUCHED,
            "remember_memory appended to an absolute path outside the memory store"
        );

        let read = server
            .retrieve_memories(Parameters(RetrieveMemoriesParams {
                category: escaping.clone(),
                is_global: true,
            }))
            .await;
        assert!(
            read.is_err(),
            "retrieve_memories read an absolute path outside the memory store: {}",
            read.as_ref().map(result_text).unwrap_or_default()
        );

        let rewrote = server
            .remove_specific_memory(Parameters(RemoveSpecificMemoryParams {
                category: escaping.clone(),
                memory_content: "ORIGINAL".into(),
                is_global: true,
            }))
            .await;
        assert!(
            rewrote.is_err(),
            "remove_specific_memory accepted an absolute category"
        );
        assert_eq!(
            fs::read_to_string(&victim).unwrap(),
            UNTOUCHED,
            "remove_specific_memory rewrote an absolute path outside the memory store"
        );

        let deleted = server
            .remove_memory_category(Parameters(RemoveMemoryCategoryParams {
                category: escaping,
                is_global: true,
            }))
            .await;
        assert!(
            deleted.is_err(),
            "remove_memory_category accepted an absolute category"
        );
        assert!(
            victim.exists(),
            "remove_memory_category deleted an absolute path outside the memory store"
        );
    }

    /// `category="*"` is documented as "all" on `retrieve_memories` and
    /// `remove_memory_category`, where it is dispatched *before* the path is
    /// ever built. On `remember_memory` and `remove_specific_memory` it carries
    /// no such meaning and reaches the filename as a plain name — which it is.
    /// Validating the category must not cost either behaviour, so all four are
    /// pinned here.
    #[tokio::test]
    async fn the_all_categories_sentinel_survives_category_validation() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());

        for (category, data) in [
            ("development", "formats with black"),
            ("personal", "prefers metric units"),
        ] {
            server
                .remember_memory(Parameters(RememberMemoryParams {
                    category: category.into(),
                    data: data.into(),
                    tags: vec![],
                    is_global: false,
                }))
                .await
                .unwrap();
        }

        let all = result_text(
            &server
                .retrieve_memories(Parameters(RetrieveMemoriesParams {
                    category: "*".into(),
                    is_global: false,
                }))
                .await
                .expect("retrieve_memories(\"*\") is the documented read-everything call"),
        );
        assert!(
            all.contains("formats with black") && all.contains("prefers metric units"),
            "the \"*\" sentinel stopped returning every category: {all}"
        );

        // No sentinel meaning here: "*" is a legal single-segment filename and
        // has always been stored as one.
        server
            .remember_memory(Parameters(RememberMemoryParams {
                category: "*".into(),
                data: "starred".into(),
                tags: vec![],
                is_global: false,
            }))
            .await
            .expect("\"*\" is a plain name on remember_memory, not a path");
        server
            .remove_specific_memory(Parameters(RemoveSpecificMemoryParams {
                category: "*".into(),
                memory_content: "starred".into(),
                is_global: false,
            }))
            .await
            .expect("\"*\" is a plain name on remove_specific_memory, not a path");

        server
            .remove_memory_category(Parameters(RemoveMemoryCategoryParams {
                category: "*".into(),
                is_global: false,
            }))
            .await
            .expect("remove_memory_category(\"*\") is the documented clear-everything call");
        let after = result_text(
            &server
                .retrieve_memories(Parameters(RetrieveMemoriesParams {
                    category: "*".into(),
                    is_global: false,
                }))
                .await
                .unwrap(),
        );
        assert!(
            !after.contains("formats with black"),
            "the \"*\" sentinel stopped clearing the store: {after}"
        );
    }

    /// The funnel: all four tools reach the filesystem through
    /// `get_memory_file`, so that is the one place "a category is a name" has
    /// to hold. Names stay names — including `*`, dots, spaces and non-ASCII,
    /// so the rule is *containment*, not a charset allowlist that would break
    /// ordinary categories a model picks.
    #[test]
    fn get_memory_file_takes_a_name_and_refuses_a_path() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());

        for name in [
            "development",
            "personal",
            "a.txt.b",
            "*",
            "über",
            "with space",
            ".hidden",
            "-dash",
        ] {
            let path = server
                .get_memory_file(name, false)
                .unwrap_or_else(|e| panic!("plain category name {name:?} was rejected: {e}"));
            assert_eq!(path, server.local_memory_dir.join(format!("{name}.txt")));
        }

        for path_like in [
            "",
            ".",
            "..",
            "./x",
            "../evil",
            "../../outside/victim",
            "a/b",
            "sub/dir/x",
            "/etc/hosts",
            "/",
            // Rejected on every platform, not just Windows: a category is
            // stored as a filename and has to mean the same thing on the
            // machine that reads it back.
            "a\\b",
            "..\\evil",
        ] {
            let err = server
                .get_memory_file(path_like, true)
                .expect_err(&format!("{path_like:?} was accepted as a category"));
            assert_eq!(
                err.kind(),
                io::ErrorKind::InvalidInput,
                "a rejected category is the caller's mistake, not an I/O failure: {err}"
            );
        }
    }

    /// A rejected category is the model's mistake, so it comes back as
    /// `INVALID_PARAMS` — the code the model can act on — rather than
    /// `INTERNAL_ERROR`, which reads as "the server broke, retry".
    #[tokio::test]
    async fn a_rejected_category_is_reported_as_invalid_params() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());

        let err = server
            .remember_memory(Parameters(RememberMemoryParams {
                category: "../escape".into(),
                data: "smuggled".into(),
                tags: vec![],
                is_global: false,
            }))
            .await
            .expect_err("a traversing category has to be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS, "got: {err:?}");
        assert!(
            err.message.contains("category"),
            "the message has to name what was wrong so the model can fix it: {err:?}"
        );
    }

    /// #58. A memory one session wrote with `is_global=true` was appended
    /// verbatim to the extension instructions — i.e. the system prompt — of
    /// *every* later session. The global store is machine-wide, so this crossed
    /// project, working-directory and model boundaries with no tool call in the
    /// receiving session, nothing in its transcript, and nothing shown to the
    /// user. `is_global` is an argument the *model* supplies, so a model
    /// summarising sensitive work could open that channel on its own.
    ///
    /// The bodies must stay out of the prompt. What crosses is an index of
    /// category names, plus the `retrieve_memories` tool that already exists —
    /// so a session that wants a global memory has to *ask*, on the
    /// tool-dispatch path where the user, the transcript and the permission
    /// inspectors can all see it. That is the bar the issue itself sets when it
    /// calls `chatrecall` the weaker channel "because it at least requires the
    /// receiving session to ask".
    #[test]
    fn global_memory_bodies_stay_out_of_the_system_prompt() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());

        server
            .remember(
                "context",
                "clinical",
                "cohort 4217 had 12 responders and 3 withdrawals",
                &[],
                true,
            )
            .unwrap();
        server
            .remember(
                "context",
                "development",
                "this project formats with black",
                &[],
                false,
            )
            .unwrap();

        let instructions = server.compose_instructions("BASE PROTOCOL");

        assert!(
            !instructions.contains("cohort 4217 had 12 responders"),
            "a global memory's body reached the system prompt of a session that \
             never asked for it:\n{instructions}"
        );
        assert!(
            instructions.contains("clinical"),
            "the global *index* has to survive, or the model can never discover \
             that a global memory exists:\n{instructions}"
        );
        assert!(
            instructions.contains("is_global=true"),
            "the prompt has to say how to fetch a global memory the model finds \
             relevant, or the index is a dead end:\n{instructions}"
        );
        assert!(
            instructions.contains("this project formats with black"),
            "local memories live under the working directory the user opened, \
             so they cross no boundary and stay inlined:\n{instructions}"
        );
    }

    /// The index carries names, never bodies — including the tag line, which is
    /// author-supplied text just like the body.
    #[test]
    fn the_global_index_lists_categories_not_contents() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());

        server
            .remember(
                "context",
                "personal",
                "patient MRN 0092134 is enrolled",
                &["mrn", "enrollment"],
                true,
            )
            .unwrap();

        let instructions = server.compose_instructions("BASE PROTOCOL");

        assert!(
            !instructions.contains("0092134"),
            "global body leaked into the prompt:\n{instructions}"
        );
        assert!(
            !instructions.contains("enrollment"),
            "global tags are author-supplied text and leak the same way a body \
             does:\n{instructions}"
        );
        assert!(
            instructions.contains("personal"),
            "the category name is the whole index:\n{instructions}"
        );
    }

    /// The extension instructions are part of the system prompt, so a
    /// `HashMap`-ordered listing reshuffles between launches and defeats prompt
    /// caching for nothing. Both sections are ordered.
    #[test]
    fn the_memory_sections_are_ordered() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());

        for category in ["zeta", "alpha", "mu"] {
            server
                .remember("context", category, "note", &[], true)
                .unwrap();
        }

        let instructions = server.compose_instructions("BASE PROTOCOL");
        let index_of = |c: &str| {
            instructions
                .find(c)
                .unwrap_or_else(|| panic!("{c} missing from:\n{instructions}"))
        };

        assert!(
            index_of("alpha") < index_of("mu") && index_of("mu") < index_of("zeta"),
            "the global index is unordered, so the system prompt changes shape \
             between launches:\n{instructions}"
        );
    }

    /// The scope is an argument the *model* chooses. The write cannot be gated
    /// from inside an MCP server — there is no channel from here to the user —
    /// but the tool result is shown in the transcript, so at minimum it has to
    /// say which store it wrote to. "Stored memory in category: x" made a
    /// machine-wide write indistinguishable from a project-local note.
    #[tokio::test]
    async fn remembering_says_which_store_it_wrote_to() {
        let temp = tempdir().unwrap();
        let server = server_at(temp.path());

        let global = server
            .remember_memory(Parameters(RememberMemoryParams {
                category: "personal".into(),
                data: "prefers metric units".into(),
                tags: vec![],
                is_global: true,
            }))
            .await
            .unwrap();
        let global = result_text(&global);
        assert!(
            global.to_lowercase().contains("global"),
            "a machine-wide write has to name its scope where the user can see \
             it, got: {global}"
        );
        assert!(
            global.to_lowercase().contains("every")
                || global.to_lowercase().contains("all sessions")
                || global.to_lowercase().contains("other sessions"),
            "the result has to say the memory is readable outside this session, \
             got: {global}"
        );

        let local = server
            .remember_memory(Parameters(RememberMemoryParams {
                category: "development".into(),
                data: "formats with black".into(),
                tags: vec![],
                is_global: false,
            }))
            .await
            .unwrap();
        let local = result_text(&local);
        assert!(
            local.to_lowercase().contains("local") || local.to_lowercase().contains("this project"),
            "a project-local write has to be distinguishable from a global one, \
             got: {local}"
        );
        assert!(
            !local.to_lowercase().contains("every session"),
            "a local write must not claim cross-session reach, got: {local}"
        );
    }

    /// The global memory store is user data the agent reads *and writes*, so a
    /// sandboxed run — a test drive, a worktree, a per-app jail — must not
    /// reach the real one. `BIOROUTER_PATH_ROOT` is how a run declares that
    /// sandbox, and `crate::paths` is the one resolver that honours it
    /// (pinned to `biorouter::config::Paths` by the cross-crate agreement
    /// test). Resolving the store with a bare `choose_app_strategy` call
    /// instead ignored the override entirely and pointed a jailed run straight
    /// at `~/.config/biorouter/memory`.
    #[test]
    #[serial_test::serial]
    fn global_memory_store_honours_the_sandbox_root() {
        let sandbox = tempdir().unwrap();
        let _env = env_lock::lock_env([(
            "BIOROUTER_PATH_ROOT",
            Some(sandbox.path().to_string_lossy().into_owned()),
        )]);

        assert_eq!(
            global_memory_dir(),
            crate::paths::in_config_dir("memory"),
            "the global memory store must resolve through crate::paths, the one \
             resolver that honours BIOROUTER_PATH_ROOT"
        );
        assert!(
            global_memory_dir().starts_with(sandbox.path()),
            "a sandboxed run resolved the global memory store to {}, outside its \
             own root {} — it would read and write the user's real memories",
            global_memory_dir().display(),
            sandbox.path().display()
        );
    }

    #[test]
    fn test_lazy_directory_creation() {
        let temp_dir = tempdir().unwrap();
        let memory_base = temp_dir.path().join("test_memory");

        let router = MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: memory_base.join("global"),
            local_memory_dir: memory_base.join("local"),
        };

        assert!(!router.global_memory_dir.exists());
        assert!(!router.local_memory_dir.exists());

        router
            .remember(
                "test_context",
                "test_category",
                "test_data",
                &["tag1"],
                false,
            )
            .unwrap();

        assert!(router.local_memory_dir.exists());
        assert!(!router.global_memory_dir.exists());

        router
            .remember(
                "test_context",
                "global_category",
                "global_data",
                &["global_tag"],
                true,
            )
            .unwrap();

        assert!(router.global_memory_dir.exists());
    }

    #[test]
    fn test_clear_nonexistent_directories() {
        let temp_dir = tempdir().unwrap();
        let memory_base = temp_dir.path().join("nonexistent_memory");

        let router = MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: memory_base.join("global"),
            local_memory_dir: memory_base.join("local"),
        };

        assert!(router.clear_all_global_or_local_memories(false).is_ok());
        assert!(router.clear_all_global_or_local_memories(true).is_ok());
    }

    #[test]
    fn test_remember_retrieve_clear_workflow() {
        let temp_dir = tempdir().unwrap();
        let memory_base = temp_dir.path().join("workflow_test");

        let router = MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: memory_base.join("global"),
            local_memory_dir: memory_base.join("local"),
        };

        router
            .remember(
                "context",
                "test_category",
                "test_data_content",
                &["test_tag"],
                false,
            )
            .unwrap();

        let memories = router.retrieve("test_category", false).unwrap();
        assert!(!memories.is_empty());

        let has_content = memories.values().any(|v| {
            v.iter()
                .any(|content| content.contains("test_data_content"))
        });
        assert!(has_content);

        router.clear_memory("test_category", false).unwrap();

        let memories_after_clear = router.retrieve("test_category", false).unwrap();
        assert!(memories_after_clear.is_empty());
    }

    #[test]
    fn test_directory_creation_on_write() {
        let temp_dir = tempdir().unwrap();
        let memory_base = temp_dir.path().join("write_test");

        let router = MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: memory_base.join("global"),
            local_memory_dir: memory_base.join("local"),
        };

        assert!(!router.local_memory_dir.exists());

        router
            .remember("context", "category", "data", &[], false)
            .unwrap();

        assert!(router.local_memory_dir.exists());
        assert!(router.local_memory_dir.join("category.txt").exists());
    }

    #[test]
    fn test_remove_specific_memory() {
        let temp_dir = tempdir().unwrap();
        let memory_base = temp_dir.path().join("remove_test");

        let router = MemoryServer {
            tool_router: ToolRouter::new(),
            instructions: String::new(),
            global_memory_dir: memory_base.join("global"),
            local_memory_dir: memory_base.join("local"),
        };

        router
            .remember("context", "category", "keep_this", &[], false)
            .unwrap();
        router
            .remember("context", "category", "remove_this", &[], false)
            .unwrap();

        let memories = router.retrieve("category", false).unwrap();
        assert_eq!(memories.len(), 1);

        router
            .remove_specific_memory_internal("category", "remove_this", false)
            .unwrap();

        let memories_after = router.retrieve("category", false).unwrap();
        let has_removed = memories_after
            .values()
            .any(|v| v.iter().any(|content| content.contains("remove_this")));
        assert!(!has_removed);

        let has_kept = memories_after
            .values()
            .any(|v| v.iter().any(|content| content.contains("keep_this")));
        assert!(has_kept);
    }
}
