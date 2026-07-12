use etcetera::{choose_app_strategy, AppStrategy, AppStrategyArgs};
use std::path::PathBuf;

pub struct Paths;

impl Paths {
    fn get_dir(dir_type: DirType) -> PathBuf {
        if let Ok(test_root) = std::env::var("BIOROUTER_PATH_ROOT") {
            let base = PathBuf::from(test_root);
            match dir_type {
                DirType::Config => base.join("config"),
                DirType::Data => base.join("data"),
                DirType::State => base.join("state"),
            }
        } else {
            let strategy = choose_app_strategy(AppStrategyArgs {
                top_level_domain: "Block".to_string(),
                author: "Block".to_string(),
                app_name: "biorouter".to_string(),
            })
            .expect("biorouter requires a home dir");

            match dir_type {
                DirType::Config => strategy.config_dir(),
                DirType::Data => strategy.data_dir(),
                DirType::State => strategy.state_dir().unwrap_or(strategy.data_dir()),
            }
        }
    }

    pub fn config_dir() -> PathBuf {
        Self::get_dir(DirType::Config)
    }

    pub fn data_dir() -> PathBuf {
        Self::get_dir(DirType::Data)
    }

    pub fn state_dir() -> PathBuf {
        Self::get_dir(DirType::State)
    }

    pub fn in_state_dir(subpath: &str) -> PathBuf {
        Self::state_dir().join(subpath)
    }

    pub fn in_config_dir(subpath: &str) -> PathBuf {
        Self::config_dir().join(subpath)
    }

    pub fn in_data_dir(subpath: &str) -> PathBuf {
        Self::data_dir().join(subpath)
    }

    /// Trusted, admin-owned managed-policy file (BR-65). Returns `None` on
    /// platforms without a well-known admin-writable location.
    ///
    /// Deliberately has **no** dedicated env override in production — an env var
    /// is user-settable and would defeat the tamper model. Only the pre-existing
    /// test seam `BIOROUTER_PATH_ROOT` is honored (it relocates the whole config
    /// root under a temp dir the test owns).
    pub fn managed_policy_path() -> Option<PathBuf> {
        if let Ok(test_root) = std::env::var("BIOROUTER_PATH_ROOT") {
            return Some(
                PathBuf::from(test_root)
                    .join("managed")
                    .join("managed-policy.yaml"),
            );
        }
        #[cfg(target_os = "macos")]
        {
            Some(PathBuf::from(
                "/Library/Application Support/BioRouter/managed-policy.yaml",
            ))
        }
        #[cfg(target_os = "linux")]
        {
            Some(PathBuf::from("/etc/biorouter/managed-policy.yaml"))
        }
        #[cfg(target_os = "windows")]
        {
            std::env::var("ProgramData").ok().map(|program_data| {
                PathBuf::from(program_data)
                    .join("BioRouter")
                    .join("managed-policy.yaml")
            })
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            None
        }
    }
}

enum DirType {
    Config,
    Data,
    State,
}
