use crate::model::{InteractionReportEntry, SeverityLevel};

/// Scoring strategy for regimen risk assessment.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum ScoringStrategy {
    /// Simple sum of severity scores
    Sum,
    /// Maximum severity in the regimen
    Max,
    /// Average severity across all interactions
    Average,
    /// Weighted score (severity × evidence × interaction_type bonus)
    Weighted,
}

impl Default for ScoringStrategy {
    fn default() -> Self {
        ScoringStrategy::Weighted
    }
}

/// Detailed severity breakdown for a regimen.
#[derive(Debug, Clone)]
pub struct RegimenSeverityProfile {
    /// Overall risk score
    pub total_score: u32,
    /// Score broken down by severity level
    pub by_severity: SeverityBreakdown,
    /// Number of interactions
    pub interaction_count: usize,
    /// Number of contraindicated interactions
    pub contraindicated_count: usize,
    /// Highest severity interaction
    #[allow(dead_code)]
    pub max_severity: Option<SeverityLevel>,
    /// Risk level description
    pub risk_level: String,
}

#[derive(Debug, Clone)]
pub struct SeverityBreakdown {
    pub minor: usize,
    pub moderate: usize,
    pub major: usize,
    pub contraindicated: usize,
}

impl SeverityBreakdown {
    pub fn new() -> Self {
        SeverityBreakdown {
            minor: 0,
            moderate: 0,
            major: 0,
            contraindicated: 0,
        }
    }
}

/// Calculate a comprehensive severity profile for a regimen.
pub fn calculate_profile(
    entries: &[InteractionReportEntry],
    strategy: ScoringStrategy,
) -> RegimenSeverityProfile {
    if entries.is_empty() {
        return RegimenSeverityProfile {
            total_score: 0,
            by_severity: SeverityBreakdown::new(),
            interaction_count: 0,
            contraindicated_count: 0,
            max_severity: None,
            risk_level: "None".to_string(),
        };
    }

    let mut breakdown = SeverityBreakdown::new();
    let mut max_sev = SeverityLevel::Minor;

    for entry in entries {
        match entry.severity {
            SeverityLevel::Minor => breakdown.minor += 1,
            SeverityLevel::Moderate => breakdown.moderate += 1,
            SeverityLevel::Major => breakdown.major += 1,
            SeverityLevel::Contraindicated => breakdown.contraindicated += 1,
        }
        if entry.severity > max_sev {
            max_sev = entry.severity;
        }
    }

    let total_score = match strategy {
        ScoringStrategy::Sum => entries.iter().map(|e| e.severity.score()).sum(),
        ScoringStrategy::Max => max_sev.score(),
        ScoringStrategy::Average => {
            let sum: u32 = entries.iter().map(|e| e.severity.score()).sum();
            sum / entries.len() as u32
        }
        ScoringStrategy::Weighted => entries.iter().map(|e| weighted_score(e)).sum(),
    };

    let contraindicated_count = breakdown.contraindicated;
    let risk_level = classify_risk(total_score, contraindicated_count);

    RegimenSeverityProfile {
        total_score,
        by_severity: breakdown,
        interaction_count: entries.len(),
        contraindicated_count,
        max_severity: Some(max_sev),
        risk_level,
    }
}

/// Weighted score for a single interaction entry.
fn weighted_score(entry: &InteractionReportEntry) -> u32 {
    let severity = entry.severity.score();

    let evidence_bonus = match entry.evidence {
        crate::model::EvidenceLevel::Established => 2,
        crate::model::EvidenceLevel::Probable => 1,
        crate::model::EvidenceLevel::Suspected => 0,
        crate::model::EvidenceLevel::Unknown => 0,
    };

    let type_bonus = match entry.interaction_type {
        crate::model::InteractionType::Both => 2,
        crate::model::InteractionType::Pharmacokinetic => 1,
        crate::model::InteractionType::Pharmacodynamic => 1,
    };

    (severity + evidence_bonus + type_bonus) as u32
}

/// Classify the overall risk level based on score and contraindications.
fn classify_risk(score: u32, contraindicated_count: usize) -> String {
    if contraindicated_count > 0 {
        "CRITICAL — Contains contraindicated combinations".to_string()
    } else if score >= 20 {
        "HIGH — Significant multi-drug risk".to_string()
    } else if score >= 10 {
        "MODERATE — Multiple interactions requiring monitoring".to_string()
    } else if score >= 3 {
        "LOW — Minor interactions, standard monitoring".to_string()
    } else {
        "MINIMAL — Few or no significant interactions".to_string()
    }
}

/// Compare two severity profiles and determine which regimen is safer.
pub fn compare_profiles(a: &RegimenSeverityProfile, b: &RegimenSeverityProfile) -> std::cmp::Ordering {
    // Prefer fewer contraindications first, then lower total score
    a.contraindicated_count
        .cmp(&b.contraindicated_count)
        .then_with(|| a.total_score.cmp(&b.total_score))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EvidenceLevel, InteractionType};

    fn test_entries() -> Vec<InteractionReportEntry> {
        vec![
            InteractionReportEntry {
                drug_a: "warfarin".into(),
                drug_b: "aspirin".into(),
                interaction_type: InteractionType::Pharmacodynamic,
                severity: SeverityLevel::Major,
                mechanism: "Bleeding risk".into(),
                evidence: EvidenceLevel::Established,
                recommendation: None,
            },
            InteractionReportEntry {
                drug_a: "fluoxetine".into(),
                drug_b: "warfarin".into(),
                interaction_type: InteractionType::Pharmacokinetic,
                severity: SeverityLevel::Moderate,
                mechanism: "CYP inhibition".into(),
                evidence: EvidenceLevel::Probable,
                recommendation: None,
            },
        ]
    }

    #[test]
    fn test_empty_profile() {
        let profile = calculate_profile(&[], ScoringStrategy::Sum);
        assert_eq!(profile.total_score, 0);
        assert_eq!(profile.interaction_count, 0);
        assert!(profile.max_severity.is_none());
    }

    #[test]
    fn test_sum_strategy() {
        let entries = test_entries();
        let profile = calculate_profile(&entries, ScoringStrategy::Sum);
        assert_eq!(profile.total_score, 5); // Major(3) + Moderate(2)
        assert_eq!(profile.interaction_count, 2);
    }

    #[test]
    fn test_max_strategy() {
        let entries = test_entries();
        let profile = calculate_profile(&entries, ScoringStrategy::Max);
        assert_eq!(profile.total_score, 3); // Major
    }

    #[test]
    fn test_average_strategy() {
        let entries = test_entries();
        let profile = calculate_profile(&entries, ScoringStrategy::Average);
        assert_eq!(profile.total_score, 2); // (3+2)/2 = 2
    }

    #[test]
    fn test_weighted_strategy() {
        let entries = test_entries();
        let profile = calculate_profile(&entries, ScoringStrategy::Weighted);
        // Major(3) + Established(2) + PD(1) = 6
        // Moderate(2) + Probable(1) + PK(1) = 4
        // Total: 10
        assert_eq!(profile.total_score, 10);
    }

    #[test]
    fn test_risk_classification() {
        let entries = test_entries();
        let profile = calculate_profile(&entries, ScoringStrategy::Weighted);
        assert!(profile.risk_level.contains("MODERATE") || profile.risk_level.contains("HIGH"));
    }

    #[test]
    fn test_contraindicated_detection() {
        let entries = vec![InteractionReportEntry {
            drug_a: "a".into(),
            drug_b: "b".into(),
            interaction_type: InteractionType::Both,
            severity: SeverityLevel::Contraindicated,
            mechanism: "test".into(),
            evidence: EvidenceLevel::Established,
            recommendation: None,
        }];
        let profile = calculate_profile(&entries, ScoringStrategy::Weighted);
        assert_eq!(profile.contraindicated_count, 1);
        assert!(profile.risk_level.contains("CRITICAL"));
    }

    #[test]
    fn test_compare_profiles() {
        let a = RegimenSeverityProfile {
            total_score: 10,
            by_severity: SeverityBreakdown::new(),
            interaction_count: 2,
            contraindicated_count: 1,
            max_severity: Some(SeverityLevel::Contraindicated),
            risk_level: "test".into(),
        };
        let b = RegimenSeverityProfile {
            total_score: 5,
            by_severity: SeverityBreakdown::new(),
            interaction_count: 1,
            contraindicated_count: 0,
            max_severity: Some(SeverityLevel::Moderate),
            risk_level: "test".into(),
        };
        // a has contraindications, b does not => b is safer
        assert_eq!(compare_profiles(&a, &b), std::cmp::Ordering::Greater);
    }

    #[test]
    fn test_severity_breakdown_counts() {
        let entries = test_entries();
        let profile = calculate_profile(&entries, ScoringStrategy::Sum);
        assert_eq!(profile.by_severity.major, 1);
        assert_eq!(profile.by_severity.moderate, 1);
        assert_eq!(profile.by_severity.minor, 0);
        assert_eq!(profile.by_severity.contraindicated, 0);
    }
}
