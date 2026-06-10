use anyhow::{bail, Result};
use biorouter::config::Config;
use biorouter::providers::base::{ProviderMetadata, ProviderType};
use biorouter::providers::providers;
use console::style;
use serde::Serialize;

#[derive(Serialize)]
struct ProviderSummary {
    name: String,
    display_name: String,
    provider_type: ProviderType,
    default_model: String,
    models: Vec<String>,
    allows_unlisted_models: bool,
}

fn provider_summary(metadata: ProviderMetadata, provider_type: ProviderType) -> ProviderSummary {
    ProviderSummary {
        name: metadata.name,
        display_name: metadata.display_name,
        provider_type,
        default_model: metadata.default_model,
        models: metadata
            .known_models
            .into_iter()
            .map(|model| model.name)
            .collect(),
        allows_unlisted_models: metadata.allows_unlisted_models,
    }
}

async fn provider_summaries() -> Vec<ProviderSummary> {
    providers()
        .await
        .into_iter()
        .map(|(metadata, provider_type)| provider_summary(metadata, provider_type))
        .collect()
}

pub async fn handle_models_current(format: &str) -> Result<()> {
    let config = Config::global();
    let provider = config.get_biorouter_provider().ok();
    let model = config.get_biorouter_model().ok();

    if format == "json" {
        println!(
            "{}",
            serde_json::json!({
                "provider": provider,
                "model": model,
            })
        );
        return Ok(());
    }

    match (provider, model) {
        (Some(provider), Some(model)) => {
            println!("Current model configuration");
            println!("  provider: {}", style(provider).cyan());
            println!("  model:    {}", style(model).cyan());
        }
        _ => {
            println!("No provider/model is configured.");
            println!(
                "Run `biorouter configure` or `biorouter models set --provider <name> --model <model>`."
            );
        }
    }

    Ok(())
}

pub async fn handle_models_providers(format: &str) -> Result<()> {
    let mut summaries = provider_summaries().await;
    summaries.sort_by(|a, b| a.name.cmp(&b.name));

    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&summaries)?);
        return Ok(());
    }

    if summaries.is_empty() {
        println!("No providers found.");
        return Ok(());
    }

    println!("Available providers");
    for provider in summaries {
        println!(
            "  {:<18} {:<12} default: {}",
            provider.name,
            format!("{:?}", provider.provider_type).to_lowercase(),
            provider.default_model
        );
    }

    Ok(())
}

pub async fn handle_models_list(provider_name: String, format: &str) -> Result<()> {
    let summaries = provider_summaries().await;
    let provider = summaries
        .into_iter()
        .find(|provider| provider.name == provider_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", provider_name))?;

    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&provider)?);
        return Ok(());
    }

    println!(
        "{} ({})",
        style(&provider.display_name).cyan().bold(),
        provider.name
    );
    println!("  type:    {:?}", provider.provider_type);
    println!("  default: {}", provider.default_model);
    if provider.models.is_empty() {
        println!("  models:  no static model list available");
        if provider.allows_unlisted_models {
            println!("           custom model names are allowed");
        }
        return Ok(());
    }

    println!("  models:");
    for model in provider.models {
        println!("    {}", model);
    }
    if provider.allows_unlisted_models {
        println!("    (custom model names are also allowed)");
    }

    Ok(())
}

pub async fn handle_models_set(provider_name: String, model: String) -> Result<()> {
    let summaries = provider_summaries().await;
    let provider = summaries
        .iter()
        .find(|provider| provider.name == provider_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", provider_name))?;

    let known_model = provider.models.iter().any(|known| known == &model);
    if !known_model && !provider.allows_unlisted_models {
        bail!(
            "Model '{}' is not listed for provider '{}'. Run `biorouter models list {}` to see known models.",
            model,
            provider_name,
            provider_name
        );
    }

    let config = Config::global();
    config.set_biorouter_provider(provider_name.clone())?;
    config.set_biorouter_model(&model)?;

    println!("Model configuration updated");
    println!("  provider: {}", style(provider_name).cyan());
    println!("  model:    {}", style(model).cyan());

    Ok(())
}
