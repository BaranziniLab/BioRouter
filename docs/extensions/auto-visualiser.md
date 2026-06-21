The Auto Visualiser extension enables biorouter to automatically generate interactive data visualizations directly in your conversation. This built-in extension leverages MCP-UI to create charts, graphs, maps, and other visual representations of your data without requiring manual requests.

This guide will cover enabling and using the Auto Visualiser MCP Server.

## Configuration

  
  
  
  

  1. Run the `configure` command:
  ```sh
  biorouter configure
  ```

  2. Choose to `Toggle Extensions`
  ```sh
  ┌   biorouter-configure 
  │
  ◇  What would you like you configure?
  │  Toggle Extensions 
  │
  ◆  Enable extensions: (use "space" to toggle and "enter" to submit)
  // highlight-start    
  │  ● autovisualiser
  // highlight-end  
  └  Extension settings updated successfully
  ```
  

## Visualization Types

The Auto Visualiser is a powerful extension that integrates with biorouter's MCP-UI system to automatically detect when data would benefit from visualization and render interactive visual components. It analyzes data patterns in your conversations and suggests the most appropriate visualization type.

| Chart Type | Description | Detected When Prompt Implies |
|------------|-------------|------------------------------|
| **Sankey Diagrams** | Flow data visualization showing relationships and quantities | Flow or process data <br/>(screening/enrollment flows, pathway fluxes, patient journeys) |
| **Radar Charts** | Multi-dimensional data comparison (spider charts) | Multi-dimensional comparisons <br/>(biomarker panels, phenotype profiles) |
| **Donut/Pie Charts** | Categorical data visualization with support for multiple charts | Categorical proportions <br/>(cohort composition, variant class distributions) |
| **Treemap Visualizations** | Hierarchical data with proportional area representation | Hierarchical data <br/>(nested diagnostic subgroups, ontology hierarchies) |
| **Chord Diagrams** | Relationship and flow visualization between entities | Relationship matrices <br/>(gene/protein interactions, drug–disease cross-references) |
| **Interactive Maps** | Geographic data visualization with location markers using Leaflet | Geographic information <br/>(location data, coordinates, addresses) |
| **Mermaid Diagrams** | Flowcharts, sequence diagrams, Gantt charts, and other diagram types using Mermaid syntax | Diagram creation <br/>(flowcharts, sequence diagrams, architecture diagrams) |
| **Line/Bar/Scatter Charts** | Traditional chart types for data analysis | Time series data <br/>(historical data, trends over time) |

### Example Visualizations

### Features

- **Automatic Detection**: biorouter intelligently chooses the best visualization type for your data
- **Interactive Components**: All visualizations are interactive with hover effects, zooming, and drilling capabilities
- **MCP-UI Integration**: Leverages the emerging MCP-UI standard for seamless rendering
- **Multiple Chart Support**: Can render multiple visualizations in a single response
- **Customizable Styling**: Supports custom colors, labels, and formatting options
- **Export Capability**: Visualizations can be saved as HTML files for sharing

## Example Usage

In this example, I'll have biorouter analyze some cohort study data and automatically visualize it using the Auto Visualiser extension.

### biorouter Prompt
```
I have biomarker measurements across three patient cohorts at multiple study visits. Can you help me understand:
1. The hierarchical breakdown of patients across our nested diagnostic subgroups
2. How our biomarker panels compare across all four study visits  
3. The patient flow through our clinical screening and enrollment process

Here's the data (mean serum CRP, mg/L):
- Cohort A: V1: 1.5, V2: 1.8, V3: 2.2, V4: 1.95
- Cohort B: V1: 1.2, V2: 1.4, V3: 1.6, V4: 1.75  
- Cohort C: V1: 0.8, V2: 0.95, V3: 1.1, V4: 1.25
```

### biorouter Output

> **Note:** I'll help you analyze your cohort study data with three different visualizations to address each of your questions. Let me create these visualizations for you.

First, let's visualize the hierarchical breakdown of patients across diagnostic subgroups:

![](/img/autovisualiser-treemap.png)

![](/img/autovisualiser-radar.png)

![](/img/autovisualiser-sankey.png)

I've created three comprehensive visualizations to help you understand your cohort study data from different perspectives:

#### 1. Hierarchical Patient Breakdown (Treemap)
The **treemap visualization** shows your cohort hierarchy with proportional areas representing the relative size of each subgroup and visit. Key insights:
- **Cohort A** is your largest enrolled group (745 patients total)
- **Cohort B** follows as second largest (595 patients total) 
- **Cohort C** is smallest but growing (410 patients total)
- You can see how each visit contributes within each cohort

#### 2. Biomarker Panel Comparison (Radar Chart)
The **radar chart** displays how all three cohorts' biomarker panels compared across the four study visits, making it easy to compare:
- **Cohort A** shows elevated V3 biomarker levels with a slight V4 decline
- **Cohort B** demonstrates a consistent upward trend across visits
- **Cohort C** shows a steady increase across all visits
- All cohorts show their highest values in V3-V4

#### 3. Screening & Enrollment Flow (Sankey Diagram)
The **Sankey diagram** represents patient flow through your screening process, with flow thickness proportional to patient counts:
- Shows how screened candidates flow into eligible patients for each cohort
- Demonstrates the conversion from eligible to enrolled patients
- **Cohort A** has the thickest flow (highest enrollment yield)
- **Cohort C** has the thinnest but represents a recruitment opportunity

#### Key Study Insights:
- **Total Enrolled**: 1,750 patients across all cohorts
- **Trend Pattern**: All cohorts show rising biomarker trajectories
- **Cohort Composition**: Cohort A (42.6%), Cohort B (34%), Cohort C (23.4%)
- **Visit Trends**: V3 shows the strongest biomarker signal overall

Would you like me to create additional visualizations or dive deeper into any specific aspect of your cohort study data?

## Troubleshooting

- **Visualization not appearing**: Ensure the Auto Visualiser extension is enabled and your data follows the expected format.

- **Data format errors**: Check that your data structure matches the required schema for the visualization type. The extension provides detailed error messages to guide you.

- **Performance with large datasets**: For very large datasets, consider aggregating or sampling your data before visualization.
