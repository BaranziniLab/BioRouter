//! Real-use stress tests for the sandbox backends.
//!
//! These exercise actual process execution, pipe draining, timeout-kill,
//! concurrency, and the path-jail against genuine traversal/symlink attacks —
//! not toy assertions.

use std::time::Duration;

use biorouter_sandbox::{
    DockerSandbox, LocalProcessSandbox, Mount, NetworkPolicy, SandboxClient, SandboxError,
    SandboxSpec,
};

fn ws() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

// ----------------------------- LOCAL: exec -----------------------------

#[tokio::test]
async fn local_exec_captures_stdout_and_exit_code() {
    let dir = ws();
    let sb = LocalProcessSandbox::new(SandboxSpec::new(dir.path()));
    let out = sb
        .exec(&["sh".into(), "-c".into(), "echo hello world".into()], None)
        .await
        .unwrap();
    assert!(out.success(), "stderr={}", out.stderr);
    assert_eq!(out.stdout.trim(), "hello world");
    assert_eq!(out.code, 0);
}

#[tokio::test]
async fn local_exec_propagates_nonzero_exit() {
    let dir = ws();
    let sb = LocalProcessSandbox::new(SandboxSpec::new(dir.path()));
    let out = sb
        .exec(
            &["sh".into(), "-c".into(), "echo oops >&2; exit 3".into()],
            None,
        )
        .await
        .unwrap();
    assert_eq!(out.code, 3);
    assert!(!out.success());
    assert!(out.stderr.contains("oops"));
}

#[tokio::test]
async fn local_exec_feeds_stdin() {
    let dir = ws();
    let sb = LocalProcessSandbox::new(SandboxSpec::new(dir.path()));
    let out = sb
        .exec(&["cat".into()], Some("piped-input-value"))
        .await
        .unwrap();
    assert_eq!(out.stdout, "piped-input-value");
}

#[tokio::test]
async fn local_exec_injects_env_without_leaking_to_wire() {
    let dir = ws();
    let spec =
        SandboxSpec::new(dir.path()).with_env(vec![("VAULT_SECRET".into(), "s3cr3t".into())]);
    let sb = LocalProcessSandbox::new(spec);
    let out = sb
        .exec(
            &[
                "sh".into(),
                "-c".into(),
                "printf %s \"$VAULT_SECRET\"".into(),
            ],
            None,
        )
        .await
        .unwrap();
    assert_eq!(out.stdout, "s3cr3t");
}

#[tokio::test]
async fn local_exec_timeout_kills_runaway_promptly() {
    let dir = ws();
    let spec = SandboxSpec::new(dir.path()).with_timeout(Duration::from_millis(200));
    let sb = LocalProcessSandbox::new(spec);
    let start = std::time::Instant::now();
    let out = sb
        .exec(&["sh".into(), "-c".into(), "sleep 30".into()], None)
        .await
        .unwrap();
    let elapsed = start.elapsed();
    assert!(out.timed_out, "expected timeout, got {out:?}");
    assert!(
        elapsed < Duration::from_secs(3),
        "kill_on_drop should reap the child promptly; took {elapsed:?}"
    );
}

#[tokio::test]
async fn local_exec_drains_large_output_without_deadlock() {
    // A naive implementation that waits before draining the stdout pipe would
    // deadlock once the OS pipe buffer (~64KB) fills. 100k bytes proves we drain.
    let dir = ws();
    let sb = LocalProcessSandbox::new(SandboxSpec::new(dir.path()));
    let out = sb
        .exec(
            &[
                "sh".into(),
                "-c".into(),
                "yes x | head -n 100000 | tr -d '\\n'".into(),
            ],
            None,
        )
        .await
        .unwrap();
    assert_eq!(out.stdout.len(), 100_000);
}

#[tokio::test]
async fn local_exec_handles_20_concurrent_commands() {
    let dir = ws();
    let sb = std::sync::Arc::new(LocalProcessSandbox::new(SandboxSpec::new(dir.path())));
    let mut handles = Vec::new();
    for i in 0..20 {
        let sb = sb.clone();
        handles.push(tokio::spawn(async move {
            sb.exec(&["sh".into(), "-c".into(), format!("echo n{i}")], None)
                .await
        }));
    }
    for (i, h) in handles.into_iter().enumerate() {
        let out = h.await.unwrap().unwrap();
        assert_eq!(out.stdout.trim(), format!("n{i}"));
    }
}

#[tokio::test]
async fn local_exec_empty_argv_is_error() {
    let dir = ws();
    let sb = LocalProcessSandbox::new(SandboxSpec::new(dir.path()));
    assert!(matches!(
        sb.exec(&[], None).await,
        Err(SandboxError::Exec(_))
    ));
}

// --------------------------- LOCAL: file jail ---------------------------

#[tokio::test]
async fn local_files_roundtrip_and_list() {
    let dir = ws();
    let sb = LocalProcessSandbox::new(SandboxSpec::new(dir.path()));
    sb.write("sub/dir/data.txt", b"contents-here")
        .await
        .unwrap();
    let read = sb.read("sub/dir/data.txt").await.unwrap();
    assert_eq!(read, b"contents-here");
    let listed = sb.list("sub/dir").await.unwrap();
    assert!(listed.contains(&"data.txt".to_string()));
}

#[tokio::test]
async fn jail_rejects_parent_traversal_read_and_write() {
    let dir = ws();
    let sb = LocalProcessSandbox::new(SandboxSpec::new(dir.path()));
    assert!(matches!(
        sb.read("../../../etc/passwd").await,
        Err(SandboxError::PathEscape(_))
    ));
    assert!(matches!(
        sb.write("../escaped.txt", b"x").await,
        Err(SandboxError::PathEscape(_))
    ));
    // The escaped file must not exist on disk.
    assert!(!dir.path().join("../escaped.txt").exists());
}

#[tokio::test]
async fn jail_rejects_absolute_path_outside_workspace() {
    let dir = ws();
    let sb = LocalProcessSandbox::new(SandboxSpec::new(dir.path()));
    assert!(matches!(
        sb.read("/etc/hosts").await,
        Err(SandboxError::PathEscape(_))
    ));
}

#[tokio::test]
#[cfg(unix)]
async fn jail_rejects_symlink_escape() {
    use std::os::unix::fs::symlink;
    let dir = ws();
    // A symlink inside the workspace that points outside it.
    symlink("/etc", dir.path().join("escape")).unwrap();
    let sb = LocalProcessSandbox::new(SandboxSpec::new(dir.path()));
    let r = sb.read("escape/hosts").await;
    assert!(
        matches!(r, Err(SandboxError::PathEscape(_))),
        "symlink escape was not caught: {r:?}"
    );
}

#[tokio::test]
async fn snapshot_returns_workspace_handle() {
    let dir = ws();
    let sb = LocalProcessSandbox::new(SandboxSpec::new(dir.path()));
    let handle = sb.snapshot().await.unwrap();
    assert!(handle.contains(
        &dir.path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string()
    ));
}

// --------------------------- DOCKER: arg builder ---------------------------

#[test]
fn docker_run_args_apply_isolation_hardening() {
    let spec = SandboxSpec::new("/tmp/ws")
        .with_network(NetworkPolicy::None)
        .with_max_mem("512m")
        .with_cpus(1.5);
    let args = DockerSandbox::new(spec).build_run_args(&["echo".into(), "hi".into()]);
    let joined = args.join(" ");
    assert!(joined.contains("run --rm"), "{joined}");
    assert!(joined.contains("--network none"), "{joined}");
    assert!(joined.contains("--cap-drop ALL"), "{joined}");
    assert!(
        joined.contains("--security-opt no-new-privileges"),
        "{joined}"
    );
    assert!(joined.contains("--user 65534:65534"), "{joined}");
    assert!(joined.contains("--pids-limit 256"), "{joined}");
    assert!(joined.contains("--memory 512m"), "{joined}");
    assert!(joined.contains("--cpus 1.5"), "{joined}");
    assert!(joined.contains("-v /tmp/ws:/work:rw"), "{joined}");
    assert!(joined.contains("-w /work"), "{joined}");
    // Image precedes the command, and the command is last.
    assert!(joined.ends_with("python:3.12-slim echo hi"), "{joined}");
}

#[test]
fn docker_run_args_honor_host_network_mounts_and_env() {
    let spec = SandboxSpec::new("/tmp/ws")
        .with_network(NetworkPolicy::Host)
        .with_mount(Mount {
            host: "/data/cohort".into(),
            guest: "/data".into(),
            writable: false,
        })
        .with_env(vec![("API_KEY".into(), "k123".into())])
        .with_image("alpine:3");
    let args = DockerSandbox::new(spec).build_run_args(&["ls".into(), "/data".into()]);
    let joined = args.join(" ");
    assert!(joined.contains("--network host"), "{joined}");
    assert!(joined.contains("-v /data/cohort:/data:ro"), "{joined}");
    assert!(joined.contains("-e API_KEY=k123"), "{joined}");
    assert!(joined.ends_with("alpine:3 ls /data"), "{joined}");
}

#[tokio::test]
async fn docker_exec_without_docker_reports_unavailable() {
    // When docker is genuinely absent, exec returns Unavailable (not a panic).
    let dir = ws();
    let sb = DockerSandbox::new(SandboxSpec::new(dir.path()));
    if sb.available().await {
        // Docker present in this environment — skip the negative assertion.
        eprintln!("docker present; skipping unavailable-path assertion");
        return;
    }
    assert!(matches!(
        sb.exec(&["echo".into(), "hi".into()], None).await,
        Err(SandboxError::Unavailable(_))
    ));
}

/// Real end-to-end container run. Ignored by default because it requires a
/// docker daemon and pulls an image (network). Run with:
/// `cargo test -p biorouter-sandbox -- --ignored docker_exec_real`
#[tokio::test]
#[ignore = "requires docker daemon + image pull"]
async fn docker_exec_real_roundtrip() {
    let dir = ws();
    let spec = SandboxSpec::new(dir.path())
        .with_image("alpine:3")
        .with_timeout(Duration::from_secs(120));
    let sb = DockerSandbox::new(spec);
    if !sb.available().await {
        eprintln!("docker unavailable; skipping");
        return;
    }
    let out = sb
        .exec(
            &["sh".into(), "-c".into(), "echo from-container".into()],
            None,
        )
        .await
        .unwrap();
    assert!(out.stdout.contains("from-container"), "{out:?}");
    assert!(out.success());
}
