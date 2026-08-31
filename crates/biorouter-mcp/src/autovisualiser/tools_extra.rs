// New visualization tools, layered onto the shared infrastructure in `common`
// and combined into the router in `AutoVisualiserRouter::new`.
//
// This file is `include!`d into mod.rs, so it shares its imports and can define
// additional `#[tool_router(router = …)]` impl blocks on `AutoVisualiserRouter`.

// ===========================================================================
// Mermaid helpers — turn typed input into valid Mermaid source. All output
// flows through `render_mermaid_source`, which escapes + renders safely.
// ===========================================================================

/// Encode identities without merging punctuation, whitespace, or Unicode.
fn mermaid_id(raw: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut id = String::with_capacity(3 + raw.len() * 2);
    id.push_str("br_");
    for byte in raw.bytes() {
        id.push(HEX[(byte >> 4) as usize] as char);
        id.push(HEX[(byte & 15) as usize] as char);
    }
    id
}

// ER attribute types and names are display tokens, not referenced node identities.
fn mermaid_visible_token(raw: &str) -> String {
    let mut s: String = raw
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() || s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        s.insert(0, 'n');
    }
    s
}

fn mermaid_gantt_start(
    raw: &str,
    ids: &std::collections::HashSet<&str>,
) -> Result<String, ErrorData> {
    let start = raw.trim_start();
    let Some(after) = start
        .strip_prefix("after")
        .filter(|rest| rest.starts_with(char::is_whitespace))
    else {
        return Ok(raw.trim().to_string());
    };
    // Remove the grammar separator only: leading/trailing whitespace can belong to an ID.
    let mut characters = after.chars();
    let _ = characters.next();
    let payload = characters.as_str();
    if ids.contains(payload) {
        return Ok(format!("after {}", mermaid_id(payload)));
    }
    let payload = payload.trim();
    if ids.contains(payload) {
        return Ok(format!("after {}", mermaid_id(payload)));
    }
    let dependencies: Vec<&str> = payload.split_whitespace().collect();
    if dependencies.is_empty() || dependencies.iter().any(|id| !ids.contains(id)) {
        return Err(invalid(
            "Gantt dependency must reference a known explicit task ID.",
        ));
    }
    let ambiguous = ids
        .iter()
        .filter(|id| id.chars().any(char::is_whitespace))
        .any(|id| {
            !id.is_empty()
                && payload.match_indices(*id).any(|(offset, found)| {
                    let (before, matched_and_after) = payload.split_at(offset);
                    let (_, after) = matched_and_after.split_at(found.len());
                    (before.is_empty() || before.ends_with(char::is_whitespace))
                        && (after.is_empty() || after.starts_with(char::is_whitespace))
                })
        });
    if ambiguous {
        return Err(invalid("Gantt dependency is ambiguous between task IDs containing spaces and multiple IDs; use unambiguous explicit IDs."));
    }
    Ok(format!(
        "after {}",
        dependencies
            .into_iter()
            .map(mermaid_id)
            .collect::<Vec<_>>()
            .join(" ")
    ))
}

/// Escape a label for use inside a Mermaid quoted string (`"…"`).
fn mermaid_label(raw: &str) -> String {
    raw.replace('"', "'")
        .replace(['\n', '\r'], " ")
        .trim()
        .to_string()
}

// ----- render_flowchart ----------------------------------------------------

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct FlowNode {
    /// Unique node id
    pub id: String,
    /// Display label (defaults to the id)
    #[serde(default)]
    pub label: Option<String>,
    /// Shape: rectangle (default), rounded, stadium, circle, diamond, hexagon, subroutine, cylinder
    #[serde(default)]
    pub shape: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct FlowEdge {
    /// Source node id
    pub from: String,
    /// Target node id
    pub to: String,
    /// Optional edge label
    #[serde(default)]
    pub label: Option<String>,
    /// Line style: solid (default), dotted, thick, open
    #[serde(default)]
    pub style: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct FlowchartData {
    /// Optional explicit node declarations (for labels/shapes). Nodes referenced
    /// only by edges are created automatically.
    #[serde(default)]
    pub nodes: Vec<FlowNode>,
    /// Directed edges between nodes
    pub edges: Vec<FlowEdge>,
    /// Layout direction: TD/TB (top-down, default), LR, RL, BT
    #[serde(default)]
    pub direction: Option<String>,
    /// Optional diagram title (shown as the page header)
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderFlowchartParams {
    #[serde(deserialize_with = "common::de_flexible")]
    pub data: FlowchartData,
}

fn shape_wrap(shape: Option<&str>, label: &str) -> String {
    let l = mermaid_label(label);
    match shape.map(|s| s.trim().to_lowercase()).as_deref() {
        Some("rounded") | Some("round") => format!("(\"{l}\")"),
        Some("stadium") | Some("pill") => format!("([\"{l}\"])"),
        Some("circle") => format!("((\"{l}\"))"),
        Some("diamond") | Some("decision") => format!("{{\"{l}\"}}"),
        Some("hexagon") => format!("{{{{\"{l}\"}}}}"),
        Some("subroutine") => format!("[[\"{l}\"]]"),
        Some("cylinder") | Some("database") | Some("db") => format!("[(\"{l}\")]"),
        _ => format!("[\"{l}\"]"),
    }
}

fn edge_arrow(style: Option<&str>) -> &'static str {
    match style.map(|s| s.trim().to_lowercase()).as_deref() {
        Some("dotted") | Some("dashed") => "-.->",
        Some("thick") | Some("bold") => "==>",
        Some("open") | Some("line") => "---",
        _ => "-->",
    }
}

// ----- render_gantt --------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct GanttTask {
    /// Task name
    pub name: String,
    /// Optional explicit task id (used for dependencies via `after`)
    #[serde(default)]
    pub id: Option<String>,
    /// Start date (e.g. 2024-01-01) or `after <taskId>`
    #[serde(default)]
    pub start: Option<String>,
    /// End date (alternative to duration)
    #[serde(default)]
    pub end: Option<String>,
    /// Duration (e.g. "5d", "2w")
    #[serde(default)]
    pub duration: Option<String>,
    /// Status: active, done, crit, milestone
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct GanttSection {
    /// Section name
    pub name: String,
    /// Tasks within this section
    pub tasks: Vec<GanttTask>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct GanttData {
    /// Optional diagram title
    #[serde(default)]
    pub title: Option<String>,
    /// Date format (default YYYY-MM-DD)
    #[serde(default, rename = "dateFormat")]
    pub date_format: Option<String>,
    /// Sections, each grouping related tasks
    pub sections: Vec<GanttSection>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderGanttParams {
    #[serde(deserialize_with = "common::de_flexible")]
    pub data: GanttData,
}

// ----- render_sequence -----------------------------------------------------

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SeqMessage {
    /// Sender participant
    pub from: String,
    /// Receiver participant
    pub to: String,
    /// Message text
    pub text: String,
    /// Arrow style: solid (default), dashed, open, cross
    #[serde(default)]
    pub arrow: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SequenceData {
    /// Optional explicit participant order (otherwise inferred from messages)
    #[serde(default)]
    pub participants: Vec<String>,
    /// Ordered messages
    pub messages: Vec<SeqMessage>,
    /// Optional diagram title
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderSequenceParams {
    #[serde(deserialize_with = "common::de_flexible")]
    pub data: SequenceData,
}

fn seq_arrow(style: Option<&str>) -> &'static str {
    match style.map(|s| s.trim().to_lowercase()).as_deref() {
        Some("dashed") | Some("dotted") => "-->>",
        Some("open") => "->",
        Some("cross") => "-x",
        _ => "->>",
    }
}

// ----- render_mindmap ------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MindNode {
    /// Node text
    pub text: String,
    /// Child nodes
    #[serde(default)]
    pub children: Vec<MindNode>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MindmapData {
    /// Root node of the mind map
    pub root: MindNode,
    /// Optional diagram title
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderMindmapParams {
    #[serde(deserialize_with = "common::de_flexible")]
    pub data: MindmapData,
}

fn mindmap_lines(node: &MindNode, depth: usize, out: &mut String) -> Result<(), ErrorData> {
    if depth > MAX_TREE_DEPTH {
        return Err(invalid("Mind map nesting is too deep."));
    }
    let indent = "  ".repeat(depth + 1);
    let text = mermaid_label(&node.text);
    if depth == 0 {
        out.push_str(&format!("{indent}root((\"{text}\"))\n"));
    } else {
        out.push_str(&format!("{indent}(\"{text}\")\n"));
    }
    for child in &node.children {
        mindmap_lines(child, depth + 1, out)?;
    }
    Ok(())
}

// ----- render_timeline -----------------------------------------------------

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct TimelinePeriod {
    /// Time period label (e.g. a year)
    pub period: String,
    /// Events that occurred in this period
    pub events: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct TimelineData {
    /// Optional diagram title
    #[serde(default)]
    pub title: Option<String>,
    /// Chronological periods
    pub periods: Vec<TimelinePeriod>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderTimelineParams {
    #[serde(deserialize_with = "common::de_flexible")]
    pub data: TimelineData,
}

// ----- render_er_diagram ---------------------------------------------------

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ErAttribute {
    /// Attribute data type (default "string")
    #[serde(default, rename = "type")]
    pub type_: Option<String>,
    /// Attribute name
    pub name: String,
    /// Optional key: PK, FK, or UK
    #[serde(default)]
    pub key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ErEntity {
    /// Entity name
    pub name: String,
    /// Entity attributes
    #[serde(default)]
    pub attributes: Vec<ErAttribute>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ErRelationship {
    /// First entity name
    pub from: String,
    /// Second entity name
    pub to: String,
    /// Relationship label (verb phrase)
    #[serde(default)]
    pub label: Option<String>,
    /// Cardinality: one-to-one, one-to-many (default), many-to-one, many-to-many
    #[serde(default)]
    pub cardinality: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ErData {
    /// Entities
    pub entities: Vec<ErEntity>,
    /// Relationships between entities
    #[serde(default)]
    pub relationships: Vec<ErRelationship>,
    /// Optional diagram title
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderErParams {
    #[serde(deserialize_with = "common::de_flexible")]
    pub data: ErData,
}

fn er_cardinality(c: Option<&str>) -> &'static str {
    match c
        .map(|s| s.trim().to_lowercase().replace([' ', '_'], "-"))
        .as_deref()
    {
        Some("one-to-one") | Some("1-to-1") | Some("1-1") => "||--||",
        Some("many-to-one") => "}o--||",
        Some("many-to-many") | Some("n-to-n") => "}o--o{",
        _ => "||--o{",
    }
}

// ----- render_state_diagram ------------------------------------------------

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct StateTransition {
    /// Source state (use "[*]" for the start)
    pub from: String,
    /// Target state (use "[*]" for the end)
    pub to: String,
    /// Optional transition label (the triggering event)
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct StateData {
    /// State transitions
    pub transitions: Vec<StateTransition>,
    /// Optional diagram title
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderStateParams {
    #[serde(deserialize_with = "common::de_flexible")]
    pub data: StateData,
}

fn state_token(raw: &str) -> String {
    if raw.trim() == "[*]" {
        "[*]".to_string()
    } else {
        mermaid_id(raw)
    }
}

// ----- render_class_diagram ------------------------------------------------

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ClassDef {
    /// Class name
    pub name: String,
    /// Attribute declarations (e.g. "+String name")
    #[serde(default)]
    pub attributes: Vec<String>,
    /// Method declarations (e.g. "+save()")
    #[serde(default)]
    pub methods: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ClassRelationship {
    /// First class name
    pub from: String,
    /// Second class name
    pub to: String,
    /// Type: inheritance, composition, aggregation, association (default), dependency, realization
    #[serde(default, rename = "type")]
    pub type_: Option<String>,
    /// Optional label
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ClassData {
    /// Classes
    pub classes: Vec<ClassDef>,
    /// Relationships between classes
    #[serde(default)]
    pub relationships: Vec<ClassRelationship>,
    /// Optional diagram title
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderClassParams {
    #[serde(deserialize_with = "common::de_flexible")]
    pub data: ClassData,
}

fn class_rel(t: Option<&str>) -> &'static str {
    match t.map(|s| s.trim().to_lowercase()).as_deref() {
        Some("inheritance") | Some("extends") => "<|--",
        Some("composition") => "*--",
        Some("aggregation") => "o--",
        Some("dependency") => "..>",
        Some("realization") | Some("implements") => "<|..",
        _ => "-->",
    }
}

// ===========================================================================
// Mermaid-backed tools
// ===========================================================================

#[tool_router(router = diagrams_router)]
impl AutoVisualiserRouter {
    /// Flowchart from typed nodes and edges
    #[tool(
        name = "render_flowchart",
        description = r#"Render a flowchart from typed nodes and edges (compiled to Mermaid).

- nodes (optional): [{id, label?, shape?}]; shape: rectangle|rounded|stadium|circle|diamond|hexagon|subroutine|cylinder
- edges (required): [{from, to, label?, style?}]; style: solid|dotted|thick|open
- direction (optional): TD (default) | LR | RL | BT
- title (optional)

Example:
{"direction":"LR","nodes":[{"id":"a","label":"Start","shape":"circle"},{"id":"b","label":"Decision","shape":"diamond"}],"edges":[{"from":"a","to":"b","label":"go"}]}"#
    )]
    pub async fn render_flowchart(
        &self,
        params: Parameters<RenderFlowchartParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let d = &params.0.data;
        if d.edges.is_empty() && d.nodes.is_empty() {
            return Err(invalid("Flowchart requires at least one node or edge."));
        }
        check_limit(d.nodes.len(), MAX_NODES, "nodes")?;
        check_limit(d.edges.len(), MAX_LINKS, "edges")?;
        let dir = match d
            .direction
            .as_deref()
            .map(|s| s.trim().to_uppercase())
            .as_deref()
        {
            Some("LR") => "LR",
            Some("RL") => "RL",
            Some("BT") => "BT",
            Some("TB") => "TB",
            _ => "TD",
        };
        let mut body = format!("flowchart {dir}\n");
        for n in &d.nodes {
            let id = mermaid_id(&n.id);
            let label = n.label.as_deref().unwrap_or(&n.id);
            body.push_str(&format!(
                "    {id}{}\n",
                shape_wrap(n.shape.as_deref(), label)
            ));
        }
        let mut declared: std::collections::HashSet<&str> =
            d.nodes.iter().map(|node| node.id.as_str()).collect();
        for raw in d
            .edges
            .iter()
            .flat_map(|edge| [edge.from.as_str(), edge.to.as_str()])
        {
            if declared.insert(raw) {
                body.push_str(&format!(
                    "    {}{}\n",
                    mermaid_id(raw),
                    shape_wrap(None, raw)
                ));
            }
        }
        for e in &d.edges {
            let from = mermaid_id(&e.from);
            let to = mermaid_id(&e.to);
            let arrow = edge_arrow(e.style.as_deref());
            match e.label.as_deref().filter(|s| !s.trim().is_empty()) {
                Some(l) => body.push_str(&format!(
                    "    {from} {arrow}|\"{}\"| {to}\n",
                    mermaid_label(l)
                )),
                None => body.push_str(&format!("    {from} {arrow} {to}\n")),
            }
        }
        self.render_mermaid_source(&body, d.title.as_deref().unwrap_or("Flowchart"))
    }

    /// Gantt chart / project timeline
    #[tool(
        name = "render_gantt",
        description = r#"Render a Gantt chart (project/experiment timeline; compiled to Mermaid).

- sections (required): [{name, tasks: [{name, start?, end?, duration?, status?, id?}]}]
  - start: a date (YYYY-MM-DD) or "after <taskId>"; provide duration (e.g. "5d") or end
  - status: active | done | crit | milestone
- dateFormat (optional, default YYYY-MM-DD), title (optional)

Example:
{"title":"Study","sections":[{"name":"Phase 1","tasks":[{"name":"Recruit","id":"t1","start":"2024-01-01","duration":"30d","status":"active"},{"name":"Analyze","start":"after t1","duration":"14d"}]}]}"#
    )]
    pub async fn render_gantt(
        &self,
        params: Parameters<RenderGanttParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let d = &params.0.data;
        if d.sections.is_empty() {
            return Err(invalid("Gantt chart requires at least one section."));
        }
        let fmt = d.date_format.as_deref().unwrap_or("YYYY-MM-DD");
        let mut ids = std::collections::HashSet::new();
        for task in d.sections.iter().flat_map(|section| &section.tasks) {
            if let Some(id) = task.id.as_deref() {
                if !ids.insert(id) {
                    return Err(invalid("Gantt explicit task IDs must be unique."));
                }
            }
        }
        let mut body = String::from("gantt\n");
        body.push_str(&format!("    dateFormat {fmt}\n"));
        let mut task_index = 0;
        for section in &d.sections {
            body.push_str(&format!("    section {}\n", mermaid_label(&section.name)));
            for task in &section.tasks {
                let mut meta: Vec<String> = Vec::new();
                if let Some(s) = task.status.as_deref().filter(|s| !s.trim().is_empty()) {
                    meta.push(s.trim().to_lowercase());
                }
                meta.push(
                    task.id
                        .as_deref()
                        .map(mermaid_id)
                        .unwrap_or_else(|| format!("br_auto_{task_index}")),
                );
                task_index += 1;
                if let Some(start) = task.start.as_deref().filter(|s| !s.trim().is_empty()) {
                    meta.push(mermaid_gantt_start(start, &ids)?);
                }
                if let Some(dur) = task.duration.as_deref().filter(|s| !s.trim().is_empty()) {
                    meta.push(dur.trim().to_string());
                } else if let Some(end) = task.end.as_deref().filter(|s| !s.trim().is_empty()) {
                    meta.push(end.trim().to_string());
                }
                body.push_str(&format!(
                    "    {} :{}\n",
                    mermaid_label(&task.name),
                    meta.join(", ")
                ));
            }
        }
        self.render_mermaid_source(&body, d.title.as_deref().unwrap_or("Gantt Chart"))
    }

    /// Sequence diagram
    #[tool(
        name = "render_sequence",
        description = r#"Render a sequence diagram (compiled to Mermaid).

- participants (optional): ordered list of names (otherwise inferred)
- messages (required): [{from, to, text, arrow?}]; arrow: solid (default)|dashed|open|cross
- title (optional)

Example:
{"messages":[{"from":"Client","to":"Server","text":"Request"},{"from":"Server","to":"Client","text":"Response","arrow":"dashed"}]}"#
    )]
    pub async fn render_sequence(
        &self,
        params: Parameters<RenderSequenceParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let d = &params.0.data;
        if d.messages.is_empty() {
            return Err(invalid("Sequence diagram requires at least one message."));
        }
        let mut body = String::from("sequenceDiagram\n");
        let mut declared: Vec<String> = Vec::new();
        let declare = |raw: &str, body: &mut String, declared: &mut Vec<String>| {
            let id = mermaid_id(raw);
            if !declared.contains(&id) {
                body.push_str(&format!("    participant {id} as {}\n", mermaid_label(raw)));
                declared.push(id);
            }
        };
        for p in &d.participants {
            declare(p, &mut body, &mut declared);
        }
        for m in &d.messages {
            declare(&m.from, &mut body, &mut declared);
            declare(&m.to, &mut body, &mut declared);
        }
        for m in &d.messages {
            body.push_str(&format!(
                "    {} {} {}: {}\n",
                mermaid_id(&m.from),
                seq_arrow(m.arrow.as_deref()),
                mermaid_id(&m.to),
                mermaid_label(&m.text)
            ));
        }
        self.render_mermaid_source(&body, d.title.as_deref().unwrap_or("Sequence Diagram"))
    }

    /// Mind map
    #[tool(
        name = "render_mindmap",
        description = r#"Render a mind map from a hierarchical root node (compiled to Mermaid).

- root (required): {text, children?: [{text, children?}]}
- title (optional)

Example:
{"root":{"text":"Project","children":[{"text":"Design","children":[{"text":"UI"}]},{"text":"Build"}]}}"#
    )]
    pub async fn render_mindmap(
        &self,
        params: Parameters<RenderMindmapParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let d = &params.0.data;
        let mut body = String::from("mindmap\n");
        mindmap_lines(&d.root, 0, &mut body)?;
        self.render_mermaid_source(&body, d.title.as_deref().unwrap_or("Mind Map"))
    }

    /// Timeline
    #[tool(
        name = "render_timeline",
        description = r#"Render a chronological timeline (compiled to Mermaid).

- periods (required): [{period, events: [string, ...]}]
- title (optional)

Example:
{"title":"Company history","periods":[{"period":"2019","events":["Founded"]},{"period":"2021","events":["Series A","First product"]}]}"#
    )]
    pub async fn render_timeline(
        &self,
        params: Parameters<RenderTimelineParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let d = &params.0.data;
        if d.periods.is_empty() {
            return Err(invalid("Timeline requires at least one period."));
        }
        let mut body = String::from("timeline\n");
        for p in &d.periods {
            let events: Vec<String> = p
                .events
                .iter()
                .map(|e| mermaid_label(e))
                .filter(|e| !e.is_empty())
                .collect();
            if events.is_empty() {
                body.push_str(&format!("    {}\n", mermaid_label(&p.period)));
            } else {
                body.push_str(&format!(
                    "    {} : {}\n",
                    mermaid_label(&p.period),
                    events.join(" : ")
                ));
            }
        }
        self.render_mermaid_source(&body, d.title.as_deref().unwrap_or("Timeline"))
    }

    /// Entity-relationship diagram
    #[tool(
        name = "render_er_diagram",
        description = r#"Render an entity-relationship (ER) diagram (compiled to Mermaid).

- entities (required): [{name, attributes?: [{name, type?, key?}]}]; key: PK|FK|UK
- relationships (optional): [{from, to, label?, cardinality?}]; cardinality: one-to-one|one-to-many (default)|many-to-one|many-to-many
- title (optional)

Example:
{"entities":[{"name":"CUSTOMER","attributes":[{"name":"id","type":"int","key":"PK"},{"name":"name","type":"string"}]},{"name":"ORDER","attributes":[{"name":"id","type":"int","key":"PK"}]}],"relationships":[{"from":"CUSTOMER","to":"ORDER","label":"places","cardinality":"one-to-many"}]}"#
    )]
    pub async fn render_er_diagram(
        &self,
        params: Parameters<RenderErParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let d = &params.0.data;
        if d.entities.is_empty() {
            return Err(invalid("ER diagram requires at least one entity."));
        }
        let mut body = String::from("erDiagram\n");
        for e in &d.entities {
            let name = mermaid_id(&e.name);
            body.push_str(&format!("    {name}[\"{}\"]\n", mermaid_label(&e.name)));
            if !e.attributes.is_empty() {
                body.push_str(&format!("    {name} {{\n"));
                for a in &e.attributes {
                    let ty = mermaid_visible_token(a.type_.as_deref().unwrap_or("string"));
                    let an = mermaid_visible_token(&a.name);
                    match a.key.as_deref().filter(|s| !s.trim().is_empty()) {
                        Some(k) => body
                            .push_str(&format!("        {ty} {an} {}\n", k.trim().to_uppercase())),
                        None => body.push_str(&format!("        {ty} {an}\n")),
                    }
                }
                body.push_str("    }\n");
            }
        }
        let mut declared: std::collections::HashSet<&str> = d
            .entities
            .iter()
            .map(|entity| entity.name.as_str())
            .collect();
        for raw in d
            .relationships
            .iter()
            .flat_map(|relation| [relation.from.as_str(), relation.to.as_str()])
        {
            if declared.insert(raw) {
                body.push_str(&format!(
                    "    {}[\"{}\"]\n",
                    mermaid_id(raw),
                    mermaid_label(raw)
                ));
            }
        }
        for r in &d.relationships {
            let label = r
                .label
                .as_deref()
                .map(mermaid_label)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "relates".to_string());
            body.push_str(&format!(
                "    {} {} {} : \"{}\"\n",
                mermaid_id(&r.from),
                er_cardinality(r.cardinality.as_deref()),
                mermaid_id(&r.to),
                label
            ));
        }
        self.render_mermaid_source(&body, d.title.as_deref().unwrap_or("ER Diagram"))
    }

    /// State diagram
    #[tool(
        name = "render_state_diagram",
        description = r#"Render a state machine diagram (compiled to Mermaid stateDiagram-v2).

- transitions (required): [{from, to, label?}]; use "[*]" as from for the start state or as to for an end state
- title (optional)

Example:
{"transitions":[{"from":"[*]","to":"Idle"},{"from":"Idle","to":"Running","label":"start"},{"from":"Running","to":"[*]","label":"stop"}]}"#
    )]
    pub async fn render_state_diagram(
        &self,
        params: Parameters<RenderStateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let d = &params.0.data;
        if d.transitions.is_empty() {
            return Err(invalid("State diagram requires at least one transition."));
        }
        let mut body = String::from("stateDiagram-v2\n");
        let mut declared = std::collections::HashSet::new();
        for raw in d
            .transitions
            .iter()
            .flat_map(|transition| [transition.from.as_str(), transition.to.as_str()])
        {
            if raw.trim() != "[*]" && declared.insert(raw) {
                body.push_str(&format!(
                    "    state \"{}\" as {}\n",
                    mermaid_label(raw),
                    mermaid_id(raw)
                ));
            }
        }
        for t in &d.transitions {
            match t.label.as_deref().filter(|s| !s.trim().is_empty()) {
                Some(l) => body.push_str(&format!(
                    "    {} --> {} : {}\n",
                    state_token(&t.from),
                    state_token(&t.to),
                    mermaid_label(l)
                )),
                None => body.push_str(&format!(
                    "    {} --> {}\n",
                    state_token(&t.from),
                    state_token(&t.to)
                )),
            }
        }
        self.render_mermaid_source(&body, d.title.as_deref().unwrap_or("State Diagram"))
    }

    /// Class / UML diagram
    #[tool(
        name = "render_class_diagram",
        description = r#"Render a class (UML) diagram (compiled to Mermaid).

- classes (required): [{name, attributes?: ["+String name", ...], methods?: ["+save()", ...]}]
- relationships (optional): [{from, to, type?, label?}]; type: inheritance|composition|aggregation|association (default)|dependency|realization
- title (optional)

Example:
{"classes":[{"name":"Animal","attributes":["+String name"],"methods":["+eat()"]},{"name":"Dog","methods":["+bark()"]}],"relationships":[{"from":"Dog","to":"Animal","type":"inheritance"}]}"#
    )]
    pub async fn render_class_diagram(
        &self,
        params: Parameters<RenderClassParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let d = &params.0.data;
        if d.classes.is_empty() {
            return Err(invalid("Class diagram requires at least one class."));
        }
        let mut body = String::from("classDiagram\n");
        for c in &d.classes {
            let name = mermaid_id(&c.name);
            body.push_str(&format!(
                "    class {name}[\"{}\"]\n",
                mermaid_label(&c.name)
            ));
            if !c.attributes.is_empty() || !c.methods.is_empty() {
                body.push_str(&format!("    class {name} {{\n"));
                for a in &c.attributes {
                    body.push_str(&format!("        {}\n", mermaid_label(a)));
                }
                for m in &c.methods {
                    body.push_str(&format!("        {}\n", mermaid_label(m)));
                }
                body.push_str("    }\n");
            }
        }
        let mut declared: std::collections::HashSet<&str> =
            d.classes.iter().map(|class| class.name.as_str()).collect();
        for raw in d
            .relationships
            .iter()
            .flat_map(|relation| [relation.from.as_str(), relation.to.as_str()])
        {
            if declared.insert(raw) {
                body.push_str(&format!(
                    "    class {}[\"{}\"]\n",
                    mermaid_id(raw),
                    mermaid_label(raw)
                ));
            }
        }
        for r in &d.relationships {
            let arrow = class_rel(r.type_.as_deref());
            match r.label.as_deref().filter(|s| !s.trim().is_empty()) {
                Some(l) => body.push_str(&format!(
                    "    {} {arrow} {} : {}\n",
                    mermaid_id(&r.from),
                    mermaid_id(&r.to),
                    mermaid_label(l)
                )),
                None => body.push_str(&format!(
                    "    {} {arrow} {}\n",
                    mermaid_id(&r.from),
                    mermaid_id(&r.to)
                )),
            }
        }
        self.render_mermaid_source(&body, d.title.as_deref().unwrap_or("Class Diagram"))
    }
}

include!("tools_charts.rs");
include!("tools_d3.rs");
include!("tools_geo.rs");
