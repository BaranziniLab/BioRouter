# CLI plural alias defect

> **What this is.** A defect note recording one source-level bug found while writing the workspace control documentation: the CLI's own error text tells the user to run a command that does not exist.
> **Status:** Historical record — found 2026-08-03 during the workspace control documentation pass. The defect is **open**; the pass was documentation-only and deliberately made no Rust change.
> **Audience:** developers working on `biorouter-cli`.

`biorouter session watch`, `send`, `attach` and `cancel` all talk to a running `biorouterd` and therefore need the daemon's shared secret. When `BIOROUTER_SERVER__SECRET_KEY` is unset, the CLI refuses early and prints a hint showing how to start the daemon and re-run the command. The hint's second example line is misspelled, and the misspelling is the kind a user will copy and paste.

## The defect

In [`crates/biorouter-cli/src/commands/session_watch.rs`](../../../crates/biorouter-cli/src/commands/session_watch.rs), the `secret_key()` error at **line 44** ends with the literal `BIOROUTER_SERVER__SECRET_KEY=<key> biorouter sessions watch <id>` — **plural `sessions`** — but there is no `sessions` command and no `sessions` alias. The clap tree in `crates/biorouter-cli/src/cli.rs` declares exactly one relevant variant, `Command::Session` at line 1216, carrying `visible_alias = "s"` and nothing else (line 1214); `watch` is a variant of its `SessionCommand` subcommand enum (line 516). No `infer_subcommands` is set anywhere in the crate, and prefix inference would not help regardless, because `sessions` is longer than `session` rather than a prefix of it. The CLI's own parity table agrees with the singular — `workspace_parity.rs` resolves `&["session", "watch"]` against the real clap tree at line 237 — so the error text is the only thing in the crate still spelling it the old way. The correct string is `BIOROUTER_SERVER__SECRET_KEY=<key> biorouter session watch <id>`; the surrounding `biorouterd agent` line above it is correct and must not be touched (`Commands::Agent` exists in `crates/biorouter-server/src/main.rs:42`). Three doc comments in the same file carry the same wrong spelling and should be corrected in the same commit — the module header at line 1 and the two function doc comments at lines 586 and 621 — though those are not user-visible.

```text
wrong:    BIOROUTER_SERVER__SECRET_KEY=<key> biorouter sessions watch <id>
correct:  BIOROUTER_SERVER__SECRET_KEY=<key> biorouter session watch <id>
```

## How to verify

Before the fix, `biorouter sessions watch abc` fails at argument parsing with clap's `unrecognized subcommand 'sessions'` — that failure *is* the defect, since it is the exact line the CLI just told the user to run. After the fix, run the CLI with `BIOROUTER_SERVER__SECRET_KEY` unset (`env -u BIOROUTER_SERVER__SECRET_KEY biorouter session watch abc`), copy the second hint line verbatim, and confirm it parses instead of erroring. Finish with `grep -rn "biorouter sessions" crates/`, which must return nothing.

> **Note.** This was known and left deliberately. The BR-71 execution plan's Task 20 carries a 2026-07-31 spelling amendment stating that the plan wrote `biorouter sessions …` in roughly forty places, that all of them were wrong, and that adding a `sessions` alias is a product decision for the CLI rather than a repair a documentation task makes. Both repairs remain available: fix the string, or add `visible_alias = "sessions"` to `Command::Session` so the plural users will type actually works. Fixing the string is the smaller change and is what this note recommends.

## Related documentation

- [Workspace control](../../agent-loop/workspace-control.md) — the live user guide for running several conversations at once, including the terminal commands this error text belongs to.
- [BR-71 workspace control implementation plan](../../agent-loop/designs/br71-execution-plan.md) — the plan of record; its Task 20 spelling amendment is the prior art for this finding.
- [Workspace Control extension](../../extensions/built-in/workspace.md) — the per-tool reference for the tools the CLI commands exercise.
