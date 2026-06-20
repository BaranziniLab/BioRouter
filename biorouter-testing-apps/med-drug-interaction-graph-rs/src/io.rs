use crate::model::{Drug, EvidenceLevel, Interaction, InteractionType, SeverityLevel};
use csv::ReaderBuilder;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IoError {
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Missing field '{field}' in {context}")]
    MissingField { field: String, context: String },
}

// ─── CSV row representations ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct DrugCsvRow {
    name: String,
    #[serde(rename = "class")]
    drug_class: String,
    #[serde(default)]
    targets: String, // comma-separated
    #[serde(default)]
    brand_names: String, // comma-separated
}

#[derive(Debug, Deserialize)]
struct InteractionCsvRow {
    drug_a: String,
    drug_b: String,
    #[serde(rename = "type")]
    interaction_type: String,
    severity: String,
    mechanism: String,
    evidence: String,
    #[serde(default)]
    recommendation: String,
}

// ─── JSON representations ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct DrugJson {
    name: String,
    class: String,
    #[serde(default)]
    targets: Vec<String>,
    #[serde(default = "default_brand_names")]
    brand_names: Vec<String>,
}

fn default_brand_names() -> Vec<String> {
    Vec::new()
}

#[derive(Debug, Deserialize)]
struct InteractionJson {
    drug_a: String,
    drug_b: String,
    #[serde(rename = "type")]
    interaction_type: String,
    severity: String,
    mechanism: String,
    evidence: String,
    recommendation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DrugDatabaseJson {
    #[serde(default)]
    drugs: Vec<DrugJson>,
    #[serde(default)]
    interactions: Vec<InteractionJson>,
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Load drugs from a CSV file.
///
/// Expected columns: name, class, targets (semicolon-sep), brand_names (semicolon-sep, optional)
pub fn load_drugs_csv<P: AsRef<Path>>(path: P) -> Result<Vec<Drug>, IoError> {
    let file = File::open(path)?;
    let mut rdr = ReaderBuilder::new().has_headers(true).from_reader(BufReader::new(file));

    let mut drugs = Vec::new();
    for result in rdr.deserialize() {
        let row: DrugCsvRow = result?;
        let targets: Vec<String> = row
            .targets
            .split(';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let brand_names: Vec<String> = row
            .brand_names
            .split(';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let drug = Drug::new(&row.name, &row.drug_class, targets).with_brand_names(brand_names);
        drugs.push(drug);
    }

    Ok(drugs)
}

/// Load interactions from a CSV file.
///
/// Expected columns: drug_a, drug_b, type, severity, mechanism, evidence, recommendation (optional)
pub fn load_interactions_csv<P: AsRef<Path>>(path: P) -> Result<Vec<Interaction>, IoError> {
    let file = File::open(path)?;
    let mut rdr = ReaderBuilder::new().has_headers(true).from_reader(BufReader::new(file));

    let mut interactions = Vec::new();
    for result in rdr.deserialize() {
        let row: InteractionCsvRow = result?;
        let itype = InteractionType::from_str(&row.interaction_type)
            .ok_or_else(|| IoError::Parse(format!("Unknown interaction type: {}", row.interaction_type)))?;
        let severity = SeverityLevel::from_str(&row.severity)
            .ok_or_else(|| IoError::Parse(format!("Unknown severity: {}", row.severity)))?;
        let evidence = EvidenceLevel::from_str(&row.evidence)
            .ok_or_else(|| IoError::Parse(format!("Unknown evidence level: {}", row.evidence)))?;

        let recommendation = if row.recommendation.is_empty() {
            None
        } else {
            Some(row.recommendation)
        };

        let interaction = Interaction {
            drug_a: row.drug_a.to_lowercase(),
            drug_b: row.drug_b.to_lowercase(),
            interaction_type: itype,
            severity,
            mechanism: row.mechanism,
            evidence,
            recommendation,
        }
        .canonicalized();

        interactions.push(interaction);
    }

    Ok(interactions)
}

/// Load a complete drug database from a JSON file.
///
/// Expected structure: { "drugs": [...], "interactions": [...] }
pub fn load_database_json<P: AsRef<Path>>(path: P) -> Result<(Vec<Drug>, Vec<Interaction>), IoError> {
    let file = File::open(path)?;
    let db: DrugDatabaseJson = serde_json::from_reader(BufReader::new(file))?;

    let drugs: Vec<Drug> = db
        .drugs
        .into_iter()
        .map(|d| {
            Drug::new(&d.name, &d.class, d.targets).with_brand_names(d.brand_names)
        })
        .collect();

    let mut interactions: Vec<Interaction> = Vec::new();
    for ij in db.interactions {
        let itype = InteractionType::from_str(&ij.interaction_type)
            .ok_or_else(|| IoError::Parse(format!("Unknown interaction type: {}", ij.interaction_type)))?;
        let severity = SeverityLevel::from_str(&ij.severity)
            .ok_or_else(|| IoError::Parse(format!("Unknown severity: {}", ij.severity)))?;
        let evidence = EvidenceLevel::from_str(&ij.evidence)
            .ok_or_else(|| IoError::Parse(format!("Unknown evidence level: {}", ij.evidence)))?;

        let interaction = Interaction {
            drug_a: ij.drug_a.to_lowercase(),
            drug_b: ij.drug_b.to_lowercase(),
            interaction_type: itype,
            severity,
            mechanism: ij.mechanism,
            evidence,
            recommendation: ij.recommendation,
        }
        .canonicalized();

        interactions.push(interaction);
    }

    Ok((drugs, interactions))
}

/// Load drugs and interactions from separate CSV files.
pub fn load_from_csvs<P: AsRef<Path>>(
    drugs_path: P,
    interactions_path: P,
) -> Result<(Vec<Drug>, Vec<Interaction>), IoError> {
    let drugs = load_drugs_csv(drugs_path)?;
    let interactions = load_interactions_csv(interactions_path)?;
    Ok((drugs, interactions))
}

/// Build a lookup map from drug name to Drug struct.
pub fn drug_lookup(drugs: &[Drug]) -> HashMap<&str, &Drug> {
    drugs.iter().map(|d| (d.name.as_str(), d)).collect()
}

/// Validate that all drugs referenced in interactions exist in the drug database.
pub fn validate_database(
    drugs: &[Drug],
    interactions: &[Interaction],
) -> Vec<String> {
    let lookup = drug_lookup(drugs);
    let mut warnings = Vec::new();

    for ix in interactions {
        if !lookup.contains_key(ix.drug_a.as_str()) {
            warnings.push(format!(
                "Interaction references unknown drug: '{}'",
                ix.drug_a
            ));
        }
        if !lookup.contains_key(ix.drug_b.as_str()) {
            warnings.push(format!(
                "Interaction references unknown drug: '{}'",
                ix.drug_b
            ));
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn sample_drugs_csv() -> String {
        "name,class,targets,brand_names\n\
         warfarin,anticoagulant,VKORC1;CYP2C9,Coumadin;Jantoven\n\
         aspirin,NSAID,COX-1;COX-2,Bayer;Ecotrin\n\
         metformin,biguanide,AMPK,Glucophage;Fortamet\n\
         fluoxetine,SSRI,SERT;CYP2D6,Prozac;Sarafem\n\
         simvastatin,statin,HMG-CoA_reductase,Zocor\n\
         omeprazole,proton_pump_inhibitor,CYP2C19;H_K_ATPase,Prilosec\n"
            .to_string()
    }

    fn sample_interactions_csv() -> String {
        "drug_a,drug_b,type,severity,mechanism,evidence,recommendation\n\
         warfarin,aspirin,pharmacodynamic,major,Additive anticoagulant effect increases bleeding risk,established,Monitor INR closely\n\
         warfarin,fluoxetine,pharmacokinetic,moderate,CYP2C9 inhibition increases warfarin levels,probable,Dose adjust warfarin\n\
         simvastatin,omeprazole,pharmacokinetic,minor,CYP3A4 minor effect,probable,Monitor for myopathy\n\
         metformin,fluoxetine,pharmacodynamic,moderate,Increased risk of hyponatremia,probable,Monitor sodium levels\n\
         aspirin,omeprazole,pharmacokinetic,minor,Altered absorption kinetics,unknown,Take aspirin 30 min before omeprazole\n"
            .to_string()
    }

    #[test]
    fn test_load_drugs_csv() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(sample_drugs_csv().as_bytes()).unwrap();
        let drugs = load_drugs_csv(f.path()).unwrap();
        assert_eq!(drugs.len(), 6);
        assert_eq!(drugs[0].name, "warfarin");
        assert_eq!(drugs[0].drug_class, "anticoagulant");
        assert_eq!(drugs[0].targets.len(), 2);
        assert_eq!(drugs[0].brand_names.len(), 2);
    }

    #[test]
    fn test_load_interactions_csv() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(sample_interactions_csv().as_bytes()).unwrap();
        let interactions = load_interactions_csv(f.path()).unwrap();
        assert_eq!(interactions.len(), 5);
        // Should be canonicalized (alphabetical order)
        for ix in &interactions {
            assert!(ix.drug_a <= ix.drug_b);
        }
    }

    #[test]
    fn test_validate_database() {
        let mut f1 = NamedTempFile::new().unwrap();
        f1.write_all(sample_drugs_csv().as_bytes()).unwrap();
        let drugs = load_drugs_csv(f1.path()).unwrap();

        let mut f2 = NamedTempFile::new().unwrap();
        f2.write_all(sample_interactions_csv().as_bytes()).unwrap();
        let interactions = load_interactions_csv(f2.path()).unwrap();

        let warnings = validate_database(&drugs, &interactions);
        assert!(warnings.is_empty(), "No warnings expected for well-formed data");
    }

    #[test]
    fn test_validate_database_unknown_drug() {
        let mut f1 = NamedTempFile::new().unwrap();
        f1.write_all(sample_drugs_csv().as_bytes()).unwrap();
        let drugs = load_drugs_csv(f1.path()).unwrap();

        let mut f2 = NamedTempFile::new().unwrap();
        f2.write_all(
            "drug_a,drug_b,type,severity,mechanism,evidence,recommendation\n\
             warfarin,nonexistent_drug,pharmacodynamic,major,test interaction,established,\n"
                .as_bytes(),
        )
        .unwrap();
        let interactions = load_interactions_csv(f2.path()).unwrap();

        let warnings = validate_database(&drugs, &interactions);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("nonexistent_drug"));
    }

    #[test]
    fn test_drug_lookup() {
        let drugs = vec![
            Drug::new("warfarin", "anticoagulant", vec![]),
            Drug::new("aspirin", "nsaid", vec![]),
        ];
        let lookup = drug_lookup(&drugs);
        assert!(lookup.contains_key("warfarin"));
        assert!(lookup.contains_key("aspirin"));
        assert!(!lookup.contains_key("metformin"));
    }

    #[test]
    fn test_load_database_json() {
        let json = r#"{
            "drugs": [
                {"name": "warfarin", "class": "anticoagulant", "targets": ["VKORC1"], "brand_names": ["Coumadin"]},
                {"name": "aspirin", "class": "NSAID", "targets": ["COX-1", "COX-2"]}
            ],
            "interactions": [
                {"drug_a": "warfarin", "drug_b": "aspirin", "type": "pharmacodynamic", "severity": "major", "mechanism": "Bleeding risk", "evidence": "established", "recommendation": "Monitor INR"}
            ]
        }"#;

        let mut f = NamedTempFile::new().unwrap();
        f.write_all(json.as_bytes()).unwrap();
        let (drugs, interactions) = load_database_json(f.path()).unwrap();
        assert_eq!(drugs.len(), 2);
        assert_eq!(interactions.len(), 1);
        assert_eq!(interactions[0].drug_a, "aspirin");
        assert_eq!(interactions[0].drug_b, "warfarin");
    }
}
