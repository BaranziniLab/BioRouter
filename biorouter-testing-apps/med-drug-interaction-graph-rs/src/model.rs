use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents the type of drug-drug interaction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InteractionType {
    /// Pharmacokinetic: one drug affects absorption/distribution/metabolism/excretion of another
    Pharmacokinetic,
    /// Pharmacodynamic: drugs have additive/synergistic/adverse effects at target level
    Pharmacodynamic,
    /// Both PK and PD interactions
    Both,
}

impl InteractionType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pharmacokinetic" | "pk" => Some(InteractionType::Pharmacokinetic),
            "pharmacodynamic" | "pd" => Some(InteractionType::Pharmacodynamic),
            "both" | "pk/pd" => Some(InteractionType::Both),
            _ => None,
        }
    }
}

impl fmt::Display for InteractionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InteractionType::Pharmacokinetic => write!(f, "Pharmacokinetic"),
            InteractionType::Pharmacodynamic => write!(f, "Pharmacodynamic"),
            InteractionType::Both => write!(f, "Pharmacokinetic/Pharmacodynamic"),
        }
    }
}

/// Severity level of a drug-drug interaction
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SeverityLevel {
    /// Minor: monitor patient, low clinical significance
    Minor,
    /// Moderate: may require dose adjustment or monitoring
    Moderate,
    /// Major: avoid combination if possible, significant clinical impact
    Major,
    /// Contraindicated: combination should never be used together
    Contraindicated,
}

impl SeverityLevel {
    /// Numeric score for severity (higher = more severe)
    pub fn score(&self) -> u32 {
        match self {
            SeverityLevel::Minor => 1,
            SeverityLevel::Moderate => 2,
            SeverityLevel::Major => 3,
            SeverityLevel::Contraindicated => 4,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "minor" => Some(SeverityLevel::Minor),
            "moderate" => Some(SeverityLevel::Moderate),
            "major" => Some(SeverityLevel::Major),
            "contraindicated" => Some(SeverityLevel::Contraindicated),
            _ => None,
        }
    }
}

impl fmt::Display for SeverityLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeverityLevel::Minor => write!(f, "Minor"),
            SeverityLevel::Moderate => write!(f, "Moderate"),
            SeverityLevel::Major => write!(f, "Major"),
            SeverityLevel::Contraindicated => write!(f, "Contraindicated"),
        }
    }
}

/// Evidence level for a drug interaction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EvidenceLevel {
    /// Established: confirmed by multiple studies / clinical guidelines
    Established,
    /// Probable: supported by case series or strong pharmacological reasoning
    Probable,
    /// Suspected: limited evidence, theoretical or case reports
    Suspected,
    /// Unknown: interaction is plausible but unverified
    Unknown,
}

impl EvidenceLevel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "established" => Some(EvidenceLevel::Established),
            "probable" => Some(EvidenceLevel::Probable),
            "suspected" => Some(EvidenceLevel::Suspected),
            "unknown" => Some(EvidenceLevel::Unknown),
            _ => None,
        }
    }
}

impl fmt::Display for EvidenceLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvidenceLevel::Established => write!(f, "Established"),
            EvidenceLevel::Probable => write!(f, "Probable"),
            EvidenceLevel::Suspected => write!(f, "Suspected"),
            EvidenceLevel::Unknown => write!(f, "Unknown"),
        }
    }
}

/// A drug node in the interaction graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Drug {
    /// Unique drug name (normalized to lowercase)
    pub name: String,
    /// Drug class (e.g., "SSRI", "ACE Inhibitor", "Statin")
    pub drug_class: String,
    /// List of pharmacological targets (e.g., "CYP2D6", "ACE", "HMG-CoA reductase")
    pub targets: Vec<String>,
    /// Optional: known brand names
    pub brand_names: Vec<String>,
}

impl Drug {
    pub fn new(name: &str, drug_class: &str, targets: Vec<String>) -> Self {
        Drug {
            name: name.to_lowercase(),
            drug_class: drug_class.to_string(),
            targets,
            brand_names: Vec::new(),
        }
    }

    pub fn with_brand_names(mut self, brands: Vec<String>) -> Self {
        self.brand_names = brands;
        self
    }
}

impl fmt::Display for Drug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.drug_class)
    }
}

/// An interaction between two drugs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Interaction {
    /// First drug name (normalized)
    pub drug_a: String,
    /// Second drug name (normalized)
    pub drug_b: String,
    /// Type of interaction
    pub interaction_type: InteractionType,
    /// Severity level
    pub severity: SeverityLevel,
    /// Mechanism of interaction (textual description)
    pub mechanism: String,
    /// Evidence level
    pub evidence: EvidenceLevel,
    /// Optional clinical recommendation
    pub recommendation: Option<String>,
}

impl Interaction {
    /// Ensure canonical ordering (alphabetical) for undirected representation
    pub fn canonicalized(mut self) -> Self {
        if self.drug_a > self.drug_b {
            std::mem::swap(&mut self.drug_a, &mut self.drug_b);
        }
        self
    }

    /// Return the pair as a sorted tuple
    pub fn pair(&self) -> (&str, &str) {
        if self.drug_a <= self.drug_b {
            (&self.drug_a, &self.drug_b)
        } else {
            (&self.drug_b, &self.drug_a)
        }
    }
}

impl fmt::Display for Interaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ↔ {} [{}] | {} | Evidence: {}",
            self.drug_a, self.drug_b, self.interaction_type, self.severity, self.evidence
        )
    }
}

/// A patient's medication list
#[derive(Debug, Clone)]
pub struct PatientRegimen {
    /// List of drug names (normalized to lowercase)
    pub medications: Vec<String>,
}

impl PatientRegimen {
    pub fn new(medications: Vec<String>) -> Self {
        PatientRegimen {
            medications: medications.into_iter().map(|m| m.to_lowercase()).collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.medications.len()
    }

    pub fn is_empty(&self) -> bool {
        self.medications.is_empty()
    }
}

/// A single pairwise interaction report entry
#[derive(Debug, Clone, Serialize)]
pub struct InteractionReportEntry {
    pub drug_a: String,
    pub drug_b: String,
    pub interaction_type: InteractionType,
    pub severity: SeverityLevel,
    pub mechanism: String,
    pub evidence: EvidenceLevel,
    pub recommendation: Option<String>,
}

impl fmt::Display for InteractionReportEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} ↔ {} | {} | Evidence: {}",
            self.severity, self.drug_a, self.drug_b, self.interaction_type, self.evidence
        )?;
        if !self.mechanism.is_empty() {
            write!(f, "\n  Mechanism: {}", self.mechanism)?;
        }
        if let Some(rec) = &self.recommendation {
            write!(f, "\n  Recommendation: {}", rec)?;
        }
        Ok(())
    }
}

/// Full interaction report for a regimen
#[derive(Debug, Clone)]
pub struct InteractionReport {
    pub entries: Vec<InteractionReportEntry>,
    pub regimen_severity_score: u32,
}

impl InteractionReport {
    pub fn new(entries: Vec<InteractionReportEntry>, regimen_severity_score: u32) -> Self {
        InteractionReport {
            entries,
            regimen_severity_score,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// An interaction chain / cascade: a path of interacting drugs
#[derive(Debug, Clone)]
pub struct InteractionChain {
    /// Ordered list of drug names forming the chain
    pub drugs: Vec<String>,
    /// Summed severity across all links in the chain
    pub total_severity_score: u32,
    /// The severity of the weakest link (bottleneck)
    pub min_severity: SeverityLevel,
}

impl fmt::Display for InteractionChain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Chain: {} (length={}, severity_score={}, bottleneck={})",
            self.drugs.join(" → "),
            self.drugs.len(),
            self.total_severity_score,
            self.min_severity
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drug_creation() {
        let drug = Drug::new("warfarin", "anticoagulant", vec!["VKORC1".into(), "CYP2C9".into()]);
        assert_eq!(drug.name, "warfarin");
        assert_eq!(drug.drug_class, "anticoagulant");
        assert_eq!(drug.targets.len(), 2);
    }

    #[test]
    fn test_severity_ordering() {
        assert!(SeverityLevel::Minor < SeverityLevel::Moderate);
        assert!(SeverityLevel::Moderate < SeverityLevel::Major);
        assert!(SeverityLevel::Major < SeverityLevel::Contraindicated);
    }

    #[test]
    fn test_severity_score() {
        assert_eq!(SeverityLevel::Minor.score(), 1);
        assert_eq!(SeverityLevel::Moderate.score(), 2);
        assert_eq!(SeverityLevel::Major.score(), 3);
        assert_eq!(SeverityLevel::Contraindicated.score(), 4);
    }

    #[test]
    fn test_interaction_canonicalization() {
        let interaction = Interaction {
            drug_a: "aspirin".into(),
            drug_b: "warfarin".into(),
            interaction_type: InteractionType::Pharmacodynamic,
            severity: SeverityLevel::Major,
            mechanism: "Increased bleeding risk".into(),
            evidence: EvidenceLevel::Established,
            recommendation: Some("Monitor INR closely".into()),
        };
        let canon = interaction.canonicalized();
        assert_eq!(canon.drug_a, "aspirin");
        assert_eq!(canon.drug_b, "warfarin");

        let interaction2 = Interaction {
            drug_a: "warfarin".into(),
            drug_b: "aspirin".into(),
            interaction_type: InteractionType::Pharmacodynamic,
            severity: SeverityLevel::Major,
            mechanism: "Increased bleeding risk".into(),
            evidence: EvidenceLevel::Established,
            recommendation: None,
        };
        let canon2 = interaction2.canonicalized();
        assert_eq!(canon2.drug_a, "aspirin");
        assert_eq!(canon2.drug_b, "warfarin");
    }

    #[test]
    fn test_patient_regimen_normalization() {
        let regimen = PatientRegimen::new(vec!["Warfarin".into(), "ASPIRIN".into(), "Metformin".into()]);
        assert_eq!(regimen.medications, vec!["warfarin", "aspirin", "metformin"]);
        assert_eq!(regimen.len(), 3);
        assert!(!regimen.is_empty());
    }

    #[test]
    fn test_interaction_pair() {
        let interaction = Interaction {
            drug_a: "warfarin".into(),
            drug_b: "aspirin".into(),
            interaction_type: InteractionType::Pharmacodynamic,
            severity: SeverityLevel::Major,
            mechanism: "test".into(),
            evidence: EvidenceLevel::Established,
            recommendation: None,
        };
        let (a, b) = interaction.pair();
        // pair() returns sorted
        assert_eq!(a, "aspirin");
        assert_eq!(b, "warfarin");
    }

    #[test]
    fn test_display_traits() {
        let drug = Drug::new("metformin", "biguanide", vec!["AMPK".into()]);
        let _ = format!("{}", drug);

        let interaction = Interaction {
            drug_a: "a".into(),
            drug_b: "b".into(),
            interaction_type: InteractionType::Pharmacokinetic,
            severity: SeverityLevel::Moderate,
            mechanism: "CYP inhibition".into(),
            evidence: EvidenceLevel::Probable,
            recommendation: None,
        };
        let _ = format!("{}", interaction);

        let entry = InteractionReportEntry {
            drug_a: "a".into(),
            drug_b: "b".into(),
            interaction_type: InteractionType::Pharmacokinetic,
            severity: SeverityLevel::Moderate,
            mechanism: "CYP inhibition".into(),
            evidence: EvidenceLevel::Probable,
            recommendation: None,
        };
        let _ = format!("{}", entry);
    }

    #[test]
    fn test_empty_regimen() {
        let regimen = PatientRegimen::new(vec![]);
        assert!(regimen.is_empty());
        assert_eq!(regimen.len(), 0);
    }
}
