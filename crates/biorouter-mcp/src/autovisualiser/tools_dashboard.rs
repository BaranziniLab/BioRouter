// Dashboard: compose several figures into a single, documented report artifact.
//
// Why this exists: every other tool returns one figure as one `ui://` resource.
// When an analysis produces six of them the user has to open six artifacts to
// see one story. `render_dashboard` renders the same figures — through the very
// same tools, unchanged — into one scrollable report with a title, contents,
// section prose, and a numbered caption under each figure.
//
// How the panels get in there: each figure is rendered in *fragment mode*
// (see `common::render_fragment`), which leaves a placeholder where the figure's
// libraries would be inlined and reports which libraries it wanted. The report
// stores each library's source exactly once and its own JS splices the sources
// back into each panel's `srcdoc`. Without this, a report containing a Mermaid
// diagram plus two D3 charts would carry ~4 MB of duplicated library code.

/// How wide a panel sits in the report grid.
const WIDTH_FULL: &str = "full";
const WIDTH_HALF: &str = "half";

/// Guard rails. Generous — a report beyond this is unreadable anyway.
const MAX_PANELS: usize = 24;
const MAX_RECEIPT_FAILURES: usize = 8;
const MAX_PROSE_LEN: usize = 8_000;

/// Which figure to render in a panel, and with what arguments.
///
/// Accepts the obvious shapes a model might emit:
///   `{"tool": "render_volcano", "params": {...}}`
///   `{"type": "volcano", "params": {...}}`
///   `{"tool": "render_volcano", "data": {...}}`   ← bare tool args, no `params`
#[derive(Debug, Serialize, rmcp::schemars::JsonSchema)]
pub struct DashboardFigure {
    /// The Auto Visualiser tool to render, e.g. `render_volcano`, `show_chart`,
    /// `render_mermaid`. The `render_` prefix may be omitted.
    pub tool: String,
    /// Exactly the arguments you would pass to that tool on its own,
    /// e.g. `{"data": {...}}`.
    pub params: Value,
}

impl<'de> Deserialize<'de> for DashboardFigure {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error as DeError;

        // Some models stringify nested tool-call arguments; accept that too.
        let value = match Value::deserialize(d)? {
            Value::String(s) => serde_json::from_str::<Value>(&s).map_err(|e| {
                D::Error::custom(format!("`figure` was a JSON string that did not parse: {e}"))
            })?,
            other => other,
        };

        let mut map = match value {
            Value::Object(map) => map,
            _ => return Err(D::Error::custom("`figure` must be a JSON object")),
        };

        let tool = ["tool", "type", "name", "kind"]
            .iter()
            .find_map(|key| map.remove(*key))
            .ok_or_else(|| {
                D::Error::custom("`figure` needs a `tool` naming the visualization to render")
            })?;
        let tool = match tool {
            Value::String(s) => s,
            other => return Err(D::Error::custom(format!("`figure.tool` must be a string, got {other}"))),
        };

        // An explicit `params`/`arguments` wins; otherwise whatever else is on
        // the object *is* the tool's arguments.
        let params = ["params", "arguments", "args", "input"]
            .iter()
            .find_map(|key| map.remove(*key))
            .unwrap_or(Value::Object(map));

        Ok(DashboardFigure { tool, params })
    }
}

/// One figure in the report, with the prose that explains it.
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct DashboardPanel {
    /// Short heading for this figure, shown above it. Strongly recommended.
    #[serde(default)]
    pub title: Option<String>,
    /// One or two sentences saying what the figure shows and what to look at.
    /// Supports `**bold**`, `*italic*`, `` `code` `` and `[links](https://…)`.
    #[serde(default)]
    pub caption: Option<String>,
    /// Longer methods / interpretation text, shown in a collapsed "Notes &
    /// methods" disclosure under the figure.
    #[serde(default)]
    pub notes: Option<String>,
    /// `full` (default, one figure per row) or `half` (two side by side).
    #[serde(default)]
    pub width: Option<String>,
    /// Fixed panel height in CSS px. Omit to let the figure size itself.
    #[serde(default)]
    pub height: Option<u32>,
    /// The visualization to render here.
    pub figure: DashboardFigure,
}

/// A titled group of panels.
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct DashboardSection {
    /// Section heading, e.g. "Quality control".
    #[serde(default)]
    pub title: Option<String>,
    /// Prose introducing the section. Blank lines separate paragraphs; lines
    /// starting with `- ` become bullets.
    #[serde(default)]
    pub description: Option<String>,
    /// The figures in this section.
    #[serde(default)]
    pub panels: Vec<DashboardPanel>,
}

/// Report colour theme. `Auto` (default) follows the desktop app's light/dark
/// setting so the report matches the rest of the UI (and stays identical in the
/// side-panel preview and the expanded view); `Light`/`Dark` force a look
/// regardless of the host — set one when the user asks for a specific background.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DashboardTheme {
    #[default]
    Auto,
    Light,
    Dark,
}

impl DashboardTheme {
    /// The value to bake into `window.__BR_VIZ_THEME__`, or `None` to follow the host.
    fn forced(self) -> Option<&'static str> {
        match self {
            DashboardTheme::Auto => None,
            DashboardTheme::Light => Some("light"),
            DashboardTheme::Dark => Some("dark"),
        }
    }
}

/// Bake a locked theme into an assembled report: `window.__BR_VIZ_THEME__` runs
/// before the report's `{{COMMON}}` (right after `<head>`), so `resolveTheme`
/// honours it and the report propagates it down to every panel.
fn inject_forced_theme(html: String, theme: &str) -> String {
    let tag = format!("<script>window.__BR_VIZ_THEME__=\"{theme}\";</script>");
    match html.split_once("<head>") {
        Some((before, after)) => {
            let mut out = String::with_capacity(html.len() + tag.len());
            out.push_str(before);
            out.push_str("<head>");
            out.push_str(&tag);
            out.push_str(after);
            out
        }
        None => format!("{tag}{html}"),
    }
}

/// Parameters for `render_dashboard`.
#[derive(Debug, Serialize, rmcp::schemars::JsonSchema)]
pub struct RenderDashboardParams {
    /// The report's title, e.g. "Differential expression: tumour vs normal".
    pub title: String,
    /// Optional one-line standfirst under the title.
    #[serde(default)]
    pub subtitle: Option<String>,
    /// Opening prose: what this report covers and the headline findings.
    /// Blank lines separate paragraphs; lines starting with `- ` become bullets.
    #[serde(default)]
    pub summary: Option<String>,
    /// Grouped figures. Use this when the report has distinct parts.
    #[serde(default)]
    pub sections: Option<Vec<DashboardSection>>,
    /// Shorthand for a single unnamed section. Use `sections` or `panels`, not both.
    #[serde(default)]
    pub panels: Option<Vec<DashboardPanel>>,
    /// Closing prose: caveats, data provenance, next steps.
    #[serde(default)]
    pub footer: Option<String>,
    /// Report colour theme: `auto` (default, follows the app's light/dark setting),
    /// `light`, or `dark`. Set `light` or `dark` when the user asks for a specific
    /// background; leave it `auto` to match whatever theme the app is in.
    #[serde(default)]
    pub theme: DashboardTheme,
}

/// The exact shape, deserialized once [`normalize_dashboard_args`] has coaxed the
/// model's arguments into it.
#[derive(Deserialize)]
struct RenderDashboardParamsRaw {
    title: String,
    #[serde(default)]
    subtitle: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    sections: Option<Vec<DashboardSection>>,
    #[serde(default)]
    panels: Option<Vec<DashboardPanel>>,
    #[serde(default)]
    footer: Option<String>,
    #[serde(default)]
    theme: DashboardTheme,
}

/// Parse a value that may have arrived as a JSON string instead of JSON.
fn de_stringified(value: Value) -> Value {
    match value {
        Value::String(s) => serde_json::from_str(&s).unwrap_or(Value::String(s)),
        other => other,
    }
}

/// Reshape the arguments a model actually sends into the documented shape.
///
/// Every *other* Auto Visualiser tool takes a single `data` argument, so models
/// generalise and wrap the whole report in one — observed with GPT-5.5, which
/// sent `{"data": {"title": …, "sections": […]}}` and then retried identically
/// after the rejection. Some models also stringify nested arguments. Rejecting
/// either costs the user a wasted turn for no reason, so accept both.
fn normalize_dashboard_args(value: Value) -> Value {
    let mut value = de_stringified(value);

    // Unwrap a `data` (or `dashboard`/`report`) envelope that carries the report.
    if let Value::Object(map) = &value {
        if !map.contains_key("title") {
            if let Some(inner) = ["data", "dashboard", "report"]
                .iter()
                .find_map(|key| map.get(*key))
            {
                let unwrapped = de_stringified(inner.clone());
                if unwrapped.is_object() {
                    value = unwrapped;
                }
            }
        }
    }

    // `sections` / `panels` may themselves arrive stringified.
    if let Value::Object(map) = &mut value {
        for key in ["sections", "panels"] {
            if let Some(entry) = map.get_mut(key) {
                *entry = de_stringified(entry.clone());
            }
        }
    }
    value
}

impl<'de> Deserialize<'de> for RenderDashboardParams {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error as DeError;
        let value = normalize_dashboard_args(Value::deserialize(d)?);
        let raw: RenderDashboardParamsRaw = serde_json::from_value(value).map_err(|e| {
            DeError::custom(format!(
                "{e}. `render_dashboard` takes the report directly \
                 (title, summary, sections/panels), not wrapped in a `data` argument."
            ))
        })?;
        Ok(RenderDashboardParams {
            title: raw.title,
            subtitle: raw.subtitle,
            summary: raw.summary,
            sections: raw.sections,
            panels: raw.panels,
            footer: raw.footer,
            theme: raw.theme,
        })
    }
}

/// Canonicalise a model-supplied tool name: `Volcano`, `volcano`,
/// `render-volcano` and `render_volcano` all mean the same tool.
fn normalize_tool_name(raw: &str) -> String {
    let base = raw
        .trim()
        .to_lowercase()
        .replace([' ', '-', '.'], "_")
        .replace("__", "_");
    match base.as_str() {
        // The one tool that isn't `render_*`.
        "chart" | "show_chart" | "render_chart" => "show_chart".to_string(),
        other if other.starts_with("render_") => other.to_string(),
        other => format!("render_{other}"),
    }
}

/// Turn a title into a URI slug: "Tumour vs Normal" -> "tumour-vs-normal".
fn slugify(title: &str) -> String {
    let slug: String = title
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "report".to_string()
    } else {
        slug.chars().take(60).collect()
    }
}

fn check_prose(value: &Option<String>, what: &str) -> Result<(), ErrorData> {
    if let Some(text) = value {
        if text.len() > MAX_PROSE_LEN {
            return Err(invalid(format!(
                "{what} is too long ({} chars, max {MAX_PROSE_LEN}). Summarise it.",
                text.len()
            )));
        }
    }
    Ok(())
}

/// Embed a library source in the report as an inert, readable text block.
///
/// The HTML tokenizer ends a `<script>` at the first `</script`, even in a
/// `text/plain` block, so a source containing that sequence is base64-encoded
/// instead. Our vendored libraries don't, but a future asset bump might.
fn asset_store_entry(key: &str, kind: &str, src: &str) -> String {
    if src.to_lowercase().contains("</script") {
        format!(
            "<script type=\"text/plain\" data-autovis-asset=\"{key}\" data-kind=\"{kind}\" data-b64=\"1\">{}</script>\n",
            STANDARD.encode(src.as_bytes())
        )
    } else {
        format!(
            "<script type=\"text/plain\" data-autovis-asset=\"{key}\" data-kind=\"{kind}\">{src}</script>\n"
        )
    }
}

/// The outcome of rendering one panel's figure.
struct BuiltPanel {
    /// The figure's HTML, still carrying `ASSET_PLACEHOLDER`. `None` on failure.
    html: Option<String>,
    /// Library keys this figure needs, in load order.
    assets: Vec<String>,
    /// Why the figure could not be rendered.
    error: Option<String>,
}

#[tool_router(router = dashboard_router)]
impl AutoVisualiserRouter {
    /// Render every panel, keeping failures local to their own panel.
    async fn build_panels(&self, panels: &[DashboardPanel]) -> Vec<BuiltPanel> {
        let mut built = Vec::with_capacity(panels.len());
        for panel in panels {
            built.push(self.build_panel(&panel.figure).await);
        }
        built
    }

    async fn build_panel(&self, figure: &DashboardFigure) -> BuiltPanel {
        match self.render_figure_fragment(figure).await {
            Ok((html, assets)) => BuiltPanel {
                html: Some(html),
                assets: assets.iter().map(|a| a.key().to_string()).collect(),
                error: None,
            },
            Err(e) => BuiltPanel {
                html: None,
                assets: Vec::new(),
                error: Some(e.message.to_string()),
            },
        }
    }

    /// Call the real figure tool in fragment mode and recover its HTML.
    ///
    /// Every panel therefore inherits that tool's validation, limits and
    /// template verbatim — a dashboard volcano plot is byte-for-byte the same
    /// figure as a standalone one, minus the duplicated libraries.
    async fn render_figure_fragment(
        &self,
        figure: &DashboardFigure,
    ) -> Result<(String, Vec<Asset>), ErrorData> {
        let name = normalize_tool_name(&figure.tool);
        let params = figure.params.clone();
        let (result, assets) = common::render_fragment(self.call_figure_tool(&name, params)).await;
        Ok((common::html_from_result(&result?)?, assets))
    }

    /// Dispatch a normalized single-figure tool name to its implementation.
    ///
    /// This is the one table mapping `render_*`/`show_chart` names onto the real
    /// tool methods and their parameter structs. Both the dashboard (which wraps
    /// this in [`common::render_fragment`]) and the standalone embedding API
    /// ([`render_standalone_figure`]) go through here, so a figure is
    /// byte-for-byte identical however it is reached. `render_dashboard` is not in
    /// this table — it composes these figures rather than being one.
    async fn call_figure_tool(
        &self,
        name: &str,
        params: Value,
    ) -> Result<CallToolResult, ErrorData> {
        // Deserialize into the tool's own parameter struct, then call it.
        macro_rules! dispatch {
            ($($tool:literal => ($method:ident, $params_ty:ty)),+ $(,)?) => {
                match name {
                    $(
                        $tool => {
                            let typed: $params_ty = serde_json::from_value(params)
                                .map_err(|e| invalid(format!("`{}` arguments are invalid: {e}", $tool)))?;
                            self.$method(Parameters(typed)).await
                        }
                    )+
                    other => Err(invalid(format!(
                        "Unknown visualization '{other}'. Use one of the Auto Visualiser \
                         render_* tool names, e.g. render_volcano, render_heatmap, show_chart."
                    ))),
                }
            };
        }

        dispatch! {
            "show_chart"             => (show_chart, ShowChartParams),
            "render_sankey"          => (render_sankey, RenderSankeyParams),
            "render_radar"           => (render_radar, RenderRadarParams),
            "render_donut"           => (render_donut, RenderDonutParams),
            "render_treemap"         => (render_treemap, RenderTreemapParams),
            "render_chord"           => (render_chord, RenderChordParams),
            "render_map"             => (render_map, RenderMapParams),
            "render_mermaid"         => (render_mermaid, RenderMermaidParams),
            "render_histogram"       => (render_histogram, RenderHistogramParams),
            "render_bubble"          => (render_bubble, RenderBubbleParams),
            "render_area"            => (render_area, RenderAreaParams),
            "render_gauge"           => (render_gauge, RenderGaugeParams),
            "render_volcano"         => (render_volcano, RenderVolcanoParams),
            "render_manhattan"       => (render_manhattan, RenderManhattanParams),
            "render_network"         => (render_network, RenderNetworkParams),
            "render_heatmap"         => (render_heatmap, RenderHeatmapParams),
            "render_sunburst"        => (render_sunburst, RenderSunburstParams),
            "render_dendrogram"      => (render_dendrogram, RenderDendrogramParams),
            "render_calendar_heatmap"=> (render_calendar_heatmap, RenderCalendarParams),
            "render_boxplot"         => (render_boxplot, RenderBoxplotParams),
            "render_wordcloud"       => (render_wordcloud, RenderWordcloudParams),
            "render_kaplan_meier"    => (render_kaplan_meier, RenderKaplanMeierParams),
            "render_forest"          => (render_forest, RenderForestParams),
            "render_flowchart"       => (render_flowchart, RenderFlowchartParams),
            "render_gantt"           => (render_gantt, RenderGanttParams),
            "render_sequence"        => (render_sequence, RenderSequenceParams),
            "render_mindmap"         => (render_mindmap, RenderMindmapParams),
            "render_timeline"        => (render_timeline, RenderTimelineParams),
            "render_er_diagram"      => (render_er_diagram, RenderErParams),
            "render_state_diagram"   => (render_state_diagram, RenderStateParams),
            "render_class_diagram"   => (render_class_diagram, RenderClassParams),
            "render_choropleth"      => (render_choropleth, RenderChoroplethParams),
        }
    }

    /// Combine several figures into one documented, self-contained report.
    #[tool(
        name = "render_dashboard",
        description = r#"Combine several figures into ONE scrollable report artifact, with a title, contents, section prose and a numbered caption under each figure.

Use this WHENEVER an answer needs more than one figure. Calling render_* several times leaves the user opening one artifact per figure; render_dashboard gives them a single page that tells the whole story.

Each panel names any other Auto Visualiser tool and passes exactly the arguments that tool takes on its own.

Example:
{
  "title": "Tumour vs normal: differential expression",
  "subtitle": "RNA-seq, n=48",
  "summary": "412 genes pass FDR < 0.05. **MYC** and **CDK4** dominate the up-regulated set.",
  "sections": [
    {
      "title": "Genome-wide signal",
      "description": "Effect size against significance across all 18,204 tested genes.",
      "panels": [
        {
          "title": "Volcano plot",
          "caption": "Points above the dashed line pass FDR < 0.05.",
          "notes": "Wald test, Benjamini-Hochberg correction.",
          "figure": {"tool": "render_volcano", "params": {"data": {"points": []}}}
        },
        {
          "title": "Top-gene expression",
          "width": "half",
          "figure": {"tool": "show_chart", "params": {"data": {"type": "bar", "datasets": []}}}
        }
      ]
    }
  ],
  "footer": "Counts from GENCODE v44."
}

Panel width is `full` (default) or `half` (two per row). Use `sections` for grouped reports, or the flat `panels` shorthand for a simple one.

Set `theme` to `light` or `dark` if the user asks for a specific background; the default `auto` follows the app's own light/dark setting.

Call this ONCE per report: the result contains the complete artifact for the side panel. Do not call it again merely to display, finalise or confirm it. Inspect the existing artifact to verify rendering; generation alone is not visual verification. Call it again only to change the report or correct figures that failed."#
    )]
    pub async fn render_dashboard(
        &self,
        params: Parameters<RenderDashboardParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        // Copy out the theme before `p`'s other fields are moved below.
        let forced_theme = p.theme.forced();

        if p.title.trim().is_empty() {
            return Err(invalid("Dashboard requires a non-empty `title`."));
        }
        check_prose(&p.summary, "`summary`")?;
        check_prose(&p.footer, "`footer`")?;

        // `sections` and `panels` are two spellings of the same thing.
        let mut sections = match (p.sections, p.panels) {
            (Some(sections), None) => sections,
            (None, Some(panels)) => vec![DashboardSection {
                title: None,
                description: None,
                panels,
            }],
            (Some(sections), Some(panels)) if panels.is_empty() => sections,
            (Some(_), Some(_)) => {
                return Err(invalid(
                    "Provide either `sections` or `panels`, not both.",
                ));
            }
            (None, None) => {
                return Err(invalid(
                    "Dashboard requires at least one figure: pass `panels` or `sections`.",
                ));
            }
        };
        sections.retain(|section| !section.panels.is_empty());

        let total_panels: usize = sections.iter().map(|s| s.panels.len()).sum();
        if total_panels == 0 {
            return Err(invalid(
                "Dashboard requires at least one figure: every section was empty.",
            ));
        }
        check_limit(total_panels, MAX_PANELS, "dashboard panels")?;

        for section in &sections {
            check_prose(&section.description, "a section `description`")?;
            for panel in &section.panels {
                check_prose(&panel.caption, "a panel `caption`")?;
                check_prose(&panel.notes, "a panel `notes`")?;
                if let Some(width) = &panel.width {
                    let w = width.trim().to_lowercase();
                    if w != WIDTH_FULL && w != WIDTH_HALF {
                        return Err(invalid(format!(
                            "Panel `width` must be '{WIDTH_FULL}' or '{WIDTH_HALF}', got '{width}'."
                        )));
                    }
                }
            }
        }

        // --- render every figure -------------------------------------------
        let mut panel_store = String::new();
        let mut asset_store = String::new();
        let mut stored_assets: Vec<Asset> = Vec::new();
        let mut failures: Vec<String> = Vec::new();
        let mut failure_receipts = Vec::new();
        let mut json_sections = Vec::with_capacity(sections.len());
        let mut panel_index = 0usize;
        let mut figure_number = 0usize;

        for section in &sections {
            let built = self.build_panels(&section.panels).await;
            let mut json_panels = Vec::with_capacity(section.panels.len());

            for (panel, built) in section.panels.iter().zip(built) {
                figure_number += 1;

                let mut entry = serde_json::Map::new();
                entry.insert("title".into(), json!(panel.title));
                entry.insert("caption".into(), json!(panel.caption));
                entry.insert("notes".into(), json!(panel.notes));
                entry.insert("height".into(), json!(panel.height));
                entry.insert(
                    "width".into(),
                    json!(panel
                        .width
                        .as_deref()
                        .map(|w| w.trim().to_lowercase())
                        .unwrap_or_else(|| WIDTH_FULL.to_string())),
                );

                match built.html {
                    Some(html) => {
                        // A report always inlines its libraries, even when
                        // `BIOROUTER_AUTOVIS_CDN` is on — which the desktop app sets by
                        // default. CDN mode only ever worked for a *standalone* figure,
                        // because the Electron main process rewrites that figure's
                        // `<script src=…>` back into an inline script before display: the
                        // renderer's CSP is `script-src 'self' 'unsafe-inline'`, so a
                        // remote script never loads. A report keeps its library tags
                        // inside base64 asset/panel blobs, where that rewriter cannot
                        // reach them, so a CDN report rendered blank figures with
                        // "Chart is not defined". Inlining costs little: the shared store
                        // holds each library exactly once, however many panels use it.
                        for key in &built.assets {
                            if let Some(asset) = ASSET_ORDER.iter().find(|a| a.key() == key.as_str())
                            {
                                if !stored_assets.contains(asset) {
                                    stored_assets.push(*asset);
                                    for (kind, src) in asset.sources() {
                                        asset_store
                                            .push_str(&asset_store_entry(asset.key(), kind, src));
                                    }
                                }
                            }
                        }

                        panel_store.push_str(&format!(
                            "<script type=\"text/plain\" id=\"autovis-panel-{panel_index}\">{}</script>\n",
                            STANDARD.encode(html.as_bytes())
                        ));
                        entry.insert("index".into(), json!(panel_index));
                        entry.insert("assets".into(), json!(built.assets.clone()));
                        entry.insert("error".into(), Value::Null);
                        panel_index += 1;
                    }
                    None => {
                        let message = built.error.unwrap_or_else(|| "unknown error".to_string());
                        if failure_receipts.len() < MAX_RECEIPT_FAILURES {
                            failure_receipts.push(json!({
                                "figure": figure_number,
                                "tool": panel.figure.tool.chars().take(64).collect::<String>(),
                                "error": message.chars().take(128).collect::<String>(),
                                "detailsTruncated": panel.figure.tool.chars().nth(64).is_some()
                                    || message.chars().nth(128).is_some(),
                            }));
                        }
                        failures.push(format!(
                            "Figure {figure_number} ({}): {message}",
                            panel.title.as_deref().unwrap_or(&panel.figure.tool)
                        ));
                        entry.insert("index".into(), Value::Null);
                        entry.insert("assets".into(), json!([] as [&str; 0]));
                        entry.insert("error".into(), json!(message));
                    }
                }
                json_panels.push(Value::Object(entry));
            }

            json_sections.push(json!({
                "title": section.title,
                "description": section.description,
                "panels": json_panels,
            }));
        }

        if failures.len() == total_panels {
            return Err(invalid(format!(
                "Every figure in the dashboard failed to render:\n{}",
                failures.join("\n")
            )));
        }

        // --- assemble the report --------------------------------------------
        let data = json!({
            "title": p.title,
            "subtitle": p.subtitle,
            "summary": p.summary,
            "footer": p.footer,
            "sections": json_sections,
        });
        let data_json = js_data(&data)?;
        let placeholder_literal = js_data(&Value::String(common::ASSET_PLACEHOLDER.to_string()))?;
        // `assemble` substitutes in order, so any `{{…}}` surviving inside a
        // user-supplied value would be treated as a later placeholder. Titles are
        // the only user text substituted before the end, so neutralise braces.
        let title_html = html_escape(&p.title).replace('{', "&#123;");

        let html = common::assemble(
            include_str!("templates/dashboard_template.html"),
            &[],
            &[
                // The template does `html.replace('{{ASSET_PLACEHOLDER}}', …)`;
                // this substitutes the quoted JS string it matches against.
                ("'{{ASSET_PLACEHOLDER}}'", &placeholder_literal),
                ("{{ASSET_STORE}}", &asset_store),
                ("{{PANEL_STORE}}", &panel_store),
                ("{{TITLE}}", &title_html),
                // Last: user prose lands here, so nothing can rewrite it.
                ("{{DASHBOARD_DATA}}", &data_json),
            ],
        );

        // When the user asked for a specific look, lock it in so both the preview
        // and the expanded view honour it; otherwise the report follows the host.
        let html = match forced_theme {
            Some(theme) => inject_forced_theme(html, theme),
            None => html,
        };

        let rendered = total_panels - failures.len();
        let mut label = format!(
            "Combined report '{}' created for the artifact panel with {rendered} figure{}.",
            p.title,
            if rendered == 1 { "" } else { "s" }
        );
        if failures.is_empty() {
            // Discourage duplicate generation without claiming that the client
            // has rendered or visually verified the HTML this function returns.
            label.push_str(
                " The report is complete and ready for the artifact panel, so you do \
                 not need to call render_dashboard again to display, finalise or confirm it. \
                 Inspect the existing artifact to verify rendering. Call this tool again \
                 only to change the report or correct a rendering failure.",
            );
        } else {
            // render_dashboard is stateless and re-renders the WHOLE report, so tell
            // the model to re-send every panel (not just the failed ones) — otherwise
            // it drops the panels that rendered fine on the retry.
            label.push_str(&format!(
                "\n\n{} figure(s) could not be rendered and show an error card in the report. \
                 Re-send the whole report with these figures' arguments fixed (keep the panels \
                 that rendered):\n{}",
                failures.len(),
                failures.join("\n")
            ));
        }

        let uri = format!("ui://dashboard/{}", slugify(&p.title));
        let mut result = common::finish(&uri, "dashboard", &label, html);
        // Keep recovery outside user-controlled titles and bounded error text.
        // Clients preferring structured content must still see partial failures.
        result.structured_content = Some(json!({
            "status": if failures.is_empty() { "created" } else { "created_with_errors" },
            "uri": uri,
            "mimeType": "text/html",
            "summary": format!("Report artifact created with {rendered} figures; {} failed.", failures.len()),
            "figuresCreated": rendered,
            "figuresFailed": failures.len(),
            "failuresOmitted": failures.len().saturating_sub(failure_receipts.len()),
            "failures": failure_receipts,
            "recovery": if failures.is_empty() {
                "Inspect the existing artifact to verify rendering; do not regenerate it merely to display or confirm it."
            } else {
                "Inspect all error cards in the existing artifact. Re-send the whole report with failed panels corrected, retaining successful panels."
            },
        }));

        // Reports are pages, not figures: ask for a reading-pane frame.
        if let Some(content) = result.content.first_mut() {
            if let rmcp::model::RawContent::Resource(embedded) = &mut content.raw {
                if let ResourceContents::BlobResourceContents { meta, .. } = &mut embedded.resource {
                    let mut meta_obj = serde_json::Map::new();
                    meta_obj.insert(
                        "mcpui.dev/ui-preferred-frame-size".to_string(),
                        json!(["1200px", "860px"]),
                    );
                    *meta = Some(rmcp::model::Meta(meta_obj));
                }
            }
        }
        Ok(result)
    }
}

/// Every asset, in the order libraries must load. Used to map the string keys a
/// panel reports back onto the `Asset` values that own the sources.
const ASSET_ORDER: [Asset; 5] = [
    Asset::D3,
    Asset::D3Sankey,
    Asset::ChartJs,
    Asset::Leaflet,
    Asset::Mermaid,
];

/// Render one named Auto Visualiser figure as a complete, self-contained
/// HTML document (inlined assets), for embedding in a sandboxed iframe.
/// `tool` is the tool name with or without the `render_`/`show_` prefix
/// (e.g. "kaplan_meier", "render_kaplan_meier", "show_chart").
/// Returns Err with a human-fixable message for unknown tools or invalid args.
///
/// `render_dashboard` is accepted too — a report embedded in an app is
/// legitimate. `args` are exactly the arguments the tool takes on its own
/// (e.g. `{"data": …}`).
///
/// Assets are always inlined, ignoring `BIOROUTER_AUTOVIS_CDN`: the document
/// lands in a `srcdoc` iframe the Electron CDN→inline rewriter cannot reach, so a
/// remote `<script src=…>` would be blocked by the renderer CSP and render blank
/// — the same reasoning that makes a dashboard inline its libraries.
pub async fn render_standalone_figure(tool: &str, args: Value) -> Result<String, String> {
    let router = AutoVisualiserRouter::new();
    let name = normalize_tool_name(tool);
    let result = common::with_inline_assets(async move {
        if name == "render_dashboard" {
            let params: RenderDashboardParams = serde_json::from_value(args)
                .map_err(|e| invalid(format!("`render_dashboard` arguments are invalid: {e}")))?;
            router.render_dashboard(Parameters(params)).await
        } else {
            router.call_figure_tool(&name, args).await
        }
    })
    .await
    .map_err(|e| e.message.to_string())?;
    common::html_from_result(&result).map_err(|e| e.message.to_string())
}
