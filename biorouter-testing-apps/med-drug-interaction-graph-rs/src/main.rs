mod cli;
mod graph;
mod io;
mod model;
mod query;
mod severity;
mod suggest;

use clap::Parser;
use cli::{Cli, Commands};
use graph::InteractionGraph;
use io::load_database_json;
use model::PatientRegimen;
use query::InteractionQuery;
use severity::{calculate_profile, ScoringStrategy};
use suggest::SuggestionEngine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Query {
            database,
            medications,
            max_chain,
            detailed,
        } => cmd_query(&database, &medications, max_chain, detailed)?,
        Commands::Drug {
            database,
            name,
            list_all,
        } => cmd_drug(&database, &name, list_all)?,
        Commands::Alternatives {
            database,
            for_drug,
            regimen,
            broad,
        } => cmd_alternatives(&database, &for_drug, &regimen, broad)?,
        Commands::Analyze {
            database,
            components,
            centrality,
            hubs,
        } => cmd_analyze(&database, components, centrality, hubs)?,
        Commands::Compare {
            database,
            regimen_a,
            regimen_b,
        } => cmd_compare(&database, &regimen_a, &regimen_b)?,
    }

    Ok(())
}

fn cmd_query(
    db_path: &std::path::Path,
    meds_str: &str,
    max_chain: usize,
    detailed: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let (drugs, interactions) = load_database_json(db_path)?;

    // Validate database
    let warnings = io::validate_database(&drugs, &interactions);
    for w in &warnings {
        eprintln!("⚠ Warning: {}", w);
    }

    let graph = InteractionGraph::new(&drugs, &interactions);
    let regimen = PatientRegimen::new(
        meds_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect(),
    );

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║         Drug-Drug Interaction Report                       ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("Regimen: {}", regimen.medications.join(", "));
    println!("Database: {} drugs, {} interactions", drugs.len(), interactions.len());
    println!();

    let query = InteractionQuery::new(&graph);
    let report = query.find_all_interactions(&regimen);

    // Severity profile
    let profile = calculate_profile(&report.entries, ScoringStrategy::Weighted);

    println!("━━━ Severity Profile ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  Risk Level:        {}", profile.risk_level);
    println!("  Total Score:       {}", profile.total_score);
    println!("  Interactions:      {}", profile.interaction_count);
    println!(
        "  Breakdown:         {} Minor | {} Moderate | {} Major | {} Contraindicated",
        profile.by_severity.minor,
        profile.by_severity.moderate,
        profile.by_severity.major,
        profile.by_severity.contraindicated
    );
    println!();

    if report.is_empty() {
        println!("✅ No interactions found between the listed medications.");
    } else {
        println!("━━━ Interactions (by severity) ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        for (i, entry) in report.entries.iter().enumerate() {
            println!();
            println!(
                "  {}. [{}] {} ↔ {}",
                i + 1,
                entry.severity,
                entry.drug_a,
                entry.drug_b,
            );
            println!("     Type:     {}", entry.interaction_type);
            println!("     Evidence: {}", entry.evidence);
            if detailed || entry.severity >= model::SeverityLevel::Major {
                println!("     Mechanism: {}", entry.mechanism);
            }
            if let Some(rec) = &entry.recommendation {
                println!("     Recommendation: {}", rec);
            }
        }
    }

    // Detect chains
    println!();
    println!("━━━ Interaction Chains ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    let chains = query.detect_chains(&regimen, max_chain);
    if chains.is_empty() {
        println!("  No multi-step interaction chains detected (max depth: {}).", max_chain);
    } else {
        for (i, chain) in chains.iter().enumerate() {
            println!(
                "  {}. {} (length={}, bottleneck={})",
                i + 1,
                chain.drugs.join(" → "),
                chain.drugs.len(),
                chain.min_severity,
            );
        }
    }

    // Hub analysis
    println!();
    println!("━━━ Hub Drugs (most interactions in database) ━━━━━━━━━━━━━");
    let centrality = graph.weighted_centrality();
    let in_regimen: Vec<&str> = regimen.medications.iter().map(|s| s.as_str()).collect();
    for (drug, score) in centrality.iter().take(5) {
        let marker = if in_regimen.contains(&drug.as_str()) {
            " ← in regimen"
        } else {
            ""
        };
        println!("  {:<20} weighted_score={}{}", drug, score, marker);
    }

    println!();
    println!("══════════════════════════════════════════════════════════════");

    Ok(())
}

fn cmd_drug(
    db_path: &std::path::Path,
    name: &str,
    list_all: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let (drugs, interactions) = load_database_json(db_path)?;
    let graph = InteractionGraph::new(&drugs, &interactions);

    if list_all {
        println!("━━━ All Drugs in Database ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        for drug in &drugs {
            let ix_count = graph.interactions_for(&drug.name).len();
            println!("  {:<20} {:<20} targets: {:<30} interactions: {}", drug.name, drug.drug_class, drug.targets.join(", "), ix_count);
        }
        return Ok(());
    }

    let name_lower = name.to_lowercase();
    let drug = drugs.iter().find(|d| d.name == name_lower);

    match drug {
        Some(d) => {
            println!("━━━ Drug: {} ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━", d.name);
            println!("  Class:   {}", d.drug_class);
            println!("  Targets: {}", d.targets.join(", "));
            if !d.brand_names.is_empty() {
                println!("  Brands:  {}", d.brand_names.join(", "));
            }

            let interactions = graph.interactions_for(&d.name);
            println!("  Interactions: {}", interactions.len());
            println!();

            for ix in &interactions {
                let other = if ix.drug_a == d.name {
                    &ix.drug_b
                } else {
                    &ix.drug_a
                };
                println!(
                    "  ↔ {:<20} [{}] {} | Evidence: {}",
                    other, ix.severity, ix.interaction_type, ix.evidence,
                );
                println!("    Mechanism: {}", ix.mechanism);
            }

            println!();
            let neighbors = graph.neighbors(&d.name);
            println!("  Direct neighbors: {}", neighbors.join(", "));
        }
        None => {
            eprintln!("Drug '{}' not found in database.", name);
            eprintln!("Available drugs: {}", drugs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>().join(", "));
        }
    }

    Ok(())
}

fn cmd_alternatives(
    db_path: &std::path::Path,
    for_drug: &str,
    regimen_str: &str,
    broad: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let (drugs, interactions) = load_database_json(db_path)?;
    let graph = InteractionGraph::new(&drugs, &interactions);
    let regimen = PatientRegimen::new(
        regimen_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect(),
    );

    let engine = SuggestionEngine::new(&graph, &drugs);

    println!("━━━ Alternatives for '{}' ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━", for_drug);
    println!("  Current regimen: {}", regimen.medications.join(", "));
    println!();

    let alternatives = if broad {
        engine.find_broad_alternatives(for_drug, &regimen)
    } else {
        engine.find_alternatives(for_drug, &regimen)
    };

    if alternatives.is_empty() {
        println!("  No safer alternatives found.");
    } else {
        println!("  {:<20} {:<20} {:<10} {:<10} {}", "Drug", "Class", "Interacts", "Worst", "Safety");
        println!("  {:<20} {:<20} {:<10} {:<10} {}", "─".repeat(20), "─".repeat(20), "─".repeat(10), "─".repeat(10), "─".repeat(8));

        for alt in &alternatives {
            println!(
                "  {:<20} {:<20} {:<10} {:<10} {}",
                alt.drug.name,
                alt.drug.drug_class,
                alt.interaction_count,
                alt.worst_severity
                    .map(|s| s.to_string())
                    .unwrap_or("None".to_string()),
                alt.safety_score,
            );

            if !alt.interactions.is_empty() {
                for ix in &alt.interactions {
                    let other = if ix.drug_a == alt.drug.name {
                        &ix.drug_b
                    } else {
                        &ix.drug_a
                    };
                    println!("    ↳ {} with {}: {}", ix.severity, other, ix.mechanism);
                }
            }
        }
    }

    Ok(())
}

fn cmd_analyze(
    db_path: &std::path::Path,
    show_components: bool,
    show_centrality: bool,
    hubs_percentile: Option<f64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (drugs, interactions) = load_database_json(db_path)?;
    let graph = InteractionGraph::new(&drugs, &interactions);

    println!("━━━ Graph Analysis ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  Nodes (drugs):     {}", graph.node_map.len());
    println!("  Edges (interactions): {}", graph.interaction_map.len());

    if show_components {
        println!();
        println!("  ── Connected Components ──");
        let components = graph.connected_components();
        for (i, comp) in components.iter().enumerate() {
            println!("    Component {}: {} drugs ({})", i + 1, comp.len(), comp.join(", "));
        }
    }

    if show_centrality {
        println!();
        println!("  ── Degree Centrality ──");
        let centrality = graph.degree_centrality();
        for (drug, degree) in &centrality {
            println!("    {:<20} degree: {}", drug, degree);
        }

        println!();
        println!("  ── Weighted Centrality (severity-weighted) ──");
        let weighted = graph.weighted_centrality();
        for (drug, score) in &weighted {
            println!("    {:<20} weighted_score: {}", drug, score);
        }
    }

    if let Some(percentile) = hubs_percentile {
        println!();
        println!("  ── Hub Drugs (top {:.0}%) ──", percentile * 100.0);
        let hubs = graph.find_hub_drugs(percentile);
        for (drug, score) in &hubs {
            println!("    {:<20} weighted_score: {}", drug, score);
        }
    }

    Ok(())
}

fn cmd_compare(
    db_path: &std::path::Path,
    regimen_a_str: &str,
    regimen_b_str: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let (drugs, interactions) = load_database_json(db_path)?;
    let graph = InteractionGraph::new(&drugs, &interactions);

    let regimen_a = PatientRegimen::new(
        regimen_a_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect(),
    );
    let regimen_b = PatientRegimen::new(
        regimen_b_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect(),
    );

    let query = InteractionQuery::new(&graph);
    let report_a = query.find_all_interactions(&regimen_a);
    let report_b = query.find_all_interactions(&regimen_b);

    let profile_a = calculate_profile(&report_a.entries, ScoringStrategy::Weighted);
    let profile_b = calculate_profile(&report_b.entries, ScoringStrategy::Weighted);

    println!("━━━ Regimen Comparison ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!();
    println!("  Regimen A: {}", regimen_a.medications.join(", "));
    println!("  Regimen B: {}", regimen_b.medications.join(", "));
    println!();
    println!(
        "  {:<25} {:<20} {:<20}",
        "", "Regimen A", "Regimen B"
    );
    println!(
        "  {:<25} {:<20} {:<20}",
        "─".repeat(25),
        "─".repeat(20),
        "─".repeat(20)
    );
    println!(
        "  {:<25} {:<20} {:<20}",
        "Interactions", profile_a.interaction_count, profile_b.interaction_count
    );
    println!(
        "  {:<25} {:<20} {:<20}",
        "Total Score", profile_a.total_score, profile_b.total_score
    );
    println!(
        "  {:<25} {:<20} {:<20}",
        "Contraindicated",
        profile_a.contraindicated_count,
        profile_b.contraindicated_count
    );
    println!(
        "  {:<25} {:<20} {:<20}",
        "Risk Level", profile_a.risk_level, profile_b.risk_level
    );

    println!();
    match severity::compare_profiles(&profile_a, &profile_b) {
        std::cmp::Ordering::Less => println!("  ✅ Regimen A is safer."),
        std::cmp::Ordering::Greater => println!("  ✅ Regimen B is safer."),
        std::cmp::Ordering::Equal => println!("  ⚖ Both regimens have equivalent safety profiles."),
    }

    Ok(())
}
