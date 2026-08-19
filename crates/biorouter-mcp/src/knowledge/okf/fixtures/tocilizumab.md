---
type: Molecule
identifier: Tocilizumab
subtype: antibody
xref: [DRUGBANK:DB06273, CHEMBL:CHEMBL1237022, RXNORM:612865, UNII:I031V2H011]
synonyms: [atlizumab, Actemra]
in_taxon: NCBITaxon:9606
edges:
  - predicate: binds
    object: IL6 receptor (IL6R)
    knowledge_level: knowledge_assertion
    agent_type: manual_agent
    primary_source: DrugBank
    effect_metric: Kd
    effect_size: 2.5            # nM
  - predicate: regulates
    object: IL6 signaling
    direction: decreased
    aspect: activity
    knowledge_level: knowledge_assertion
    agent_type: manual_agent
    primary_source: DrugBank
  - predicate: treats
    object: rheumatoid arthritis
    clinical_phase: approved
    knowledge_level: knowledge_assertion
    agent_type: manual_agent
    primary_source: DrugCentral
  - predicate: treats
    object: COVID-19
    clinical_phase: approved
    knowledge_level: statistical_association
    agent_type: data_analysis_pipeline
    primary_source: RECOVERY trial
    effect_metric: relative_risk
    effect_size: 0.85
    ci_lower: 0.76
    ci_upper: 0.94
    sample_size: 4116
    publications: [PMID:33933206]
  - predicate: has_phenotype             # adverse effect
    object: neutropenia
    knowledge_level: knowledge_assertion
    agent_type: manual_agent
    primary_source: SIDER
    frequency: common
  - predicate: member_of
    object: IL6 inhibitors                # MolecularClass (subtype: pharmacologic)
    knowledge_level: knowledge_assertion
    agent_type: manual_agent
    primary_source: ATC
  - predicate: reported_in
    object: RECOVERY trial
    knowledge_level: knowledge_assertion
    agent_type: manual_agent
    primary_source: RECOVERY trial
---

# Tocilizumab

A recombinant humanized monoclonal antibody against the interleukin-6 receptor (IL-6R),
blocking IL-6 signaling. First approved for rheumatoid arthritis; repurposed during
COVID-19 (RECOVERY, REMAP-CAP).

## Citations
- [Tocilizumab in patients admitted to hospital with COVID-19 (RECOVERY)](https://pubmed.ncbi.nlm.nih.gov/33933206/) (PMID:33933206)
