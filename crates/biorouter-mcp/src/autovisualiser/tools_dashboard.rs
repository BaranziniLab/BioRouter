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

/// Parameters for `render_dashboard`.
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
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

        // Deserialize into the tool's own parameter struct, then call it.
        macro_rules! dispatch {
            ($($tool:literal => ($method:ident, $params_ty:ty)),+ $(,)?) => {
                match name.as_str() {
                    $(
                        $tool => {
                            let typed: $params_ty = serde_json::from_value(params)
                                .map_err(|e| invalid(format!("`{}` arguments are invalid: {e}", $tool)))?;
                            let (result, assets) =
                                common::render_fragment(self.$method(Parameters(typed))).await;
                            (result?, assets)
                        }
                    )+
                    other => {
                        return Err(invalid(format!(
                            "Unknown visualization '{other}'. Use one of the Auto Visualiser \
                             render_* tool names, e.g. render_volcano, render_heatmap, show_chart."
                        )));
                    }
                }
            };
        }

        let (result, assets) = dispatch! {
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
        };

        Ok((common::html_from_result(&result)?, assets))
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

Panel width is `full` (default) or `half` (two per row). Use `sections` for grouped reports, or the flat `panels` shorthand for a simple one."#
    )]
    pub async fn render_dashboard(
        &self,
        params: Parameters<RenderDashboardParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;

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
        let cdn = common::use_cdn();
        let mut panel_store = String::new();
        let mut asset_store = String::new();
        let mut stored_assets: Vec<Asset> = Vec::new();
        let mut failures: Vec<String> = Vec::new();
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
                        // CDN mode: the tags are tiny, so inline them per panel
                        // and skip the shared store entirely.
                        let (html, asset_keys) = if cdn {
                            let assets: Vec<Asset> = ASSET_ORDER
                                .iter()
                                .copied()
                                .filter(|a| built.assets.iter().any(|k| k == a.key()))
                                .collect();
                            (
                                html.replace(common::ASSET_PLACEHOLDER, &common::asset_html(&assets)),
                                Vec::new(),
                            )
                        } else {
                            for key in &built.assets {
                                if let Some(asset) =
                                    ASSET_ORDER.iter().find(|a| a.key() == key.as_str())
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
                            (html, built.assets.clone())
                        };

                        panel_store.push_str(&format!(
                            "<script type=\"text/plain\" id=\"autovis-panel-{panel_index}\">{}</script>\n",
                            STANDARD.encode(html.as_bytes())
                        ));
                        entry.insert("index".into(), json!(panel_index));
                        entry.insert("assets".into(), json!(asset_keys));
                        entry.insert("error".into(), Value::Null);
                        panel_index += 1;
                    }
                    None => {
                        let message = built.error.unwrap_or_else(|| "unknown error".to_string());
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

        let rendered = total_panels - failures.len();
        let mut label = format!(
            "Combined report '{}' rendered inline for the user with {rendered} figure{}.",
            p.title,
            if rendered == 1 { "" } else { "s" }
        );
        if !failures.is_empty() {
            label.push_str(&format!(
                "\n\n{} figure(s) could not be rendered and show an error card in the report. \
                 Fix the arguments and call render_dashboard again:\n{}",
                failures.len(),
                failures.join("\n")
            ));
        }

        let uri = format!("ui://dashboard/{}", slugify(&p.title));
        let mut result = common::finish(&uri, "dashboard", &label, html);

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
