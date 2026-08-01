//! BR-71 CLI parity gate (operator requirement, 2026-07-30).
//!
//! One row per workspace capability, mapping it to the CLI that delivers it.
//! Three properties are enforced, and the FIRST is a compile error rather than a
//! test failure:
//!
//! 1. `cli_counterpart` and `surface` are exhaustive `match`es with **no
//!    wildcard arm**. A new `WorkspaceCapability` variant that nobody gave a CLI
//!    row is `error[E0004]: non-exhaustive patterns` — the build fails.
//! 2. The tool-backed rows equal `WorkspaceClient::get_tools()` exactly, and the
//!    route-backed rows equal the `workspace`-tagged OpenAPI operations exactly.
//!    So a capability added to a *surface* without a variant here is a red test.
//! 3. Every row resolves against the real clap tree.
//!
//! Together: a capability cannot enter the daemon or the GUI without either a
//! working CLI counterpart or a written, bounded, operator-visible exception.
//!
//! ⚠ `ALL_CAPABILITIES` is hand-listed (counting enum variants needs a derive
//! this crate does not carry). It cannot go stale *usefully*: a variant added to
//! the enum but omitted here describes a surface entry that property 2 then
//! reports as unclaimed. A variant omitted here that describes NOTHING is
//! already dead code.

#[cfg(test)]
use std::collections::BTreeSet;

/// Every capability of BR-71 workspace control, at the granularity a user cares
/// about. Adding a variant without a `cli_counterpart` arm does not compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceCapability {
    // The agent-facing tool surface (`WorkspaceClient::get_tools()`).
    List,
    ReadConversation,
    SendPrompt,
    SetTools,
    Close,
    Watch,
    Open,
    Spawn,
    // The daemon HTTP surface the GUI stands on (tag = "workspace").
    RouteReply,
    RouteInterrupt,
    RouteCancel,
    RouteObserve,
    RouteRunning,
}

pub const ALL_CAPABILITIES: &[WorkspaceCapability] = &[
    WorkspaceCapability::List,
    WorkspaceCapability::ReadConversation,
    WorkspaceCapability::SendPrompt,
    WorkspaceCapability::SetTools,
    WorkspaceCapability::Close,
    WorkspaceCapability::Watch,
    WorkspaceCapability::Open,
    WorkspaceCapability::Spawn,
    WorkspaceCapability::RouteReply,
    WorkspaceCapability::RouteInterrupt,
    WorkspaceCapability::RouteCancel,
    WorkspaceCapability::RouteObserve,
    WorkspaceCapability::RouteRunning,
];

pub enum Surface {
    Tool(&'static str),
    Route {
        method: &'static str,
        path: &'static str,
    },
}

/// Where this capability is reachable from the GUI or the daemon.
///
/// ⚠ NO WILDCARD ARM — see property 1 in the module docs.
pub fn surface(capability: WorkspaceCapability) -> Surface {
    use WorkspaceCapability as C;
    match capability {
        C::List => Surface::Tool("workspace_list"),
        C::ReadConversation => Surface::Tool("workspace_read_conversation"),
        C::SendPrompt => Surface::Tool("workspace_send_prompt"),
        C::SetTools => Surface::Tool("workspace_set_tools"),
        C::Close => Surface::Tool("workspace_close"),
        C::Watch => Surface::Tool("workspace_watch"),
        C::Open => Surface::Tool("workspace_open"),
        // The ONE spawn tool (BR-71 decisions 20/22), advertised under its
        // pre-existing bare name — `biorouter::agents::subagent_tool::
        // SUBAGENT_TOOL_NAME`, not a `workspace_`-prefixed alias. The `workspace__`
        // form the model sees is the extension manager's prefix, applied later.
        C::Spawn => Surface::Tool("subagent"),
        C::RouteReply => Surface::Route {
            method: "POST",
            path: "/reply",
        },
        C::RouteInterrupt => Surface::Route {
            method: "POST",
            path: "/interrupt",
        },
        C::RouteCancel => Surface::Route {
            method: "POST",
            path: "/agent/cancel",
        },
        C::RouteObserve => Surface::Route {
            method: "GET",
            path: "/sessions/{session_id}/events",
        },
        C::RouteRunning => Surface::Route {
            method: "GET",
            path: "/sessions/running",
        },
    }
}

pub enum Counterpart {
    /// `biorouter <path…> [--args…]`, resolved against the real clap tree.
    Cli {
        path: &'static [&'static str],
        args: &'static [&'static str],
    },
    /// A capability with no terminal analogue. Requires a stated reason and is
    /// capped at two by the tests.
    Asymmetry { reason: &'static str },
}

/// ⚠ NO WILDCARD ARM. This is the compile-time half of the guarantee.
pub fn cli_counterpart(capability: WorkspaceCapability) -> Counterpart {
    use WorkspaceCapability as C;
    match capability {
        C::List => Counterpart::Cli {
            path: &["session", "list"],
            args: &["subagents"],
        },
        C::ReadConversation => Counterpart::Cli {
            path: &["session", "export"],
            args: &["format"],
        },
        C::SendPrompt => Counterpart::Cli {
            path: &["session", "send"],
            args: &[],
        },
        C::Watch => Counterpart::Cli {
            path: &["session", "watch"],
            args: &["follow"],
        },
        C::Close => Counterpart::Cli {
            path: &["session", "cancel"],
            args: &[],
        },
        C::Spawn => Counterpart::Asymmetry {
            reason: "Spawning is a TOOL the model calls, not a command the user \
                     types, and it already works in the CLI: the CLI's reply path \
                     is Agent::reply -> prepare_tools_and_prompt -> list_tools -> \
                     ensure_spawn_extension, so `workspace__subagent` is advertised \
                     in a `biorouter session` exactly as in the daemon. Verified in \
                     Task 42b's investigation; there is nothing for a subcommand to add.",
        },
        C::SetTools => Counterpart::Asymmetry {
            reason: "Reconfiguring ANOTHER session's extensions/skills/model from a \
                     terminal is outside the operator's requirement (spin up, monitor, \
                     inject) and would duplicate `biorouter extension`/`skill`, which \
                     are machine-wide rather than session-scoped. Revisit only with \
                     an explicit request.",
        },
        C::Open => Counterpart::Cli {
            path: &["session"],
            args: &["name"],
        },
        C::RouteReply => Counterpart::Cli {
            path: &["session", "send"],
            args: &[],
        },
        C::RouteInterrupt => Counterpart::Cli {
            path: &["session", "attach"],
            args: &[],
        },
        C::RouteCancel => Counterpart::Cli {
            path: &["session", "cancel"],
            args: &[],
        },
        C::RouteObserve => Counterpart::Cli {
            path: &["session", "attach"],
            args: &["of"],
        },
        C::RouteRunning => Counterpart::Cli {
            path: &["session", "list"],
            args: &["subagents"],
        },
    }
}

/// Walk the real clap tree. Returns `None` for a path that does not exist —
/// which is what makes check C non-vacuous.
pub fn resolve<'a>(root: &'a clap::Command, path: &[&str]) -> Option<&'a clap::Command> {
    let mut current = root;
    for segment in path {
        current = current
            .get_subcommands()
            .find(|c| c.get_name() == *segment)?;
    }
    Some(current)
}

/// The `(METHOD, path)` of every OpenAPI operation tagged `workspace`.
///
/// `include_str!` rather than a runtime read: the path is resolved at compile
/// time from `CARGO_MANIFEST_DIR`, so a moved or missing document is a build
/// error rather than a test that silently sees an empty set. The document is
/// kept current by `just generate-openapi` and by Task 44's
/// `git diff --exit-code ui/desktop/openapi.json`.
///
/// ⚠ **`#[cfg(test)]`, and that is not tidiness.** `ui/desktop/openapi.json` is
/// **276 KB** (measured 2026-07-31). The capability table above is production
/// code because it is a contract, but embedding a quarter-megabyte document into
/// the shipped `biorouter` binary to satisfy a test is not — and a reviewer who
/// notices the size later will delete the wrong half. Keep the table public and
/// the reader test-only.
#[cfg(test)]
pub fn workspace_tagged_operations() -> BTreeSet<(String, String)> {
    const DOC: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../ui/desktop/openapi.json"
    ));
    let doc: serde_json::Value = serde_json::from_str(DOC).expect("openapi.json is valid JSON");
    let mut out = BTreeSet::new();
    for (path, methods) in doc["paths"].as_object().expect("paths is an object") {
        for (method, operation) in methods.as_object().expect("methods is an object") {
            let tagged = operation["tags"]
                .as_array()
                .is_some_and(|tags| tags.iter().any(|t| t == "workspace"));
            if tagged {
                out.insert((method.to_uppercase(), path.clone()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// A: every tool the workspace extension advertises has a row, and every
    /// tool-backed row names a tool that still exists. EXACT SET EQUALITY, both
    /// directions — a subset check passes the exact regression this gate exists
    /// to catch.
    #[test]
    fn the_table_matches_the_workspace_tool_surface_exactly() {
        let advertised: BTreeSet<String> =
            biorouter::agents::workspace_extension::WorkspaceClient::get_tools()
                .into_iter()
                .map(|tool| tool.name.to_string())
                .collect();
        assert!(
            !advertised.is_empty(),
            "get_tools() returned nothing — the selector, not the surface, is broken"
        );
        let tabled: BTreeSet<String> = ALL_CAPABILITIES
            .iter()
            .filter_map(|c| match surface(*c) {
                Surface::Tool(name) => Some(name.to_string()),
                Surface::Route { .. } => None,
            })
            .collect();
        assert_eq!(
            tabled,
            advertised,
            "workspace tools and CLI-parity rows disagree.\n\
             In the surface, not the table (add a CLI row): {:?}\n\
             In the table, not the surface (delete the row): {:?}",
            advertised.difference(&tabled).collect::<Vec<_>>(),
            tabled.difference(&advertised).collect::<Vec<_>>()
        );
    }

    /// B: every daemon operation tagged `workspace` has a row, and vice versa.
    #[test]
    fn the_table_matches_the_tagged_daemon_route_surface_exactly() {
        let described = workspace_tagged_operations();
        assert!(
            !described.is_empty(),
            "no operation carries the `workspace` tag — the selector matched \
             nothing, which is how a gate exits 0 having checked nothing"
        );
        let tabled: BTreeSet<(String, String)> = ALL_CAPABILITIES
            .iter()
            .filter_map(|c| match surface(*c) {
                Surface::Route { method, path } => Some((method.to_string(), path.to_string())),
                Surface::Tool(_) => None,
            })
            .collect();
        assert_eq!(
            tabled, described,
            "tagged routes and CLI-parity rows disagree"
        );
    }

    /// C: every row's CLI counterpart resolves against the REAL clap tree.
    /// A row is a claim about the binary; this is what makes it checkable.
    #[test]
    fn every_counterpart_exists_in_the_real_cli() {
        let root = crate::cli::command_tree();
        for capability in ALL_CAPABILITIES {
            match cli_counterpart(*capability) {
                Counterpart::Cli { path, args } => {
                    let command = resolve(&root, path).unwrap_or_else(|| {
                        panic!(
                            "{capability:?}: no `biorouter {}` subcommand",
                            path.join(" ")
                        )
                    });
                    for arg in args {
                        assert!(
                            command.get_arguments().any(|a| a.get_id().as_str() == *arg),
                            "{capability:?}: `biorouter {}` has no --{arg}",
                            path.join(" ")
                        );
                    }
                }
                Counterpart::Asymmetry { reason } => assert!(
                    reason.len() > 40,
                    "{capability:?}: an accepted asymmetry needs a real reason, \
                     not a placeholder"
                ),
            }
        }
    }

    /// C-negative: the resolver can actually FAIL. Without this, a `resolve`
    /// that returns the root for any path makes check C vacuous — which is the
    /// "assertion true for all inputs" failure this campaign keeps finding.
    #[test]
    fn the_counterpart_resolver_rejects_a_subcommand_that_does_not_exist() {
        let root = crate::cli::command_tree();
        assert!(resolve(&root, &["session", "list"]).is_some());
        assert!(resolve(&root, &["session", "teleport"]).is_none());
        assert!(resolve(&root, &["teleport"]).is_none());
        let sessions_list = resolve(&root, &["session", "list"]).unwrap();
        assert!(sessions_list
            .get_arguments()
            .any(|a| a.get_id().as_str() == "subagents"));
        assert!(!sessions_list
            .get_arguments()
            .any(|a| a.get_id().as_str() == "teleport"));
    }

    /// D: at most one accepted asymmetry per capability family, and each one is
    /// deliberate. A table that answers "Asymmetry" for everything is a table
    /// that gates nothing.
    #[test]
    fn accepted_asymmetries_stay_a_short_and_declared_list() {
        let asymmetric: Vec<_> = ALL_CAPABILITIES
            .iter()
            .filter(|c| matches!(cli_counterpart(**c), Counterpart::Asymmetry { .. }))
            .collect();
        assert!(
            asymmetric.len() <= 2,
            "{} accepted asymmetries — each new one needs operator sign-off in \
             the plan, not a table edit: {asymmetric:?}",
            asymmetric.len()
        );
    }
}
