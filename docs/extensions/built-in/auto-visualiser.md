# Auto Visualiser extension

> **What this is.** User guide to the built-in Auto Visualiser extension: how to enable it, which figures it can produce, and a worked cohort-data example.
> **Status:** Superseded in part — the extension is live and healthy, but the chart catalogue below covers only 8 of the 34 tools the code now registers. The current truth is the source: `crates/biorouter-mcp/src/autovisualiser/` (`mod.rs`, `tools_charts.rs`, `tools_d3.rs`, `tools_extra.rs`, `tools_geo.rs`, `tools_dashboard.rs`).
> **Audience:** end users.

The Auto Visualiser extension lets BioRouter generate interactive data visualizations directly inside a conversation. It uses MCP-UI — the emerging Model Context Protocol standard for returning renderable UI from a tool — to return charts, graphs, maps and diagrams as `ui://` resources that the chat renders inline, without you having to ask for a figure explicitly.

> **Warning.** The tool table in [Visualization types](#visualization-types) is out of date. The extension registers 34 tools. Not documented here: the scientific figures `render_volcano`, `render_manhattan`, `render_kaplan_meier` and `render_forest`; the relationship and hierarchy figures `render_heatmap`, `render_sunburst`, `render_dendrogram`, `render_wordcloud` and `render_calendar_heatmap`; the typed Mermaid wrappers (`render_flowchart`, `render_gantt`, `render_sequence`, `render_mindmap`, `render_timeline`, `render_er_diagram`, `render_state_diagram`, `render_class_diagram`); and the composite report tool `render_dashboard`, which combines several figures into one scrollable artifact. Until this page is refreshed, read the tool definitions in the source files named above.

## Configuration

1. Run the `configure` command:

   ```bash
   biorouter configure
   ```

2. Choose `Toggle Extensions`, then enable `autovisualiser`:

   ```text
   ┌   biorouter-configure
   │
   ◇  What would you like you configure?
   │  Toggle Extensions
   │
   ◆  Enable extensions: (use "space" to toggle and "enter" to submit)
   │  ● autovisualiser
   └  Extension settings updated successfully
   ```

## Visualization types

The Auto Visualiser integrates with BioRouter's MCP-UI system to detect when data would benefit from visualization and to render an interactive component. It analyzes data patterns in your conversation and picks the most appropriate visualization type.

| Chart type | Description | Detected when prompt implies |
|------------|-------------|------------------------------|
| **Sankey Diagrams** | Flow data visualization showing relationships and quantities | Flow or process data <br/>(screening/enrollment flows, pathway fluxes, patient journeys) |
| **Radar Charts** | Multi-dimensional data comparison (spider charts) | Multi-dimensional comparisons <br/>(biomarker panels, phenotype profiles) |
| **Donut/Pie Charts** | Categorical data visualization with support for multiple charts | Categorical proportions <br/>(cohort composition, variant class distributions) |
| **Treemap Visualizations** | Hierarchical data with proportional area representation | Hierarchical data <br/>(nested diagnostic subgroups, ontology hierarchies) |
| **Chord Diagrams** | Relationship and flow visualization between entities | Relationship matrices <br/>(gene/protein interactions, drug–disease cross-references) |
| **Interactive Maps** | Geographic data visualization with location markers using Leaflet | Geographic information <br/>(location data, coordinates, addresses) |
| **Mermaid Diagrams** | Flowcharts, sequence diagrams, Gantt charts, and other diagram types using Mermaid syntax | Diagram creation <br/>(flowcharts, sequence diagrams, architecture diagrams) |
| **Line/Bar/Scatter Charts** | Traditional chart types for data analysis | Time series data <br/>(historical data, trends over time) |

## Features

- **Automatic detection**: BioRouter chooses the visualization type that suits your data.
- **Interactive components**: figures support hover effects, zooming, and drill-down.
- **MCP-UI integration**: rendering goes through the MCP-UI standard.
- **Multiple chart support**: one response can render several visualizations.
- **Customizable styling**: custom colors, labels, and formatting options are supported.
- **Export capability**: visualizations can be saved as HTML files for sharing.

## Asset-loading environment variables

Libraries such as D3, Chart.js, Leaflet and Mermaid are inlined into each figure by default, so figures work offline. Two environment variables change that behaviour and are worth knowing if figures fail to re-render when you reopen a chat.

| Variable | Effect |
|----------|--------|
| `BIOROUTER_AUTOVIS_CDN=1` | Switch from inlined libraries to pinned CDN script tags, shrinking the persisted figure blob from megabytes to a few KB. Recommended if large Mermaid diagrams fail to re-render on chat reopen. The desktop app sets this by default. |
| `BIOROUTER_AUTOVIS_DEBUG=1` | Dump the generated HTML to the app cache directory (`<cache>/autovisualiser/<name>-<pid>.html`). Also on by default in debug builds. |

## Example usage

In this example, BioRouter analyzes cohort study data and visualizes it with the Auto Visualiser extension.

### BioRouter prompt

```text
I have biomarker measurements across three patient cohorts at multiple study visits. Can you help me understand:
1. The hierarchical breakdown of patients across our nested diagnostic subgroups
2. How our biomarker panels compare across all four study visits  
3. The patient flow through our clinical screening and enrollment process

Here's the data (mean serum CRP, mg/L):
- Cohort A: V1: 1.5, V2: 1.8, V3: 2.2, V4: 1.95
- Cohort B: V1: 1.2, V2: 1.4, V3: 1.6, V4: 1.75  
- Cohort C: V1: 0.8, V2: 0.95, V3: 1.1, V4: 1.25
```

### BioRouter output

> **Note.** The transcript below is an illustration of the shape of a response, not a recording of a real run. The patient counts and percentages in it are made up, and the three figures it refers to (a treemap, a radar chart and a Sankey diagram) are not reproduced on this page.

I'll help you analyze your cohort study data with three different visualizations to address each of your questions. Let me create these visualizations for you.

First, let's visualize the hierarchical breakdown of patients across diagnostic subgroups.

I've created three comprehensive visualizations to help you understand your cohort study data from different perspectives:

#### Hierarchical patient breakdown (treemap)

The **treemap visualization** shows your cohort hierarchy with proportional areas representing the relative size of each subgroup and visit. Key insights:

- **Cohort A** is your largest enrolled group (745 patients total)
- **Cohort B** follows as second largest (595 patients total)
- **Cohort C** is smallest but growing (410 patients total)
- You can see how each visit contributes within each cohort

#### Biomarker panel comparison (radar chart)

The **radar chart** displays how all three cohorts' biomarker panels compared across the four study visits, making it easy to compare:

- **Cohort A** shows elevated V3 biomarker levels with a slight V4 decline
- **Cohort B** demonstrates a consistent upward trend across visits
- **Cohort C** shows a steady increase across all visits
- All cohorts show their highest values in V3-V4

#### Screening and enrollment flow (Sankey diagram)

The **Sankey diagram** represents patient flow through your screening process, with flow thickness proportional to patient counts:

- Shows how screened candidates flow into eligible patients for each cohort
- Demonstrates the conversion from eligible to enrolled patients
- **Cohort A** has the thickest flow (highest enrollment yield)
- **Cohort C** has the thinnest but represents a recruitment opportunity

#### Key study insights

- **Total Enrolled**: 1,750 patients across all cohorts
- **Trend Pattern**: All cohorts show rising biomarker trajectories
- **Cohort Composition**: Cohort A (42.6%), Cohort B (34%), Cohort C (23.4%)
- **Visit Trends**: V3 shows the strongest biomarker signal overall

Would you like me to create additional visualizations or dive deeper into any specific aspect of your cohort study data?

## Troubleshooting

- **Visualization not appearing**: ensure the Auto Visualiser extension is enabled and your data follows the expected format.
- **Data format errors**: check that your data structure matches the required schema for the visualization type. The extension provides detailed error messages to guide you.
- **Performance with large datasets**: for very large datasets, consider aggregating or sampling your data before visualization.
- **Large diagrams fail to re-render when a chat is reopened**: set `BIOROUTER_AUTOVIS_CDN=1` so the persisted figure stores CDN script tags instead of megabytes of inlined library source.

## Related documentation

- [Extensions and skills guide](../extensions-and-skills-guide.md) — how extensions are configured, discovered and enabled generally.
- [Environment variables](../../configuration/environment-variables.md) — the full set of `BIOROUTER_*` variables, including the asset-loading flags above.
- [Auto Visualiser stress test](../../history/autovis-stress-test/README.md) — the historical stress-test campaign that hardened these tools, useful for knowing which figure types were exercised.
- [Troubleshooting](../../troubleshooting/README.md) — where to go when a figure or extension misbehaves.
