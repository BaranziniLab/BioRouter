---
name: cdw-query-cohorts
description: Systematically build and refine patient cohorts from CDW clinical data
---

Use this skill when the user wants to identify a group of patients based on
clinical criteria (diagnoses, medications, lab results, procedures, encounter types).

## When to activate
- User mentions "cohort", "patient population", "inclusion criteria", "exclusion criteria"
- User asks to find patients with specific conditions, treatments, or clinical characteristics

## Approach
1. Clarify the clinical definition of the cohort with the user
2. Use `CDW-get_available_tables` to identify relevant tables
3. Use `CDW-explore_table_schema` on candidate tables
4. Build an initial SQL query and validate with `CDW-execute_cdw_query`
5. Iterate based on row counts and user feedback
6. Present the final cohort size and sample records

## Notes
- All queries are read-only against de-identified data
- Prefer date range filters to keep queries fast
- Always show the patient count before presenting full results
