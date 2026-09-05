use crate::workflow::build_workflow::{
    apply_values_to_parameters, build_workflow_from_template, resolve_sub_workflow_path,
    WorkflowError,
};
use crate::workflow::read_workflow_file_content::WorkflowFile;
use crate::workflow::{
    WorkflowParameter, WorkflowParameterInputType, WorkflowParameterRequirement,
};
use std::path::PathBuf;
use tempfile::TempDir;

#[allow(clippy::type_complexity)]
const NO_USER_PROMPT: Option<fn(&str, &str) -> Result<String, anyhow::Error>> = None;

fn setup_workflow_file(instructions_and_parameters: &str) -> (TempDir, String, PathBuf) {
    let workflow_content = format!(
        r#"{{
            "version": "1.0.0",
            "title": "Test Workflow",
            "description": "A test workflow",
            {}
        }}"#,
        instructions_and_parameters
    );
    let temp_dir = tempfile::tempdir().unwrap();
    let workflow_path = temp_dir.path().join("test_workflow.json");

    std::fs::write(&workflow_path, workflow_content).unwrap();
    let workflow_dir = temp_dir.path().to_path_buf();
    let workflow_content = std::fs::read_to_string(&workflow_path).unwrap();

    (temp_dir, workflow_content, workflow_dir)
}

fn setup_test_file(temp_dir: &TempDir, filename: &str, content: &str) -> std::path::PathBuf {
    let file_path = temp_dir.path().join(filename);
    std::fs::write(&file_path, content).unwrap();
    file_path
}

fn setup_yaml_workflow_file(instructions_and_parameters: &str) -> (TempDir, WorkflowFile) {
    let workflow_content = format!(
        r#"version: "1.0.0"
title: "Test Workflow"
description: "A test workflow"
{}"#,
        instructions_and_parameters
    );
    let temp_dir = tempfile::tempdir().unwrap();
    let workflow_path = temp_dir.path().join("test_workflow.yaml");

    std::fs::write(&workflow_path, workflow_content).unwrap();

    let workflow_file = WorkflowFile {
        content: std::fs::read_to_string(&workflow_path).unwrap(),
        parent_dir: temp_dir.path().to_path_buf(),
        file_path: workflow_path,
    };

    (temp_dir, workflow_file)
}

fn setup_yaml_workflow_files(
    parent_content: &str,
    child_content: &str,
) -> (TempDir, WorkflowFile, WorkflowFile) {
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path();

    let parent_path = temp_path.join("parent.yaml");
    std::fs::write(&parent_path, parent_content).unwrap();

    let child_path = temp_path.join("child.yaml");
    std::fs::write(&child_path, child_content).unwrap();

    let parent_workflow_file = WorkflowFile {
        content: std::fs::read_to_string(&parent_path).unwrap(),
        parent_dir: temp_path.to_path_buf(),
        file_path: parent_path,
    };

    let child_workflow_file = WorkflowFile {
        content: std::fs::read_to_string(&child_path).unwrap(),
        parent_dir: temp_path.to_path_buf(),
        file_path: child_path,
    };

    (temp_dir, parent_workflow_file, child_workflow_file)
}

#[test]
fn test_build_workflow_from_template_success() {
    let instructions_and_parameters = r#"
                "instructions": "Test instructions with {{ my_name }}",
                "parameters": [
                    {
                        "key": "my_name",
                        "input_type": "string",
                        "requirement": "required",
                        "description": "A test parameter"
                    }
                ]"#;

    let (_temp_dir, workflow_content, workflow_dir) =
        setup_workflow_file(instructions_and_parameters);

    let params = vec![("my_name".to_string(), "value".to_string())];
    let workflow =
        build_workflow_from_template(workflow_content, &workflow_dir, params, NO_USER_PROMPT)
            .unwrap();

    assert_eq!(workflow.title, "Test Workflow");
    assert_eq!(workflow.description, "A test workflow");
    assert_eq!(
        workflow.instructions.unwrap(),
        "Test instructions with value"
    );
    assert_eq!(workflow.parameters.as_ref().unwrap().len(), 1);
    let param = &workflow.parameters.as_ref().unwrap()[0];
    assert_eq!(param.key, "my_name");
    assert!(matches!(
        param.input_type,
        WorkflowParameterInputType::String
    ));
    assert!(matches!(
        param.requirement,
        WorkflowParameterRequirement::Required
    ));
    assert_eq!(param.description, "A test parameter");
}

#[test]
fn test_build_workflow_from_template_success_variable_in_prompt() {
    let instructions_and_parameters = r#"
                "instructions": "Test instructions",
                "prompt": "My prompt {{ my_name }}",
                "parameters": [
                    {
                        "key": "my_name",
                        "input_type": "string",
                        "requirement": "required",
                        "description": "A test parameter"
                    }
                ]"#;

    let (_temp_dir, workflow_content, workflow_dir) =
        setup_workflow_file(instructions_and_parameters);

    let params = vec![("my_name".to_string(), "value".to_string())];
    let workflow =
        build_workflow_from_template(workflow_content, &workflow_dir, params, NO_USER_PROMPT)
            .unwrap();

    assert_eq!(workflow.title, "Test Workflow");
    assert_eq!(workflow.description, "A test workflow");
    assert_eq!(workflow.instructions.unwrap(), "Test instructions");
    assert_eq!(workflow.prompt.unwrap(), "My prompt value");
    let param = &workflow.parameters.as_ref().unwrap()[0];
    assert_eq!(param.key, "my_name");
    assert!(matches!(
        param.input_type,
        WorkflowParameterInputType::String
    ));
    assert!(matches!(
        param.requirement,
        WorkflowParameterRequirement::Required
    ));
    assert_eq!(param.description, "A test parameter");
}

#[test]
fn test_build_workflow_from_template_wrong_parameters_in_workflow_file() {
    let instructions_and_parameters = r#"
                "instructions": "Test instructions with {{ expected_param1 }} {{ expected_param2 }}",
                "parameters": [
                    {
                        "key": "wrong_param_key",
                        "input_type": "string",
                        "requirement": "required",
                        "description": "A test parameter"
                    }
                ]"#;
    let (_temp_dir, workflow_content, workflow_dir) =
        setup_workflow_file(instructions_and_parameters);

    let build_workflow_result =
        build_workflow_from_template(workflow_content, &workflow_dir, Vec::new(), NO_USER_PROMPT);
    assert!(build_workflow_result.is_err());
    let err = build_workflow_result.unwrap_err();
    println!("{}", err);

    match err {
        WorkflowError::TemplateRendering { source } => {
            let err_str = source.to_string();
            assert!(err_str.contains("Unnecessary parameter definitions: wrong_param_key."));
            assert!(err_str.contains("Missing definitions for parameters in the workflow file:"));
            assert!(err_str.contains("expected_param1"));
            assert!(err_str.contains("expected_param2"));
        }
        _ => panic!("Expected TemplateRendering error"),
    }
}

#[test]
fn test_build_workflow_from_template_with_default_values_in_workflow_file() {
    let instructions_and_parameters = r#"
                "instructions": "Test instructions with {{ param_with_default }} {{ param_without_default }}",
                "parameters": [
                    {
                        "key": "param_with_default",
                        "input_type": "string",
                        "requirement": "optional",
                        "default": "my_default_value",
                        "description": "A test parameter"
                    },
                    {
                        "key": "param_without_default",
                        "input_type": "string",
                        "requirement": "required",
                        "description": "A test parameter"
                    }
                ]"#;
    let (_temp_dir, workflow_content, workflow_dir) =
        setup_workflow_file(instructions_and_parameters);
    let params = vec![("param_without_default".to_string(), "value1".to_string())];

    let workflow =
        build_workflow_from_template(workflow_content, &workflow_dir, params, NO_USER_PROMPT)
            .unwrap();

    assert_eq!(workflow.title, "Test Workflow");
    assert_eq!(workflow.description, "A test workflow");
    assert_eq!(
        workflow.instructions.unwrap(),
        "Test instructions with my_default_value value1"
    );
}

#[test]
fn test_build_workflow_from_template_optional_parameters_with_empty_default_values_in_workflow_file(
) {
    let instructions_and_parameters = r#"
                "instructions": "Test instructions with {{ optional_param }}",
                "parameters": [
                    {
                        "key": "optional_param",
                        "input_type": "string",
                        "requirement": "optional",
                        "description": "A test parameter",
                        "default": ""
                    }
                ]"#;
    let (_temp_dir, workflow_content, workflow_dir) =
        setup_workflow_file(instructions_and_parameters);

    let workflow =
        build_workflow_from_template(workflow_content, &workflow_dir, Vec::new(), NO_USER_PROMPT)
            .unwrap();
    assert_eq!(workflow.title, "Test Workflow");
    assert_eq!(workflow.description, "A test workflow");
    assert_eq!(workflow.instructions.unwrap(), "Test instructions with ");
}

#[test]
fn test_build_workflow_from_template_optional_parameters_without_default_values_in_workflow_file() {
    let instructions_and_parameters = r#"
                "instructions": "Test instructions with {{ optional_param }}",
                "parameters": [
                    {
                        "key": "optional_param",
                        "input_type": "string",
                        "requirement": "optional",
                        "description": "A test parameter"
                    }
                ]"#;
    let (_temp_dir, workflow_content, workflow_dir) =
        setup_workflow_file(instructions_and_parameters);

    let build_workflow_result =
        build_workflow_from_template(workflow_content, &workflow_dir, Vec::new(), NO_USER_PROMPT);
    assert!(build_workflow_result.is_err());
    let err = build_workflow_result.unwrap_err();
    println!("{}", err);
    match err {
        WorkflowError::TemplateRendering { source } => {
            assert!(source.to_string().to_lowercase().contains("missing"));
        }
        _ => panic!("Expected TemplateRendering error"),
    }
}

#[test]
fn test_build_workflow_from_template_wrong_input_type_in_workflow_file() {
    let instructions_and_parameters = r#"
                "instructions": "Test instructions with {{ param }}",
                "parameters": [
                    {
                        "key": "param",
                        "input_type": "some_invalid_type",
                        "requirement": "required",
                        "description": "A test parameter"
                    }
                ]"#;
    let params = vec![("param".to_string(), "value".to_string())];
    let (_temp_dir, workflow_content, workflow_dir) =
        setup_workflow_file(instructions_and_parameters);

    let build_workflow_result =
        build_workflow_from_template(workflow_content, &workflow_dir, params, NO_USER_PROMPT);
    assert!(build_workflow_result.is_err());
    let err = build_workflow_result.unwrap_err();
    match err {
        WorkflowError::TemplateRendering { source } => {
            let err_msg = source.to_string();
            eprint!("Error: {}", err_msg);
            assert!(err_msg.contains("unknown variant `some_invalid_type`"));
        }
        _ => panic!("Expected TemplateRendering error, got: {:?}", err),
    }
}

#[test]
fn test_build_workflow_from_template_success_without_parameters() {
    let instructions_and_parameters = r#"
                "instructions": "Test instructions"
                "#;
    let (_temp_dir, workflow_content, workflow_dir) =
        setup_workflow_file(instructions_and_parameters);

    let workflow =
        build_workflow_from_template(workflow_content, &workflow_dir, Vec::new(), NO_USER_PROMPT)
            .unwrap();
    assert_eq!(workflow.instructions.unwrap(), "Test instructions");
    assert!(workflow.parameters.is_none());
}

#[test]
fn test_build_workflow_from_template_missing_prompt_and_instructions() {
    let instructions_and_parameters = "";
    let (_temp_dir, workflow_content, workflow_dir) =
        setup_workflow_file(instructions_and_parameters);

    let build_workflow_result =
        build_workflow_from_template(workflow_content, &workflow_dir, Vec::new(), NO_USER_PROMPT);
    assert!(build_workflow_result.is_err());
    let err = build_workflow_result.unwrap_err();
    println!("{}", err);

    match err {
        WorkflowError::TemplateRendering { source } => {
            let err_str = source.to_string();
            assert!(err_str
                .contains("Workflow must specify at least one of `instructions` or `prompt`."));
        }
        _ => panic!("Expected TemplateRendering error"),
    }
}

#[test]
fn test_template_inheritance() {
    let parent_content = r#"
                version: 1.0.0
                title: Parent
                description: Parent workflow
                prompt: |
                    show me the news for day: {{ date }}
                    {% block prompt -%}
                    What is the capital of France?
                    {%- endblock %}
                    {% if is_enabled %}
                        Feature is enabled.
                    {% else %}
                        Feature is disabled.
                    {% endif %}
                parameters:
                    - key: date
                      input_type: string
                      requirement: required
                      description: date specified by the user
                    - key: is_enabled
                      input_type: boolean
                      requirement: required
                      description: whether the feature is enabled
            "#;

    let child_content = r#"
                {% extends "parent.yaml" -%}
                {% block prompt -%}
                What is the capital of Germany?
                {%- endblock %}
            "#;

    let (_temp_dir, parent_workflow_file, child_workflow_file) =
        setup_yaml_workflow_files(parent_content, child_content);

    let params = vec![
        ("date".to_string(), "today".to_string()),
        ("is_enabled".to_string(), "true".to_string()),
    ];

    let parent_workflow = build_workflow_from_template(
        parent_workflow_file.content,
        &parent_workflow_file.parent_dir,
        params.clone(),
        NO_USER_PROMPT,
    )
    .unwrap();
    assert_eq!(parent_workflow.description, "Parent workflow");
    assert_eq!(
            parent_workflow.prompt.unwrap(),
            "show me the news for day: today\nWhat is the capital of France?\n\n    Feature is enabled.\n"
        );
    assert_eq!(parent_workflow.parameters.as_ref().unwrap().len(), 2);
    assert_eq!(parent_workflow.parameters.as_ref().unwrap()[0].key, "date");
    assert_eq!(
        parent_workflow.parameters.as_ref().unwrap()[1].key,
        "is_enabled"
    );

    let child_workflow = build_workflow_from_template(
        child_workflow_file.content,
        &child_workflow_file.parent_dir,
        params,
        NO_USER_PROMPT,
    )
    .unwrap();
    assert_eq!(child_workflow.title, "Parent");
    assert_eq!(child_workflow.description, "Parent workflow");
    assert_eq!(
            child_workflow.prompt.unwrap().trim(),
            "show me the news for day: today\nWhat is the capital of Germany?\n\n    Feature is enabled."
        );
    assert_eq!(child_workflow.parameters.as_ref().unwrap().len(), 2);
    assert_eq!(child_workflow.parameters.as_ref().unwrap()[0].key, "date");
    assert_eq!(
        child_workflow.parameters.as_ref().unwrap()[1].key,
        "is_enabled"
    );
}

mod sub_workflow_path_resolution {
    use super::*;

    fn create_workflow_file(
        temp_path: &std::path::Path,
        workflow_folder: &str,
        workflow_file_name: &str,
        content: &str,
    ) -> std::path::PathBuf {
        let workflows_dir = temp_path.join(workflow_folder);
        std::fs::create_dir_all(&workflows_dir).unwrap();
        let workflow_path = workflows_dir.join(workflow_file_name);
        std::fs::write(&workflow_path, content).unwrap();
        workflow_path
    }

    #[test]
    fn test_resolve_sub_workflow_path_relative() {
        let temp_dir = tempfile::tempdir().unwrap();
        let parent_dir = temp_dir.path();

        // Create the sub-workflow file
        let sub_workflow_content = r#"
version: 1.0.0
title: Child Workflow
description: A child workflow
instructions: Child instructions"#;
        create_workflow_file(
            parent_dir,
            "sub-workflows",
            "child.yaml",
            sub_workflow_content,
        );

        let result = resolve_sub_workflow_path("./sub-workflows/child.yaml", parent_dir);
        assert!(result.is_ok());

        let expected_path = parent_dir.join("./sub-workflows/child.yaml");
        assert_eq!(result.unwrap(), expected_path.to_str().unwrap());
    }

    #[test]
    fn test_resolve_sub_workflow_path_absolute() {
        let temp_dir = tempfile::tempdir().unwrap();
        let parent_dir = temp_dir.path();

        let sub_workflow_content = r#"
version: 1.0.0
title: Absolute Workflow
description: A workflow with absolute path
instructions: Absolute instructions"#;
        let absolute_path = create_workflow_file(
            parent_dir,
            "absolute",
            "workflow.yaml",
            sub_workflow_content,
        );
        let absolute_path_str = absolute_path.to_str().unwrap();

        let result = resolve_sub_workflow_path(absolute_path_str, parent_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), absolute_path_str);
    }

    #[test]
    fn test_resolve_sub_workflow_path_nonexistent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let parent_dir = temp_dir.path();

        let result = resolve_sub_workflow_path("./sub-workflows/nonexistent.yaml", parent_dir);

        assert!(result.is_err());
        match result {
            Err(WorkflowError::WorkflowParsing { source }) => {
                let error_msg = source.to_string();
                assert!(error_msg.contains("Sub-workflow file does not exist"));
                assert!(error_msg.contains("nonexistent.yaml"));
            }
            _ => panic!("Expected WorkflowError::WorkflowParsing"),
        }
    }

    #[test]
    fn test_build_workflow_with_relative_sub_workflow_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path();
        let sub_workflow_content = r#"
version: 1.0.0
title: Child Workflow
description: A child workflow
instructions: Child instructions
            "#;
        create_workflow_file(
            temp_path,
            "sub-workflows",
            "child.yaml",
            sub_workflow_content,
        );
        let main_workflow_content = r#"{
                "version": "1.0.0",
                "title": "Main Workflow",
                "description": "Main workflow with sub-workflow",
                "instructions": "Main instructions",
                "sub_workflows": [
                    {
                        "name": "child",
                        "path": "./sub-workflows/child.yaml"
                    }
                ]
            }"#;
        let main_workflow_path =
            create_workflow_file(temp_path, "main", "main.json", main_workflow_content);

        let workflow_file = WorkflowFile {
            content: main_workflow_content.to_string(),
            parent_dir: temp_path.to_path_buf(),
            file_path: main_workflow_path,
        };

        let workflow = build_workflow_from_template(
            workflow_file.content,
            &workflow_file.parent_dir,
            Vec::new(),
            NO_USER_PROMPT,
        )
        .unwrap();

        assert_eq!(workflow.title, "Main Workflow");
        assert!(workflow.sub_workflows.is_some());

        let sub_workflows = workflow.sub_workflows.unwrap();
        assert_eq!(sub_workflows.len(), 1);
        assert_eq!(sub_workflows[0].name, "child");

        let expected_absolute_path = temp_path.join("./sub-workflows/child.yaml");
        assert_eq!(
            sub_workflows[0].path,
            expected_absolute_path.to_str().unwrap()
        );
    }
}

mod file_parameter_tests {
    use super::*;

    #[test]
    fn test_build_workflow_file_parameter_valid_paths() {
        let instructions_and_parameters = r#"instructions: "Test file content: {{ FILE_PARAM }}"
parameters:
  - key: FILE_PARAM
    input_type: file
    requirement: required
    description: A file parameter"#;

        let (temp_dir, workflow_file) = setup_yaml_workflow_file(instructions_and_parameters);

        let test_content = "Hello from file!\nThis is line 2\n    Indented line 3";
        let test_file_path = setup_test_file(&temp_dir, "test_file.txt", test_content);

        let params = vec![(
            "FILE_PARAM".to_string(),
            test_file_path.to_string_lossy().to_string(),
        )];
        let result = build_workflow_from_template(
            workflow_file.content,
            &workflow_file.parent_dir,
            params,
            NO_USER_PROMPT,
        );

        assert!(result.is_ok());
        let workflow = result.unwrap();

        let instructions = workflow.instructions.as_ref().unwrap();
        assert!(instructions.contains("Hello from file!"));
        assert!(instructions.contains("Test file content:"));
    }

    #[test]
    fn test_build_workflow_file_parameter_nonexistent_file() {
        let instructions_and_parameters = r#"instructions: "Test file content: {{ FILE_PARAM }}"
parameters:
  - key: FILE_PARAM
    input_type: file
    requirement: required
    description: A file parameter"#;

        let (_temp_dir, workflow_file) = setup_yaml_workflow_file(instructions_and_parameters);

        let params = vec![(
            "FILE_PARAM".to_string(),
            "/nonexistent/path/file.txt".to_string(),
        )];
        let result = build_workflow_from_template(
            workflow_file.content,
            &workflow_file.parent_dir,
            params,
            NO_USER_PROMPT,
        );

        assert!(result.is_err());
        if let Err(WorkflowError::TemplateRendering { source }) = result {
            assert!(source.to_string().contains("Failed to read parameter file"));
        } else {
            panic!("Expected TemplateRendering error");
        }
    }

    #[test]
    fn test_build_workflow_file_parameter_with_default_rejected() {
        let instructions_and_parameters = r#"instructions: "Test file content: {{ FILE_PARAM }}"
parameters:
  - key: FILE_PARAM
    input_type: file
    requirement: required
    description: A file parameter
    default: "/etc/passwd""#;

        let (_temp_dir, workflow_file) = setup_yaml_workflow_file(instructions_and_parameters);

        let params = vec![];
        let result = build_workflow_from_template(
            workflow_file.content,
            &workflow_file.parent_dir,
            params,
            NO_USER_PROMPT,
        );

        assert!(result.is_err());
        if let Err(WorkflowError::TemplateRendering { source }) = result {
            assert!(source
                .to_string()
                .contains("File parameters cannot have default values"));
        } else {
            panic!("Expected TemplateRendering error for file parameter with default");
        }
    }
}

/// A `select` parameter's `options` list is enforced.
///
/// It was decorative: `apply_values_to_parameters` special-cased only the `File`
/// input type, so any string at all was substituted into the prompt. The list is
/// the author's statement about what the workflow can handle.
#[test]
fn a_select_parameter_rejects_a_value_outside_its_options() {
    let parameters = vec![WorkflowParameter {
        key: "detail".to_string(),
        input_type: WorkflowParameterInputType::Select,
        requirement: WorkflowParameterRequirement::Optional,
        description: "how much detail".to_string(),
        default: Some("brief".to_string()),
        options: Some(vec!["brief".to_string(), "full".to_string()]),
    }];

    let accepted = apply_values_to_parameters(
        &[("detail".to_string(), "full".to_string())],
        Some(parameters.clone()),
        "/tmp",
        None::<fn(&str, &str) -> anyhow::Result<String>>,
    );
    assert!(accepted.is_ok(), "a listed value must be accepted");

    let refused = apply_values_to_parameters(
        &[("detail".to_string(), "exhaustive".to_string())],
        Some(parameters.clone()),
        "/tmp",
        None::<fn(&str, &str) -> anyhow::Result<String>>,
    )
    .expect_err("a value outside the options must be refused");
    let message = refused.to_string();
    assert!(message.contains("detail"), "{message}");
    assert!(
        message.contains("brief") && message.contains("full"),
        "the refusal must name the allowed values: {message}"
    );

    // The default takes the same path, so a workflow whose own default is not
    // among its options says so rather than running.
    let mut broken = parameters;
    broken[0].default = Some("nonsense".to_string());
    assert!(
        apply_values_to_parameters(
            &[],
            Some(broken),
            "/tmp",
            None::<fn(&str, &str) -> anyhow::Result<String>>,
        )
        .is_err(),
        "a default outside the options is a broken workflow"
    );
}
