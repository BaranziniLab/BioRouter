---
name: spoke-knowledge-graph
description: Traverse the SPOKE biomedical knowledge graph to find entity relationships
---

Use this skill when the user wants to explore relationships between biomedical
entities (diseases, genes, compounds, pathways, side effects) in the SPOKE graph.

## When to activate
- User asks about connections between diseases, genes, drugs, or biological processes
- User wants to find drug repurposing candidates, disease mechanisms, or biomarkers
- User asks "what is related to X" or "how are X and Y connected"

## Approach
1. Call `get_spoke_schema` to understand available node and edge types
2. Identify the entities the user is asking about (use standard identifiers where possible)
3. Write a Cypher query with `query_spoke` to traverse the relevant subgraph
4. Explain the biological meaning of each relationship type found
5. Summarize findings with the most biologically relevant connections highlighted

## Notes
- SPOKE integrates: OMIM, DrugBank, DisGeNET, SIDER, Reactome, UniProt, and more
- Node labels: Disease, Gene, Compound, Pathway, SideEffect, Anatomy, etc.
- Limit Cypher queries to avoid full-graph traversals (use LIMIT and specific labels)
