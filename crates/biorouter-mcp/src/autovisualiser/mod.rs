//! Auto Visualiser MCP server.
//!
//! Each tool turns structured data into a self-contained interactive HTML figure
//! and returns it as a `ui://…` resource for inline rendering. All tools share
//! the pipeline in [`common`]: validate → JSON-encode (safely) → inject into a
//! template with the libraries it needs → return a `CallToolResult`.

mod common;

use common::{
    check_limit, html_escape, invalid, js_data, js_value, render, Asset, MAX_LABELS, MAX_LINKS,
    MAX_MARKERS, MAX_MATRIX_DIM, MAX_MERMAID_LEN, MAX_NODES, MAX_TREE_DEPTH, MAX_VALUES,
};
use indoc::formatdoc;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ErrorData, Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

// ===========================================================================
// Shared enums (lenient parsing: accept any case / surrounding whitespace).
// ===========================================================================

/// Chart type for `show_chart`.
#[derive(Debug, Serialize, rmcp::schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChartType {
    Line,
    Scatter,
    Bar,
}

impl<'de> Deserialize<'de> for ChartType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(
            match common::parse_keyword::<D::Error>(&s, &["line", "scatter", "bar"])?.as_str() {
                "scatter" => ChartType::Scatter,
                "bar" => ChartType::Bar,
                _ => ChartType::Line,
            },
        )
    }
}

/// Chart type for `render_donut`.
#[derive(Debug, Serialize, rmcp::schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DonutChartType {
    Doughnut,
    Pie,
}

impl<'de> Deserialize<'de> for DonutChartType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(
            match common::parse_keyword::<D::Error>(&s, &["doughnut", "pie", "donut"])?.as_str() {
                "pie" => DonutChartType::Pie,
                _ => DonutChartType::Doughnut,
            },
        )
    }
}

// ===========================================================================
// Parameter / data structs
// ===========================================================================

/// Sankey node structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SankeyNode {
    /// The name of the node
    pub name: String,
    /// Optional category for the node
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// Sankey link structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SankeyLink {
    /// Source node name
    pub source: String,
    /// Target node name
    pub target: String,
    /// Flow value
    pub value: f64,
}

/// Sankey data structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SankeyData {
    /// Array of nodes
    pub nodes: Vec<SankeyNode>,
    /// Array of links between nodes
    pub links: Vec<SankeyLink>,
}

/// Parameters for render_sankey tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderSankeyParams {
    /// The data for the Sankey diagram
    pub data: SankeyData,
}

/// Radar dataset structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RadarDataset {
    /// Label for this dataset
    pub label: String,
    /// Data values for each category
    pub data: Vec<f64>,
}

/// Radar chart data structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RadarData {
    /// Category labels
    pub labels: Vec<String>,
    /// Datasets to compare
    pub datasets: Vec<RadarDataset>,
}

/// Parameters for render_radar tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderRadarParams {
    /// The data for the radar chart
    pub data: RadarData,
}

/// Data item for donut/pie charts - can be a number or labeled value
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
#[serde(untagged)]
pub enum DonutDataItem {
    /// Simple numeric value
    Number(f64),
    /// Labeled value with explicit label
    LabeledValue {
        /// Label for this data point
        label: String,
        /// Numeric value
        value: f64,
    },
}

/// Single donut/pie chart data
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SingleDonutChart {
    /// Data values - can be numbers or objects with label and value
    pub data: Vec<DonutDataItem>,
    /// Optional chart title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional chart type (doughnut or pie)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub chart_type: Option<DonutChartType>,
    /// Optional labels array (used when data is just numbers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
}

/// Donut chart data wrapper - matches the old schema structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
#[serde(untagged)]
pub enum DonutChartData {
    /// Single donut chart
    Single(SingleDonutChart),
    /// Multiple donut charts
    Multiple(Vec<SingleDonutChart>),
}

/// Root structure for donut chart data - matches old schema
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct DonutData {
    /// The chart data (single or multiple charts)
    pub data: DonutChartData,
}

/// Parameters for render_donut tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderDonutParams {
    /// The data for the donut/pie chart(s) - wrapped in data property
    #[serde(flatten)]
    pub data: DonutData,
}

/// Treemap node structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct TreemapNode {
    /// Name of the node
    pub name: String,
    /// Value for leaf nodes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Category for coloring
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Children nodes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<TreemapNode>>,
}

/// Parameters for render_treemap tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderTreemapParams {
    /// The hierarchical data for the treemap
    pub data: TreemapNode,
}

/// Chord diagram data structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ChordData {
    /// Labels for each entity
    pub labels: Vec<String>,
    /// 2D matrix of flows (matrix[i][j] = flow from i to j)
    pub matrix: Vec<Vec<f64>>,
}

/// Parameters for render_chord tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderChordParams {
    /// The data for the chord diagram
    pub data: ChordData,
}

/// Map marker structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MapMarker {
    /// Latitude (required)
    pub lat: f64,
    /// Longitude (required)
    pub lng: f64,
    /// Location name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Numeric value for sizing/coloring
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Description text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Custom popup HTML
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popup: Option<String>,
    /// Custom marker color
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Custom marker label
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Use default Leaflet icon
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "useDefaultIcon")]
    pub use_default_icon: Option<bool>,
}

/// Map center point
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MapCenter {
    /// Latitude
    pub lat: f64,
    /// Longitude
    pub lng: f64,
}

/// Map data structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MapData {
    /// Array of markers
    pub markers: Vec<MapMarker>,
    /// Optional title for the map
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional subtitle
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Optional center point
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center: Option<MapCenter>,
    /// Optional initial zoom level
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom: Option<f64>,
    /// Optional boolean to enable/disable clustering
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clustering: Option<bool>,
    /// Optional cluster radius
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "clusterRadius")]
    pub cluster_radius: Option<f64>,
    /// Optional boolean to auto-fit map to markers
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "autoFit")]
    pub auto_fit: Option<bool>,
}

/// Parameters for render_map tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderMapParams {
    /// The data for the map visualization
    pub data: MapData,
}

/// Chart data point for scatter charts
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ChartPoint {
    /// X coordinate
    pub x: f64,
    /// Y coordinate
    pub y: f64,
}

/// Chart dataset structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ChartDataset {
    /// Label for this dataset
    pub label: String,
    /// Data points - can be numbers or x/y points
    pub data: ChartDataValues,
    /// Optional background color for the dataset
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "backgroundColor")]
    pub background_color: Option<String>,
    /// Optional border color for the dataset
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "borderColor")]
    pub border_color: Option<String>,
    /// Optional border width for the dataset
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "borderWidth")]
    pub border_width: Option<f64>,
    /// Optional tension for line curves (0 = straight lines, higher = more curved)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tension: Option<f64>,
    /// Optional fill setting for area under the line
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<bool>,
}

/// Chart data values - can be simple numbers or x/y points
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
#[serde(untagged)]
pub enum ChartDataValues {
    /// Simple numeric values (for line/bar charts with labels)
    Numbers(Vec<f64>),
    /// X/Y points (for scatter charts or line charts without labels)
    Points(Vec<ChartPoint>),
}

/// Chart data structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ChartData {
    /// Chart type
    #[serde(rename = "type")]
    pub chart_type: ChartType,
    /// Datasets to display
    pub datasets: Vec<ChartDataset>,
    /// Optional labels for x-axis (for line/bar charts)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    /// Optional chart title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional subtitle
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Optional x-axis label
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "xAxisLabel")]
    pub x_axis_label: Option<String>,
    /// Optional y-axis label
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "yAxisLabel")]
    pub y_axis_label: Option<String>,
}

/// Parameters for show_chart tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ShowChartParams {
    /// The data for the chart
    pub data: ChartData,
}

/// Parameters for render_mermaid tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderMermaidParams {
    /// The Mermaid diagram code to render
    pub mermaid_code: String,
}

// ===========================================================================
// Router
// ===========================================================================

/// An extension for automatic data visualization and UI generation
#[derive(Clone)]
pub struct AutoVisualiserRouter {
    tool_router: ToolRouter<Self>,
    #[allow(dead_code)]
    cache_dir: PathBuf,
    instructions: String,
}

impl Default for AutoVisualiserRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AutoVisualiserRouter {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "biorouter-autovisualiser".to_string(),
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

#[tool_router(router = tool_router)]
impl AutoVisualiserRouter {
    pub fn new() -> Self {
        use etcetera::{choose_app_strategy, AppStrategy};
        // - macOS/Linux: ~/.cache/biorouter/autovisualiser/
        // - Windows:     ~\AppData\Local\BaranziniLab\BioRouter\cache\autovisualiser\
        let cache_dir = choose_app_strategy(crate::APP_STRATEGY.clone())
            .unwrap()
            .cache_dir()
            .join("autovisualiser");
        let _ = std::fs::create_dir_all(&cache_dir);

        let instructions = formatdoc! {r#"
            This extension provides tools for automatic data visualization.
            Use these tools when you are presenting data to the user which could be complemented by a visual expression.
            Choose the most appropriate chart type based on the data you have and can provide.
            Match the data format to the chart type you have chosen. The user may request a specific
            chart, or you can pick the most appropriate one and shape the data to fit it.

            ## Statistical & comparison charts
            - **show_chart**: line, scatter, or bar charts
            - **render_histogram**: distribution of a single numeric variable (auto-binned)
            - **render_boxplot**: distribution/spread comparison across groups (quartiles + outliers)
            - **render_bubble**: 3-variable scatter (x, y, and size)
            - **render_area**: line/area chart, optionally stacked, for composition over time
            - **render_radar**: multi-dimensional comparison (spider chart)
            - **render_donut**: pie/donut charts for categorical proportions (single or grid)
            - **render_gauge**: a single KPI value against a range

            ## Scientific / biomedical
            - **render_volcano**: differential-expression volcano plot (log2 fold-change vs -log10 p)
            - **render_manhattan**: GWAS Manhattan plot across chromosomes
            - **render_kaplan_meier**: survival curves (step functions, optional censoring)
            - **render_forest**: forest plot of effect sizes with confidence intervals

            ## Relationships, flows & hierarchies
            - **render_network**: force-directed node-link graph (knowledge graphs, PPI, gene networks)
            - **render_sankey**: flow diagrams between stages
            - **render_chord**: pairwise flows between entities (square matrix)
            - **render_heatmap**: matrix as a colour grid (expression/correlation matrices)
            - **render_treemap**: hierarchical proportional boxes
            - **render_sunburst**: hierarchical radial chart
            - **render_dendrogram**: hierarchical clustering / phylogenetic tree
            - **render_wordcloud**: term-frequency word cloud
            - **render_calendar_heatmap**: value-per-day calendar grid

            ## Diagrams (Mermaid)
            - **render_mermaid**: any raw Mermaid syntax
            - **render_flowchart**: typed nodes/edges → flowchart
            - **render_gantt**: project/experiment timelines
            - **render_sequence**: sequence diagrams
            - **render_mindmap**: mind maps
            - **render_timeline**: chronological timelines
            - **render_er_diagram**: entity-relationship diagrams
            - **render_state_diagram**: state machines
            - **render_class_diagram**: class/UML diagrams

            ## Geographic
            - **render_map**: interactive map with location markers
            - **render_choropleth**: value-shaded regions from GeoJSON
        "#};

        Self {
            tool_router: Self::tool_router()
                + Self::diagrams_router()
                + Self::charts_router()
                + Self::d3_router()
                + Self::geo_router(),
            cache_dir,
            instructions,
        }
    }

    /// show a Sankey diagram from flow data
    #[tool(
        name = "render_sankey",
        description = r#"show a Sankey diagram from flow data
The data must contain:
- nodes: Array of objects with 'name' and optional 'category' properties
- links: Array of objects with 'source', 'target', and 'value' properties

Example:
{
  "nodes": [
    {"name": "Source A", "category": "source"},
    {"name": "Target B", "category": "target"}
  ],
  "links": [
    {"source": "Source A", "target": "Target B", "value": 100}
  ]
}"#
    )]
    pub async fn render_sankey(
        &self,
        params: Parameters<RenderSankeyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let data = &params.0.data;
        if data.nodes.is_empty() {
            return Err(invalid("Sankey diagram requires at least one node."));
        }
        if data.links.is_empty() {
            return Err(invalid("Sankey diagram requires at least one link."));
        }
        check_limit(data.nodes.len(), MAX_NODES, "nodes")?;
        check_limit(data.links.len(), MAX_LINKS, "links")?;
        // Every link must reference an existing node, else D3 sankey throws.
        let names: std::collections::HashSet<&str> =
            data.nodes.iter().map(|n| n.name.as_str()).collect();
        for link in &data.links {
            if !names.contains(link.source.as_str()) {
                return Err(invalid(format!(
                    "Sankey link references unknown source node '{}'. Add it to 'nodes'.",
                    link.source
                )));
            }
            if !names.contains(link.target.as_str()) {
                return Err(invalid(format!(
                    "Sankey link references unknown target node '{}'. Add it to 'nodes'.",
                    link.target
                )));
            }
        }
        let data_json = js_value(data)?;
        render(
            "ui://sankey/diagram",
            "sankey",
            "Sankey diagram rendered inline for the user.",
            include_str!("templates/sankey_template.html"),
            &[Asset::D3, Asset::D3Sankey],
            &[("{{SANKEY_DATA}}", &data_json)],
        )
    }

    /// show a radar chart (spider chart) for multi-dimensional data comparison
    #[tool(
        name = "render_radar",
        description = r#"show a radar chart (spider chart) for multi-dimensional data comparison

The data must contain:
- labels: Array of strings representing the dimensions/axes
- datasets: Array of dataset objects with 'label' and 'data' properties

Example:
{
  "labels": ["Speed", "Strength", "Endurance", "Agility", "Intelligence"],
  "datasets": [
    {"label": "Player 1", "data": [85, 70, 90, 75, 80]},
    {"label": "Player 2", "data": [75, 85, 80, 90, 70]}
  ]
}"#
    )]
    pub async fn render_radar(
        &self,
        params: Parameters<RenderRadarParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let data = &params.0.data;
        if data.labels.is_empty() {
            return Err(invalid("Radar chart requires at least one axis label."));
        }
        if data.datasets.is_empty() {
            return Err(invalid("Radar chart requires at least one dataset."));
        }
        check_limit(data.labels.len(), MAX_LABELS, "axes")?;
        for ds in &data.datasets {
            if ds.data.len() != data.labels.len() {
                return Err(invalid(format!(
                    "Dataset '{}' has {} values but there are {} axis labels; they must match.",
                    ds.label,
                    ds.data.len(),
                    data.labels.len()
                )));
            }
        }
        let data_json = js_value(data)?;
        render(
            "ui://radar/chart",
            "radar",
            "Radar chart rendered inline for the user.",
            include_str!("templates/radar_template.html"),
            &[Asset::ChartJs],
            &[("{{RADAR_DATA}}", &data_json)],
        )
    }

    /// show pie or donut charts for categorical data visualization
    #[tool(
        name = "render_donut",
        description = r#"show pie or donut charts for categorical data visualization
Supports single or multiple charts in a grid layout.

Each chart should contain:
- data: Array of values or objects with 'label' and 'value'
- type: Optional 'doughnut' (default) or 'pie'
- title: Optional chart title
- labels: Optional array of labels (if data is just numbers)

Example single chart:
{"title": "Budget", "type": "doughnut", "data": [
  {"label": "Marketing", "value": 25000},
  {"label": "Development", "value": 35000}
]}

Example multiple charts:
[{"title": "Q1 Sales", "labels": ["Product A", "Product B"], "data": [45000, 38000]}]"#
    )]
    pub async fn render_donut(
        &self,
        params: Parameters<RenderDonutParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let chart_data = &params.0.data.data;
        let charts: Vec<&SingleDonutChart> = match chart_data {
            DonutChartData::Single(c) => vec![c],
            DonutChartData::Multiple(v) => v.iter().collect(),
        };
        if charts.is_empty() {
            return Err(invalid("Donut chart requires at least one chart."));
        }
        for c in &charts {
            if c.data.is_empty() {
                return Err(invalid(
                    "Each donut/pie chart requires at least one data value.",
                ));
            }
            check_limit(c.data.len(), MAX_LABELS, "slices")?;
        }
        let data_json = js_value(chart_data)?;
        render(
            "ui://donut/chart",
            "donut",
            "Donut/pie chart rendered inline for the user.",
            include_str!("templates/donut_template.html"),
            &[Asset::ChartJs],
            &[("{{CHARTS_DATA}}", &data_json)],
        )
    }

    /// show a treemap visualization for hierarchical data
    #[tool(
        name = "render_treemap",
        description = r#"show a treemap visualization for hierarchical data with proportional area representation as boxes

The data should be a hierarchical structure with:
- name: Name of the node (required)
- value: Numeric value for leaf nodes (optional for parent nodes)
- children: Array of child nodes (optional)
- category: Category for coloring (optional)

Example:
{
  "name": "Root",
  "children": [
    {"name": "Group A", "children": [
      {"name": "Item 1", "value": 100, "category": "Type1"},
      {"name": "Item 2", "value": 200, "category": "Type2"}
    ]},
    {"name": "Item 3", "value": 150, "category": "Type1"}
  ]
}"#
    )]
    pub async fn render_treemap(
        &self,
        params: Parameters<RenderTreemapParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let data = &params.0.data;
        let (count, depth) = treemap_stats(data, 1);
        check_limit(count, MAX_NODES, "nodes")?;
        if depth > MAX_TREE_DEPTH {
            return Err(invalid(format!(
                "Treemap nesting depth {depth} exceeds the maximum of {MAX_TREE_DEPTH}."
            )));
        }
        let data_json = js_value(data)?;
        render(
            "ui://treemap/visualization",
            "treemap",
            "Treemap rendered inline for the user.",
            include_str!("templates/treemap_template.html"),
            &[Asset::D3],
            &[("{{TREEMAP_DATA}}", &data_json)],
        )
    }

    /// Show a chord diagram visualization for relationships and flows
    #[tool(
        name = "render_chord",
        description = r#"Show a chord diagram visualization for showing relationships and flows between entities.

The data must contain:
- labels: Array of strings representing the entities
- matrix: 2D array of numbers representing flows (matrix[i][j] = flow from i to j)

Example:
{
  "labels": ["North America", "Europe", "Asia", "Africa"],
  "matrix": [
    [0, 15, 25, 8],
    [18, 0, 20, 12],
    [22, 18, 0, 15],
    [5, 10, 18, 0]
  ]
}"#
    )]
    pub async fn render_chord(
        &self,
        params: Parameters<RenderChordParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let data = &params.0.data;
        let n = data.labels.len();
        if n == 0 {
            return Err(invalid("Chord diagram requires at least one label."));
        }
        check_limit(n, MAX_MATRIX_DIM, "labels")?;
        if data.matrix.len() != n {
            return Err(invalid(format!(
                "Chord matrix must be square: {} rows for {} labels.",
                data.matrix.len(),
                n
            )));
        }
        for (i, row) in data.matrix.iter().enumerate() {
            if row.len() != n {
                return Err(invalid(format!(
                    "Chord matrix row {i} has {} entries but must have {n} (one per label).",
                    row.len()
                )));
            }
        }
        let data_json = js_value(data)?;
        render(
            "ui://chord/diagram",
            "chord",
            "Chord diagram rendered inline for the user.",
            include_str!("templates/chord_template.html"),
            &[Asset::D3],
            &[("{{CHORD_DATA}}", &data_json)],
        )
    }

    /// show an interactive map visualization with location markers
    #[tool(
        name = "render_map",
        description = r#"show an interactive map visualization with location markers using Leaflet.

The data must contain:
- markers: Array of objects with 'lat', 'lng', and optional properties
- title/subtitle: Optional strings
- center: Optional center point {lat, lng}
- zoom: Optional initial zoom level (default 4)
- clustering: Optional boolean (default true)
- autoFit: Optional boolean (default true)

Marker properties: lat (required), lng (required), name, value, description, popup, color, label, useDefaultIcon

Example:
{"title": "Store Locations", "markers": [
  {"lat": 37.7749, "lng": -122.4194, "name": "SF Store", "value": 150000},
  {"lat": 40.7128, "lng": -74.0060, "name": "NYC Store", "value": 200000}
]}"#
    )]
    pub async fn render_map(
        &self,
        params: Parameters<RenderMapParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let data = &params.0.data;
        if data.markers.is_empty() {
            return Err(invalid("Map requires at least one marker."));
        }
        check_limit(data.markers.len(), MAX_MARKERS, "markers")?;
        for m in &data.markers {
            if !m.lat.is_finite() || m.lat < -90.0 || m.lat > 90.0 {
                return Err(invalid(format!(
                    "Marker latitude {} is out of range (-90..90).",
                    m.lat
                )));
            }
            if !m.lng.is_finite() || m.lng < -180.0 || m.lng > 180.0 {
                return Err(invalid(format!(
                    "Marker longitude {} is out of range (-180..180).",
                    m.lng
                )));
            }
        }
        let data_json = js_value(data)?;
        render(
            "ui://map/visualization",
            "map",
            "Map rendered inline for the user.",
            include_str!("templates/map_template.html"),
            &[Asset::Leaflet],
            &[("{{MAP_DATA}}", &data_json)],
        )
    }

    /// show a Mermaid diagram from Mermaid syntax
    #[tool(
        name = "render_mermaid",
        description = r#"show a Mermaid diagram from raw Mermaid syntax

Provide the Mermaid code as a string. Supports flowcharts, sequence diagrams, Gantt charts, etc.
For structured input, prefer the typed tools (render_flowchart, render_gantt, render_sequence, ...).

Example:
graph TD;
    A-->B;
    A-->C;
    B-->D;
    C-->D;
"#
    )]
    pub async fn render_mermaid(
        &self,
        params: Parameters<RenderMermaidParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.render_mermaid_source(&params.0.mermaid_code, "Mermaid Diagram")
    }

    /// show interactive line, scatter, or bar charts
    #[tool(
        name = "show_chart",
        description = r#"show interactive line, scatter, or bar charts

Required: type ('line', 'scatter', or 'bar'), datasets array
Optional: labels, title, subtitle, xAxisLabel, yAxisLabel

Example:
{
  "type": "line",
  "title": "Monthly Sales",
  "labels": ["Jan", "Feb", "Mar"],
  "datasets": [{"label": "Product A", "data": [65, 59, 80]}]
}"#
    )]
    pub async fn show_chart(
        &self,
        params: Parameters<ShowChartParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let data = &params.0.data;
        if data.datasets.is_empty() {
            return Err(invalid("Chart requires at least one dataset."));
        }
        let data_json = js_value(data)?;
        render(
            "ui://chart/interactive",
            "chart",
            "Chart rendered inline for the user.",
            include_str!("templates/chart_template.html"),
            &[Asset::ChartJs],
            &[("{{CHART_DATA}}", &data_json)],
        )
    }

    // -- internal helpers ---------------------------------------------------

    /// Render Mermaid source through the mermaid template (shared by every
    /// Mermaid-backed tool). `source` is injected as a safely-escaped JS string.
    fn render_mermaid_source(
        &self,
        source: &str,
        title: &str,
    ) -> Result<CallToolResult, ErrorData> {
        let trimmed = source.trim();
        if trimmed.is_empty() {
            return Err(invalid("Mermaid diagram requires non-empty source."));
        }
        if trimmed.len() > MAX_MERMAID_LEN {
            return Err(invalid(format!(
                "Mermaid source is too large ({} bytes, max {MAX_MERMAID_LEN}).",
                trimmed.len()
            )));
        }
        let code_json = js_data(&Value::String(trimmed.to_string()))?;
        render(
            "ui://mermaid/diagram",
            "mermaid",
            "Diagram rendered inline for the user.",
            include_str!("templates/mermaid_template.html"),
            &[Asset::Mermaid],
            &[
                ("{{MERMAID_CODE}}", &code_json),
                ("{{TITLE}}", &html_escape(title)),
            ],
        )
    }
}

/// Count nodes and maximum depth of a treemap tree.
fn treemap_stats(node: &TreemapNode, depth: usize) -> (usize, usize) {
    let mut count = 1;
    let mut max_depth = depth;
    if let Some(children) = &node.children {
        for child in children {
            let (c, d) = treemap_stats(child, depth + 1);
            count += c;
            max_depth = max_depth.max(d);
        }
    }
    (count, max_depth)
}

include!("tools_extra.rs");

#[cfg(test)]
mod tests;
