use crate::graph::InteractionGraph;
use crate::model::{Drug, InteractionReportEntry, PatientRegimen, SeverityLevel};
use std::collections::{HashMap, HashSet};

/// Suggestion engine for finding alternative medications.
pub struct SuggestionEngine<'a> {
    pub graph: &'a InteractionGraph,
    pub drugs: &'a [Drug],
}

/// A suggested alternative drug.
#[derive(Debug, Clone)]
pub struct DrugSuggestion {
    pub drug: Drug,
    /// Whether the same drug class as the original
    #[allow(dead_code)]
    pub same_class: bool,
    /// Number of interactions the suggestion has with the current regimen
    pub interaction_count: usize,
    /// Severity of the worst interaction with the current regimen
    pub worst_severity: Option<SeverityLevel>,
    /// All interactions the suggestion would have with the current regimen
    pub interactions: Vec<InteractionReportEntry>,
    /// Overall safety score (lower = safer)
    pub safety_score: u32,
}

impl<'a> SuggestionEngine<'a> {
    pub fn new(graph: &'a InteractionGraph, drugs: &'a [Drug]) -> Self {
        SuggestionEngine { graph, drugs }
    }

    /// Find alternative drugs for a given drug, considering the current regimen.
    ///
    /// Returns drugs in the same class that have fewer or lower-severity interactions
    /// with the existing regimen (excluding the drug being replaced) than the original drug.
    pub fn find_alternatives(
        &self,
        original_drug: &str,
        regimen: &PatientRegimen,
    ) -> Vec<DrugSuggestion> {
        let original_lower = original_drug.to_lowercase();

        // Build a "rest of regimen" excluding the drug being replaced
        let rest_regimen = PatientRegimen::new(
            regimen.medications.iter().filter(|m| *m != &original_lower).cloned().collect(),
        );

        // Find the original drug's class
        let original_class = self.drugs.iter().find(|d| d.name == original_lower).map(|d| d.drug_class.as_str());

        let original_class = match original_class {
            Some(c) => c,
            None => return Vec::new(),
        };

        // Get original drug's interactions with the rest of the regimen (excluding itself)
        let original_worst = self.worst_interaction_with_regimen(&original_lower, &rest_regimen);

        // Find all drugs in the same class
        let same_class: Vec<&Drug> = self
            .drugs
            .iter()
            .filter(|d| d.drug_class == original_class && d.name != original_lower)
            .collect();

        let mut suggestions: Vec<DrugSuggestion> = same_class
            .into_iter()
            .map(|drug| {
                let interactions = self.interactions_with_regimen(&drug.name, &rest_regimen);
                let worst = interactions.iter().map(|e| e.severity).max();
                let safety_score = self.calculate_safety_score(&interactions);

                DrugSuggestion {
                    drug: drug.clone(),
                    same_class: true,
                    interaction_count: interactions.len(),
                    worst_severity: worst,
                    interactions,
                    safety_score,
                }
            })
            .collect();

        // Filter: only suggest drugs that are safer than or equal to the original
        let original_safety = self.calculate_safety_score_for_drug(&original_lower, &rest_regimen);
        suggestions.retain(|s| {
            match (s.worst_severity, original_worst) {
                (None, _) => true, // No interactions = safe
                (Some(s_sev), Some(o_sev)) => s_sev <= o_sev && s.safety_score <= original_safety,
                (Some(_), None) => false, // Suggestion has interactions but original didn't
            }
        });

        // Sort by safety score (ascending = safer first)
        suggestions.sort_by_key(|s| s.safety_score);

        suggestions
    }

    /// Find all alternatives across all drug classes for the given drug.
    /// Broader search: includes drugs from different classes.
    pub fn find_broad_alternatives(
        &self,
        original_drug: &str,
        regimen: &PatientRegimen,
    ) -> Vec<DrugSuggestion> {
        let original_lower = original_drug.to_lowercase();
        let original_safety = self.calculate_safety_score_for_drug(&original_lower, regimen);

        let suggestions: Vec<DrugSuggestion> = self
            .drugs
            .iter()
            .filter(|d| d.name != original_lower)
            .map(|drug| {
                let interactions = self.interactions_with_regimen(&drug.name, regimen);
                let worst = interactions.iter().map(|e| e.severity).max();
                let safety_score = self.calculate_safety_score(&interactions);

                DrugSuggestion {
                    drug: drug.clone(),
                    same_class: false,
                    interaction_count: interactions.len(),
                    worst_severity: worst,
                    interactions,
                    safety_score,
                }
            })
            .filter(|s| s.safety_score < original_safety)
            .collect();

        let mut sorted = suggestions;
        sorted.sort_by_key(|s| s.safety_score);
        sorted
    }

    /// Find "gap" drugs: drugs that interact with many drugs in the regimen
    /// but are NOT in the regimen. Useful for identifying hidden risk factors.
    #[allow(dead_code)]
    pub fn find_unlisted_interactors(&self, regimen: &PatientRegimen) -> Vec<(Drug, usize, SeverityLevel)> {
        let regimen_set: HashSet<&str> = regimen.medications.iter().map(|s| s.as_str()).collect();
        let mut interactor_count: HashMap<String, (usize, SeverityLevel)> = HashMap::new();

        for med in &regimen.medications {
            for ix in self.graph.interactions_for(med) {
                let other = if &ix.drug_a == med {
                    &ix.drug_b
                } else {
                    &ix.drug_a
                };
                if !regimen_set.contains(other.as_str()) {
                    let entry = interactor_count
                        .entry(other.clone())
                        .or_insert((0, SeverityLevel::Minor));
                    entry.0 += 1;
                    if ix.severity > entry.1 {
                        entry.1 = ix.severity;
                    }
                }
            }
        }

        let mut result: Vec<(Drug, usize, SeverityLevel)> = interactor_count
            .into_iter()
            .filter_map(|(name, (count, sev))| {
                self.drugs.iter().find(|d| d.name == name).map(|d| {
                    (d.clone(), count, sev)
                })
            })
            .collect();

        result.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.2.cmp(&a.2)));
        result
    }

    // ─── Internal helpers ──────────────────────────────────────────────────

    fn interactions_with_regimen(
        &self,
        drug_name: &str,
        regimen: &PatientRegimen,
    ) -> Vec<InteractionReportEntry> {
        let drug_lower = drug_name.to_lowercase();
        regimen
            .medications
            .iter()
            .filter_map(|med| {
                if med == &drug_lower {
                    return None;
                }
                self.graph
                    .interaction_map
                    .get(&Self::canonical_pair(&drug_lower, med))
                    .map(|ix| InteractionReportEntry {
                        drug_a: ix.drug_a.clone(),
                        drug_b: ix.drug_b.clone(),
                        interaction_type: ix.interaction_type,
                        severity: ix.severity,
                        mechanism: ix.mechanism.clone(),
                        evidence: ix.evidence,
                        recommendation: ix.recommendation.clone(),
                    })
            })
            .collect()
    }

    fn worst_interaction_with_regimen(
        &self,
        drug_name: &str,
        regimen: &PatientRegimen,
    ) -> Option<SeverityLevel> {
        self.interactions_with_regimen(drug_name, regimen)
            .iter()
            .map(|e| e.severity)
            .max()
    }

    fn calculate_safety_score(&self, interactions: &[InteractionReportEntry]) -> u32 {
        interactions.iter().map(|e| e.severity.score()).sum()
    }

    fn calculate_safety_score_for_drug(&self, drug_name: &str, regimen: &PatientRegimen) -> u32 {
        let interactions = self.interactions_with_regimen(drug_name, regimen);
        self.calculate_safety_score(&interactions)
    }

    fn canonical_pair(a: &str, b: &str) -> (String, String) {
        if a <= b {
            (a.to_string(), b.to_string())
        } else {
            (b.to_string(), a.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::InteractionGraph;
    use crate::model::{EvidenceLevel, Interaction, InteractionType};

    fn setup() -> (InteractionGraph, Vec<Drug>, PatientRegimen) {
        let drugs = vec![
            Drug::new("warfarin", "anticoagulant", vec!["VKORC1".into()]),
            Drug::new("aspirin", "nsaid", vec!["COX-1".into()]),
            Drug::new("ibuprofen", "nsaid", vec!["COX-1".into(), "COX-2".into()]),
            Drug::new("naproxen", "nsaid", vec!["COX-1".into(), "COX-2".into()]),
            Drug::new("fluoxetine", "ssri", vec!["SERT".into()]),
            Drug::new("sertraline", "ssri", vec!["SERT".into()]),
            Drug::new("metformin", "biguanide", vec!["AMPK".into()]),
            Drug::new("omeprazole", "ppi", vec!["CYP2C19".into()]),
        ];

        let interactions: Vec<Interaction> = vec![
            Interaction {
                drug_a: "aspirin".into(),
                drug_b: "warfarin".into(),
                interaction_type: InteractionType::Pharmacodynamic,
                severity: SeverityLevel::Major,
                mechanism: "Additive anticoagulation".into(),
                evidence: EvidenceLevel::Established,
                recommendation: None,
            },
            Interaction {
                drug_a: "ibuprofen".into(),
                drug_b: "warfarin".into(),
                interaction_type: InteractionType::Both,
                severity: SeverityLevel::Contraindicated,
                mechanism: "Major bleeding risk".into(),
                evidence: EvidenceLevel::Established,
                recommendation: None,
            },
            Interaction {
                drug_a: "naproxen".into(),
                drug_b: "warfarin".into(),
                interaction_type: InteractionType::Both,
                severity: SeverityLevel::Major,
                mechanism: "Increased bleeding risk".into(),
                evidence: EvidenceLevel::Probable,
                recommendation: None,
            },
            Interaction {
                drug_a: "fluoxetine".into(),
                drug_b: "warfarin".into(),
                interaction_type: InteractionType::Pharmacokinetic,
                severity: SeverityLevel::Moderate,
                mechanism: "CYP2C9 inhibition".into(),
                evidence: EvidenceLevel::Probable,
                recommendation: None,
            },
            Interaction {
                drug_a: "sertraline".into(),
                drug_b: "warfarin".into(),
                interaction_type: InteractionType::Pharmacokinetic,
                severity: SeverityLevel::Minor,
                mechanism: "Mild CYP effect".into(),
                evidence: EvidenceLevel::Suspected,
                recommendation: None,
            },
            Interaction {
                drug_a: "omeprazole".into(),
                drug_b: "warfarin".into(),
                interaction_type: InteractionType::Pharmacokinetic,
                severity: SeverityLevel::Minor,
                mechanism: "Minor CYP2C19 effect".into(),
                evidence: EvidenceLevel::Suspected,
                recommendation: None,
            },
        ]
        .into_iter()
        .map(|i| i.canonicalized())
        .collect();

        let graph = InteractionGraph::new(&drugs, &interactions);
        let regimen = PatientRegimen::new(vec![
            "warfarin".into(),
            "aspirin".into(),
            "fluoxetine".into(),
        ]);

        (graph, drugs, regimen)
    }

    #[test]
    fn test_find_alternatives_for_aspirin() {
        let (graph, drugs, regimen) = setup();
        let engine = SuggestionEngine::new(&graph, &drugs);

        let alternatives = engine.find_alternatives("aspirin", &regimen);

        // Should find ibuprofen and naproxen as same-class alternatives
        assert!(!alternatives.is_empty());

        // All should be NSAIDs
        for alt in &alternatives {
            assert_eq!(alt.drug.drug_class, "nsaid");
            assert!(alt.same_class);
        }
    }

    #[test]
    fn test_alternatives_safer_than_original() {
        let (graph, drugs, regimen) = setup();
        let engine = SuggestionEngine::new(&graph, &drugs);

        let alternatives = engine.find_alternatives("aspirin", &regimen);

        // Alternatives should be safer or equal
        for alt in &alternatives {
            assert!(alt.safety_score <= 3); // aspirin's score with warfarin is 3
        }
    }

    #[test]
    fn test_suggestions_sorted_by_safety() {
        let (graph, drugs, regimen) = setup();
        let engine = SuggestionEngine::new(&graph, &drugs);

        let alternatives = engine.find_alternatives("fluoxetine", &regimen);

        // Should be sorted by safety score
        for window in alternatives.windows(2) {
            assert!(window[0].safety_score <= window[1].safety_score);
        }
    }

    #[test]
    fn test_broad_alternatives() {
        let (graph, drugs, regimen) = setup();
        let engine = SuggestionEngine::new(&graph, &drugs);

        let alternatives = engine.find_broad_alternatives("aspirin", &regimen);
        // Should include drugs from other classes too
        assert!(!alternatives.is_empty());
    }

    #[test]
    fn test_find_unlisted_interactors() {
        let (graph, drugs, regimen) = setup();
        let engine = SuggestionEngine::new(&graph, &drugs);

        let interactors = engine.find_unlisted_interactors(&regimen);

        // Should find drugs that interact with regimen drugs but aren't in regimen
        assert!(!interactors.is_empty());

        // None of these should be in the regimen
        for (drug, _, _) in &interactors {
            assert!(!regimen.medications.contains(&drug.name));
        }
    }
}
