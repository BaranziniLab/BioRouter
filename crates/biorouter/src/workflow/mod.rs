use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use crate::agents::extension::ExtensionConfig;
use crate::agents::types::RetryConfig;
use crate::utils::contains_unicode_tags;
use crate::workflow::read_workflow_file_content::read_workflow_file;
use crate::workflow::yaml_format_utils::reformat_fields_with_multiline_values;
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub mod build_workflow;
pub mod local_workflows;
pub mod read_workflow_file_content;
pub mod template_workflow;
pub mod validate_workflow;
mod workflow_extension_adapter;
pub mod yaml_format_utils;

pub const BUILT_IN_WORKFLOW_DIR_PARAM: &str = "workflow_dir";
pub const WORKFLOW_FILE_EXTENSIONS: &[&str] = &["yaml", "json"];

fn default_version() -> String {
    "1.0.0".to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Workflow {
    // Required fields
    #[serde(default = "default_version")]
    pub version: String, // version of the file format, sem ver

    pub title: String, // short title of the workflow

    pub description: String, // a longer description of the workflow

    // Optional fields
    // Note: at least one of instructions or prompt need to be set
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>, // the instructions for the model

    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>, // the prompt to start the session with

    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "workflow_extension_adapter::deserialize_workflow_extensions"
    )]
    pub extensions: Option<Vec<ExtensionConfig>>, // a list of extensions to enable

    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge_bases: Option<WorkflowKnowledgeBases>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<Settings>, // settings for the workflow

    #[serde(skip_serializing_if = "Option::is_none")]
    pub activities: Option<Vec<String>>, // the activity pills that show up when loading the

    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<Author>, // any additional author information

    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<WorkflowParameter>>, // any additional parameters for the workflow

    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<Response>, // response configuration including JSON schema

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_workflows: Option<Vec<SubWorkflow>>, // sub-workflows for the workflow

    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Author {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>, // creator/contact information of the workflow

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>, // any additional metadata for the author
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub biorouter_provider: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub biorouter_model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema, PartialEq, Eq)]
pub struct WorkflowKnowledgeBases {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub visible: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Response {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct SubWorkflow {
    pub name: String,
    pub path: String,
    #[serde(default, deserialize_with = "deserialize_value_map_as_string")]
    pub values: Option<HashMap<String, String>>,
    #[serde(default)]
    pub sequential_when_repeated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn deserialize_value_map_as_string<'de, D>(
    deserializer: D,
) -> Result<Option<HashMap<String, String>>, D::Error>
where
    D: Deserializer<'de>,
{
    // First, try to deserialize a map of values
    let opt_raw: Option<HashMap<String, Value>> = Option::deserialize(deserializer)?;

    match opt_raw {
        Some(raw_map) => {
            let mut result = HashMap::new();
            for (k, v) in raw_map {
                let s = match v {
                    Value::String(s) => s,
                    _ => serde_json::to_string(&v).map_err(serde::de::Error::custom)?,
                };
                result.insert(k, s);
            }
            Ok(Some(result))
        }
        None => Ok(None),
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowParameterRequirement {
    Required,
    Optional,
    UserPrompt,
}

impl fmt::Display for WorkflowParameterRequirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_string(self).unwrap().trim_matches('"')
        )
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowParameterInputType {
    String,
    Number,
    Boolean,
    Date,
    /// File parameter that imports content from a file path.
    /// Cannot have default values to prevent importing sensitive user files.
    File,
    Select,
}

impl fmt::Display for WorkflowParameterInputType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_string(self).unwrap().trim_matches('"')
        )
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct WorkflowParameter {
    pub key: String,
    pub input_type: WorkflowParameterInputType,
    pub requirement: WorkflowParameterRequirement,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
}

/// Builder for creating Workflow instances
pub struct WorkflowBuilder {
    // Required fields with default values
    version: String,
    title: Option<String>,
    description: Option<String>,
    instructions: Option<String>,

    // Optional fields
    prompt: Option<String>,
    extensions: Option<Vec<ExtensionConfig>>,
    knowledge_bases: Option<WorkflowKnowledgeBases>,
    skills: Option<Vec<String>>,
    settings: Option<Settings>,
    activities: Option<Vec<String>>,
    author: Option<Author>,
    parameters: Option<Vec<WorkflowParameter>>,
    response: Option<Response>,
    sub_workflows: Option<Vec<SubWorkflow>>,
    retry: Option<RetryConfig>,
}

impl Workflow {
    /// Returns true if harmful content is detected in instructions, prompt, or activities fields
    pub fn check_for_security_warnings(&self) -> bool {
        if [self.instructions.as_deref(), self.prompt.as_deref()]
            .iter()
            .flatten()
            .any(|&field| contains_unicode_tags(field))
        {
            return true;
        }

        if let Some(activities) = &self.activities {
            return activities
                .iter()
                .any(|activity| contains_unicode_tags(activity));
        }

        false
    }

    pub fn to_yaml(&self) -> Result<String> {
        let workflow_yaml = serde_yaml::to_string(self)
            .map_err(|err| anyhow::anyhow!("Failed to serialize workflow: {}", err))?;
        let formatted_workflow_yaml =
            reformat_fields_with_multiline_values(&workflow_yaml, &["prompt", "instructions"]);
        Ok(formatted_workflow_yaml)
    }

    pub fn builder() -> WorkflowBuilder {
        WorkflowBuilder {
            version: default_version(),
            title: None,
            description: None,
            instructions: None,
            prompt: None,
            extensions: None,
            knowledge_bases: None,
            skills: None,
            settings: None,
            activities: None,
            author: None,
            parameters: None,
            response: None,
            sub_workflows: None,
            retry: None,
        }
    }

    pub fn from_file_path(file_path: &Path) -> Result<Self> {
        let file = read_workflow_file(file_path)?;
        Self::from_content(&file.content)
    }

    pub fn from_content(content: &str) -> Result<Self> {
        let workflow: Workflow = match serde_yaml::from_str::<serde_yaml::Value>(content) {
            Ok(yaml_value) => {
                if let Some(nested_workflow) = yaml_value.get("workflow") {
                    serde_yaml::from_value(nested_workflow.clone())
                        .map_err(|e| anyhow::anyhow!("Failed to parse nested workflow: {}", e))?
                } else {
                    serde_yaml::from_str(content)
                        .map_err(|e| anyhow::anyhow!("Failed to parse workflow: {}", e))?
                }
            }
            Err(_) => serde_yaml::from_str(content)
                .map_err(|e| anyhow::anyhow!("Failed to parse workflow: {}", e))?,
        };

        if let Some(ref retry_config) = workflow.retry {
            if let Err(validation_error) = retry_config.validate() {
                return Err(anyhow::anyhow!(
                    "Invalid retry configuration: {}",
                    validation_error
                ));
            }
        }

        Ok(workflow)
    }
}

impl WorkflowBuilder {
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }

    pub fn extensions(mut self, extensions: Vec<ExtensionConfig>) -> Self {
        self.extensions = Some(extensions);
        self
    }

    pub fn knowledge_bases(mut self, knowledge_bases: WorkflowKnowledgeBases) -> Self {
        self.knowledge_bases = Some(knowledge_bases);
        self
    }

    pub fn skills(mut self, skills: Vec<String>) -> Self {
        self.skills = Some(skills);
        self
    }

    pub fn settings(mut self, settings: Settings) -> Self {
        self.settings = Some(settings);
        self
    }

    pub fn activities(mut self, activities: Vec<String>) -> Self {
        self.activities = Some(activities);
        self
    }

    pub fn author(mut self, author: Author) -> Self {
        self.author = Some(author);
        self
    }

    pub fn parameters(mut self, parameters: Vec<WorkflowParameter>) -> Self {
        self.parameters = Some(parameters);
        self
    }

    pub fn response(mut self, response: Response) -> Self {
        self.response = Some(response);
        self
    }

    pub fn sub_workflows(mut self, sub_workflows: Vec<SubWorkflow>) -> Self {
        self.sub_workflows = Some(sub_workflows);
        self
    }

    pub fn retry(mut self, retry: RetryConfig) -> Self {
        self.retry = Some(retry);
        self
    }

    pub fn build(self) -> Result<Workflow, &'static str> {
        let title = self.title.ok_or("Title is required")?;
        let description = self.description.ok_or("Description is required")?;

        if self.instructions.is_none() && self.prompt.is_none() {
            return Err("At least one of 'prompt' or 'instructions' is required");
        }

        Ok(Workflow {
            version: self.version,
            title,
            description,
            instructions: self.instructions,
            prompt: self.prompt,
            extensions: self.extensions,
            knowledge_bases: self.knowledge_bases,
            skills: self.skills,
            settings: self.settings,
            activities: self.activities,
            author: self.author,
            parameters: self.parameters,
            response: self.response,
            sub_workflows: self.sub_workflows,
            retry: self.retry,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_content_with_json() {
        let content = r#"{
            "version": "1.0.0",
            "title": "Test Workflow",
            "description": "A test workflow",
            "prompt": "Test prompt",
            "instructions": "Test instructions",
            "extensions": [
                {
                    "type": "stdio",
                    "name": "test_extension",
                    "cmd": "test_cmd",
                    "args": ["arg1", "arg2"],
                    "timeout": 300,
                    "description": "Test extension"
                }
            ],
            "parameters": [
                {
                    "key": "test_param",
                    "input_type": "string",
                    "requirement": "required",
                    "description": "A test parameter"
                }
            ],
            "response": {
                "json_schema": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string"
                        },
                        "age": {
                            "type": "number"
                        }
                    },
                    "required": ["name"]
                }
            },
            "sub_workflows": [
                {
                    "name": "test_sub_workflow",
                    "path": "test_sub_workflow.yaml",
                    "values": {
                        "sub_workflow_param": "sub_workflow_value"
                    }
                }
            ]
        }"#;

        let workflow = Workflow::from_content(content).unwrap();
        assert_eq!(workflow.version, "1.0.0");
        assert_eq!(workflow.title, "Test Workflow");
        assert_eq!(workflow.description, "A test workflow");
        assert_eq!(workflow.instructions, Some("Test instructions".to_string()));
        assert_eq!(workflow.prompt, Some("Test prompt".to_string()));

        assert!(workflow.extensions.is_some());
        let extensions = workflow.extensions.unwrap();
        assert_eq!(extensions.len(), 1);

        assert!(workflow.parameters.is_some());
        let parameters = workflow.parameters.unwrap();
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].key, "test_param");
        assert!(matches!(
            parameters[0].input_type,
            WorkflowParameterInputType::String
        ));
        assert!(matches!(
            parameters[0].requirement,
            WorkflowParameterRequirement::Required
        ));

        assert!(workflow.response.is_some());
        let response = workflow.response.unwrap();
        assert!(response.json_schema.is_some());
        let json_schema = response.json_schema.unwrap();
        assert_eq!(json_schema["type"], "object");
        assert!(json_schema["properties"].is_object());
        assert_eq!(json_schema["properties"]["name"]["type"], "string");
        assert_eq!(json_schema["properties"]["age"]["type"], "number");
        assert_eq!(json_schema["required"], serde_json::json!(["name"]));

        assert!(workflow.sub_workflows.is_some());
        let sub_workflows = workflow.sub_workflows.unwrap();
        assert_eq!(sub_workflows.len(), 1);
        assert_eq!(sub_workflows[0].name, "test_sub_workflow");
        assert_eq!(sub_workflows[0].path, "test_sub_workflow.yaml");
        assert_eq!(
            sub_workflows[0].values,
            Some(HashMap::from([(
                "sub_workflow_param".to_string(),
                "sub_workflow_value".to_string()
            )]))
        );
    }

    #[test]
    fn test_from_content_with_yaml() {
        let content = r#"version: 1.0.0
title: Test Workflow
description: A test workflow
prompt: Test prompt
instructions: Test instructions
extensions:
  - type: stdio
    name: test_extension
    cmd: test_cmd
    args: [arg1, arg2]
    timeout: 300
    description: Test extension
parameters:
  - key: test_param
    input_type: string
    requirement: required
    description: A test parameter
response:
  json_schema:
    type: object
    properties:
      name:
        type: string
      age:
        type: number
    required:
      - name
sub_workflows:
  - name: test_sub_workflow
    path: test_sub_workflow.yaml
    values:
      sub_workflow_param: sub_workflow_value"#;

        let workflow = Workflow::from_content(content).unwrap();
        assert_eq!(workflow.version, "1.0.0");
        assert_eq!(workflow.title, "Test Workflow");
        assert_eq!(workflow.description, "A test workflow");
        assert_eq!(workflow.instructions, Some("Test instructions".to_string()));
        assert_eq!(workflow.prompt, Some("Test prompt".to_string()));

        assert!(workflow.extensions.is_some());
        let extensions = workflow.extensions.unwrap();
        assert_eq!(extensions.len(), 1);

        assert!(workflow.parameters.is_some());
        let parameters = workflow.parameters.unwrap();
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].key, "test_param");
        assert!(matches!(
            parameters[0].input_type,
            WorkflowParameterInputType::String
        ));
        assert!(matches!(
            parameters[0].requirement,
            WorkflowParameterRequirement::Required
        ));

        assert!(workflow.response.is_some());
        let response = workflow.response.unwrap();
        assert!(response.json_schema.is_some());
        let json_schema = response.json_schema.unwrap();
        assert_eq!(json_schema["type"], "object");
        assert!(json_schema["properties"].is_object());
        assert_eq!(json_schema["properties"]["name"]["type"], "string");
        assert_eq!(json_schema["properties"]["age"]["type"], "number");
        assert_eq!(json_schema["required"], serde_json::json!(["name"]));

        assert!(workflow.sub_workflows.is_some());
        let sub_workflows = workflow.sub_workflows.unwrap();
        assert_eq!(sub_workflows.len(), 1);
        assert_eq!(sub_workflows[0].name, "test_sub_workflow");
        assert_eq!(sub_workflows[0].path, "test_sub_workflow.yaml");
        assert_eq!(
            sub_workflows[0].values,
            Some(HashMap::from([(
                "sub_workflow_param".to_string(),
                "sub_workflow_value".to_string()
            )]))
        );
    }

    #[test]
    fn test_from_content_invalid_json() {
        let content = "{ invalid json }";

        let result = Workflow::from_content(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_content_missing_required_fields() {
        let content = r#"{
            "version": "1.0.0",
            "description": "A test workflow"
        }"#;

        let result = Workflow::from_content(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_content_with_author() {
        let content = r#"{
            "version": "1.0.0",
            "title": "Test Workflow",
            "description": "A test workflow",
            "instructions": "Test instructions",
            "author": {
                "contact": "test@example.com"
            }
        }"#;

        let workflow = Workflow::from_content(content).unwrap();

        assert!(workflow.author.is_some());
        let author = workflow.author.unwrap();
        assert_eq!(author.contact, Some("test@example.com".to_string()));
    }

    #[test]
    fn test_inline_python_extension() {
        let content = r#"{
            "version": "1.0.0",
            "title": "Test Workflow",
            "description": "A test workflow",
            "instructions": "Test instructions",
            "extensions": [
                {
                    "type": "inline_python",
                    "name": "test_python",
                    "code": "print('hello world')",
                    "timeout": 300,
                    "description": "Test python extension",
                    "dependencies": ["numpy", "matplotlib"]
                }
            ]
        }"#;

        let workflow = Workflow::from_content(content).unwrap();

        assert!(workflow.extensions.is_some());
        let extensions = workflow.extensions.unwrap();
        assert_eq!(extensions.len(), 1);

        match &extensions[0] {
            ExtensionConfig::InlinePython {
                name,
                code,
                description,
                timeout,
                dependencies,
                ..
            } => {
                assert_eq!(name, "test_python");
                assert_eq!(code, "print('hello world')");
                assert_eq!(description, "Test python extension");
                assert_eq!(timeout, &Some(300));
                assert!(dependencies.is_some());
                let deps = dependencies.as_ref().unwrap();
                assert!(deps.contains(&"numpy".to_string()));
                assert!(deps.contains(&"matplotlib".to_string()));
            }
            _ => panic!("Expected InlinePython extension"),
        }
    }

    #[test]
    fn test_from_content_with_activities() {
        let content = r#"{
            "version": "1.0.0",
            "title": "Test Workflow",
            "description": "A test workflow",
            "instructions": "Test instructions",
            "activities": ["activity1", "activity2"]
        }"#;

        let workflow = Workflow::from_content(content).unwrap();

        assert!(workflow.activities.is_some());
        let activities = workflow.activities.unwrap();
        assert_eq!(activities, vec!["activity1", "activity2"]);
    }

    #[test]
    fn test_from_content_with_nested_workflow_yaml() {
        let content = r#"name: test_workflow
workflow:
  title: Nested Workflow Test
  description: A test workflow with nested structure
  instructions: Test instructions for nested workflow
  activities:
    - Test activity 1
    - Test activity 2
  prompt: Test prompt
  extensions: []
isGlobal: true"#;

        let workflow = Workflow::from_content(content).unwrap();
        assert_eq!(workflow.title, "Nested Workflow Test");
        assert_eq!(
            workflow.description,
            "A test workflow with nested structure"
        );
        assert_eq!(
            workflow.instructions,
            Some("Test instructions for nested workflow".to_string())
        );
        assert_eq!(workflow.prompt, Some("Test prompt".to_string()));
        assert!(workflow.activities.is_some());
        let activities = workflow.activities.unwrap();
        assert_eq!(activities, vec!["Test activity 1", "Test activity 2"]);
        assert!(workflow.extensions.is_some());
        let extensions = workflow.extensions.unwrap();
        assert_eq!(extensions.len(), 0);
    }

    #[test]
    fn test_check_for_security_warnings() {
        let mut workflow = Workflow {
            version: "1.0.0".to_string(),
            title: "Test".to_string(),
            description: "Test".to_string(),
            instructions: Some("clean instructions".to_string()),
            prompt: Some("clean prompt".to_string()),
            extensions: None,
            knowledge_bases: None,
            skills: None,
            settings: None,
            activities: Some(vec!["clean activity 1".to_string()]),
            author: None,
            parameters: None,
            response: None,
            sub_workflows: None,
            retry: None,
        };

        assert!(!workflow.check_for_security_warnings());

        // Malicious activities
        workflow.activities = Some(vec![
            "clean activity".to_string(),
            format!("malicious{}activity", '\u{E0041}'),
        ]);
        assert!(workflow.check_for_security_warnings());

        // Malicious instructions
        workflow.instructions = Some(format!("instructions{}", '\u{E0041}'));
        assert!(workflow.check_for_security_warnings());

        // Malicious prompt
        workflow.prompt = Some(format!("prompt{}", '\u{E0042}'));
        assert!(workflow.check_for_security_warnings());
    }

    #[test]
    fn test_from_content_with_null_description() {
        let content = r#"{
            "version": "1.0.0",
            "title": "Test Workflow",
            "description": "A test workflow",
            "instructions": "Test instructions",
            "extensions": [
                {
                    "type": "stdio",
                    "name": "test_extension",
                    "cmd": "test_cmd",
                    "args": [],
                    "timeout": 300,
                    "description": null
                }
            ]
        }"#;

        let workflow = Workflow::from_content(content).unwrap();

        assert!(workflow.extensions.is_some());
        let extensions = workflow.extensions.unwrap();
        assert_eq!(extensions.len(), 1);

        if let ExtensionConfig::Stdio {
            name, description, ..
        } = &extensions[0]
        {
            assert_eq!(name, "test_extension");
            assert_eq!(description, "");
        } else {
            panic!("Expected Stdio extension");
        }
    }
}
