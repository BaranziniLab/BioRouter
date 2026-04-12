use crate::workflow::read_workflow_file_content::read_parameter_file_content;
use crate::workflow::template_workflow::render_workflow_content_with_params;
use crate::workflow::validate_workflow::validate_workflow_template_from_content;
use crate::workflow::{
    Workflow, WorkflowParameter, WorkflowParameterInputType, WorkflowParameterRequirement,
    BUILT_IN_WORKFLOW_DIR_PARAM,
};
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("Missing required parameters: {parameters:?}")]
    MissingParams { parameters: Vec<String> },
    #[error("Template rendering failed: {source}")]
    TemplateRendering { source: anyhow::Error },
    #[error("Workflow parsing failed: {source}")]
    WorkflowParsing { source: anyhow::Error },
}

fn render_workflow_template<F>(
    workflow_content: String,
    workflow_dir: &Path,
    params: Vec<(String, String)>,
    user_prompt_fn: Option<F>,
) -> Result<(String, Vec<String>)>
where
    F: Fn(&str, &str) -> Result<String, anyhow::Error>,
{
    let workflow_dir_str = workflow_dir.display().to_string();

    let workflow_parameters =
        validate_workflow_template_from_content(&workflow_content, Some(workflow_dir_str.clone()))?
            .parameters;

    let (params_for_template, missing_params) =
        apply_values_to_parameters(&params, workflow_parameters, &workflow_dir_str, user_prompt_fn)?;

    let rendered_content = if missing_params.is_empty() {
        render_workflow_content_with_params(&workflow_content, &params_for_template)?
    } else {
        String::new()
    };

    Ok((rendered_content, missing_params))
}

pub fn build_workflow_from_template<F>(
    workflow_content: String,
    workflow_dir: &Path,
    params: Vec<(String, String)>,
    user_prompt_fn: Option<F>,
) -> Result<Workflow, WorkflowError>
where
    F: Fn(&str, &str) -> Result<String, anyhow::Error>,
{
    let (rendered_content, missing_params) =
        render_workflow_template(workflow_content, workflow_dir, params.clone(), user_prompt_fn)
            .map_err(|source| WorkflowError::TemplateRendering { source })?;

    if !missing_params.is_empty() {
        return Err(WorkflowError::MissingParams {
            parameters: missing_params,
        });
    }

    let mut workflow = Workflow::from_content(&rendered_content)
        .map_err(|source| WorkflowError::WorkflowParsing { source })?;

    if let Some(ref mut sub_workflows) = workflow.sub_workflows {
        for sub_workflow in sub_workflows {
            sub_workflow.path = resolve_sub_workflow_path(&sub_workflow.path, workflow_dir)?;
        }
    }

    Ok(workflow)
}

pub fn build_workflow_from_template_with_positional_params<F>(
    workflow_content: String,
    workflow_dir: &Path,
    params: Vec<String>,
    user_prompt_fn: Option<F>,
) -> Result<Workflow, WorkflowError>
where
    F: Fn(&str, &str) -> Result<String, anyhow::Error>,
{
    let workflow_dir_str = workflow_dir.display().to_string();

    let workflow_parameters =
        validate_workflow_template_from_content(&workflow_content, Some(workflow_dir_str.clone()))
            .map_err(|source| WorkflowError::TemplateRendering { source })?
            .parameters;

    let param_pairs: Vec<(String, String)> = if let Some(workflow_params) = &workflow_parameters {
        let required_count = workflow_params.iter().filter(|p| p.default.is_none()).count();
        if params.len() < required_count {
            let required_keys: Vec<String> = workflow_params
                .iter()
                .filter(|p| p.default.is_none())
                .map(|p| p.key.clone())
                .collect();
            return Err(WorkflowError::MissingParams {
                parameters: required_keys,
            });
        }
        workflow_params
            .iter()
            .zip(params.iter())
            .map(|(rp, p)| (rp.key.clone(), p.clone()))
            .collect()
    } else {
        vec![]
    };

    build_workflow_from_template(workflow_content, workflow_dir, param_pairs, user_prompt_fn)
}

pub fn apply_values_to_parameters<F>(
    user_params: &[(String, String)],
    workflow_parameters: Option<Vec<WorkflowParameter>>,
    workflow_dir: &str,
    user_prompt_fn: Option<F>,
) -> Result<(HashMap<String, String>, Vec<String>)>
where
    F: Fn(&str, &str) -> Result<String, anyhow::Error>,
{
    let mut param_map: HashMap<String, String> = user_params.iter().cloned().collect();
    param_map.insert(
        BUILT_IN_WORKFLOW_DIR_PARAM.to_string(),
        workflow_dir.to_string(),
    );
    let mut missing_params: Vec<String> = Vec::new();
    for param in workflow_parameters.unwrap_or_default() {
        if !param_map.contains_key(&param.key) {
            match (&param.default, &param.requirement) {
                (Some(default), _) => param_map.insert(param.key.clone(), default.clone()),
                (None, WorkflowParameterRequirement::UserPrompt) if user_prompt_fn.is_some() => {
                    let input_value =
                        user_prompt_fn.as_ref().unwrap()(&param.key, &param.description)?;
                    param_map.insert(param.key.clone(), input_value)
                }
                _ => {
                    missing_params.push(param.key.clone());
                    None
                }
            };
        } else if matches!(param.input_type, WorkflowParameterInputType::File) {
            let file_path = param_map.get(&param.key).unwrap();
            let file_content = read_parameter_file_content(file_path)?;
            param_map.insert(param.key.clone(), file_content);
        }
    }
    Ok((param_map, missing_params))
}

fn resolve_sub_workflow_path(
    sub_workflow_path: &str,
    parent_workflow_dir: &Path,
) -> Result<String, WorkflowError> {
    let path = if Path::new(sub_workflow_path).is_absolute() {
        Path::new(sub_workflow_path).to_path_buf()
    } else {
        parent_workflow_dir.join(sub_workflow_path)
    };
    if !path.exists() {
        return Err(WorkflowError::WorkflowParsing {
            source: anyhow::anyhow!("Sub-workflow file does not exist: {}", path.display()),
        });
    }

    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests;
