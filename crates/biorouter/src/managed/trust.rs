//! Ownership / permission verification for the managed-policy file (BR-65).
//!
//! Before parsing the managed file we verify that neither it nor its parent
//! directory can be rewritten by a non-privileged user — mirroring Gemini CLI's
//! "ownership verification against privilege escalation". A file that fails the
//! check is *ignored* (fail-open on presence, so a bad MDM push cannot brick the
//! agent), but the failure is surfaced to the caller so it can `warn!`.

use std::path::Path;

/// Verify the managed file and its parent directory are trusted: owned by root
/// or the current user, and not group/other-writable. Returns `Err(reason)`
/// when the file must be ignored.
#[cfg(unix)]
pub fn verify_trusted(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;

    // SAFETY: `geteuid` is always safe — it only reads the current process's
    // effective uid and cannot fail.
    let euid = unsafe { libc::geteuid() };

    let check = |p: &Path| -> Result<(), String> {
        let meta = std::fs::metadata(p).map_err(|e| format!("cannot stat {}: {e}", p.display()))?;
        let uid = meta.uid();
        // Trusted owner: root (a real admin deployment) or the current user (a
        // user-mode dev/test install under BIOROUTER_PATH_ROOT).
        if uid != 0 && uid != euid {
            return Err(format!(
                "{} is owned by uid {uid}, expected root (0) or current user ({euid})",
                p.display()
            ));
        }
        // Reject group- or world-writable, so a non-owner cannot rewrite policy.
        let mode = meta.mode();
        if mode & 0o022 != 0 {
            return Err(format!(
                "{} is group/other-writable (mode {:o})",
                p.display(),
                mode & 0o777
            ));
        }
        Ok(())
    };

    check(path)?;
    if let Some(parent) = path.parent() {
        check(parent)?;
    }
    Ok(())
}

/// On Windows the managed file lives under `%ProgramData%`, which is
/// admin-writable-only by default. Deep ACL owner verification is deferred to
/// phase 2 (the `windows` crate); until then we trust the location.
#[cfg(not(unix))]
pub fn verify_trusted(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn write_with_mode(path: &Path, mode: u32) {
        std::fs::write(path, "permissions: {}\n").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn accepts_owner_owned_non_world_writable_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let file = dir.path().join("managed-policy.yaml");
        write_with_mode(&file, 0o644);
        assert!(verify_trusted(&file).is_ok());
    }

    #[test]
    fn rejects_world_writable_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let file = dir.path().join("managed-policy.yaml");
        write_with_mode(&file, 0o666);
        let err = verify_trusted(&file).unwrap_err();
        assert!(err.contains("writable"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_world_writable_parent_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
        let file = dir.path().join("managed-policy.yaml");
        write_with_mode(&file, 0o644);
        let err = verify_trusted(&file).unwrap_err();
        assert!(err.contains("writable"), "unexpected error: {err}");
    }

    #[test]
    fn missing_file_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("absent.yaml");
        assert!(verify_trusted(&file).is_err());
    }
}
