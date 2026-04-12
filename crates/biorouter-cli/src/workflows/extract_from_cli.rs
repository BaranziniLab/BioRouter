use std::path::PathBuf;

use anyhow::{anyhow, Result};
use biorouter::workflow::{Workflow, SubWorkflow};

use crate::cli::InputConfig;
use crate::workflows::print_workflow::print_workflow_info;
use crate::workflows::workflow::load_workflow;
use crate::workflows::search_workflow::load_workflow_file;

pub fn extract_workflow_info_from_cli(
    workflow_name: String,
    params: Vec<(String, String)>,
    additional_sub_workflows: Vec<String>,
    quiet: bool,
) -> Result<(InputConfig, Workflow)> {
    let mut workflow = load_workflow(&workflow_name, params.clone()).unwrap_or_else(|err| {
        eprintln!("{}: {}", console::style("Error").red().bold(), err);
        std::process::exit(1);
    });
    if !quiet {
        print_workflow_info(&workflow, params);
    }

    if !additional_sub_workflows.is_empty() {
        let mut all_sub_workflows = workflow.sub_workflows.clone().unwrap_or_default();
        for sub_workflow_name in additional_sub_workflows {
            match load_workflow_file(&sub_workflow_name) {
                Ok(workflow_file) => {
                    let name = extract_workflow_name(&sub_workflow_name);
                    let workflow_file_path = workflow_file.file_path;
                    let additional_sub_workflow = SubWorkflow {
                        path: workflow_file_path.to_string_lossy().to_string(),
                        name,
                        values: None,
                        sequential_when_repeated: true,
                        description: None,
                    };
                    all_sub_workflows.push(additional_sub_workflow);
                }
                Err(e) => {
                    return Err(anyhow!(
                        "Could not retrieve sub-workflow '{}': {}",
                        sub_workflow_name,
                        e
                    ));
                }
            }
        }
        workflow.sub_workflows = Some(all_sub_workflows);
    }

    let input_config = InputConfig {
        contents: workflow.prompt.clone().filter(|s| !s.trim().is_empty()),
        additional_system_prompt: workflow.instructions.clone(),
    };

    Ok((input_config, workflow))
}

fn extract_workflow_name(workflow_identifier: &str) -> String {
    // If it's a path (contains / or \), extract the file stem
    if workflow_identifier.contains('/') || workflow_identifier.contains('\\') {
        PathBuf::from(workflow_identifier)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    } else {
        // If it's just a name (like "weekly-updates"), use it directly
        workflow_identifier.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_extract_workflow_info_from_cli_basic() {
        let (_temp_dir, workflow_path) = create_workflow();
        let params = vec![("name".to_string(), "my_value".to_string())];
        let workflow_name = workflow_path.to_str().unwrap().to_string();

        let (input_config, workflow) =
            extract_workflow_info_from_cli(workflow_name, params, Vec::new(), false).unwrap();
        let settings = workflow.settings;
        let sub_workflows = workflow.sub_workflows;
        let response = workflow.response;

        assert_eq!(input_config.contents, Some("test_prompt".to_string()));
        assert_eq!(
            input_config.additional_system_prompt,
            Some("test_instructions my_value".to_string())
        );
        assert!(workflow.extensions.is_none());

        assert!(settings.is_some());
        let settings = settings.unwrap();
        assert_eq!(settings.biorouter_provider, Some("test_provider".to_string()));
        assert_eq!(settings.biorouter_model, Some("test_model".to_string()));
        assert_eq!(settings.temperature, Some(0.7));

        assert!(sub_workflows.is_some());
        let sub_workflows = sub_workflows.unwrap();
        assert!(sub_workflows.len() == 1);
        let full_sub_workflow_path = workflow_path
            .parent()
            .unwrap()
            .join("existing_sub_workflow.yaml")
            .to_string_lossy()
            .to_string();
        assert_eq!(sub_workflows[0].path, full_sub_workflow_path);
        assert_eq!(sub_workflows[0].name, "existing_sub_workflow".to_string());
        assert!(sub_workflows[0].values.is_none());
        assert!(response.is_some());
        let response = response.unwrap();
        assert_eq!(
            response.json_schema,
            Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "result": {"type": "string"}
                }
            }))
        );
    }

    #[test]
    fn test_extract_workflow_info_from_cli_with_additional_sub_workflows() {
        let (temp_dir, workflow_path) = create_workflow();

        std::fs::create_dir_all(temp_dir.path().join("path/to")).unwrap();
        std::fs::create_dir_all(temp_dir.path().join("another")).unwrap();

        let sub_workflow1_path = temp_dir.path().join("path/to/sub_workflow1.yaml");
        let sub_workflow2_path = temp_dir.path().join("another/sub_workflow2.yaml");

        std::fs::write(&sub_workflow1_path, "title: Sub Workflow 1").unwrap();
        std::fs::write(&sub_workflow2_path, "title: Sub Workflow 2").unwrap();

        let params = vec![("name".to_string(), "my_value".to_string())];
        let workflow_name = workflow_path.to_str().unwrap().to_string();
        let additional_sub_workflows = vec![
            sub_workflow1_path.to_string_lossy().to_string(),
            sub_workflow2_path.to_string_lossy().to_string(),
        ];

        let (input_config, workflow) =
            extract_workflow_info_from_cli(workflow_name, params, additional_sub_workflows, false)
                .unwrap();
        let settings = workflow.settings;
        let sub_workflows = workflow.sub_workflows;
        let response = workflow.response;

        assert_eq!(input_config.contents, Some("test_prompt".to_string()));
        assert_eq!(
            input_config.additional_system_prompt,
            Some("test_instructions my_value".to_string())
        );
        assert!(workflow.extensions.is_none());

        assert!(settings.is_some());
        let settings = settings.unwrap();
        assert_eq!(settings.biorouter_provider, Some("test_provider".to_string()));
        assert_eq!(settings.biorouter_model, Some("test_model".to_string()));
        assert_eq!(settings.temperature, Some(0.7));

        assert!(sub_workflows.is_some());
        let sub_workflows = sub_workflows.unwrap();
        assert!(sub_workflows.len() == 3);
        let full_sub_workflow_path = workflow_path
            .parent()
            .unwrap()
            .join("existing_sub_workflow.yaml")
            .to_string_lossy()
            .to_string();
        assert_eq!(sub_workflows[0].path, full_sub_workflow_path);
        assert_eq!(sub_workflows[0].name, "existing_sub_workflow".to_string());
        assert!(sub_workflows[0].values.is_none());
        assert_eq!(
            sub_workflows[1].path,
            sub_workflow1_path
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .to_string()
        );
        assert_eq!(sub_workflows[1].name, "sub_workflow1".to_string());
        assert!(sub_workflows[1].values.is_none());
        assert_eq!(
            sub_workflows[2].path,
            sub_workflow2_path
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .to_string()
        );
        assert_eq!(sub_workflows[2].name, "sub_workflow2".to_string());
        assert!(sub_workflows[2].values.is_none());
        assert!(response.is_some());
        let response = response.unwrap();
        assert_eq!(
            response.json_schema,
            Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "result": {"type": "string"}
                }
            }))
        );
    }

    fn create_workflow() -> (TempDir, PathBuf) {
        let test_workflow_content = r#"
title: test_workflow
description: A test workflow
instructions: test_instructions {{name}}
prompt: test_prompt
parameters:
- key: name
  description: name
  input_type: string
  requirement: required
settings:
  biorouter_provider: test_provider
  biorouter_model: test_model
  temperature: 0.7
sub_workflows:
- path: existing_sub_workflow.yaml
  name: existing_sub_workflow
response:
  json_schema:
    type: object
    properties:
      result:
        type: string
"#;
        let sub_workflow_content = r#"
title: existing_sub_workflow
description: An existing sub workflow
instructions: sub workflow instructions
prompt: sub workflow prompt
"#;
        let temp_dir = tempfile::tempdir().unwrap();
        let workflow_path: std::path::PathBuf = temp_dir.path().join("test_workflow.yaml");
        let sub_workflow_path: std::path::PathBuf = temp_dir.path().join("existing_sub_workflow.yaml");

        std::fs::write(&workflow_path, test_workflow_content).unwrap();
        std::fs::write(&sub_workflow_path, sub_workflow_content).unwrap();
        let canonical_workflow_path = workflow_path.canonicalize().unwrap();
        (temp_dir, canonical_workflow_path)
    }
}
