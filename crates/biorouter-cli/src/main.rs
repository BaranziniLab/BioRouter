use biorouter::agents::turn_abort::{exit as abort_exit, TurnFailed};
use biorouter_cli::cli::cli;
use std::process::ExitCode;

// Tuned jemalloc as the global allocator (default-on `jemalloc` feature). The
// long-running CLI/TUI churns conversation buffers per turn; jemalloc returns
// freed pages to the OS far more readily than the system allocator's retained
// arenas.
#[cfg(all(feature = "jemalloc", not(target_os = "windows")))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[tokio::main]
async fn main() -> ExitCode {
    // BR-69: if this process was re-exec'd as the shell-sandbox helper (hidden
    // `__br-sandbox` marker, Linux only), apply the in-process Landlock/seccomp
    // restrictions and `execve` the target program. Never returns in that case;
    // a normal invocation falls straight through. Must run before any real work.
    biorouter_mcp::run_shell_sandbox_helper_if_invoked();
    tune_allocator();
    if let Err(e) = biorouter_cli::logging::setup_logging(None, None) {
        eprintln!("Warning: Failed to initialize logging: {}", e);
    }

    match cli().await {
        Ok(()) => ExitCode::from(abort_exit::OK),
        Err(e) => {
            eprintln!("Error: {e:?}");
            // A turn that ran but did not complete its work gets its own exit
            // code, so a caller can tell "the provider rejected our key" (75)
            // from "the agent ran and disagreed with you" (0) — which is what a
            // 403 used to look like. Everything else keeps the historical rc 1.
            match e.downcast_ref::<TurnFailed>() {
                Some(failed) => ExitCode::from(failed.exit_code()),
                None => ExitCode::from(abort_exit::GENERIC),
            }
        }
    }
}

/// Lower jemalloc's dirty/muzzy decay so freed pages return to the OS within
/// ~1s instead of being retained (jemalloc's own defaults — `muzzy_decay_ms:0`,
/// `retain:true` — retain aggressively). Applied to all current arenas
/// (`MALLCTL_ARENAS_ALL` = 4096) and as the default for future arenas. Errors
/// are ignored (`background_thread` is unsupported on macOS). Set
/// `BIOROUTER_DEBUG_ALLOC=1` to print the applied values.
#[cfg(all(feature = "jemalloc", not(target_os = "windows")))]
fn tune_allocator() {
    use tikv_jemalloc_ctl::raw;
    unsafe {
        let _ = raw::write(b"arena.4096.dirty_decay_ms\0", 1000_isize);
        let _ = raw::write(b"arena.4096.muzzy_decay_ms\0", 1000_isize);
        let _ = raw::write(b"arenas.dirty_decay_ms\0", 1000_isize);
        let _ = raw::write(b"arenas.muzzy_decay_ms\0", 1000_isize);
        let _ = raw::write(b"background_thread\0", true);
        if std::env::var_os("BIOROUTER_DEBUG_ALLOC").is_some() {
            let d: isize = raw::read(b"arenas.dirty_decay_ms\0").unwrap_or(-1);
            let m: isize = raw::read(b"arenas.muzzy_decay_ms\0").unwrap_or(-1);
            eprintln!("[alloc] jemalloc active; dirty_decay_ms={d} muzzy_decay_ms={m}");
        }
    }
}

#[cfg(not(all(feature = "jemalloc", not(target_os = "windows"))))]
fn tune_allocator() {}
