use crate::graph::InteractionGraph;
use crate::model::{
    Interaction, InteractionChain, InteractionReport, InteractionReportEntry, PatientRegimen,
    SeverityLevel,
};

/// Core query engine for drug-drug interactions.
pub struct InteractionQuery<'a> {
    pub graph: &'a InteractionGraph,
}

impl<'a> InteractionQuery<'a> {
    pub fn new(graph: &'a InteractionGraph) -> Self {
        InteractionQuery { graph }
    }

    /// Find all pairwise interactions for a given patient regimen.
    /// Returns entries sorted by severity (descending).
    pub fn find_all_interactions(&self, regimen: &PatientRegimen) -> InteractionReport {
        let mut entries = Vec::new();

        // Check all pairs
        let meds = &regimen.medications;
        for i in 0..meds.len() {
            for j in (i + 1)..meds.len() {
                if let Some(ix) = self.find_interaction(&meds[i], &meds[j]) {
                    entries.push(InteractionReportEntry {
                        drug_a: ix.drug_a.clone(),
                        drug_b: ix.drug_b.clone(),
                        interaction_type: ix.interaction_type,
                        severity: ix.severity,
                        mechanism: ix.mechanism.clone(),
                        evidence: ix.evidence,
                        recommendation: ix.recommendation.clone(),
                    });
                }
            }
        }

        // Sort by severity descending (most severe first)
        entries.sort_by(|a, b| b.severity.cmp(&a.severity));

        let score = self.calculate_regimen_score(&entries);
        InteractionReport::new(entries, score)
    }

    /// Find a specific interaction between two drugs.
    pub fn find_interaction(&self, drug_a: &str, drug_b: &str) -> Option<&Interaction> {
        let a = drug_a.to_lowercase();
        let b = drug_b.to_lowercase();
        let key = if a <= b {
            (a, b)
        } else {
            (b, a)
        };
        self.graph.interaction_map.get(&key)
    }

    /// Find all drugs that interact with a given drug.
    #[allow(dead_code)]
    pub fn find_interactions_for_drug(&self, drug_name: &str) -> Vec<&Interaction> {
        self.graph.interactions_for(drug_name)
    }

    /// Detect interaction chains within a regimen.
    /// A chain is a path of drugs where consecutive drugs interact.
    pub fn detect_chains(
        &self,
        regimen: &PatientRegimen,
        max_chain_len: usize,
    ) -> Vec<InteractionChain> {
        let chains = self.graph.find_chains(&regimen.medications, max_chain_len);

        chains
            .into_iter()
            .map(|path| {
                let total_score: u32 = path
                    .windows(2)
                    .filter_map(|w| self.find_interaction(&w[0], &w[1]))
                    .map(|ix| ix.severity.score())
                    .sum();

                let min_severity = path
                    .windows(2)
                    .filter_map(|w| self.find_interaction(&w[0], &w[1]))
                    .map(|ix| ix.severity)
                    .min()
                    .unwrap_or(SeverityLevel::Minor);

                InteractionChain {
                    drugs: path,
                    total_severity_score: total_score,
                    min_severity,
                }
            })
            .collect()
    }

    /// Calculate a severity score for the entire regimen.
    /// The score is a weighted sum of all interaction severities.
    pub fn calculate_regimen_score(&self, entries: &[InteractionReportEntry]) -> u32 {
        if entries.is_empty() {
            return 0;
        }

        let base_score: u32 = entries.iter().map(|e| e.severity.score()).sum();

        // Bonus for multiple severe interactions (compound risk)
        let severe_count = entries
            .iter()
            .filter(|e| e.severity >= SeverityLevel::Major)
            .count();

        let compound_bonus = if severe_count >= 3 {
            severe_count as u32 * 2
        } else if severe_count >= 2 {
            severe_count as u32
        } else {
            0
        };

        base_score + compound_bonus
    }

    /// Rank interactions by a combined severity-evidence score.
    /// Higher scores indicate more dangerous/confirmed interactions.
    pub fn rank_interactions(&self, entries: &[InteractionReportEntry]) -> Vec<InteractionReportEntry> {
        let mut ranked = entries.to_vec();
        ranked.sort_by(|a, b| {
            let score_a = self.combined_score(a);
            let score_b = self.combined_score(b);
            score_b.cmp(&score_a)
        });
        ranked
    }

    fn combined_score(&self, entry: &InteractionReportEntry) -> u32 {
        let severity_score = entry.severity.score() * 10;
        let evidence_score = match entry.evidence {
            crate::model::EvidenceLevel::Established => 4,
            crate::model::EvidenceLevel::Probable => 3,
            crate::model::EvidenceLevel::Suspected => 2,
            crate::model::EvidenceLevel::Unknown => 1,
        };
        severity_score + evidence_score
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::InteractionGraph;
    use crate::model::{Drug, EvidenceLevel, InteractionType, SeverityLevel};

    fn setup() -> (InteractionGraph, PatientRegimen) {
        let drugs = vec![
            Drug::new("warfarin", "anticoagulant", vec!["VKORC1".into()]),
            Drug::new("aspirin", "nsaid", vec!["COX-1".into()]),
            Drug::new("fluoxetine", "ssri", vec!["SERT".into()]),
            Drug::new("omeprazole", "ppi", vec!["CYP2C19".into()]),
            Drug::new("metformin", "biguanide", vec!["AMPK".into()]),
        ];

        let interactions = vec![
            Interaction {
                drug_a: "aspirin".into(),
                drug_b: "warfarin".into(),
                interaction_type: InteractionType::Pharmacodynamic,
                severity: SeverityLevel::Major,
                mechanism: "Additive anticoagulation".into(),
                evidence: EvidenceLevel::Established,
                recommendation: Some("Monitor INR".into()),
            }
            .canonicalized(),
            Interaction {
                drug_a: "fluoxetine".into(),
                drug_b: "warfarin".into(),
                interaction_type: InteractionType::Pharmacokinetic,
                severity: SeverityLevel::Moderate,
                mechanism: "CYP2C9 inhibition".into(),
                evidence: EvidenceLevel::Probable,
                recommendation: Some("Adjust warfarin dose".into()),
            }
            .canonicalized(),
            Interaction {
                drug_a: "omeprazole".into(),
                drug_b: "fluoxetine".into(),
                interaction_type: InteractionType::Pharmacokinetic,
                severity: SeverityLevel::Minor,
                mechanism: "CYP2C19 effect".into(),
                evidence: EvidenceLevel::Suspected,
                recommendation: None,
            }
            .canonicalized(),
            Interaction {
                drug_a: "metformin".into(),
                drug_b: "omeprazole".into(),
                interaction_type: InteractionType::Pharmacokinetic,
                severity: SeverityLevel::Minor,
                mechanism: "Altered absorption".into(),
                evidence: EvidenceLevel::Unknown,
                recommendation: None,
            }
            .canonicalized(),
        ];

        let graph = InteractionGraph::new(&drugs, &interactions);
        let regimen = PatientRegimen::new(vec![
            "warfarin".into(),
            "aspirin".into(),
            "fluoxetine".into(),
            "omeprazole".into(),
            "metformin".into(),
        ]);

        (graph, regimen)
    }

    #[test]
    fn test_find_all_interactions() {
        let (graph, regimen) = setup();
        let query = InteractionQuery::new(&graph);
        let report = query.find_all_interactions(&regimen);

        // 5 choose 2 = 10 pairs; 4 interactions exist
        assert_eq!(report.len(), 4);
        // Most severe should be first
        assert_eq!(report.entries[0].severity, SeverityLevel::Major);
    }

    #[test]
    fn test_find_specific_interaction() {
        let (graph, _regimen) = setup();
        let query = InteractionQuery::new(&graph);

        let ix = query.find_interaction("warfarin", "aspirin");
        assert!(ix.is_some());
        assert_eq!(ix.unwrap().severity, SeverityLevel::Major);

        let ix2 = query.find_interaction("warfarin", "metformin");
        assert!(ix2.is_none());
    }

    #[test]
    fn test_find_interactions_for_drug() {
        let (graph, _regimen) = setup();
        let query = InteractionQuery::new(&graph);

        let ix = query.find_interactions_for_drug("warfarin");
        assert_eq!(ix.len(), 2); // aspirin + fluoxetine
    }

    #[test]
    fn test_detect_chains() {
        let (graph, regimen) = setup();
        let query = InteractionQuery::new(&graph);
        let chains = query.detect_chains(&regimen, 10);

        // Should find at least one chain (e.g., warfarin -> fluoxetine -> omeprazole)
        assert!(!chains.is_empty());

        // Each chain should have length >= 3
        for chain in &chains {
            assert!(chain.drugs.len() >= 3);
        }
    }

    #[test]
    fn test_calculate_regimen_score() {
        let (graph, regimen) = setup();
        let query = InteractionQuery::new(&graph);
        let report = query.find_all_interactions(&regimen);

        let score = report.regimen_severity_score;
        assert!(score > 0);

        // With a Major(3) + Moderate(2) + Minor(1) + Minor(1) = 7 base + bonus for severe >= 2
        assert!(score >= 7);
    }

    #[test]
    fn test_rank_interactions() {
        let (graph, regimen) = setup();
        let query = InteractionQuery::new(&graph);
        let report = query.find_all_interactions(&regimen);

        let ranked = query.rank_interactions(&report.entries);
        assert_eq!(ranked.len(), 4);
        // First should be most severe + established evidence
        assert_eq!(ranked[0].severity, SeverityLevel::Major);
    }

    #[test]
    fn test_no_interactions() {
        let drugs = vec![
            Drug::new("metformin", "biguanide", vec![]),
            Drug::new("lisinopril", "ace_inhibitor", vec![]),
        ];
        let interactions = vec![]; // no interactions
        let graph = InteractionGraph::new(&drugs, &interactions);
        let query = InteractionQuery::new(&graph);

        let regimen = PatientRegimen::new(vec!["metformin".into(), "lisinopril".into()]);
        let report = query.find_all_interactions(&regimen);

        assert!(report.is_empty());
        assert_eq!(report.regimen_severity_score, 0);
    }
}
