---
name: cdw-explore-schema
description: Explore the CDW clinical schema, tables, and column relationships
---

Use this skill when the user wants to understand what data is available in the
Clinical Data Warehouse before writing queries.

## When to activate
- User asks "what tables are available", "what data do you have", "show me the schema"
- User is unfamiliar with the CDW and needs orientation

## Approach
1. Call `CDW-get_available_tables` to list all available tables with descriptions
2. For tables of interest, call `CDW-explore_table_schema` to show columns and types
3. Use `CDW-search_clinical_concepts` to find relevant medical codes (ICD, CPT, LOINC)
4. Summarize the relevant tables and suggest next steps for the user's research goal

## Notes
- The CDW schema reference is bundled — fast lookups without hitting the database
- Always orient the user to what is de-identified vs. potentially re-identifiable
