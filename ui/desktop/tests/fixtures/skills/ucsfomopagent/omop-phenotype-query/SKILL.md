---
name: omop-phenotype-query
description: Query the OMOP CDM to identify patient phenotypes and clinical concepts
---

Use this skill when the user wants to define or query patient phenotypes using
the OMOP Common Data Model at UCSF.

## When to activate
- User asks about OMOP, phenotypes, standard concepts, or the OHDSI framework
- User wants to count patients with specific conditions using standard vocabularies

## Approach
1. Use `list_ucsf_omop_tables` to show the available OMOP domain tables
2. Write a phenotyping query using standard OMOP concepts (condition_occurrence,
   drug_exposure, measurement, observation, procedure_occurrence)
3. Run with `query_ucsf_omop` and present results with concept names
4. Suggest refinements using concept ancestors for broader/narrower definitions

## Notes
- OMOP uses standard concept IDs — avoid local source codes
- All queries are read-only on de-identified data
- concept_id = 0 means unmapped — filter these out for clean phenotypes
