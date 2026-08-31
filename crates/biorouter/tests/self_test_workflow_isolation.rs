use biorouter::workflow::build_workflow::build_workflow_from_template;
use std::path::Path;

fn render_prompt(extra: &[(&str, &str)]) -> String {
    let mut parameters = vec![
        ("test_phases".to_owned(), "apps,previews".to_owned()),
        ("parallel_tests".to_owned(), "false".to_owned()),
        ("cleanup_after".to_owned(), "false".to_owned()),
        ("workspace_dir".to_owned(), "/tmp/selftest-qa".to_owned()),
    ];
    for (key, value) in extra {
        parameters.retain(|(existing, _)| existing.as_str() != *key);
        parameters.push(((*key).to_owned(), (*value).to_owned()));
    }
    let workflow = build_workflow_from_template(
        include_str!("../../../biorouter-self-test.yaml").to_owned(),
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
        parameters,
        None::<fn(&str, &str) -> anyhow::Result<String>>,
    )
    .expect("self-test workflow renders without credentials or provider calls");
    format!(
        "{}\n{}",
        workflow.instructions.expect("self-test has instructions"),
        workflow.prompt.expect("self-test has an execution prompt")
    )
}

#[test]
fn cleanup_disabled_never_instructs_app_deletion_or_collision_replacement() {
    let prompt = render_prompt(&[]);
    assert!(!prompt.contains("`delete_app`"));
    assert!(!prompt.contains("Delete only the"));
    assert!(!prompt.contains("delete only it"));
    assert_eq!(
        prompt
            .matches("stop this phase and report the collision")
            .count(),
        2
    );
}

#[test]
fn distinct_app_parameters_reach_create_launch_and_preview_contracts() {
    let prompt = render_prompt(&[
        ("sdk_app_id", "qa-sdk-unique"),
        ("preview_app_id", "qa-preview-unique"),
    ]);
    assert!(prompt.contains("exact app id `qa-sdk-unique`"));
    assert!(prompt.contains("exact app id `qa-preview-unique`"));
    assert!(prompt.contains("id `qa-preview-unique`"));
    assert!(prompt.contains("`/apps/qa-preview-unique/`"));
    assert!(!prompt.contains("biorouter-self-test-sdk-v2"));
    assert!(!prompt.contains("biorouter-self-test-preview"));
}

#[test]
fn explicit_cleanup_is_limited_to_apps_created_by_this_run() {
    let prompt = render_prompt(&[("cleanup_after", "true")]);
    assert_eq!(prompt.matches("`delete_app`").count(), 2);
    assert_eq!(prompt.matches("only if this run created it").count(), 2);
    assert!(!prompt.contains("delete only it"));
}

#[test]
fn preview_only_does_not_render_the_dashboard_phase() {
    let prompt = render_prompt(&[("test_phases", "previews")]);
    assert!(!prompt.contains("exact app id `biorouter-self-test-sdk-v2`"));
    assert!(prompt.contains("exact app id `biorouter-self-test-preview`"));
    assert!(!prompt.contains("`delete_app`"));
}
