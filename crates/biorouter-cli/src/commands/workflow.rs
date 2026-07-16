use anyhow::Result;
use biorouter::workflow::validate_workflow::validate_workflow_template_from_file;
use console::style;
use std::collections::HashMap;

use crate::workflows::github_workflow::WorkflowSource;
use crate::workflows::search_workflow::{list_available_workflows, load_workflow_file};
use biorouter::workflow_deeplink;

pub fn handle_validate(workflow_name: &str) -> Result<()> {
    // Load and validate the workflow file
    let workflow_file = load_workflow_file(workflow_name)?;
    validate_workflow_template_from_file(&workflow_file).map_err(|err| {
        anyhow::anyhow!(
            "{} workflow file is invalid: {}",
            style("✗").red().bold(),
            err
        )
    })?;
    println!("{} workflow file is valid", style("✓").green().bold());
    Ok(())
}

/// Install a workflow file (.json or .yaml) into the global workflow library
/// (`~/.config/biorouter/workflows/`), where it becomes available to `list`,
/// `deeplink`, and `open` — the same library the desktop GUI reads.
pub fn handle_install(workflow_path: &str) -> Result<()> {
    let workflow_file = load_workflow_file(workflow_path)?;
    let workflow = validate_workflow_template_from_file(&workflow_file).map_err(|err| {
        anyhow::anyhow!(
            "{} workflow file is invalid: {}",
            style("✗").red().bold(),
            err
        )
    })?;
    let title = workflow.title.clone();
    let saved = biorouter::workflow::local_workflows::save_workflow_to_file(workflow, None)?;
    println!(
        "{} installed workflow '{}'",
        style("✓").green().bold(),
        style(&title).bold()
    );
    println!(
        "  {} {}",
        style("path:").dim(),
        style(saved.display()).dim()
    );
    Ok(())
}

pub fn handle_deeplink(workflow_name: &str, params: &[String]) -> Result<String> {
    let params_map = parse_params(params)?;
    match generate_deeplink(workflow_name, params_map) {
        Ok((deeplink_url, workflow)) => {
            println!(
                "{} Generated deeplink for: {}",
                style("✓").green().bold(),
                workflow.title
            );
            println!("{}", deeplink_url);
            Ok(deeplink_url)
        }
        Err(err) => {
            println!(
                "{} Failed to encode workflow: {}",
                style("✗").red().bold(),
                err
            );
            Err(err)
        }
    }
}

pub fn handle_open(workflow_name: &str, params: &[String]) -> Result<()> {
    handle_open_with(
        workflow_name,
        params,
        |url| open::that(url),
        &mut std::io::stdout(),
    )
}

fn handle_open_with<F, W>(
    workflow_name: &str,
    params: &[String],
    opener: F,
    out: &mut W,
) -> Result<()>
where
    F: FnOnce(&str) -> std::io::Result<()>,
    W: std::io::Write,
{
    let params_map = parse_params(params)?;
    match generate_deeplink(workflow_name, params_map) {
        Ok((deeplink_url, workflow)) => match opener(&deeplink_url) {
            Ok(_) => {
                writeln!(
                    out,
                    "{} Opened workflow '{}' in Biorouter Desktop",
                    style("✓").green().bold(),
                    workflow.title
                )?;
                Ok(())
            }
            Err(err) => {
                writeln!(
                    out,
                    "{} Failed to open workflow in Biorouter Desktop: {}",
                    style("✗").red().bold(),
                    err
                )?;
                writeln!(out, "Generated deeplink: {}", deeplink_url)?;
                writeln!(out, "You can manually copy and open the URL above, or ensure Biorouter Desktop is installed.")?;
                Err(anyhow::anyhow!("Failed to open workflow: {}", err))
            }
        },
        Err(err) => {
            writeln!(
                out,
                "{} Failed to encode workflow: {}",
                style("✗").red().bold(),
                err
            )?;
            Err(err)
        }
    }
}

pub fn handle_list(format: &str, verbose: bool) -> Result<()> {
    let workflows = match list_available_workflows() {
        Ok(workflows) => workflows,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to list workflows: {}", e));
        }
    };

    match format {
        "json" => {
            println!("{}", serde_json::to_string(&workflows)?);
        }
        _ => {
            if workflows.is_empty() {
                println!("No workflows found");
                return Ok(());
            } else {
                println!("Available workflows:");
                for workflow in workflows {
                    let source_info = match workflow.source {
                        WorkflowSource::Local => format!("local: {}", workflow.path),
                        WorkflowSource::GitHub => format!("github: {}", workflow.path),
                    };

                    let description = if let Some(desc) = &workflow.description {
                        if desc.is_empty() {
                            "(none)"
                        } else {
                            desc
                        }
                    } else {
                        "(none)"
                    };

                    let output = format!("{} - {} - {}", workflow.name, description, source_info);
                    if verbose {
                        println!("  {}", output);
                        if let Some(title) = &workflow.title {
                            println!("    Title: {}", title);
                        }
                        println!("    Path: {}", workflow.path);
                    } else {
                        println!("{}", output);
                    }
                }
            }
        }
    }
    Ok(())
}

fn parse_params(params: &[String]) -> Result<HashMap<String, String>> {
    let mut params_map = HashMap::new();
    for param in params {
        let parts: Vec<&str> = param.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!(
                "Invalid parameter format: '{}'. Expected format: key=value",
                param
            ));
        }
        params_map.insert(parts[0].to_string(), parts[1].to_string());
    }
    Ok(params_map)
}

fn generate_deeplink(
    workflow_name: &str,
    params: HashMap<String, String>,
) -> Result<(String, biorouter::workflow::Workflow)> {
    let workflow_file = load_workflow_file(workflow_name)?;
    // Load the workflow file first to validate it
    let workflow = validate_workflow_template_from_file(&workflow_file)?;
    match workflow_deeplink::encode(&workflow) {
        Ok(encoded) => {
            let mut full_url = format!("biorouter://workflow?config={}", encoded);

            // Append parameters as additional query parameters
            for (key, value) in params {
                // URL-encode the parameter keys and values
                let encoded_key = urlencoding::encode(&key);
                let encoded_value = urlencoding::encode(&value);
                full_url.push_str(&format!("&{}={}", encoded_key, encoded_value));
            }

            Ok((full_url, workflow))
        }
        Err(err) => Err(anyhow::anyhow!("Failed to encode workflow: {}", err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_workflow_file(dir: &TempDir, filename: &str, content: &str) -> String {
        let file_path = dir.path().join(filename);
        fs::write(&file_path, content).expect("Failed to write test workflow file");
        file_path.to_string_lossy().into_owned()
    }

    const VALID_WORKFLOW_CONTENT: &str = r#"
title: "Test Workflow with Valid JSON Schema"
description: "A test workflow with valid JSON schema"
prompt: "Test prompt content"
instructions: "Test instructions"
response:
  json_schema:
    type: object
    properties:
      result:
        type: string
        description: "The result"
      count:
        type: number
        description: "A count value"
    required:
      - result
"#;

    const INVALID_WORKFLOW_CONTENT: &str = r#"
title: "Test Workflow"
description: "A test workflow for deeplink generation"
prompt: "Test prompt content {{ name }}"
instructions: "Test instructions"
"#;

    #[test]
    fn test_handle_deeplink_valid_workflow() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workflow_path =
            create_test_workflow_file(&temp_dir, "test_workflow.yaml", VALID_WORKFLOW_CONTENT);

        let result = handle_deeplink(&workflow_path, &[]);
        assert!(result.is_ok());
        let url = result.unwrap();
        assert!(url.starts_with("biorouter://workflow?config="));
        let encoded_part = url.strip_prefix("biorouter://workflow?config=").unwrap();
        assert!(!encoded_part.is_empty());
    }

    #[test]
    fn test_handle_deeplink_with_parameters() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workflow_path =
            create_test_workflow_file(&temp_dir, "test_workflow.yaml", VALID_WORKFLOW_CONTENT);

        let params = vec!["name=John".to_string(), "age=30".to_string()];
        let result = handle_deeplink(&workflow_path, &params);
        assert!(result.is_ok());
        let url = result.unwrap();
        assert!(url.starts_with("biorouter://workflow?config="));
        assert!(url.contains("&name=John"));
        assert!(url.contains("&age=30"));
    }

    #[test]
    fn test_handle_deeplink_invalid_workflow() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workflow_path =
            create_test_workflow_file(&temp_dir, "test_workflow.yaml", INVALID_WORKFLOW_CONTENT);
        let result = handle_deeplink(&workflow_path, &[]);
        assert!(result.is_err());
    }

    fn run_handle_open(
        workflow_path: &str,
        params: &[String],
        opener_result: std::io::Result<()>,
    ) -> (Result<()>, String, String) {
        let captured_url = std::cell::RefCell::new(String::new());
        let mut out = Vec::new();
        let result = handle_open_with(
            workflow_path,
            params,
            |url| {
                *captured_url.borrow_mut() = url.to_string();
                opener_result
            },
            &mut out,
        );
        let output = String::from_utf8(out).unwrap();
        (result, captured_url.into_inner(), output)
    }

    #[test]
    fn test_handle_open_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let workflow_path =
            create_test_workflow_file(&temp_dir, "test_workflow.yaml", VALID_WORKFLOW_CONTENT);

        let (expected_url, _) = generate_deeplink(&workflow_path, HashMap::new()).unwrap();
        let (result, captured_url, _) = run_handle_open(&workflow_path, &[], Ok(()));

        assert!(result.is_ok());
        assert_eq!(captured_url, expected_url);
    }

    #[test]
    fn test_handle_open_with_parameters() {
        let temp_dir = TempDir::new().unwrap();
        let workflow_path =
            create_test_workflow_file(&temp_dir, "test_workflow.yaml", VALID_WORKFLOW_CONTENT);

        let (base_url, _) = generate_deeplink(&workflow_path, HashMap::new()).unwrap();

        let params = vec!["name=Alice".to_string(), "role=developer".to_string()];
        let (result, captured_url, _) = run_handle_open(&workflow_path, &params, Ok(()));

        assert!(result.is_ok());
        assert!(captured_url.starts_with(&base_url));
        assert!(captured_url.contains("&name=Alice"));
        assert!(captured_url.contains("&role=developer"));
    }

    #[test]
    fn test_handle_open_opener_fails() {
        let temp_dir = TempDir::new().unwrap();
        let workflow_path =
            create_test_workflow_file(&temp_dir, "test_workflow.yaml", VALID_WORKFLOW_CONTENT);

        let (expected_url, _) = generate_deeplink(&workflow_path, HashMap::new()).unwrap();
        let opener_err = std::io::Error::new(std::io::ErrorKind::NotFound, "desktop not found");
        let (result, _, output) = run_handle_open(&workflow_path, &[], Err(opener_err));

        assert!(result.is_err());
        assert!(output.contains("Failed to open workflow in Biorouter Desktop"));
        assert!(output.contains("desktop not found"));
        assert!(output.contains(&expected_url));
    }

    #[test]
    fn test_handle_open_invalid_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let workflow_path =
            create_test_workflow_file(&temp_dir, "invalid.yaml", INVALID_WORKFLOW_CONTENT);

        let (result, _, output) = run_handle_open(&workflow_path, &[], Ok(()));

        assert!(result.is_err());
        assert!(output.contains("Failed to encode workflow"));
    }

    #[test]
    fn test_handle_validation_valid_workflow() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workflow_path =
            create_test_workflow_file(&temp_dir, "test_workflow.yaml", VALID_WORKFLOW_CONTENT);

        let result = handle_validate(&workflow_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_validation_invalid_workflow() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workflow_path =
            create_test_workflow_file(&temp_dir, "test_workflow.yaml", INVALID_WORKFLOW_CONTENT);
        let result = handle_validate(&workflow_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_deeplink_valid_workflow() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workflow_path =
            create_test_workflow_file(&temp_dir, "test_workflow.yaml", VALID_WORKFLOW_CONTENT);

        let result = generate_deeplink(&workflow_path, HashMap::new());
        assert!(result.is_ok());
        let (url, workflow) = result.unwrap();
        assert!(url.starts_with("biorouter://workflow?config="));
        assert_eq!(workflow.title, "Test Workflow with Valid JSON Schema");
        assert_eq!(
            workflow.description,
            "A test workflow with valid JSON schema"
        );
        let encoded_part = url.strip_prefix("biorouter://workflow?config=").unwrap();
        assert!(!encoded_part.is_empty());
    }

    #[test]
    fn test_generate_deeplink_with_parameters() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workflow_path =
            create_test_workflow_file(&temp_dir, "test_workflow.yaml", VALID_WORKFLOW_CONTENT);

        let mut params = HashMap::new();
        params.insert("name".to_string(), "Alice".to_string());
        params.insert("role".to_string(), "developer".to_string());

        let result = generate_deeplink(&workflow_path, params);
        assert!(result.is_ok());
        let (url, workflow) = result.unwrap();
        assert!(url.starts_with("biorouter://workflow?config="));
        assert!(url.contains("&name=Alice"));
        assert!(url.contains("&role=developer"));
        assert_eq!(workflow.title, "Test Workflow with Valid JSON Schema");
    }

    #[test]
    fn test_generate_deeplink_invalid_workflow() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let workflow_path =
            create_test_workflow_file(&temp_dir, "test_workflow.yaml", INVALID_WORKFLOW_CONTENT);

        let result = generate_deeplink(&workflow_path, HashMap::new());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_params_basic() {
        let params = vec!["name=John".to_string(), "age=30".to_string()];
        let result = parse_params(&params);
        assert!(result.is_ok());
        let map = result.unwrap();
        assert_eq!(map.get("name"), Some(&"John".to_string()));
        assert_eq!(map.get("age"), Some(&"30".to_string()));
    }

    #[test]
    fn test_parse_params_with_equals_in_value() {
        let params = vec!["key=value=with=equals".to_string()];
        let result = parse_params(&params);
        assert!(result.is_ok());
        let map = result.unwrap();
        assert_eq!(map.get("key"), Some(&"value=with=equals".to_string()));
    }

    #[test]
    fn test_parse_params_empty_value() {
        let params = vec!["key=".to_string()];
        let result = parse_params(&params);
        assert!(result.is_ok());
        let map = result.unwrap();
        assert_eq!(map.get("key"), Some(&"".to_string()));
    }

    #[test]
    fn test_parse_params_no_equals() {
        let params = vec!["invalid".to_string()];
        let result = parse_params(&params);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Invalid parameter format"));
    }

    #[test]
    fn test_parse_params_empty_key() {
        let params = vec!["=value".to_string()];
        let result = parse_params(&params);
        assert!(result.is_ok());
        let map = result.unwrap();
        // Empty key is technically valid according to current implementation
        assert_eq!(map.get(""), Some(&"value".to_string()));
    }

    #[test]
    fn test_parse_params_special_characters() {
        let params = vec![
            "url=https://example.com/path?query=test".to_string(),
            "message=Hello World!".to_string(),
            "email=user@example.com".to_string(),
        ];
        let result = parse_params(&params);
        assert!(result.is_ok());
        let map = result.unwrap();
        assert_eq!(
            map.get("url"),
            Some(&"https://example.com/path?query=test".to_string())
        );
        assert_eq!(map.get("message"), Some(&"Hello World!".to_string()));
        assert_eq!(map.get("email"), Some(&"user@example.com".to_string()));
    }

    #[test]
    fn test_parse_params_empty_list() {
        let params: Vec<String> = vec![];
        let result = parse_params(&params);
        assert!(result.is_ok());
        let map = result.unwrap();
        assert!(map.is_empty());
    }
}
